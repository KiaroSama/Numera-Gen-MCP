use numera_gen_mcp::{
    devtools::supervisor::{Bounds, Outcome, run},
    logging::Logger,
};
use numera_task::{
    context::sha,
    fixtures::{BASELINE, INPUTS, SOURCE},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

struct Fixture {
    root: PathBuf,
    logger: Option<Logger>,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../.ci-work/tooling-tests")
            .join(format!("task capture 日本語 {}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        Self {
            logger: Some(Logger::new(&root.join("owner-logs"), "INFO")),
            root,
        }
    }
    fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command.env_clear().current_dir(&self.root);
        for key in ["PATH", "SystemRoot", "WINDIR", "LLVM_PROFILE_FILE"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
    }
    fn execute(&self, command: &mut Command) -> Outcome {
        let outcome = run(
            command,
            &self.root,
            Bounds::new(10000, 5000).unwrap(),
            &AtomicBool::new(false),
            self.logger.as_ref().unwrap(),
        )
        .unwrap();
        assert!(outcome.reason.is_none());
        assert!(outcome.cleanup_verified);
        outcome
    }
    fn git(&self, args: &[&str]) -> Vec<u8> {
        let mut command = self.command("git");
        command
            .args([
                "-c",
                "core.hooksPath=.empty-hooks",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                numera_gen_mcp::runtime_paths::command_path(&self.root.join("empty-gitconfig")),
            )
            .env("GIT_AUTHOR_NAME", "Numera contract fixture")
            .env("GIT_COMMITTER_NAME", "Numera contract fixture")
            .env("GIT_AUTHOR_EMAIL", "Kiaro.Sama.Dev@gmail.com")
            .env("GIT_COMMITTER_EMAIL", "Kiaro.Sama.Dev@gmail.com")
            .env("GIT_AUTHOR_DATE", "2026-11-03T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-11-03T00:00:00Z")
            .env("GIT_TERMINAL_PROMPT", "0");
        let outcome = self.execute(&mut command);
        let phase = match args.first().copied() {
            Some("init") => "init",
            Some("add") => "add",
            Some("commit") => "commit",
            Some("rev-parse") => "revision",
            _ => "unexpected",
        };
        assert_eq!(outcome.code, 0, "Isolated Git phase failed: {phase}");
        outcome.stdout
    }
    fn task(&self, operation: &str, source: &str, expected: i32) -> Outcome {
        let mut command = self.command(env!("CARGO_BIN_EXE_numera-task"));
        command
            .arg("--root")
            .arg(&self.root)
            .arg(operation)
            .env("GITHUB_ACTIONS", "true")
            .env("GITHUB_SHA", source)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                numera_gen_mcp::runtime_paths::command_path(&self.root.join("empty-gitconfig")),
            );
        let outcome = self.execute(&mut command);
        let stderr = String::from_utf8_lossy(&outcome.stderr);
        assert!(!stderr.contains("synthetic-private-marker"));
        // The task emits only fixed public diagnostics; never print captured child details.
        let diagnostic = stderr
            .lines()
            .find(|line| line.starts_with("ERROR: "))
            .unwrap_or("No fixed task diagnostic.");
        assert_eq!(outcome.code, expected, "{operation}: {diagnostic}");
        outcome
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.logger.take());
        if let Err(error) = fs::remove_dir_all(&self.root) {
            if std::thread::panicking() {
                eprintln!("Task fixture cleanup failed: {:?}", error.kind());
            } else {
                panic!("Task fixture cleanup failed: {:?}", error.kind());
            }
        }
    }
}

