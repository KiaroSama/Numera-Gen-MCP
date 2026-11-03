use numera_task::formatting;
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../.ci-work/tooling-tests")
            .join(format!("formatting {}", uuid::Uuid::new_v4()));
        for directory in ["native", "tests/rust", "tooling/src", "tooling/tests"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            if std::thread::panicking() {
                eprintln!("Formatting fixture cleanup failed: {:?}", error.kind());
            } else {
                panic!("Formatting fixture cleanup failed: {:?}", error.kind());
            }
        }
    }
}

#[test]
fn formatting_archive_returns_only_tracked_workspace_rust_with_exact_unicode_bytes() {
    let fixture = Fixture::new();
    let expected = BTreeMap::from([
        (
            "native/lib.rs",
            b"pub const NAME: &str = \"Numera\";\n".to_vec(),
        ),
        (
            "tests/rust/example.rs",
            "// تصویر 東京\r\nfn example() {}\r\n".as_bytes().to_vec(),
        ),
        ("tooling/src/lib.rs", b"pub fn task() {}\n".to_vec()),
        (
            "tooling/tests/example.rs",
            b"#[test]\nfn example() {}\n".to_vec(),
        ),
    ])
    .into_iter()
    .map(|(name, bytes)| (name.to_owned(), bytes))
    .collect::<BTreeMap<_, _>>();
    for (name, bytes) in &expected {
        fs::write(fixture.0.join(name), bytes).unwrap();
    }
    fs::write(fixture.0.join("native/untracked.rs"), b"not tracked").unwrap();
    fs::write(fixture.0.join("native/ignored.json"), b"not Rust").unwrap();
    fs::write(fixture.0.join("secrets.md"), b"synthetic private fixture").unwrap();
    let inventory = expected
        .keys()
        .map(String::as_str)
        .chain([
            "native/ignored.json",
            "secrets.md",
            ".env",
            "README.md",
            "scripts/original.py",
        ])
        .collect::<Vec<_>>()
        .join("\0")
        + "\0";
    let bytes = formatting::archive(&fixture.0, inventory.as_bytes()).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    assert_eq!(archive.len(), expected.len());
    let mut actual = BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        assert!(file.is_file());
        assert_eq!(file.unix_mode().unwrap() & 0o170000, 0o100000);
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        actual.insert(file.name().unwrap().into_owned(), data);
    }
    assert_eq!(actual, expected);
    assert_eq!(
        formatting::archive(&fixture.0, inventory.as_bytes()).unwrap(),
        bytes
    );
}

#[test]
fn formatting_archive_rejects_malformed_duplicate_linked_missing_and_oversized_sources() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("native/lib.rs"), b"fn valid() {}\n").unwrap();
    fs::write(fixture.0.join("native/LIB.rs"), b"fn valid() {}\n").unwrap();
    fs::write(fixture.0.join("native/other.rs"), b"fn other() {}\n").unwrap();
    fs::write(fixture.0.join("native/.private.rs"), b"private fixture").unwrap();
    formatting::archive(&fixture.0, b"native/lib.rs\0native/other.rs\0").unwrap();
    for inventory in [
        b"".as_slice(),
        b"native/lib.rs",
        b"native/lib.rs\0native/lib.rs\0",
        b"native/lib.rs\0native/LIB.rs\0",
        b"native/.private.rs\0",
        b"native/../lib.rs\0",
        b"native//lib.rs\0",
        b"native/missing.rs\0",
        b"native/lib.rs\0\0",
        b"native/invalid\xff.rs\0",
    ] {
        assert!(formatting::archive(&fixture.0, inventory).is_err());
    }
    let file = fixture.0.join("native/lib.rs");
    fs::write(&file, [0xff]).unwrap();
    assert!(formatting::archive(&fixture.0, b"native/lib.rs\0").is_err());
    fs::write(&file, b"source").unwrap();
    let link = fixture.0.join("native/linked.rs");
    fs::hard_link(&file, &link).unwrap();
    assert!(formatting::archive(&fixture.0, b"native/linked.rs\0").is_err());
    fs::remove_file(link).unwrap();
    fs::write(&file, b"x").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert!(formatting::archive(&fixture.0, b"native/lib.rs\0").is_err());
    fs::remove_file(&file).unwrap();
    fs::create_dir(&file).unwrap();
    assert!(formatting::archive(&fixture.0, b"native/lib.rs\0").is_err());
}
