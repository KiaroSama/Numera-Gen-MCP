use super::{invalid, valid};
use numera_gen_mcp::media::Kind;

fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [&((body.len() + 8) as u32).to_be_bytes()[..], kind, body].concat()
}

fn full(kind: &[u8; 4], words: &[u32]) -> Vec<u8> {
    let body = words.iter().flat_map(|word| word.to_be_bytes()).collect::<Vec<_>>();
    atom(kind, &[&[0; 4][..], &body].concat())
}

fn container(kind: &[u8]) -> bool {
    matches!(kind, b"moov" | b"trak" | b"mdia" | b"minf" | b"dinf" | b"stbl")
}

fn find<'a>(bytes: &'a [u8], target: &[u8; 4]) -> Option<&'a [u8]> {
    let mut offset = 0;
    while offset < bytes.len() {
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        let body = &bytes[offset + 8..offset + size];
        if kind == target {
            return Some(body);
        }
        if container(kind) && let Some(found) = find(body, target) {
            return Some(found);
        }
        offset += size;
    }
    None
}

fn replace(bytes: &[u8], target: &[u8; 4], replacement: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let kind: &[u8; 4] = bytes[offset + 4..offset + 8].try_into().unwrap();
        let body = &bytes[offset + 8..offset + size];
        if kind == target {
            result.extend_from_slice(replacement);
        } else if container(kind) {
            result.extend_from_slice(&atom(kind, &replace(body, target, replacement)));
        } else {
            result.extend_from_slice(&bytes[offset..offset + size]);
        }
        offset += size;
    }
    result
}

fn payload(bytes: &[u8], data: &[u8]) -> Vec<u8> {
    let bytes = replace(bytes, b"mdat", &atom(b"mdat", data));
    replace(&bytes, b"stsz", &full(b"stsz", &[0, 1, data.len() as u32]))
}

fn video(codec: &[u8; 4], configuration: Option<(&[u8; 4], &[u8])>, data: &[u8]) -> Vec<u8> {
    let bytes = crate::bytes::mp4();
    let mut entry = find(&bytes, b"stsd").unwrap()[16..].to_vec();
    if let Some((kind, body)) = configuration {
        entry.extend_from_slice(&atom(kind, body));
    }
    let stsd = atom(b"stsd", &[&[0,0,0,0,0,0,0,1][..], &atom(codec, &entry)].concat());
    payload(&replace(&bytes, b"stsd", &stsd), data)
}

fn avcc() -> Vec<u8> {
    // ISO AVCDecoderConfigurationRecord: one SPS and one PPS, four-byte NAL sizes.
    vec![1,66,0,30,0xff,0xe1,0,4,0x67,66,0,30,1,0,2,0x68,0]
}

fn hvcc() -> Vec<u8> {
    // ISO HEVCDecoderConfigurationRecord: VPS/SPS/PPS arrays, four-byte NAL sizes.
    let mut bytes = vec![0;23];
    bytes[0] = 1;
    bytes[21] = 3;
    bytes[22] = 3;
    for (kind, size) in [(32,6usize),(33,15),(34,3)] {
        bytes.extend_from_slice(&[kind,0,1]);
        bytes.extend_from_slice(&(size as u16).to_be_bytes());
        let mut nal = vec![0;size];
        nal[0] = kind << 1;
        nal[1] = 1;
        bytes.extend_from_slice(&nal);
    }
    bytes
}

pub fn positive() {
    for codec in [b"avc1", b"avc3"] {
        for extension in [false, true] {
            let mut config = avcc();
            if extension { config.extend_from_slice(&[0xfc,0xf8,0xf8,0]); }
            let bytes = video(codec, Some((b"avcC", &config)), &[0,0,0,2,0x65,0x88]);
            assert_eq!(valid(Kind::Video, &bytes)["sample_count"], 1);
        }
    }
    for codec in [b"hvc1", b"hev1"] {
        let bytes = video(codec, Some((b"hvcC", &hvcc())), &[0,0,0,3,0x26,1,0]);
        assert_eq!(valid(Kind::Video, &bytes)["duration_ms"], 1000);
    }
    let original = crate::bytes::mp4();
    let jpeg = find(&original, b"mdat").unwrap();
    let mut bytes = video(b"mjpg", None, jpeg);
    bytes = replace(&bytes, b"stsz", &full(b"stsz", &[jpeg.len() as u32,1]));
    let co64 = atom(b"co64", &[&[0,0,0,0,0,0,0,1][..], &32u64.to_be_bytes()].concat());
    bytes = replace(&bytes, b"stco", &co64);
    for version in [0,1] {
        let mut ctts = full(b"ctts", &[1,1,0]);
        ctts[8] = version;
        let stbl = atom(b"stbl", &[find(&bytes,b"stbl").unwrap(), &ctts, &full(b"stss", &[1,1])].concat());
        let mut result = replace(&bytes,b"stbl",&stbl);
        result.extend_from_slice(&[&1u32.to_be_bytes()[..],b"free",&16u64.to_be_bytes()].concat());
        result.extend_from_slice(&[&0u32.to_be_bytes()[..],b"free"].concat());
        assert_eq!(valid(Kind::Video,&result)["width"],2);
    }
}

