use std::{fs, path::Path};

#[path = "../bootstrap/policy.rs"]
#[allow(
    dead_code,
    reason = "The same std-only policy is also compiled by the standalone hosted bootstrap."
)]
mod policy;
use policy as bootstrap_policy;

#[path = "../bootstrap/runtime.rs"]
#[allow(
    dead_code,
    reason = "Hosted bootstrap operations are not invoked by source contracts."
)]
mod bootstrap_runtime;

#[test]
fn bootstrap_policy_refuses_unknown_targets_flags_and_preparation_privilege_changes() {
    use bootstrap_policy::{flags, host_target};
    assert_eq!(
        host_target("linux", "x86_64").unwrap(),
        "x86_64-unknown-linux-gnu"
    );
    assert_eq!(
        host_target("macos", "aarch64").unwrap(),
        "aarch64-apple-darwin"
    );
    assert_eq!(
        host_target("windows", "aarch64").unwrap(),
        "aarch64-pc-windows-msvc"
    );
    assert!(host_target("linux", "x86").is_err());
    assert!(flags("true", "true", "native", "qualify").is_ok());
    assert!(flags("false", "false", "source-preparation", "source-preparation").is_ok());
    for (coverage, build, job, operation) in [
        ("TRUE", "true", "native", "qualify"),
        ("false", "0", "native", "qualify"),
        ("true", "false", "source-preparation", "source-preparation"),
        ("false", "false", "native", "qualify"),
        ("false", "false", "source-preparation", "sign-qualified"),
    ] {
        assert!(flags(coverage, build, job, operation).is_err());
    }
}

#[test]
fn preparation_path_and_receipt_inputs_refuse_unreviewed_changes_without_reimplementing_hashing() {
    use bootstrap_policy::{allowed_changes, checksum_output, json_string};
    assert!(allowed_changes(b"Cargo.lock\0tooling/src/example.rs\0").is_ok());
    for invalid in [
        b"README.md\0".as_slice(),
        b"../outside.rs\0",
        b"source.rs",
        b"Cargo.toml\0",
        b"tooling\\source.rs\0",
        b"tooling/source.rs\0tooling/source.rs\0",
        b"\xff.rs\0",
    ] {
        assert!(allowed_changes(invalid).is_err());
    }
    assert_eq!(json_string("branch/تصویر\n\""), "\"branch/تصویر\\n\\\"\"");
    let digest = "a".repeat(64);
    assert_eq!(
        checksum_output(format!("{digest}  patch\n").as_bytes()).unwrap(),
        digest
    );
    assert!(checksum_output(b"not a hash\n").is_err());
    assert!(checksum_output(format!("{}  patch\n", "A".repeat(64)).as_bytes()).is_err());
}

#[test]
fn initial_hosted_build_uses_the_qualified_native_owner_in_every_context() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let action =
        fs::read_to_string(root.join(".github/actions/native-bootstrap/action.yml")).unwrap();
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let driver = fs::read_to_string(root.join("tooling/bootstrap/main.rs")).unwrap();
    let policy = fs::read_to_string(root.join("tooling/bootstrap/policy.rs")).unwrap();
    let runtime = fs::read_to_string(root.join("tooling/bootstrap/runtime.rs")).unwrap();
    assert!(action.contains("using: composite"));
    for target in [
        "x86_64-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ] {
        assert!(action.contains(target));
        assert!(policy.contains(target));
    }
    for digest in [
        "0128674d9c1a0aad8eb111dc4e45e7def89119f22369089d73729905a315e658",
        "e132f203a88251f3fa4e22a2bdfd7a25261453bc95af55a39f056c364ac0f8e0",
        "51a089b26e3bf9cdbf0631a56ee4644d96babc3ff3f1e0c1dab8948c53bbd56d",
        "62a07c39d57a18afe9dd144c722300b966b14655141655c60d300b89f5e7ad66",
        "e2143157034ae30d2dbe6954e30fd76c8036c412a3b7407db83441e53b484d00",
        "cb26ee662e19fbc12f33209e988a55817fb7dcbe0d94a1836c3475b8dc55cdc7",
    ] {
        assert!(action.contains(digest));
    }
    for required in [
        "--max-time 90",
        "--connect-timeout 10",
        "--speed-time 30",
        "--speed-limit 1024",
        "--max-filesize 16777216",
        "--proto '=https'",
        "--proto-redir '=https'",
        "bounded 600000 180000",
        "bounded 900000",
        "--no-self-update",
        "tooling/bootstrap/main.rs",
        "--edition=2024",
        "--crate-type=bin",
        "numera-image-gen-mcp-0.2.0-",
        "System32/tar.exe",
        "NUMERA_BOOTSTRAP_DRIVER",
    ] {
        assert!(
            action.contains(required),
            "missing bootstrap boundary: {required}"
        );
    }
    assert!(driver.contains("1.99.1") || runtime.contains("1.99.1"));
    assert!(runtime.contains("nightly-2026-10-10"));
    assert!(runtime.contains("--locked"));
    assert!(policy.contains("GITHUB_REPOSITORY"));
    assert!(policy.contains("GITHUB_EVENT_NAME"));
    assert!(policy.contains("GITHUB_ACTIONS"));
    assert_eq!(
        workflow
            .matches("uses: ./.github/actions/native-bootstrap")
            .count(),
        7
    );
    assert!(!workflow.contains("uses: dtolnay/rust-toolchain"));
    assert!(workflow.contains("coverage: 'true'"));
    assert!(workflow.contains("name: native-${{ matrix.target }}"));
    assert!(workflow.contains("needs: [native, native-coverage, execution-matrix]"));
    assert!(workflow.contains("bounded 900000 60000 --exec ./target/debug/numera-task"));
    for forbidden in [
        "case ",
        "while ",
        "jq ",
        "python",
        "node ",
        "npx ",
        "continue-on-error",
        "curl |",
    ] {
        assert!(
            !action.contains(forbidden),
            "procedural bootstrap escaped Rust: {forbidden}"
        );
    }
    for forbidden in [
        "process_group(",
        "devtools::",
        "numera_task::",
        "serde",
        "reqwest",
        "sha2::",
        "ring::digest",
    ] {
        assert!(
            !driver.contains(forbidden) && !runtime.contains(forbidden),
            "bootstrap must remain std-only and inherit ownership: {forbidden}"
        );
    }
}

