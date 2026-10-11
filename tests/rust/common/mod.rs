use std::path::{Path, PathBuf};

pub struct Workspace(pub PathBuf);
impl Workspace {
    pub fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".ci-work/native-tests")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).expect("isolated workspace");
        Self(root)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            if std::thread::panicking() {
                eprintln!("Fixture cleanup failed: {:?}.", error.kind());
            } else {
                panic!("Fixture cleanup failed: {:?}.", error.kind());
            }
        }
    }
}