pub fn negative() {
    let bytes = crate::bytes::mp4();
    for (kind, at, value) in [
        (b"mvhd",12,0),(b"tkhd",12,0),(b"mdhd",12,0),(b"mdhd",16,0),(b"mdhd",16,u32::MAX),
        (b"hdlr",8,u32::from_be_bytes(*b"soun")),(b"stsd",4,0),(b"stsz",4,1),(b"stsz",8,0),
        (b"stts",8,0),(b"stts",12,0),(b"stts",8,2),(b"stts",12,999),
        (b"stsc",8,0),(b"stsc",8,2),(b"stsc",12,0),(b"stsc",16,0),(b"stsc",16,2),
        (b"stco",8,0),(b"stco",8,u32::MAX),
    ] {
        let mut body = find(&bytes,kind).unwrap().to_vec();
        body[at..at+4].copy_from_slice(&value.to_be_bytes());
        invalid(Kind::Video,&replace(&bytes,kind,&atom(kind,&body)));
    }
    for kind in [b"stco",b"stsz",b"stsd",b"vmhd",b"dref",b"mvhd"] {
        invalid(Kind::Video,&replace(&bytes,kind,&[]));
        invalid(Kind::Video,&replace(&bytes,kind,&atom(kind,&[0;3])));
        let duplicate = atom(kind,find(&bytes,kind).unwrap()).repeat(2);
        invalid(Kind::Video,&replace(&bytes,kind,&duplicate));
    }
    let mut dref = find(&bytes,b"dref").unwrap().to_vec();
    *dref.last_mut().unwrap() = 0;
    invalid(Kind::Video,&replace(&bytes,b"dref",&atom(b"dref",&dref)));
    for extra in [full(b"stss",&[1,0]),full(b"stss",&[1,2]),full(b"ctts",&[2,1,0]),full(b"ctts",&[1,2,0]),
        [&1u32.to_be_bytes()[..],b"tiny",&8u64.to_be_bytes()].concat()] {
        let stbl = atom(b"stbl",&[find(&bytes,b"stbl").unwrap(),&extra].concat());
        invalid(Kind::Video,&replace(&bytes,b"stbl",&stbl));
    }
    for extra in [atom(b"moof",&[]),atom(b"sidx",&[]),atom(b"ftyp",b"isom\0\0\0\0")] {
        invalid(Kind::Video,&[bytes.as_slice(),&extra].concat());
    }
    let moov = atom(b"moov",&[find(&bytes,b"moov").unwrap(),&atom(b"mvex",&[])].concat());
    invalid(Kind::Video,&replace(&bytes,b"moov",&moov));
    invalid(Kind::Video,&replace(&bytes,b"ftyp",&atom(b"ftyp",b"nope\0\0\0\0")));
    let avc_data = [0,0,0,2,0x65,0x88];
    for (at,value) in [(0,0),(4,0xfe),(5,0xe0),(6,0xff),(8,0xe7),(8,0x68),(12,0),(15,0x67)] {
        let mut config = avcc();
        config[at] = value;
        invalid(Kind::Video,&video(b"avc1",Some((b"avcC",&config)),&avc_data));
    }
    for length in 0..avcc().len() {
        invalid(Kind::Video,&video(b"avc1",Some((b"avcC",&avcc()[..length])),&avc_data));
    }
    for extra in [&[0u8][..],&[0xfc,0xf8,0xf8,1][..],&[0xfc,0xf8,0xf8,0,7][..]] {
        invalid(Kind::Video,&video(b"avc1",Some((b"avcC",&[avcc().as_slice(),extra].concat())),&avc_data));
    }
    for data in [vec![0,0,0],vec![0,0,0,5,0x65,0],vec![0,0,0,1,0x65],vec![0,0,0,2,0xe5,0],
        vec![0,0,0,2,0,0],vec![0,0,0,2,0x78,0],vec![0,0,0,2,6,0]] {
        invalid(Kind::Video,&video(b"avc1",Some((b"avcC",&avcc())),&data));
    }
    let hevc_data = [0,0,0,3,0x26,1,0];
    for (at,value) in [(0,0),(22,0),(23,31),(26,0xff),(28,0xc0),(29,0)] {
        let mut config = hvcc();
        config[at] = value;
        invalid(Kind::Video,&video(b"hvc1",Some((b"hvcC",&config)),&hevc_data));
    }
    for length in [0,22,24,27,hvcc().len()-1] {
        invalid(Kind::Video,&video(b"hvc1",Some((b"hvcC",&hvcc()[..length])),&hevc_data));
    }
    for data in [vec![0,0,0,3,0xa6,1,0],vec![0,0,0,3,0x26,0,0],vec![0,0,0,3,0x40,1,0]] {
        invalid(Kind::Video,&video(b"hvc1",Some((b"hvcC",&hvcc())),&data));
    }
}
