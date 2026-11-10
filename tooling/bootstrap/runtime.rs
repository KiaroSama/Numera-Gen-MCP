use crate::policy::{Config, checksum_output, fail};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub fn command_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.as_os_str().to_string_lossy();
        if let Some(value) = text.strip_prefix("\\\\?\\UNC\\") {
            return PathBuf::from(format!("\\\\{value}"));
        }
        if let Some(value) = text.strip_prefix("\\\\?\\") {
            return PathBuf::from(value);
        }
    }
    path.to_path_buf()
}

fn read_bound(mut stream: impl Read, limit: usize, exceeded: &AtomicBool) -> io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let count = match stream.read(&mut chunk) {
            Ok(count) => count,
            Err(error) => {
                exceeded.store(true, Ordering::Release);
                return Err(error);
            }
        };
        if count == 0 {
            return Ok(result);
        }
        if count > limit.saturating_sub(result.len()) {
            exceeded.store(true, Ordering::Release);
        } else if !exceeded.load(Ordering::Acquire) {
            result.extend_from_slice(&chunk[..count]);
        }
    }
}

impl Config {
    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let root = command_path(&self.root);
        let mut command = Command::new(program);
        command
            .current_dir(&root)
            .stdin(Stdio::null())
            .env("CARGO_HOME", root.join(".ci-cache/cargo"))
            .env("RUSTUP_HOME", root.join(".ci-cache/rustup"))
            .env("CARGO_TARGET_DIR", root.join("target"))
            .env("TEMP", root.join(".ci-work/temp"))
            .env("TMP", root.join(".ci-work/temp"))
            .env("TMPDIR", root.join(".ci-work/temp"))
            .env("CARGO_BUILD_JOBS", "2")
            .env("RUST_TEST_THREADS", "2")
            .env("NUMERA_TEST_WORKERS", "2")
            .env("CARGO_TERM_COLOR", "never")
            .env("RUSTUP_DIST_SERVER", "https://static.rust-lang.org")
            .env("RUSTUP_UPDATE_ROOT", "https://static.rust-lang.org/rustup")
            .env("RUSTUP_TERM_COLOR", "never")
            .env_remove("RUSTUP_TOOLCHAIN");
        for (key, _) in std::env::vars_os() {
            if key
                .to_str()
                .is_some_and(|key| key.starts_with("NUMERA_SIGNING_") || key.starts_with("GIT_"))
            {
                command.env_remove(key);
            }
        }
        command
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_TERMINAL_PROMPT", "0");
        command
    }
    pub fn child(&self, program: &str, args: &[&str]) -> io::Result<()> {
        // The authenticated outer numera-dev owns all descendants; never create another group.
        let status = self
            .command(program)
            .args(args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        if !status.success() {
            return Err(fail("Required bootstrap child failed."));
        }
        Ok(())
    }
    pub fn capture(&self, program: &str, args: &[&str], limit: usize) -> io::Result<Vec<u8>> {
        let mut child = self
            .command(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| fail("Bootstrap stdout is unavailable."))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| fail("Bootstrap stderr is unavailable."))?;
        let exceeded = AtomicBool::new(false);
        let result = std::thread::scope(|scope| {
            let flag = &exceeded;
            let output = scope.spawn(move || read_bound(stdout, limit, flag));
            let diagnostic = scope.spawn(move || read_bound(stderr, 65536, flag));
            let status = loop {
                if exceeded.load(Ordering::Acquire) {
                    let _ = child.kill();
                    break child.wait();
                }
                match child.try_wait() {
                    Ok(Some(status)) => break Ok(status),
                    Ok(None) => std::thread::park_timeout(Duration::from_millis(10)),
                    Err(error) => {
                        // Scoped readers must not join before the direct writer has been reaped.
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(error);
                    }
                }
            };
            let output = output.join();
            let diagnostic = diagnostic.join();
            let status = status?;
            let output = output.map_err(|_| fail("Bootstrap output owner failed."))??;
            let diagnostic =
                diagnostic.map_err(|_| fail("Bootstrap diagnostic owner failed."))??;
            if exceeded.load(Ordering::Acquire) || diagnostic.len() > 65536 || !status.success() {
                return Err(fail(
                    "Required bootstrap capture failed; private output withheld.",
                ));
            }
            Ok(output)
        });
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        result
    }
    pub fn git(&self, args: &[&str], limit: usize) -> io::Result<Vec<u8>> {
        let mut all = vec![
            "--no-pager",
            "--no-replace-objects",
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
        ];
        all.extend_from_slice(args);
        self.capture("git", &all, limit)
    }
    pub fn contained(&self, relative: &str) -> io::Result<PathBuf> {
        if relative
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
            || relative.chars().any(|ch| matches!(ch, '\\' | ':'))
        {
            return Err(fail("Unsafe bootstrap path."));
        }
        let path = self.root.join(relative);
        let mut ancestor = Some(path.as_path());
        while let Some(current) = ancestor {
            if let Ok(info) = fs::symlink_metadata(current)
                && (info.is_symlink() || !current.canonicalize()?.starts_with(&self.root))
            {
                return Err(fail("Bootstrap path escapes its owning workspace."));
            }
            if current == self.root {
                break;
            }
            ancestor = current.parent();
        }
        Ok(path)
    }
    pub fn write_new(&self, relative: &str, bytes: &[u8]) -> io::Result<()> {
        let path = self.contained(relative)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }
    pub fn hash(&self, path: &Path) -> io::Result<String> {
        let path = command_path(path);
        let path = path
            .to_str()
            .ok_or_else(|| fail("Hash input path must be Unicode."))?;
        let output = if cfg!(target_os = "macos") {
            self.capture("shasum", &["-a", "256", path], 4096)?
        } else {
            self.capture("sha256sum", &[path], 4096)?
        };
        checksum_output(&output)
    }
    pub fn toolchain(&self) -> io::Result<()> {
        if self.coverage {
            self.child(
                "rustup",
                &[
                    "toolchain",
                    "install",
                    "nightly-2026-10-10",
                    "--profile",
                    "minimal",
                    "--component",
                    "llvm-tools-preview",
                    "--no-self-update",
                ],
            )?;
        }
        Ok(())
    }
    pub fn version(&self) -> io::Result<()> {
        self.child("cargo", &["+1.99.1", "--version"])
    }
    pub fn build_task(&self) -> io::Result<()> {
        if !self.build {
            return Err(fail("Preparation cannot compile the unresolved workspace."));
        }
        self.child(
            "cargo",
            &["+1.99.1", "build", "--locked", "-p", "numera-task"],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_read_failure_notifies_the_owner_before_returning_without_private_output() {
        struct Failed;
        impl Read for Failed {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "synthetic private detail",
                ))
            }
        }
        let failed = AtomicBool::new(false);
        assert_eq!(
            read_bound(Failed, 16, &failed).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert!(failed.load(Ordering::Acquire));
        let exceeded = AtomicBool::new(false);
        assert!(
            read_bound(io::Cursor::new(b"bounded-output"), 4, &exceeded)
                .unwrap()
                .is_empty()
        );
        assert!(exceeded.load(Ordering::Acquire));
        let ordinary = AtomicBool::new(false);
        assert_eq!(
            read_bound(io::Cursor::new(b"small"), 5, &ordinary).unwrap(),
            b"small"
        );
        assert!(!ordinary.load(Ordering::Acquire));
    }
}
