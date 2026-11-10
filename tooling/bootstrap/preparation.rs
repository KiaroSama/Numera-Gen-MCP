use crate::policy::{Config, allowed_changes, fail, json_string};
use std::{fs, io};

impl Config {
    pub fn checkout(&self, clean: bool) -> io::Result<()> {
        let head = self.git(&["rev-parse", "--verify", "HEAD"], 1024)?;
        if std::str::from_utf8(&head)
            .map_err(|_| fail("Invalid bootstrap checkout identity."))?
            .trim()
            != self.source
        {
            return Err(fail("Bootstrap checkout differs from hosted source."));
        }
        if clean
            && !self
                .git(
                    &["status", "--porcelain", "--untracked-files=no"],
                    1024 * 1024,
                )?
                .is_empty()
        {
            return Err(fail(
                "Source preparation requires a clean tracked checkout.",
            ));
        }
        Ok(())
    }
    pub fn prepare_checkout(&self) -> io::Result<()> {
        self.preparation()?;
        self.checkout(true)
    }
    pub fn prepare_lock(&self) -> io::Result<()> {
        self.preparation()?;
        self.checkout(true)?;
        self.child("cargo", &["+1.99.1", "generate-lockfile"])
    }
    pub fn prepare_format(&self, check: bool) -> io::Result<()> {
        self.preparation()?;
        self.checkout(false)?;
        if check {
            self.child("cargo", &["+1.99.1", "fmt", "--all", "--", "--check"])?;
            self.child(
                "rustup",
                &[
                    "run",
                    "1.99.1",
                    "rustfmt",
                    "--edition",
                    "2024",
                    "--check",
                    "tooling/bootstrap/main.rs",
                ],
            )
        } else {
            self.child("cargo", &["+1.99.1", "fmt", "--all"])?;
            self.child(
                "rustup",
                &[
                    "run",
                    "1.99.1",
                    "rustfmt",
                    "--edition",
                    "2024",
                    "tooling/bootstrap/main.rs",
                ],
            )
        }
    }
    pub fn prepare_package(&self) -> io::Result<()> {
        self.preparation()?;
        self.checkout(false)?;
        let changed = self.git(&["diff", "--name-only", "-z"], 1024 * 1024)?;
        let paths = allowed_changes(&changed)?;
        if paths.is_empty()
            || !self
                .git(&["diff", "--cached", "--name-only", "-z"], 1024 * 1024)?
                .is_empty()
            || !self
                .git(
                    &["diff", "--name-only", "--diff-filter=ACDRTUXB", "-z"],
                    1024 * 1024,
                )?
                .is_empty()
        {
            return Err(fail(
                "Preparation must contain only existing unstaged lock or Rust modifications.",
            ));
        }
        let mut arguments = vec!["diff", "--binary", "--"];
        arguments.extend(paths.iter().map(String::as_str));
        let patch = self.git(&arguments, 16 * 1024 * 1024)?;
        if patch.is_empty() {
            return Err(fail("Source preparation returned no patch."));
        }
        let directory = self.contained(".ci-work/source-preparation")?;
        fs::create_dir(&directory)?;
        self.write_new(
            ".ci-work/source-preparation/source-preparation.patch",
            &patch,
        )?;
        let digest = self.hash(&directory.join("source-preparation.patch"))?;
        let receipt = format!(
            "{{\"schema\":1,\"operation\":\"source-preparation\",\"repository\":\"KiaroSama/Numera-Gen-MCP\",\"source_sha\":{},\"ref\":{},\"run_id\":{},\"run_attempt\":{},\"patch_sha256\":{},\"qualification_verified\":false}}\n",
            json_string(&self.source),
            json_string(&self.reference),
            json_string(&self.run),
            json_string(&self.attempt),
            json_string(&digest)
        );
        self.write_new(
            ".ci-work/source-preparation/receipt.json",
            receipt.as_bytes(),
        )?;
        let receipt_hash = self.hash(&directory.join("receipt.json"))?;
        let sums = format!("{digest}  source-preparation.patch\n{receipt_hash}  receipt.json\n");
        self.write_new(".ci-work/source-preparation/SHA256SUMS", sums.as_bytes())?;
        if self.git(&["diff", "--name-only", "-z"], 1024 * 1024)? != changed
            || self.git(&arguments, 16 * 1024 * 1024)? != patch
        {
            return Err(fail("Preparation source bytes changed during capture."));
        }
        self.checkout(false)?;
        Ok(())
    }
}