#[test]
fn source_preparation_cannot_build_qualify_or_receive_signing_inputs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let action =
        fs::read_to_string(root.join(".github/actions/native-bootstrap/action.yml")).unwrap();
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let source = fs::read_to_string(root.join("tooling/bootstrap/preparation.rs")).unwrap();
    let policy = fs::read_to_string(root.join("tooling/bootstrap/policy.rs")).unwrap();
    let prepare = workflow
        .split_once("  source-preparation:\n")
        .unwrap()
        .1
        .split_once("  native:\n")
        .unwrap()
        .0;
    assert!(action.contains("build-task:"));
    assert!(prepare.contains("build-task: 'false'"));
    assert!(prepare.contains("inputs.operation == 'source-preparation'"));
    for command in [
        "prepare-checkout",
        "prepare-lock",
        "prepare-format",
        "prepare-format-check",
        "prepare-package",
    ] {
        assert!(
            prepare.contains(command),
            "missing preparation stage: {command}"
        );
    }
    for required in [
        "source-preparation.patch",
        "receipt.json",
        "SHA256SUMS",
        "GITHUB_SHA",
        "GITHUB_RUN_ID",
        "GITHUB_RUN_ATTEMPT",
    ] {
        assert!(
            source.contains(required) || policy.contains(required),
            "missing Rust preparation boundary: {required}"
        );
    }
    for forbidden in [
        "secrets.",
        "environment:",
        "cargo build",
        "cargo test",
        "jq ",
        "while ",
        "case ",
        "continue-on-error",
    ] {
        assert!(
            !prepare.contains(forbidden),
            "preparation cannot interpret, qualify or sign: {forbidden}"
        );
    }
    assert!(prepare.contains("if-no-files-found: error"));
    assert!(prepare.contains("persist-credentials: false"));
    let signing = workflow.split_once("  signing:\n").unwrap().1;
    for required in [
        "!cancelled()",
        "github.ref == 'refs/heads/main'",
        "inputs.operation == 'qualify'",
        "inputs.operation == 'sign-qualified'",
        "needs.execution-matrix.result == 'success'",
        "needs.execution-matrix.result == 'skipped'",
    ] {
        assert!(signing.contains(required), "{required}");
    }
}

#[test]
fn standalone_bootstrap_formatting_is_aggregated_and_retained_only_after_evidence_checks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.0) {
                if std::thread::panicking() {
                    eprintln!(
                        "Bootstrap format fixture cleanup failed: {:?}",
                        error.kind()
                    );
                } else {
                    panic!(
                        "Bootstrap format fixture cleanup failed: {:?}",
                        error.kind()
                    );
                }
            }
        }
    }
    let fixture = Fixture(
        root.join(".ci-work/tooling-tests")
            .join(format!("bootstrap-format {}", uuid::Uuid::new_v4())),
    );
    fs::create_dir_all(&fixture.0).unwrap();
    let context = numera_task::context::Context {
        root: fixture.0.clone(),
        logger: numera_gen_mcp::logging::Logger::new(&fixture.0.join("logs"), "ERROR"),
    };
    for check in [false, true] {
        let command = numera_task::formatting::bootstrap_command(&context, check);
        assert_eq!(command.get_program(), "rustup");
        let expected = if check {
            vec![
                "run",
                "1.99.1",
                "rustfmt",
                "--check",
                "--edition",
                "2024",
                "tooling/bootstrap/main.rs",
            ]
        } else {
            vec![
                "run",
                "1.99.1",
                "rustfmt",
                "--edition",
                "2024",
                "tooling/bootstrap/main.rs",
            ]
        };
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            expected
                .iter()
                .map(std::ffi::OsStr::new)
                .collect::<Vec<_>>()
        );
    }
    let ci = fs::read_to_string(root.join("tooling/src/ci.rs")).unwrap();
    let stable = ci
        .split_once("pub fn stable(")
        .unwrap()
        .1
        .split_once("pub fn coverage(")
        .unwrap()
        .0;
    assert!(stable.contains("bootstrap_command(context, true)"));
    assert!(
        stable
            .split_whitespace()
            .collect::<String>()
            .contains("matches!(gate.as_str(),\"format\"|\"standalone-format\")")
    );
    assert!(stable.contains("crate::identity_tools::prepare(context)"));
    assert!(stable.contains("Err(_) => failures.push(\"identity-test-tools\".into())"));
    assert!(
        stable
            .find("crate::identity_tools::prepare(context)")
            .unwrap()
            < stable.find("&mut tests,").unwrap()
    );
    assert!(
        stable.find("crate::evidence::verify(context)").unwrap()
            < stable.find("crate::formatting::retain(context)").unwrap()
    );
    let retention = fs::read_to_string(root.join("tooling/src/formatting.rs")).unwrap();
    assert!(retention.contains("bootstrap_command(context, false)"));
    assert!(
        retention.find("bootstrap_command(context, false)").unwrap()
            < retention.find("let bytes = archive(").unwrap()
    );
    for gate in ["format", "standalone-format", "identity-test-tools"] {
        assert!(numera_task::ci::finish(vec![gate.into()]).is_err());
    }
}
