// Test-only DER reader: inspect public PFX algorithm parameters without importing a private key.
#[derive(Clone, Copy)]
struct Der<'a>(&'a [u8]);
impl<'a> Der<'a> {
    fn next(&mut self, tag: u8) -> &'a [u8] {
        assert_eq!(self.0.first().copied(), Some(tag));
        let first = self.0[1];
        let (header, length) = if first < 128 { (2, first as usize) } else {
            let count = (first & 127) as usize;
            assert!((1..=4).contains(&count));
            assert!(self.0.len() >= 2 + count && self.0[2] != 0);
            let length = self.0[2..2 + count].iter().fold(0usize, |value, byte| value * 256 + usize::from(*byte));
            assert!(length >= 128);
            (2 + count, length)
        };
        let end = header + length;
        assert!(end <= self.0.len());
        let value = &self.0[header..end];
        self.0 = &self.0[end..];
        value
    }
    fn sequence(&mut self) -> Self { Self(self.next(0x30)) }
    fn oid(&mut self, value: &[u8]) { assert_eq!(self.next(6), value); }
    fn integer(&mut self) -> u32 {
        let value = self.next(2);
        assert!(!value.is_empty() && value.len() <= 4 && value[0] & 128 == 0);
        value.iter().fold(0, |number, byte| number * 256 + u32::from(*byte))
    }
    fn end(&self) { assert!(self.0.is_empty()); }
}
const DATA: &[u8] = &[42, 134, 72, 134, 247, 13, 1, 7, 1];
const ENCRYPTED_DATA: &[u8] = &[42, 134, 72, 134, 247, 13, 1, 7, 6];
const SHROUDED_KEY: &[u8] = &[42, 134, 72, 134, 247, 13, 1, 12, 10, 1, 2];
fn pbes2(algorithm: &mut Der<'_>) {
    algorithm.oid(&[42, 134, 72, 134, 247, 13, 1, 5, 13]);
    let mut params = algorithm.sequence();
    let mut kdf = params.sequence();
    kdf.oid(&[42, 134, 72, 134, 247, 13, 1, 5, 12]);
    let mut pbkdf2 = kdf.sequence();
    assert!(pbkdf2.next(4).len() >= 8);
    assert_eq!(pbkdf2.integer(), 200000);
    if pbkdf2.0.first() == Some(&2) { assert_eq!(pbkdf2.integer(), 32); }
    let mut prf = pbkdf2.sequence();
    prf.oid(&[42, 134, 72, 134, 247, 13, 2, 9]);
    assert!(prf.next(5).is_empty());
    prf.end(); pbkdf2.end(); kdf.end();
    let mut cipher = params.sequence();
    cipher.oid(&[96, 134, 72, 1, 101, 3, 4, 1, 42]);
    assert_eq!(cipher.next(4).len(), 16);
    cipher.end(); params.end(); algorithm.end();
}
fn content_data<'a>(content: &mut Der<'a>) -> &'a [u8] {
    content.oid(DATA);
    let mut explicit = Der(content.next(0xa0));
    let bytes = explicit.next(4);
    explicit.end(); content.end(); bytes
}
pub fn assert_pfx_parameters(bytes: &[u8]) {
    let mut root = Der(bytes);
    let mut pfx = root.sequence(); root.end();
    assert_eq!(pfx.integer(), 3);
    let mut content = pfx.sequence();
    let safe = content_data(&mut content);
    let mut envelope = Der(safe);
    let mut contents = envelope.sequence(); envelope.end();
    let mut layers = 0;
    while !contents.0.is_empty() {
        let mut content = contents.sequence();
        if content.0.starts_with(&[6, 9, 42, 134, 72, 134, 247, 13, 1, 7, 6]) {
            content.oid(ENCRYPTED_DATA);
            let mut explicit = Der(content.next(0xa0));
            let mut encrypted = explicit.sequence(); explicit.end();
            assert_eq!(encrypted.integer(), 0);
            let mut info = encrypted.sequence(); info.oid(DATA);
            pbes2(&mut info.sequence());
            assert!(!info.next(0x80).is_empty());
            info.end(); encrypted.end(); content.end(); layers += 1;
        } else {
            let bags = content_data(&mut content);
            let mut root = Der(bags);
            let mut bags = root.sequence(); root.end();
            while !bags.0.is_empty() {
                let mut bag = bags.sequence(); bag.oid(SHROUDED_KEY);
                let mut explicit = Der(bag.next(0xa0));
                let mut encrypted = explicit.sequence(); explicit.end();
                pbes2(&mut encrypted.sequence());
                assert!(!encrypted.next(4).is_empty()); encrypted.end();
                if !bag.0.is_empty() { bag.next(0x31); }
                bag.end(); layers += 1;
            }
        }
    }
    assert_eq!(layers, 2);
    let mut mac = pfx.sequence();
    let mut digest = mac.sequence(); let mut algorithm = digest.sequence();
    algorithm.oid(&[96, 134, 72, 1, 101, 3, 4, 2, 1]);
    assert!(algorithm.next(5).is_empty()); algorithm.end();
    assert_eq!(digest.next(4).len(), 32); digest.end();
    assert!(mac.next(4).len() >= 8);
    assert_eq!(mac.integer(), 200000); mac.end(); pfx.end();
}
