use std::io::Cursor;

pub fn wav() -> Vec<u8> {
    // Two signed PCM samples, mono 8 kHz: exactly 0.25 ms.
    let mut b = b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x04\0\0\0".to_vec();
    b.extend_from_slice(&[0, 0, 1, 0]);
    b
}

pub fn glb() -> Vec<u8> {
    let mut text = include_bytes!("triangle.gltf").to_vec();
    while !text.len().is_multiple_of(4) { text.push(b' '); }
    let mut bin = Vec::new();
    for v in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] { bin.extend_from_slice(&v.to_le_bytes()); }
    let mut b = b"glTF".to_vec();
    b.extend_from_slice(&2u32.to_le_bytes());
    b.extend_from_slice(&((28 + text.len() + bin.len()) as u32).to_le_bytes());
    b.extend_from_slice(&(text.len() as u32).to_le_bytes());
    b.extend_from_slice(b"JSON");
    b.extend_from_slice(&text);
    b.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    b.extend_from_slice(b"BIN\0");
    b.extend_from_slice(&bin);
    b
}

fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

fn full_atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> { atom(kind, &[&[0u8; 4][..], body].concat()) }

pub fn mp4() -> Vec<u8> {
    // One real JPEG sample in an unfragmented visual track; not a decoder for other codecs.
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 2, image::Rgb([4, 8, 16])));
    let mut jpeg = Cursor::new(Vec::new());
    image.write_to(&mut jpeg, image::ImageFormat::Jpeg).unwrap();
    let jpeg = jpeg.into_inner();
    let ftyp = atom(b"ftyp", b"isom\0\0\0\0isommp42");
    let sample_offset = (ftyp.len() + 8) as u32;
    let mdat = atom(b"mdat", &jpeg);
    let mut mvhd = vec![0u8; 96];
    mvhd[8..12].copy_from_slice(&1000u32.to_be_bytes());
    mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    let mut tkhd = vec![0u8; 80];
    tkhd[8..12].copy_from_slice(&1u32.to_be_bytes());
    tkhd[72..76].copy_from_slice(&(2u32 << 16).to_be_bytes());
    tkhd[76..80].copy_from_slice(&(2u32 << 16).to_be_bytes());
    let mut mdhd = vec![0u8; 20];
    mdhd[8..12].copy_from_slice(&1000u32.to_be_bytes());
    mdhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    let mut hdlr = vec![0u8; 20];
    hdlr[4..8].copy_from_slice(b"vide");
    let mut entry = vec![0u8; 78];
    entry[6..8].copy_from_slice(&1u16.to_be_bytes());
    entry[24..26].copy_from_slice(&2u16.to_be_bytes());
    entry[26..28].copy_from_slice(&2u16.to_be_bytes());
    entry[28..32].copy_from_slice(&0x00480000u32.to_be_bytes());
    entry[32..36].copy_from_slice(&0x00480000u32.to_be_bytes());
    entry[40..42].copy_from_slice(&1u16.to_be_bytes());
    entry[74..76].copy_from_slice(&24u16.to_be_bytes());
    entry[76..78].copy_from_slice(&0xffffu16.to_be_bytes());
    let stsd = full_atom(b"stsd", &[&1u32.to_be_bytes()[..], &atom(b"jpeg", &entry)].concat());
    let words = |values: &[u32]| values.iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<_>>();
    let stts = full_atom(b"stts", &words(&[1, 1, 1000]));
    let stsc = full_atom(b"stsc", &words(&[1, 1, 1, 1]));
    let stsz = full_atom(b"stsz", &words(&[0, 1, jpeg.len() as u32]));
    let stco = full_atom(b"stco", &words(&[1, sample_offset]));
    let stbl = atom(b"stbl", &[stsd, stts, stsc, stsz, stco].concat());
    let url = atom(b"url ", &[0, 0, 0, 1]);
    let dref = full_atom(b"dref", &[&1u32.to_be_bytes()[..], &url].concat());
    let dinf = atom(b"dinf", &dref);
    let minf = atom(b"minf", &[full_atom(b"vmhd", &[0u8; 8]), dinf, stbl].concat());
    let mdia = atom(b"mdia", &[full_atom(b"mdhd", &mdhd), full_atom(b"hdlr", &hdlr), minf].concat());
    let trak = atom(b"trak", &[full_atom(b"tkhd", &tkhd), mdia].concat());
    let moov = atom(b"moov", &[full_atom(b"mvhd", &mvhd), trak].concat());
    [ftyp, mdat, moov].concat()
}

pub fn png() -> Vec<u8> {
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255])));
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png).unwrap();
    output.into_inner()
}
