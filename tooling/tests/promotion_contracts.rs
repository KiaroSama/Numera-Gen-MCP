use numera_task::{context::sha, promotion, test_execution::execution_identity};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn identity() -> Value {
    execution_identity(&json!({
        "GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP", "GITHUB_EVENT_NAME":"workflow_dispatch",
        "GITHUB_SHA":"a".repeat(40), "GITHUB_REF":"refs/heads/chore/tooling-media",
        "GITHUB_JOB":"native", "GITHUB_RUN_ID":"7", "GITHUB_RUN_ATTEMPT":"2"
    }), "built", "x86_64-unknown-linux-gnu").unwrap()
}

#[test]
fn promotion_accepts_only_the_exact_successful_original_attempt_workflow_and_source() {
    let run = json!({"id":7,"run_attempt":2,"head_sha":"a".repeat(40),
        "head_branch":"chore/tooling-media","event":"workflow_dispatch","status":"completed",
        "conclusion":"success","workflow_id":12,"path":".github/workflows/ci.yml",
        "repository":{"id":1400814502,"full_name":"KiaroSama/Numera-Gen-MCP"},
        "head_repository":{"id":1400814502}});
    promotion::validate_run(&run, &identity(), 12, true).unwrap();
    for (field, value) in [
        ("id", json!(8)), ("run_attempt", json!(1)), ("head_sha", json!("b".repeat(40))),
        ("head_branch", json!("main")), ("event", json!("push")),
        ("status", json!("in_progress")), ("conclusion", json!("failure")),
        ("workflow_id", json!(13)), ("path", json!(".github/workflows/other.yml")),
        ("repository", json!({"id":2})), ("head_repository", json!({"id":2})),
    ] {
        let mut changed = run.clone();
        changed[field] = value;
        assert!(promotion::validate_run(&changed, &identity(), 12, true).is_err(), "{field}");
    }
    for field in ["id", "run_attempt", "head_sha", "head_branch", "workflow_id", "path", "repository", "head_repository"] {
        let mut changed = run.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(promotion::validate_run(&changed, &identity(), 12, true).is_err(), "missing {field}");
    }
    let mut pending = run.clone();
    pending["status"] = json!("in_progress");
    pending["conclusion"] = Value::Null;
    promotion::validate_run(&pending, &identity(), 12, false).unwrap();
}

#[test]
fn promoted_matrix_preserves_all_seven_original_identities_and_false_qualification_boundary() {
    let identity = identity();
    let mut contexts = BTreeMap::new();
    for (label, (target, phase)) in numera_task::matrix::contexts() {
        let mut current = identity.clone();
        current["target"] = json!(target);
        current["phase"] = json!(phase);
        current["job"] = json!(if phase == "coverage" { "native-coverage" } else { "native" });
        current["platform"] = json!(if target.contains("windows") { "win32" }
            else if target.contains("apple") { "darwin" } else { "linux" });
        contexts.insert(label, json!({"reviewed_named_execution":{"identity":current},
            "tooling_harness_execution":{"sdk_tested_binary_sha256":sha(b"original binary")}}));
    }
    let matrix = json!({"schema_version":1,"component":"native-seven-context-execution",
        "source_sha":"a".repeat(40),"run_id":7,"run_attempt":2,"cases":470,"suites":50,
        "contexts":contexts,"current_named_execution_verified":true,
        "current_assertion_review_verified":true,"semantic_parity_claimed":false,
        "qualification_verified":false,"original_evidence_rewritten":false});
    promotion::verify_execution(&matrix, &identity).unwrap();
    for mutation in ["missing", "extra", "attempt", "source", "semantic", "qualified", "identity", "binary"] {
        let mut changed = matrix.clone();
        match mutation {
            "missing" => { changed["contexts"].as_object_mut().unwrap().remove("coverage"); }
            "extra" => changed["contexts"]["other"] = json!({}),
            "attempt" => changed["run_attempt"] = json!(3),
            "source" => changed["source_sha"] = json!("b".repeat(40)),
            "semantic" => changed["semantic_parity_claimed"] = json!(true),
            "qualified" => changed["qualification_verified"] = json!(true),
            "identity" => changed["contexts"]["coverage"]["reviewed_named_execution"]["identity"]["run_id"] = json!(8),
            "binary" => changed["contexts"]["aarch64-pc-windows-msvc"]["tooling_harness_execution"]["sdk_tested_binary_sha256"] = Value::Null,
            _ => unreachable!(),
        }
        assert!(promotion::verify_execution(&changed, &identity).is_err(), "{mutation}");
    }
}

