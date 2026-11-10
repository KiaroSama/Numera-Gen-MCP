use crate::context::fail;
use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

pub use numera_gen_mcp::devtools::private_files::Secret;

pub struct PrivateInputs {
    pub directory: PathBuf,
    pub pfx: PathBuf,
    pub password: PathBuf,
}
impl PrivateInputs {
    pub fn create(root: &Path, pfx: &[u8], password: &[u8]) -> io::Result<Self> {
        if pfx.is_empty()
            || pfx.len() > 32768
            || password.is_empty()
            || password.len() > 4095
            || !password
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(b))
        {
            return Err(fail("Selected signing inputs are missing or malformed."));
        }
        let parent = root.join(".ci-work");
        fs::create_dir_all(&parent)?;
        if !parent.canonicalize()?.starts_with(root.canonicalize()?) {
            return Err(fail("Private input path escapes workspace."));
        }
        let directory = parent.join("signing");
        create_directory(&directory)?;
        let inputs = Self {
            pfx: directory.join("publisher.pfx"),
            password: directory.join("password.txt"),
            directory,
        };
        write(&inputs.pfx, pfx)?;
        write(&inputs.password, password)?;
        Ok(inputs)
    }
    pub fn cleanup(self) -> io::Result<()> {
        for path in [&self.pfx, &self.password] {
            fs::remove_file(path)?;
        }
        fs::remove_dir(&self.directory)?;
        if self.directory.try_exists()? {
            return Err(fail("Private input cleanup failed."));
        }
        Ok(())
    }
}
impl Drop for PrivateInputs {
    fn drop(&mut self) {
        for path in [&self.pfx, &self.password] {
            if path.exists() && fs::remove_file(path).is_err() {
                eprintln!("ERROR: Private signing input cleanup remains unverified.");
            }
        }
        if self.directory.exists() && fs::remove_dir(&self.directory).is_err() {
            eprintln!("ERROR: Private signing directory cleanup remains unverified.");
        }
    }
}
fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    numera_gen_mcp::devtools::private_files::write_new(path, bytes)
}
pub(crate) fn create_directory(path: &Path) -> io::Result<()> {
    numera_gen_mcp::devtools::private_files::create_directory(path)
}
