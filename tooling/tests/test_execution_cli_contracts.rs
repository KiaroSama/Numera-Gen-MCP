use numera_task::{
    context::Context,
    test_execution::{execute_harness, harness_command},
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            if std::thread::panicking() {
                eprintln!("Harness fixture cleanup failed: {:?}", error.kind());
            } else {
                panic!("Harness fixture cleanup failed: {:?}", error.kind());
            }
        }
    }
}

#[test]
fn actual_unfiltered_harness_keeps_original_command_identity_and_measures_phases() {
    const CHILD_ROOT: &str = "NUMERA_HARNESS_CONTRACT_CHILD_ROOT";
    const MODE: &str = "NUMERA_HARNESS_CONTRACT_MODE";
    const NAME: &str = "actual_unfiltered_harness_keeps_original_command_identity_and_measures_phases";
    if std::env::var(MODE).as_deref() == Ok("leaf") {
        let root = std::env::var_os(CHILD_ROOT).unwrap();
        assert_eq!(
            std::env::current_dir().unwrap().canonicalize().unwrap(),
            Path::new(&root).canonicalize().unwrap()
        );
        return;
    }
    let driver = std::env::var(MODE).as_deref() == Ok("driver");
    let fixture = if driver {
        None
    } else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../.ci-work/tooling-tests")
            .join(format!("harness 日本語 {}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("native")).unwrap();
        fs::write(root.join("native/lib.rs"), b"inert source identity\n").unwrap();
        Some(Fixture(root.canonicalize().unwrap()))
    };
    let root = fixture.as_ref().map_or_else(
        || PathBuf::from(std::env::var_os(CHILD_ROOT).unwrap()),
        |fixture| fixture.0.clone(),
    );
    let context = Context {
        logger: numera_gen_mcp::logging::Logger::new(&root.join("logs"), "ERROR"),
        root,
    };
    let executable = std::env::current_exe().unwrap();
    let command = || {
        let mut command =
            harness_command(&context, &Command::new("cargo"), "native/lib.rs", &executable);
        command.env(CHILD_ROOT, &context.root);
        command
    };
    if !driver {
        // Capture the driver's raw harness report, which must not enter this outer libtest stream.
        let output = context
            .capture(
                command()
                    .args(["--exact", NAME, "--nocapture"])
                    .env(MODE, "driver"),
                10000,
                5000,
                1024 * 1024,
            )
            .unwrap();
        assert!(String::from_utf8(output).unwrap().contains("HARNESS_CONTRACT_VERIFIED"));
        return;
    }
    let report = execute_harness(
        &context,
        || {
            let mut command = command();
            command.env(MODE, "leaf");
            command
        },
        "native/lib.rs",
        Instant::now() + Duration::from_secs(10),
        5000,
    )
    .unwrap();
    assert_eq!(report["passed"], json!([NAME]));
    assert_eq!(report["exit_code"], 0);
    assert_eq!(report["cleanup_verified"], true);
    let phases = report["phase_durations_us"].as_object().unwrap();
    assert_eq!(phases.len(), 4);
    let total = report["duration_us"].as_u64().unwrap();
    let mut sum = 0;
    for name in ["initial_hashes", "listing", "execution", "final_hashes"] {
        sum += phases[name].as_u64().unwrap();
    }
    assert!(sum <= total);
    assert!(total > 0);
    println!("HARNESS_CONTRACT_VERIFIED");
}