#[test]
fn actual_capture_binds_tracked_dependencies_excludes_private_inputs_and_never_overwrites() {
    let fixture = Fixture::new();
    let public = BTreeMap::from([
        ("Cargo.toml", b"inert manifest; never compiled\n".as_slice()),
        ("Cargo.lock", b"inert lock\n".as_slice()),
        ("native/lib.rs", b"inert library\n".as_slice()),
        ("native/main.rs", b"inert entry\n".as_slice()),
        (
            "schemas/fixture.json",
            b"{\"type\":\"object\"}\n".as_slice(),
        ),
        ("tooling/src/helper.rs", b"inert task source\n".as_slice()),
        ("tests/rust/helper.rs", b"inert test source\n".as_slice()),
        (
            "tests/fixtures/oracle/common/fixture.json",
            b"{\"independent\":true}\n".as_slice(),
        ),
        (
            "scripts/rust-oracle.mjs",
            b"inert historical generator; never executed\n".as_slice(),
        ),
        (
            ".github/workflows/ci.yml",
            b"inert workflow; never dispatched\n".as_slice(),
        ),
    ]);
    for (name, bytes) in &public {
        fixture.write(name, bytes);
    }
    fixture.write("empty-gitconfig", b"");
    fs::create_dir(fixture.root.join(".empty-hooks")).unwrap();
    fixture.git(&["init", "--initial-branch=fixture"]);
    for name in ["docs/unconsumed-notes.md", "state/synthetic.json"] {
        fixture.write(name, b"public unconsumed fixture\n");
    }
    let mut add = vec![
        "add",
        "--",
        "docs/unconsumed-notes.md",
        "state/synthetic.json",
    ];
    add.extend(public.keys().copied());
    fixture.git(&add);
    fixture.git(&["commit", "-m", "test: create isolated evidence fixture"]);
    let source = String::from_utf8(fixture.git(&["rev-parse", "HEAD"])).unwrap();
    let source = source.trim();
    for name in [
        ".env",
        "secrets.md",
        ".ai/memory.md",
        "state/private.db",
        ".ci-work/parity/current-user-dpapi.txt",
        ".ci-work/parity/config-dpapi.json",
    ] {
        fixture.write(name, b"synthetic-private-marker");
    }
    for name in INPUTS {
        fixture.write(
            &format!(".ci-work/parity/{name}"),
            "{\"fixture\":\"تصویر 日本語\"}\n".as_bytes(),
        );
    }
    fixture.write(
        ".ci-work/native/fixture-materialization.json",
        &serde_json::to_vec(&json!({"schema":1,"current_source":source,
            "historical_source":SOURCE,"baseline":BASELINE,
            "expectations_generated_by_target":false}))
        .unwrap(),
    );
    let output = fixture.task("evidence-capture", source, 0);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["verified"], true);
    let path = fixture.root.join(".ci-work/native/oracle-evidence.zip");
    let original = fs::read(&path).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&original)).unwrap();
    assert_eq!(archive.len(), 16);
    let mut manifest = Vec::new();
    archive
        .by_name("provenance.json")
        .unwrap()
        .read_to_end(&mut manifest)
        .unwrap();
    let manifest: Value = serde_json::from_slice(&manifest).unwrap();
    let expected = public
        .iter()
        .map(|(name, bytes)| ((*name).to_owned(), sha(bytes)))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        manifest["dependencies"],
        serde_json::to_value(expected).unwrap()
    );
    for name in INPUTS {
        let mut bytes = Vec::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(
            bytes,
            fs::read(fixture.root.join(format!(".ci-work/parity/{name}"))).unwrap()
        );
    }
    assert!(!manifest.to_string().contains("synthetic-private-marker"));
    fixture.task("evidence-capture", source, 1);
    assert_eq!(fs::read(&path).unwrap(), original);
    for name in [
        "schemas/fixture.json",
        "tooling/src/helper.rs",
        ".ci-work/parity/config.json",
    ] {
        let before = fs::read(fixture.root.join(name)).unwrap();
        fixture.write(name, b"{\"changed\":true}\n");
        fixture.task("evidence-verify", source, 1);
        assert_eq!(fs::read(&path).unwrap(), original);
        fixture.write(name, &before);
    }
    fixture.task("evidence-verify", &"b".repeat(40), 1);
    fixture.task("evidence-verify", source, 0);
    assert_eq!(fs::read(&path).unwrap(), original);
    fixture.write("docs/unconsumed-notes.md", b"changed unconsumed fixture\n");
    fixture.git(&["add", "--", "docs/unconsumed-notes.md"]);
    fixture.git(&["commit", "-m", "test: change isolated unconsumed notes"]);
    let changed = String::from_utf8(fixture.git(&["rev-parse", "HEAD"])).unwrap();
    assert_ne!(changed.trim(), source);
    let outcome = fixture.task("evidence-verify", source, 1);
    assert!(outcome.stdout.is_empty());
    assert!(
        String::from_utf8(outcome.stderr)
            .unwrap()
            .lines()
            .any(|line| line == "ERROR: Evidence checkout differs from hosted SHA.")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn actual_task_lifecycle_keeps_reports_separate_and_logs_success_failure_and_file_fallback() {
    let fixture = Fixture::new();
    let mut run_ids = std::collections::BTreeSet::new();
    for (case, operation, code) in [
        ("success", "coverage-gate", 0),
        ("failure", "synthetic-private-command", 1),
        ("file-fallback", "coverage-gate", 0),
    ] {
        let root = fixture.root.join(case);
        fs::create_dir_all(root.join("native")).unwrap();
        fs::create_dir_all(root.join(".ci-work/native")).unwrap();
        fs::write(root.join("Cargo.toml"), b"inert; never compiled").unwrap();
        fs::write(root.join("native/lib.rs"), b"inert; never compiled").unwrap();
        fs::write(
            root.join(".ci-work/native/coverage.json"),
            br#"{"data":[{"totals":{"regions":{"count":100,"covered":85},"branches":{"count":100,"covered":80}}}]}"#,
        )
        .unwrap();
        let logs = root.join(".ci-work/native/logs");
        if case == "file-fallback" {
            fs::write(&logs, b"unrelated blocking sentinel").unwrap();
        }
        let mut command = fixture.command(env!("CARGO_BIN_EXE_numera-task"));
        command
            .arg("--root")
            .arg(&root)
            .arg(operation)
            .env("GITHUB_ACTIONS", "true")
            .env("NUMERA_SIGNING_PASSWORD", "synthetic-private-password");
        let outcome = fixture.execute(&mut command);
        assert_eq!(outcome.code, code, "{case}");
        let stdout = std::str::from_utf8(&outcome.stdout).unwrap();
        let stderr = std::str::from_utf8(&outcome.stderr).unwrap();
        let events = stderr
            .lines()
            .filter(|line| line.starts_with('{'))
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(events.len(), if code == 0 { 2 } else { 3 }, "{case}");
        assert_eq!(events[0]["message"], "Maintenance operation started.");
        assert_eq!(events[0]["level"], "INFO");
        let last = events.last().unwrap();
        assert_eq!(last["message"], "Maintenance operation finished.");
        assert_eq!(last["exit_code"], code);
        assert!(last["duration_ms"].as_u64().is_some());
        let run_id = events[0]["run_id"].as_str().unwrap();
        uuid::Uuid::parse_str(run_id).unwrap();
        assert!(run_ids.insert(run_id.to_owned()));
        assert!(events.iter().all(|event| {
            event["component"] == "task"
                && event["run_id"] == run_id
                && event["timestamp"].as_str().unwrap().ends_with('Z')
        }));
        if code == 1 {
            let expected = "Unsupported task command; no fallback was executed.";
            assert_eq!(events[1]["level"], "ERROR");
            assert_eq!(events[1]["message"], expected);
            assert!(
                stderr
                    .lines()
                    .any(|line| line == format!("ERROR: {expected}"))
            );
            assert!(stdout.is_empty());
        } else {
            let reports = stdout
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(reports.len(), 2);
            assert_eq!(reports[0]["metric"], "regions");
            assert_eq!(reports[1]["metric"], "branches");
        }
        for private in [
            root.to_str().unwrap(),
            "synthetic-private-command",
            "synthetic-private-password",
        ] {
            assert!(
                !stderr.contains(private) && !stdout.contains(private),
                "{case}"
            );
        }
        if case == "file-fallback" {
            assert!(stderr.contains("WARNING: File logging unavailable; using stderr."));
            assert_eq!(fs::read(&logs).unwrap(), b"unrelated blocking sentinel");
        } else {
            let files = fs::read_dir(&logs)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            assert_eq!(files.len(), 1);
            assert!(
                files[0]
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .contains("_UTC-")
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&files[0]).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            let file_events = fs::read_to_string(&files[0])
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(file_events, events);
            fs::rename(&files[0], files[0].with_extension("closed")).unwrap();
        }
        assert!(!root.join(".ci-work/signing").exists());
        assert!(!root.join("target").exists());
    }
}
