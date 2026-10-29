use numera_task::signing::{codesign_failure, public_identity, validate_authority, validate_password};

#[test]
fn private_codesign_output_is_projected_to_fixed_codes_only() {
    use numera_task::context::diagnostic;
    for (input, expected) in [
        ("no identity found", "identity_not_found"),
        ("The specified item could not be found in the keychain", "identity_not_found"),
        ("errSecInternalComponent", "internal_component"),
        ("User interaction is not allowed", "interaction_required"),
        ("Interaction with the security server is not allowed", "interaction_required"),
        ("unable to build chain to self-signed root", "chain_unavailable"),
        ("resource fork, Finder information, or similar detritus not allowed", "extended_attributes"),
        ("unrecognized diagnostic", "rejected"),
    ] {
        let private = format!("/private/synthetic-path synthetic-passphrase: {input}");
        let error = codesign_failure(private.as_bytes());
        assert_eq!(diagnostic(&error), format!("macos_signing_{expected}"));
        assert!(!error.to_string().contains("synthetic"));
    }
    for reason in ["capability_unavailable", "allocation_failed", "native_operation_failed",
        "single_identity_required", "identity_not_found", "certificate_bound", "certificate_mismatch", "native_signature_failed"] {
        let private = format!("/private/synthetic-path {{\"code\": \"macos_signing_{reason}\"}}");
        assert_eq!(diagnostic(&codesign_failure(private.as_bytes())), format!("macos_signing_{reason}"));
    }
    assert_eq!(diagnostic(&codesign_failure(&[0xff, 0, 0x80])), "macos_signing_rejected");
}

#[test]
fn private_children_keep_only_selected_os_environment_and_capture_never_logs_private_output() {
    use numera_task::{context::Context, signing::private_command};
    use std::{fs, path::Path, process::Command};
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = project.join(".ci-work/tooling-tests").join(format!("signing capture {}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let result = std::panic::catch_unwind(|| {
        let context = Context { root: root.canonicalize().unwrap(),
            logger: numera_gen_mcp::logging::Logger::new(&root.join("logs"), "INFO") };
        let executable = std::env::current_exe().unwrap();
        let mut command = private_command(&context, &executable);
        let keys = command.get_envs().map(|(key, value)| {
            assert!(value.is_some()); key.to_str().unwrap().to_owned()
        }).collect::<Vec<_>>();
        assert!(keys.iter().all(|key| ["PATH", "SystemRoot", "WINDIR", "COMSPEC", "PATHEXT", "HOME", "USERPROFILE", "TMP", "TEMP", "TMPDIR"].contains(&key.as_str())));
        assert!(!keys.iter().any(|key| key.starts_with("NUMERA_SIGNING_") || ["GH_TOKEN", "GITHUB_TOKEN", "FAL_KEY"].contains(&key.as_str())));
        // The fixture switches are deliberately added after production environment selection.
        command.args(["--exact", "private_capture_worker", "--nocapture"])
            .env("NUMERA_PRIVATE_CAPTURE_FIXTURE", "normal");
        let output = context.capture(&mut command, 5000, 2000, 1024).unwrap();
        assert!(String::from_utf8(output).unwrap().contains("verified"));
        let mut failed: Command = private_command(&context, &executable);
        failed.args(["--exact", "private_capture_worker", "--nocapture"])
            .env("NUMERA_PRIVATE_CAPTURE_FIXTURE", "failure");
        assert!(context.capture(&mut failed, 5000, 2000, 1024).is_err());
        let mut excessive = private_command(&context, &executable);
        excessive.args(["--exact", "private_capture_worker", "--nocapture"])
            .env("NUMERA_PRIVATE_CAPTURE_FIXTURE", "excessive");
        assert!(context.capture(&mut excessive, 5000, 2000, 128).is_err());
        drop(context);
        let logs = fs::read_dir(root.join("logs")).unwrap().collect::<std::io::Result<Vec<_>>>().unwrap();
        assert!(!logs.is_empty(), "the real capture must leave a file log, not only stderr");
        for entry in logs {
            let text = fs::read_to_string(entry.path()).unwrap();
            assert!(!text.contains("synthetic-private-child-value"));
            assert!(!text.contains("synthetic-signing-key"));
            assert!(text.contains("cleanup_verified"));
        }
        let ledger = fs::read_to_string(root.join(".ci-work/processes.jsonl")).unwrap();
        let finished = ledger.lines().map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|row| row["event"] == "finish").collect::<Vec<_>>();
        assert_eq!(finished.len(), 3);
        assert!(finished.iter().all(|row| row["cleanup_verified"] == true));
        assert!(finished.iter().any(|row| row["exit_code"] == 7), "the failing private child must actually execute");
        assert!(ledger.lines().any(|line| {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            row["event"] == "terminate" && row["reason"] == "output_limit"
        }));
        assert!(!ledger.contains("synthetic-private-child-value"));
    });
    fs::remove_dir_all(&root).unwrap();
    result.unwrap();
}

#[test]
fn private_capture_worker() {
    use std::io::Write;
    let Ok(mode) = std::env::var("NUMERA_PRIVATE_CAPTURE_FIXTURE") else { return; };
    assert!(std::env::var_os("NUMERA_SIGNING_PFX_BASE64").is_none());
    assert!(std::env::var_os("NUMERA_SIGNING_PASSWORD").is_none());
    assert!(std::env::var_os("GITHUB_TOKEN").is_none());
    assert!(std::env::var_os("GH_TOKEN").is_none());
    std::io::stderr().write_all(b"synthetic-private-child-value synthetic-signing-key\n").unwrap();
    match mode.as_str() {
        "normal" => println!("verified"),
        "failure" => std::process::exit(7),
        "excessive" => std::io::stderr().write_all(&vec![b'x'; 4096]).unwrap(),
        _ => panic!("Unknown private capture fixture mode"),
    }
}

use serde_json::json;
#[test]
fn approved_signer_and_private_input_authority_fail_closed() {
    public_identity().expect("approved source identity");
    for input in [vec![],vec![b'x';4096],b"private\nvalue".to_vec(),b"bad value".to_vec()] {assert!(validate_password(&input).is_err());}
    validate_password(b"synthetic+/=").unwrap();
    let valid=json!({"GITHUB_ACTIONS":"true","GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP",
        "GITHUB_REF":"refs/heads/main","GITHUB_EVENT_NAME":"workflow_dispatch",
        "GITHUB_JOB":"signing","NUMERA_CI_OPERATION":"qualify"});
    validate_authority(&valid).unwrap();
    for field in ["GITHUB_ACTIONS","GITHUB_REPOSITORY","GITHUB_REF","GITHUB_EVENT_NAME","GITHUB_JOB","NUMERA_CI_OPERATION"]{
        let mut changed=valid.clone();changed[field]=json!("unrelated");assert!(validate_authority(&changed).is_err(),"{field}");
        changed.as_object_mut().unwrap().remove(field);assert!(validate_authority(&changed).is_err(),"missing {field}");
    }
    for reference in ["refs/heads/feature/native-rust-distribution", "refs/heads/chore/tooling-media", "refs/tags/v0.3.0", "refs/pull/22/merge"] {
        let mut changed=valid.clone();changed["GITHUB_REF"]=json!(reference);assert!(validate_authority(&changed).is_err());
    }
    let mut preparation=valid.clone();preparation["NUMERA_CI_OPERATION"]=json!("source-preparation");
    assert!(validate_authority(&preparation).is_err());
}
