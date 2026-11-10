use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".ci-work/native-tests")
            .join(format!("cargo output 東京 {}", uuid::Uuid::new_v4()));
        fs::create_dir_all(path.join("target/release/deps")).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn output(&self) -> PathBuf {
        path(&self.0, Binary::Server)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            if std::thread::panicking() {
                eprintln!("Cargo output fixture cleanup failed: {:?}", error.kind());
            } else {
                panic!("Cargo output fixture cleanup failed: {:?}", error.kind());
            }
        }
    }
}

#[test]
fn compiler_hardlinks_and_detached_copies_do_not_relax_package_artifact_reads() {
    let fixture = Fixture::new();
    let original = fixture.0.join("target/release/deps/compiled-output");
    let bytes = b"inert compiler bytes, never executed";
    fs::write(&original, bytes).unwrap();
    fs::hard_link(&original, fixture.output()).unwrap();
    assert_eq!(read(&fixture.0, Binary::Server).unwrap(), bytes);
    assert!(
        super::super::artifact::read(
            fixture.output().parent().unwrap(),
            Binary::Server.filename(),
            1024,
        )
        .is_err()
    );
    let copied = path(&fixture.0, Binary::Developer);
    fs::copy(&original, &copied).unwrap();
    assert_eq!(read(&fixture.0, Binary::Developer).unwrap(), bytes);
    assert_eq!(
        super::super::artifact::read(copied.parent().unwrap(), Binary::Developer.filename(), 1024)
            .unwrap(),
        bytes
    );
    assert_eq!(fs::read(original).unwrap(), bytes);
}

#[test]
fn noncanonical_roots_nonregular_and_oversized_compiler_inputs_are_rejected() {
    let fixture = Fixture::new();
    fs::write(fixture.output(), b"tiny").unwrap();
    // PathBuf::join normalizes ParentDir on verbatim paths, so retain the raw spelling.
    let separator = std::path::MAIN_SEPARATOR;
    let mut traversal = fixture.0.as_os_str().to_os_string();
    traversal.push(format!("{separator}target{separator}.."));
    let traversal = PathBuf::from(traversal);
    assert!(
        traversal
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    );
    assert!(read(&traversal, Binary::Server).is_err());
    assert!(read(Path::new("relative"), Binary::Server).is_err());
    assert!(
        Opened::new(&fixture.0, Binary::Server)
            .unwrap()
            .read(3)
            .is_err()
    );
    fs::OpenOptions::new()
        .write(true)
        .open(fixture.output())
        .unwrap()
        .set_len(MAX_BYTES as u64 + 1)
        .unwrap();
    assert!(read(&fixture.0, Binary::Server).is_err());
    fs::remove_file(fixture.output()).unwrap();
    fs::create_dir(fixture.output()).unwrap();
    assert!(read(&fixture.0, Binary::Server).is_err());
}

#[cfg(unix)]
#[test]
fn linked_leaf_and_parent_and_fifo_never_become_compiler_outputs() {
    use std::{
        ffi::CString,
        os::unix::{ffi::OsStrExt, fs::symlink},
    };
    let fixture = Fixture::new();
    let external = fixture.0.join("outside-release");
    fs::write(&external, b"untrusted").unwrap();
    symlink(&external, fixture.output()).unwrap();
    assert!(read(&fixture.0, Binary::Server).is_err());
    fs::remove_file(fixture.output()).unwrap();
    let name = CString::new(fixture.output().as_os_str().as_bytes()).unwrap();
    // SAFETY: name is NUL-terminated and points only at this owned disposable fixture.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(read(&fixture.0, Binary::Server).is_err());
    fs::remove_file(fixture.output()).unwrap();
    fs::rename(
        fixture.0.join("target/release"),
        fixture.0.join("moved-release"),
    )
    .unwrap();
    symlink(
        fixture.0.join("moved-release"),
        fixture.0.join("target/release"),
    )
    .unwrap();
    assert!(read(&fixture.0, Binary::Server).is_err());
}

#[cfg(unix)]
#[test]
fn opened_inode_content_and_ancestor_replacements_are_rejected_before_returning_bytes() {
    let fixture = Fixture::new();
    let output = fixture.output();
    fs::write(&output, b"original").unwrap();
    let opened = Opened::new(&fixture.0, Binary::Server).unwrap();
    fs::write(&output, b"changed-size").unwrap();
    assert!(opened.read(1024).is_err());
    fs::write(&output, b"original").unwrap();
    let opened = Opened::new(&fixture.0, Binary::Server).unwrap();
    fs::write(&output, b"modified").unwrap();
    File::options()
        .write(true)
        .open(&output)
        .unwrap()
        .set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1))
        .unwrap();
    assert!(opened.read(1024).is_err());
    fs::write(&output, b"original").unwrap();
    let opened = Opened::new(&fixture.0, Binary::Server).unwrap();
    fs::rename(&output, fixture.0.join("retained-inode")).unwrap();
    fs::write(&output, b"new file").unwrap();
    assert!(opened.read(1024).is_err());
    let replacement = fixture.0.join("replacement-release");
    fs::create_dir(&replacement).unwrap();
    fs::hard_link(&output, replacement.join(Binary::Server.filename())).unwrap();
    // Create the alias before the snapshot so only the parent identity changes below.
    let opened = Opened::new(&fixture.0, Binary::Server).unwrap();
    fs::rename(
        fixture.0.join("target/release"),
        fixture.0.join("original-release"),
    )
    .unwrap();
    fs::rename(replacement, fixture.0.join("target/release")).unwrap();
    let current = Opened::new(&fixture.0, Binary::Server).unwrap();
    assert_eq!(
        identity(&opened.file).unwrap(),
        identity(&current.file).unwrap()
    );
    assert!(unchanged(&opened.metadata, &current.metadata).unwrap());
    assert_ne!(
        identity(opened.directories.last().unwrap()).unwrap(),
        identity(current.directories.last().unwrap()).unwrap()
    );
    assert!(opened.read(1024).is_err());
}

#[cfg(windows)]
#[test]
fn held_windows_output_disallows_alias_writes_and_delete_until_the_read_finishes() {
    let fixture = Fixture::new();
    fs::write(fixture.output(), b"original").unwrap();
    let alias = fixture.0.join("target/release/deps/alias");
    fs::hard_link(fixture.output(), &alias).unwrap();
    let opened = Opened::new(&fixture.0, Binary::Server).unwrap();
    assert!(fs::OpenOptions::new().write(true).open(&alias).is_err());
    assert!(fs::remove_file(fixture.output()).is_err());
    assert_eq!(opened.read(1024).unwrap(), b"original");
    fs::write(alias, b"changed").unwrap();
    assert_eq!(read(&fixture.0, Binary::Server).unwrap(), b"changed");
}