#[test]
fn original_intermediate_metadata_is_bound_to_the_successful_producing_job_attempt() {
    let identity = identity();
    let job = json!({"id":11,"name":"native-x86_64-unknown-linux-gnu","run_id":7,
        "run_attempt":2,"head_sha":"a".repeat(40),"head_branch":"chore/tooling-media",
        "status":"completed","conclusion":"success",
        "started_at":"2026-11-02T10:00:00Z","completed_at":"2026-11-02T10:10:00Z"});
    let artifact = json!({"id":21,"name":"native-unsigned-intermediate-x86_64-unknown-linux-gnu",
        "expired":false,"size_in_bytes":100,"digest":format!("sha256:{}", sha(b"outer zip")),
        "created_at":"2026-11-02T10:09:00Z","workflow_run":{"id":7,
            "repository_id":1400814502,"head_repository_id":1400814502,
            "head_sha":"a".repeat(40),"head_branch":"chore/tooling-media"}});
    promotion::verify_intermediate(&artifact, &job, &identity).unwrap();
    for (field, value) in [
        ("id", json!(0)), ("name", json!("unrelated")), ("expired", json!(true)),
        ("size_in_bytes", json!(0)), ("digest", json!("sha256:bad")),
        ("created_at", json!("2026-11-02T09:59:59Z")),
        ("created_at", json!("2026-11-02T10:10:01Z")),
        ("workflow_run", json!({"id":8})),
    ] {
        let mut changed = artifact.clone();
        changed[field] = value;
        assert!(promotion::verify_intermediate(&changed, &job, &identity).is_err(), "{field}");
    }
    let mut changed = job.clone();
    changed["run_attempt"] = json!(1);
    assert!(promotion::verify_intermediate(&artifact, &changed, &identity).is_err());
    changed = job.clone();
    changed["conclusion"] = json!("failure");
    assert!(promotion::verify_intermediate(&artifact, &changed, &identity).is_err());
}

#[test]
fn intermediate_archive_digest_comes_from_one_original_successful_package_log() {
    let name = "unsigned-intermediate-numera-gen-mcp-0.3.0-x86_64-unknown-linux-gnu.tar.gz";
    let row = json!({"component":"native-package","archive":name,"sha256":sha(b"archive"),
        "internal_intermediate":true});
    let log = format!("2026-11-02T10:08:00Z {row}\n");
    assert_eq!(promotion::package_digest(log.as_bytes(), name).unwrap(), sha(b"archive"));
    for changed in [String::new(), format!("{log}{log}"),
        log.replace("true", "false"), log.replace(&sha(b"archive"), "bad"),
        log.replace("2026-11-02T10:08:00Z", "untrusted-prefix"), log.replace(name, "other")]
    {
        assert!(promotion::package_digest(changed.as_bytes(), name).is_err());
    }
}

