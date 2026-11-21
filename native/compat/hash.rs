use ring::digest::{SHA256, digest};

pub fn sha256(bytes: &[u8]) -> String {
    let hash = digest(&SHA256, bytes);
    let mut result = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in hash.as_ref() {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 15) as usize] as char);
    }
    result
}
