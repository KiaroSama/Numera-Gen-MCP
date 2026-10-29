use numera_gen_mcp::{compat::value::json, devtools::supervisor::{self, Bounds}, logging::Logger,
    runtime_paths::RuntimePaths};
use std::{fs, io::{self, Write}, path::{Path, PathBuf}, process::Command, sync::atomic::AtomicBool};

#[derive(Debug)]
struct SafeFailure(&'static str);
impl std::fmt::Display for SafeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { formatter.write_str(self.0) }
}
impl std::error::Error for SafeFailure {}
pub fn fail(message: &'static str) -> io::Error { io::Error::other(SafeFailure(message)) }
pub fn diagnostic(error: &io::Error) -> String {
    if let Some(safe) = error.get_ref().and_then(|error| error.downcast_ref::<SafeFailure>()) {
        safe.to_string()
    } else { format!("Task operation failed ({:?}); private details withheld.", error.kind()) }
}
pub fn sha(bytes: &[u8]) -> String { numera_gen_mcp::compat::hash::sha256(bytes) }

pub struct Context { pub root: PathBuf, pub logger: Logger }
impl Context {
    pub fn new(root: &Path) -> io::Result<Self> {
        let root = root.canonicalize()?;
        if !root.join("Cargo.toml").is_file() || !root.join("native/lib.rs").is_file() {
            return Err(fail("Expected the owning Numera repository root."));
        }
        RuntimePaths::new(&root).map_err(|_| fail("Cannot establish contained task state."))?;
        let logger = Logger::new(&root.join(".ci-work/native/logs"), "INFO");
        Ok(Self { root, logger })
    }
    pub fn hosted(&self) -> io::Result<()> {
        if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
            return Err(fail("Compilation and verification are hosted CI only."));
        }
        Ok(())
    }
    pub fn source_sha(&self) -> io::Result<String> {
        let sha = std::env::var("GITHUB_SHA").map_err(|_| fail("Exact hosted source SHA required."))?;
        if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
            return Err(fail("Invalid hosted source identity."));
        }
        Ok(sha)
    }
    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command.current_dir(&self.root);
        command.env("CARGO_BUILD_JOBS", "2").env("RUST_TEST_THREADS", "2").env("NUMERA_TEST_WORKERS", "2")
            .env("CARGO_HOME", self.root.join(".ci-cache/cargo"))
            .env("RUSTUP_HOME", self.root.join(".ci-cache/rustup"))
            .env("CARGO_TARGET_DIR", self.root.join("target"))
            .env("CARGO_TERM_COLOR", "never").env("NUMERA_BUILD_SHA", std::env::var("GITHUB_SHA").unwrap_or_else(|_| "unverified-source".into()));
        let mut paths = vec![self.root.join(".ci-cache/cargo/bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        if let Ok(path) = std::env::join_paths(paths) { command.env("PATH", path); }
        command
    }
    pub fn run(&self, command: &mut Command, wall: u64, idle: u64, private: bool) -> io::Result<Vec<u8>> {
        let outcome = self.outcome(command, wall, idle, 2 * 1024 * 1024)?;
        if !private {
            io::stdout().write_all(&outcome.stdout)?;
            io::stderr().write_all(&outcome.stderr)?;
        }
        if outcome.code != 0 || outcome.reason.is_some() || !outcome.cleanup_verified {
            self.logger.log("ERROR", "task", "Required owned command failed.", json!({"exit_code":outcome.code}));
            return Err(fail("Required owned command failed; inspect bounded diagnostics."));
        }
        Ok(outcome.stdout)
    }
    pub fn outcome(&self, command: &mut Command, wall: u64, idle: u64, limit: usize) -> io::Result<supervisor::Outcome> {
        supervisor::run(command, &self.root, Bounds::new(wall, idle)?.with_output_limit(limit)?, &AtomicBool::new(false), &self.logger)
    }
    pub fn capture(&self, command: &mut Command, wall: u64, idle: u64, limit: usize) -> io::Result<Vec<u8>> {
        let outcome = self.outcome(command, wall, idle, limit)?;
        if outcome.code != 0 || !outcome.cleanup_verified || outcome.reason.is_some() {
            self.logger.log("ERROR", "task", "Required bounded capture failed.",
                json!({"exit_code":outcome.code,"termination":outcome.reason,"cleanup_verified":outcome.cleanup_verified,
                    "stdout_bytes":outcome.stdout.len(),"stderr_bytes":outcome.stderr.len()}));
            return Err(fail("Required bounded capture failed; child output withheld from public diagnostics."));
        }
        Ok(outcome.stdout)
    }
    pub fn relative(&self, name: &str) -> io::Result<PathBuf> {
        numera_gen_mcp::files::paths::safe_relative(name).map_err(|_| fail("Unsafe task path."))?;
        let path = self.root.join(name);
        RuntimePaths::new(&self.root).and_then(|paths| paths.verify(&path))
            .map_err(|_| fail("Task path escapes workspace."))?;
        Ok(path)
    }
    pub fn write_new(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let path = self.relative(name)?;
        if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
        let mut options = fs::OpenOptions::new(); options.write(true).create_new(true);
        #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
        let mut file = options.open(path)?; file.write_all(bytes)?; file.sync_all()
    }
}