#[test]
fn actual_promotion_stage_and_sign_refuse_changed_original_bytes_before_private_import() {
    use numera_gen_mcp::devtools::supervisor::{Bounds, run};
    use numera_task::{archives, context::Context, distribution};
    use std::{fs, path::Path, process::Command, sync::atomic::AtomicBool};

    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.ci-work/tooling-tests")
        .join(format!("promotion 日本語 {}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&parent).unwrap();
    let result = std::panic::catch_unwind(|| {
        let owner = Context { root:parent.canonicalize().unwrap(),
            logger:numera_gen_mcp::logging::Logger::new(&parent.join("logs"), "ERROR") };
        let target = numera_gen_mcp::devtools::artifact::target();
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let prefix = distribution::prefix(target, true).unwrap();
        let name = format!("{prefix}.{}", if cfg!(windows) { "zip" } else { "tar.gz" });
        let mut environment = json!({"GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP",
            "GITHUB_EVENT_NAME":"workflow_dispatch","GITHUB_SHA":"a".repeat(40),
            "GITHUB_REF":"refs/heads/main","GITHUB_JOB":"signing",
            "GITHUB_RUN_ID":"9","GITHUB_RUN_ATTEMPT":"1"});
        let identity = execution_identity(&environment, "extracted-signed", target).unwrap();
        environment["GITHUB_ACTIONS"] = json!("true");
        environment["NUMERA_CI_OPERATION"] = json!("sign-qualified");
        for case in ["valid", "archive", "binary", "identity", "missing", "tree", "filename"] {
            let root = parent.join(case);
            let download = root.join(".ci-work/signing-download");
            fs::create_dir_all(&download).unwrap();
            fs::create_dir(root.join("native")).unwrap();
            fs::write(root.join("Cargo.toml"), b"inert fixture; never compiled").unwrap();
            fs::write(root.join("native/lib.rs"), b"inert fixture; never compiled").unwrap();
            let mut entries = BTreeMap::from([
                (format!("bin/numera-gen-mcp{suffix}"), b"inert original binary".to_vec()),
                (format!("bin/numera-dev{suffix}"), b"inert original developer".to_vec()),
                ("docs/assets/logo.png".into(), numera_gen_mcp::build_identity::LOGO.to_vec()),
            ]);
            let manifest = json!({"version":"0.3.0","target":target,"source_sha":"a".repeat(40),
                "publisher":"KiaroSama","binary_sha256":sha(b"inert original binary"),
                "files":entries.iter().map(|(path,bytes)|json!({"path":path,
                    "bytes":bytes.len(),"sha256":sha(bytes)})).collect::<Vec<_>>()});
            entries.insert("manifest.json".into(), serde_json::to_vec(&manifest).unwrap());
            let bytes = archives::encode(&prefix, &entries, cfg!(windows)).unwrap();
            fs::write(download.join(&name), &bytes).unwrap();
            fs::write(download.join(format!("{name}.sha256")), format!("{}  {name}\n", sha(&bytes))).unwrap();
            let mut receipt = json!({"schema_version":1,"component":"native-authenticated-signing-source",
                "identity":identity,"archive":name,"archive_sha256":sha(&bytes),
                "unsigned_binary_sha256":sha(b"inert original binary"),
                "qualification_verified":false,"retirement_authorized":false});
            match case {
                "archive" => receipt["archive_sha256"] = json!("0".repeat(64)),
                "binary" => receipt["unsigned_binary_sha256"] = json!("0".repeat(64)),
                "identity" => receipt["identity"]["run_attempt"] = json!(2),
                _ => (),
            }
            if case != "missing" {
                fs::create_dir_all(root.join(".ci-work/native")).unwrap();
                fs::write(root.join(promotion::RECEIPT), serde_json::to_vec(&receipt).unwrap()).unwrap();
            }
            let invoke = |operation: &str| {
                let mut command = Command::new(env!("CARGO_BIN_EXE_numera-task"));
                command.env_clear().args(["--root"]).arg(root.canonicalize().unwrap()).args([operation,target]);
                for key in ["PATH", "SystemRoot", "WINDIR", "LLVM_PROFILE_FILE"] {
                    if let Some(value) = std::env::var_os(key) { command.env(key,value); }
                }
                for (key,value) in environment.as_object().unwrap() { command.env(key,value.as_str().unwrap()); }
                command.env("NUMERA_SIGNING_PFX_BASE64", "synthetic-invalid-identity")
                    .env("NUMERA_SIGNING_PASSWORD", "synthetic-private-password");
                let outcome = run(&mut command,&owner.root,Bounds::new(5000,2000).unwrap(),
                    &AtomicBool::new(false),&owner.logger).unwrap();
                assert!(outcome.reason.is_none(), "{case}/{operation}");
                assert!(outcome.cleanup_verified, "{case}/{operation}");
                for private in ["synthetic-invalid-identity", "synthetic-private-password"] {
                    assert!(!String::from_utf8_lossy(&outcome.stdout).contains(private));
                    assert!(!String::from_utf8_lossy(&outcome.stderr).contains(private));
                }
                outcome
            };
            let staged = invoke("stage");
            let prepared = root.join(format!(".ci-work/prepared/{target}"));
            if matches!(case,"valid" | "tree" | "filename") {
                assert_eq!(staged.code,0,"{case}");
                assert_eq!(distribution::read_tree(&prepared,target,&"a".repeat(40)).unwrap(),entries);
                if case == "tree" {
                    let mut changed = manifest.clone();
                    fs::write(prepared.join(format!("bin/numera-dev{suffix}")), b"changed developer").unwrap();
                    for row in changed["files"].as_array_mut().unwrap() {
                        if row["path"] == format!("bin/numera-dev{suffix}") {
                            row["bytes"] = json!(b"changed developer".len());
                            row["sha256"] = json!(sha(b"changed developer"));
                        }
                    }
                    fs::write(prepared.join("manifest.json"),serde_json::to_vec(&changed).unwrap()).unwrap();
                    distribution::read_tree(&prepared,target,&"a".repeat(40)).unwrap();
                }
                if case == "filename" {
                    receipt["archive"] = json!("../unrelated.tar.gz");
                    fs::write(root.join(promotion::RECEIPT), serde_json::to_vec(&receipt).unwrap()).unwrap();
                }
                let signed = invoke("sign");
                assert_eq!(signed.code,1);
                let expected = match case {
                    "valid" => "Malformed selected identity.",
                    "filename" => "Original signing archive filename differs.",
                    _ => "Prepared signing tree is not the original executed intermediate.",
                };
                assert_eq!(std::str::from_utf8(&signed.stderr).unwrap().trim(),format!("ERROR: {expected}"));
            } else {
                assert_eq!(staged.code,1,"{case}");
                assert!(!prepared.exists(),"{case} wrote before original authentication");
            }
            assert_eq!(fs::read(download.join(&name)).unwrap(),bytes);
            assert!(!root.join(".ci-work/signing").exists());
            assert!(!root.join(".ci-work/release").exists());
            assert!(!root.join("target").exists());
        }
    });
    fs::remove_dir_all(&parent).unwrap();
    result.unwrap();
}

#[test]
fn signing_only_workflow_selects_one_original_artifact_without_rerunning_native_suites() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let workflow = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    assert!(workflow.contains("- sign-qualified"));
    for name in ["qualification-run-id:", "qualification-run-attempt:"] {
        assert!(workflow.contains(name));
    }
    let signing = workflow.split_once("  signing:\n").unwrap().1
        .split_once("  signed-execution-matrix:\n").unwrap().0;
    for required in ["!cancelled()", "github.ref == 'refs/heads/main'",
        "needs.execution-matrix.result == 'success'", "needs.execution-matrix.result == 'skipped'",
        "signing-source", "artifact-ids: ${{ steps.source.outputs.artifact-id }}",
        "run-id: ${{ steps.source.outputs.run-id }}", "github-token: ${{ github.token }}",
        "repository: KiaroSama/Numera-Gen-MCP", "digest-mismatch: error"]
    {
        assert!(signing.contains(required), "{required}");
    }
    for forbidden in [" stable", " coverage", "case-execution", "continue-on-error"] {
        assert!(!signing.contains(forbidden), "{forbidden}");
    }
    assert!(signing.find("signing-source").unwrap() < signing.find("actions/download-artifact@").unwrap());
    assert!(signing.find("signing-source").unwrap() < signing.find("secrets.NUMERA_SIGNING").unwrap());
    let source = std::fs::read_to_string(root.join("tooling/src/signing.rs")).unwrap();
    let sign = source.split_once("pub fn sign(").unwrap().1;
    assert!(sign.find("promotion::verify_prepared").unwrap() < sign.find("NUMERA_SIGNING_PFX_BASE64").unwrap());
}
