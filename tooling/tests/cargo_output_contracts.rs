use numera_task::{cargo_output, context::Context, distribution::read_regular};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../.ci-work/tooling-tests")
            .join(format!("cargo boundary 東京 {}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("target/release/deps")).unwrap();
        fs::create_dir(root.join("native")).unwrap();
        fs::write(root.join("Cargo.toml"), b"inert fixture, never compiled").unwrap();
        fs::write(root.join("native/lib.rs"), b"inert fixture, never compiled").unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn context(&self) -> Context {
        Context::new(&self.0).unwrap()
    }
    fn file(&self, name: &str) -> PathBuf {
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        self.0
            .join("target/release")
            .join(format!("{name}{suffix}"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            if std::thread::panicking() {
                eprintln!("Cargo boundary fixture cleanup failed: {:?}", error.kind());
            } else {
                panic!("Cargo boundary fixture cleanup failed: {:?}", error.kind());
            }
        }
    }
}

#[test]
fn trusted_cargo_output_accepts_real_hardlinks_without_weakening_package_reader() {
    let fixture = Fixture::new();
    let original = fixture.0.join("target/release/deps/compiler-original");
    let bytes = b"inert source-bound compiler output, never executed";
    fs::write(&original, bytes).unwrap();
    fs::hard_link(&original, fixture.file("numera-gen-mcp")).unwrap();
    fs::copy(&original, fixture.file("numera-dev")).unwrap();
    let context = fixture.context();
    assert_eq!(
        cargo_output::read(&context, "numera-gen-mcp").unwrap(),
        bytes
    );
    assert!(read_regular(&fixture.file("numera-gen-mcp"), 1024).is_err());
    assert_eq!(cargo_output::read(&context, "numera-dev").unwrap(), bytes);
    assert_eq!(
        read_regular(&fixture.file("numera-dev"), 1024).unwrap(),
        bytes
    );
    assert_eq!(fs::read(original).unwrap(), bytes);
}

#[test]
fn trusted_cargo_output_rejects_arbitrary_names_missing_and_nonregular_targets() {
    let fixture = Fixture::new();
    let context = fixture.context();
    for name in [
        "",
        "numera-task",
        "../numera-gen-mcp",
        "numera-gen-mcp/extra",
        "NUMERA-DEV",
    ] {
        assert!(cargo_output::read(&context, name).is_err(), "{name}");
    }
    assert!(cargo_output::read(&context, "numera-gen-mcp").is_err());
    fs::create_dir(fixture.file("numera-gen-mcp")).unwrap();
    assert!(cargo_output::read(&context, "numera-gen-mcp").is_err());
    assert!(read_regular(&fixture.file("numera-gen-mcp"), 1024).is_err());
}
