#![cfg(target_os = "linux")]

use numera_gen_mcp::{
    devtools::{
        linux_snapshot,
        supervisor::{Bounds, run_with_observation},
    },
    logging::Logger,
};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.ci-work/tooling-tests")
            .join(format!("coverage observation 東京 {}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn fixture_command(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "coverage_observation_fixture", "--nocapture"])
        .env("NUMERA_OBSERVATION_FIXTURE", mode);
    command
}

#[test]
fn coverage_observation_fixture() {
    let Ok(mode) = std::env::var("NUMERA_OBSERVATION_FIXTURE") else {
        return;
    };
    match mode.as_str() {
        "owner" => {
            let root = PathBuf::from(std::env::var_os("NUMERA_OBSERVATION_ROOT").unwrap());
            let helper_mode = std::env::var("NUMERA_OBSERVATION_HELPER").unwrap();
            let mut helper = if helper_mode == "real" {
                let mut command = Command::new(env!("CARGO_BIN_EXE_numera-task"));
                command.arg("--coverage-sampler");
                command
            } else if helper_mode == "missing" {
                Command::new(root.join("missing-observer"))
            } else {
                fixture_command(&helper_mode)
            };
            let mut report = fixture_command(
                &std::env::var("NUMERA_OBSERVATION_REPORT").unwrap_or_else(|_| "report".into()),
            );
            let code = linux_snapshot::run_owner(
                &mut report,
                &mut helper,
                &root.join("observation.json"),
                "0123456789abcdef0123456789abcdef",
            )
            .unwrap();
            std::process::exit(code);
        }
        "blocked" => {
            let until = Instant::now() + Duration::from_secs(6);
            while Instant::now() < until {
                std::thread::park_timeout(Duration::from_millis(10));
            }
        }
        "malformed" => {
            print!("private-marker /private/path not-json");
        }
        "exit" => std::process::exit(37),
        "output" => {
            use std::io::Write;
            std::io::stdout()
                .write_all(&vec![b'x'; 3 * 1024 * 1024])
                .unwrap();
        }
        "report" => {
            println!("actual-report-progress");
            let until = Instant::now() + Duration::from_secs(6);
            while Instant::now() < until {
                std::thread::park_timeout(Duration::from_millis(10));
            }
        }
        "sampler" => linux_snapshot::sample_from_stdio().unwrap(),
        _ => panic!("Unsupported observation fixture."),
    }
}

#[test]
fn report_observation_never_becomes_output_progress_or_weakens_idle_cleanup() {
    for helper in ["real", "blocked", "malformed"] {
        let fixture = Fixture::new();
        let mut command = fixture_command("owner");
        command
            .env("NUMERA_OBSERVATION_ROOT", &fixture.0)
            .env("NUMERA_OBSERVATION_HELPER", helper)
            .current_dir(&fixture.0);
        let logger = Logger::new(&fixture.0.join("logs"), "ERROR");
        let result = run_with_observation(
            &mut command,
            &fixture.0,
            Bounds::new(5000, 1800).unwrap(),
            &AtomicBool::new(false),
            &logger,
        )
        .unwrap();
        assert_eq!(result.reason.as_deref(), Some("idle"));
        assert_eq!(result.code, 124);
        assert!(result.cleanup_verified);
        assert!(
            String::from_utf8(result.stdout)
                .unwrap()
                .contains("actual-report-progress")
        );
        assert!(
            !String::from_utf8(result.stderr)
                .unwrap()
                .contains("private-marker")
        );
        let receipt = numera_task::coverage_diagnostics::read_receipt(
            &fixture.0.join("observation.json"),
            "0123456789abcdef0123456789abcdef",
        );
        assert_eq!(result.observation.as_ref().unwrap().signal_status, 1);
        if helper == "real" {
            let receipt = receipt.unwrap();
            assert_eq!(receipt.owner_pid, receipt.group as u32);
            assert_ne!(receipt.report_pid, receipt.owner_pid);
            assert!(receipt.signal_ms.is_some());
            assert_eq!(receipt.status, 0, "The real sampler must produce a valid numeric pair.");
            let snapshot = receipt.snapshot.unwrap();
            assert!(!snapshot.first.is_empty() && !snapshot.second.is_empty());
            assert!(snapshot.first.iter().any(|row| row.pid == receipt.report_pid));
            assert!(snapshot.second.iter().any(|row| row.pid == receipt.report_pid));
        } else if let Ok(receipt) = receipt {
            assert_ne!(receipt.status, 0, "bad helper cannot produce a successful observation");
        }
    }
}

#[test]
fn observation_opt_in_preserves_exit_wall_cancellation_and_original_output_limit() {
    for (mode, wall, idle, cancel, reason, code) in [
        ("exit", 5000, 1800, false, None, 37),
        ("report", 1800, 5000, false, Some("wall"), 124),
        ("report", 5000, 1800, true, Some("cancelled"), 124),
        ("output", 5000, 1800, false, Some("output_limit"), 124),
    ] {
        let fixture = Fixture::new();
        let mut command = fixture_command("owner");
        command
            .env("NUMERA_OBSERVATION_ROOT", &fixture.0)
            .env("NUMERA_OBSERVATION_HELPER", "blocked")
            .env("NUMERA_OBSERVATION_REPORT", mode)
            .current_dir(&fixture.0);
        let logger = Logger::new(&fixture.0.join("logs"), "ERROR");
        let outcome = run_with_observation(
            &mut command,
            &fixture.0,
            Bounds::new(wall, idle).unwrap(),
            &AtomicBool::new(cancel),
            &logger,
        )
        .unwrap();
        assert_eq!(outcome.code, code);
        assert_eq!(outcome.reason.as_deref(), reason);
        assert!(outcome.cleanup_verified);
        if mode == "output" {
            assert_eq!(outcome.stdout.len(), 2 * 1024 * 1024);
        }
    }
}

#[test]
fn observer_startup_failure_preserves_report_exit_and_local_observer_cli_stays_hosted_only() {
    for helper in ["missing", "malformed", "blocked"] {
        let fixture = Fixture::new();
        let mut command = fixture_command("owner");
        command
            .env("NUMERA_OBSERVATION_ROOT", &fixture.0)
            .env("NUMERA_OBSERVATION_HELPER", helper)
            .env("NUMERA_OBSERVATION_REPORT", "exit")
            .current_dir(&fixture.0);
        let logger = Logger::new(&fixture.0.join("logs"), "ERROR");
        let outcome = run_with_observation(
            &mut command,
            &fixture.0,
            Bounds::new(5000, 1800).unwrap(),
            &AtomicBool::new(false),
            &logger,
        )
        .unwrap();
        assert_eq!(outcome.code, 37);
        assert!(outcome.reason.is_none() && outcome.cleanup_verified);
    }
    let fixture = Fixture::new();
    let mut command = Command::new(env!("CARGO_BIN_EXE_numera-task"));
    command
        .arg("--coverage-observer")
        .env_remove("GITHUB_ACTIONS")
        .current_dir(&fixture.0);
    let logger = Logger::new(&fixture.0.join("logs"), "ERROR");
    let outcome = run_with_observation(
        &mut command,
        &fixture.0,
        Bounds::new(5000, 1800).unwrap(),
        &AtomicBool::new(false),
        &logger,
    )
    .unwrap();
    assert_eq!(outcome.code, 1);
    assert!(outcome.reason.is_none() && outcome.cleanup_verified);
    assert!(String::from_utf8(outcome.stderr).unwrap().contains("hosted CI only"));
    assert!(!fixture.0.join("target").exists());
}

#[test]
fn numeric_observation_receipts_reject_partial_unknown_fields_stale_ids_and_links() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let path = fixture.0.join("observation.json");
    for bytes in [b"{".as_slice(), b"{\"nonce\":\"wrong\",\"private\":\"marker\"}", &[0xff]] {
        fs::write(&path, bytes).unwrap();
        assert!(
            numera_task::coverage_diagnostics::read_receipt(
                &path,
                "0123456789abcdef0123456789abcdef"
            )
            .is_err()
        );
    }
    fs::remove_file(&path).unwrap();
    let original = fixture.0.join("original");
    fs::write(&original, b"{}").unwrap();
    symlink(&original, &path).unwrap();
    assert!(
        numera_task::coverage_diagnostics::read_receipt(&path, "0123456789abcdef0123456789abcdef")
            .is_err()
    );
    fs::remove_file(&path).unwrap();
    fs::hard_link(&original, &path).unwrap();
    assert!(
        numera_task::coverage_diagnostics::read_receipt(&path, "0123456789abcdef0123456789abcdef")
            .is_err()
    );
}
