use super::{invalid, valid};
use numera_gen_mcp::media::Kind;

fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut bytes = tag.to_vec();
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(body);
    if !body.len().is_multiple_of(2) {
        bytes.push(0);
    }
    bytes
}

fn riff(chunks: &[u8]) -> Vec<u8> {
    [b"RIFF".as_slice(), &((chunks.len() + 4) as u32).to_le_bytes(), b"WAVE", chunks].concat()
}

fn format(code: u16, bits: u16) -> Vec<u8> {
    let align = bits / 8;
    [code.to_le_bytes().as_slice(), &1u16.to_le_bytes(), &8000u32.to_le_bytes(),
        &(8000u32 * u32::from(align)).to_le_bytes(), &align.to_le_bytes(), &bits.to_le_bytes()].concat()
}

pub fn positive() {
    for (code, bits) in [(1, 8), (1, 16), (1, 24), (1, 32), (3, 32), (3, 64)] {
        let data = vec![0; usize::from(bits / 8) * 2];
        let mut fmt = format(code, bits);
        for extension in [false, true] {
            if extension {
                fmt.extend_from_slice(&0u16.to_le_bytes());
            }
            let bytes = riff(&[chunk(b"JUNK", &[7]), chunk(b"fmt ", &fmt), chunk(b"data", &data)].concat());
            let info = valid(Kind::Audio, &bytes);
            assert_eq!(info["sample_count"], 2);
            assert_eq!(info["bits_per_sample"], bits);
            assert_eq!(info["encoding"], if code == 1 { "pcm" } else { "ieee_float" });
        }
    }
}

pub fn negative() {
    let fmt = format(1, 16);
    for chunks in [
        chunk(b"data", &[0, 0]),
        chunk(b"fmt ", &fmt),
        [chunk(b"fmt ", &fmt), chunk(b"fmt ", &fmt), chunk(b"data", &[0, 0])].concat(),
        [chunk(b"fmt ", &fmt), chunk(b"data", &[])].concat(),
        [chunk(b"fmt ", &fmt), chunk(b"data", &[0, 0]), chunk(b"data", &[0, 0])].concat(),
        [chunk(b"fmt ", &fmt), chunk(b"data", &[0])].concat(),
        [chunk(b"fmt ", &[0; 15]), chunk(b"data", &[0, 0])].concat(),
    ] {
        invalid(Kind::Audio, &riff(&chunks));
    }
    for (at, bytes) in [
        (0, 2u16.to_le_bytes().to_vec()), (2, 0u16.to_le_bytes().to_vec()),
        (2, 65u16.to_le_bytes().to_vec()), (4, 0u32.to_le_bytes().to_vec()),
        (4, 768001u32.to_le_bytes().to_vec()), (12, 1u16.to_le_bytes().to_vec()),
        (14, 12u16.to_le_bytes().to_vec()),
    ] {
        let mut changed = fmt.clone();
        changed[at..at + bytes.len()].copy_from_slice(&bytes);
        invalid(Kind::Audio, &riff(&[chunk(b"fmt ", &changed), chunk(b"data", &[0, 0])].concat()));
    }
    for bits in [32, 64] {
        let data = if bits == 32 { f32::INFINITY.to_le_bytes().to_vec() } else { f64::NAN.to_le_bytes().to_vec() };
        invalid(Kind::Audio, &riff(&[chunk(b"fmt ", &format(3, bits)), chunk(b"data", &data)].concat()));
    }
    for extension in [&[0u8][..], &[1, 0][..], &[0, 0, 7][..]] {
        invalid(Kind::Audio, &riff(&[chunk(b"fmt ", &[fmt.as_slice(), extension].concat()), chunk(b"data", &[0, 0])].concat()));
    }
    let mut unpadded = riff(&chunk(b"JUNK", &[1]));
    unpadded.pop();
    let length = (unpadded.len() - 8) as u32;
    unpadded[4..8].copy_from_slice(&length.to_le_bytes());
    invalid(Kind::Audio, &unpadded);
    invalid(Kind::Audio, &riff(&chunk(b"JUNK", &[]).repeat(4097)));
}
