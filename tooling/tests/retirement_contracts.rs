use numera_task::{
    context::sha, inventory, matrix, retirement, test_execution::execution_identity,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
    path::Path,
};

fn manifest() -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    serde_json::from_slice(
        &std::fs::read(root.join("docs/testing/tooling-migration.json")).unwrap(),
    )
    .unwrap()
}

fn trees(manifest: &Value) -> (retirement::Tree, retirement::Tree) {
    let mut original = BTreeMap::from([
        (
            "Cargo.toml".into(),
            format!("100644 blob {}", "1".repeat(40)),
        ),
        (
            "native/main.rs".into(),
            format!("100644 blob {}", "2".repeat(40)),
        ),
    ]);
    for row in manifest["entries"].as_array().unwrap() {
        original.insert(
            row["source"].as_str().unwrap().into(),
            format!("100644 blob {}", "3".repeat(40)),
        );
    }
    let mut retired = original.clone();
    for row in manifest["entries"].as_array().unwrap() {
        retired.remove(row["source"].as_str().unwrap());
    }
    (original, retired)
}

#[test]
fn retirement_requires_one_qualified_parent_and_exactly_the_original_51_deletions() {
    let manifest = manifest();
    let (original, retired) = trees(&manifest);
    let q = "a".repeat(40);
    let r = "b".repeat(40);
    retirement::verify_transition(&manifest, &q, &r, &[q.clone()], &original, &retired).unwrap();
    for mutation in [
        "parent",
        "merge",
        "same-sha",
        "missing-original",
        "partial",
        "addition",
        "edit",
        "mode",
        "powershell",
        "extra-deletion",
        "empty-current-rust",
    ] {
        let (mut original, mut retired) = trees(&manifest);
        let mut parents = vec![q.clone()];
        let mut current = r.clone();
        let first = manifest["entries"][0]["source"].as_str().unwrap();
        match mutation {
            "parent" => parents[0] = "c".repeat(40),
            "merge" => parents.push("c".repeat(40)),
            "same-sha" => current = q.clone(),
            "missing-original" => {
                original.remove(first);
            }
            "partial" => {
                retired.insert(first.into(), original[first].clone());
            }
            "addition" => {
                retired.insert(
                    "tooling/src/unrequested.rs".into(),
                    format!("100644 blob {}", "4".repeat(40)),
                );
            }
            "edit" => {
                retired.insert(
                    "native/main.rs".into(),
                    format!("100644 blob {}", "4".repeat(40)),
                );
            }
            "mode" => {
                retired.insert(
                    "native/main.rs".into(),
                    format!("100755 blob {}", "2".repeat(40)),
                );
            }
            "powershell" => {
                let path = manifest["entries"].as_array().unwrap().iter()
                    .find_map(|row| row["source"].as_str().filter(|path| path.ends_with(".ps1")))
                    .unwrap();
                retired.insert(path.into(), original[path].clone());
            }
            "extra-deletion" => {
                retired.remove("Cargo.toml");
            }
            "empty-current-rust" => {
                original.remove("native/main.rs");
                retired.remove("native/main.rs");
            }
            _ => unreachable!(),
        }
        assert!(
            retirement::verify_transition(&manifest, &q, &current, &parents, &original, &retired)
                .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn retired_inventory_preserves_historical_blobs_and_refuses_a_partial_working_copy() {
    let manifest = manifest();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let directory = root
        .join(".ci-work/tooling-tests")
        .join(format!("retired-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let result = std::panic::catch_unwind(|| {
        let context = numera_task::context::Context {
            root: root.canonicalize().unwrap(),
            logger: numera_gen_mcp::logging::Logger::new(&directory.join("logs"), "ERROR"),
        };
        let originals = numera_task::ledger_inputs::tooling_originals(&context, &manifest).unwrap();
        assert_eq!(originals.len(), 51);
        for row in manifest["entries"].as_array().unwrap() {
            for field in ["rust", "tests"] {
                for path in row[field].as_array().unwrap() {
                    let destination = directory.join(path.as_str().unwrap());
                    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
                    std::fs::write(destination, b"inert source fixture\n").unwrap();
                }
            }
        }
        inventory::tooling_manifest(&directory, &manifest, false).unwrap();
        assert!(
            inventory::tooling_manifest(&directory, &manifest, true).is_err(),
            "source shape grants no acceptance"
        );
        let path = manifest["entries"][0]["source"].as_str().unwrap();
        std::fs::create_dir_all(directory.join(path).parent().unwrap()).unwrap();
        std::fs::write(directory.join(path), &originals[path]).unwrap();
        assert!(inventory::tooling_manifest(&directory, &manifest, false).is_err());
        for (path, bytes) in &originals {
            let destination = directory.join(path);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::write(destination, bytes).unwrap();
        }
        inventory::tooling_manifest(&directory, &manifest, false).unwrap();
        let path = originals.keys().find(|path| path.ends_with(".ps1")).unwrap();
        std::fs::write(directory.join(path), b"changed original").unwrap();
        assert!(inventory::tooling_manifest(&directory, &manifest, false).is_err());
    });
    std::fs::remove_dir_all(&directory).unwrap();
    result.unwrap();
}

#[test]
fn git_tree_parser_preserves_exact_modes_objects_and_rejects_ambiguous_members() {
    let bytes = format!(
        "100644 blob {}\ttooling/src/example.rs\0100755 blob {}\tscripts/example.ps1\0",
        "1".repeat(40),
        "2".repeat(40)
    );
    let tree = retirement::parse_tree(bytes.as_bytes()).unwrap();
    assert_eq!(tree.len(), 2);
    assert_eq!(
        tree["scripts/example.ps1"],
        format!("100755 blob {}", "2".repeat(40))
    );
    for invalid in [
        bytes.trim_end_matches('\0').to_owned(),
        format!(
            "{bytes}100644 blob {}\ttooling/src/example.rs\0",
            "3".repeat(40)
        ),
        format!("100644 blob {}\t../outside\0", "1".repeat(40)),
        format!("100644 blob {}\tmissing-object\0", "x".repeat(40)),
        format!("100644 tree {}\twrong-kind\0", "1".repeat(40)),
    ] {
        assert!(retirement::parse_tree(invalid.as_bytes()).is_err());
    }
}

fn qualified(source: &str, run: u64) -> (Value, Value, Vec<u8>, Value, Vec<u8>) {
    // Synthetic authenticated-boundary shapes test rejection; these are not hosted proof.
    let env = json!({"GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP", "GITHUB_EVENT_NAME":"workflow_dispatch",
        "GITHUB_SHA":source,"GITHUB_REF":"refs/heads/main","GITHUB_JOB":"signing",
        "GITHUB_RUN_ID":run.to_string(),"GITHUB_RUN_ATTEMPT":"1"});
    let identity =
        execution_identity(&env, "extracted-signed", "x86_64-unknown-linux-gnu").unwrap();
    let signed = matrix::contexts().keys().filter(|target| **target != "coverage").map(|target| {
        let identity = execution_identity(&env, "extracted-signed", target).unwrap();
        ((*target).to_owned(), json!({"schema_version":1,"component":"native-signed-stage-execution","identity":identity,
            "signed_stage_execution_verified":true,"semantic_parity_claimed":false,"qualification_verified":false,"retirement_authorized":false}))
    }).collect::<BTreeMap<_,_>>();
    let qualification = json!({"schema_version":1,"component":"native-current-qualification","source_sha":source,
        "qualification_run_id":run-1,"qualification_run_attempt":1,"signing_identity":identity,
        "source_review_sha256":"1".repeat(64),"tooling_source_review_sha256":"2".repeat(64),
        "runtime_cases":470,"runtime_suites":50,"native_contexts":6,"coverage_contexts":1,"signed_contexts":6,
        "coverage":{"report_sha256":"3".repeat(64),"metrics":[
            {"metric":"regions","count":100,"covered":85,"percent":85.0,"floor":85.0},
            {"metric":"branches","count":100,"covered":80,"percent":80.0,"floor":80.0}]},
        "reviewer":"primary","independent_review_claimed":false,"qualification_verified":true,
        "retirement_authorized":false,"owner_host_changed":false,"original_evidence_rewritten":false});
    let signed = json!({"schema_version":1,"component":"native-signed-six-context-execution","source_sha":source,
        "run_id":run,"run_attempt":1,"contexts":signed,"original_execution_identity":{"source_sha":source,"run_id":run-1,"run_attempt":1},
        "current_signed_stage_execution_verified":true,
        "semantic_parity_claimed":false,"qualification_verified":false,"retirement_authorized":false,"original_evidence_rewritten":false});
    let q = serde_json::to_vec(&qualification).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("current-qualification.json", q.clone()),
        (
            "signed-execution-matrix.json",
            serde_json::to_vec(&signed).unwrap(),
        ),
    ] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    let bytes = writer.finish().unwrap().into_inner();
    let artifact = json!({"id":42,"name":"native-signed-execution-matrix","size_in_bytes":bytes.len(),
        "digest":format!("sha256:{}",sha(&bytes)),"expired":false,"created_at":"2026-11-09T10:01:00Z",
        "workflow_run":{"id":run,"repository_id":1400814502u64,"head_repository_id":1400814502u64,"head_sha":source,"head_branch":"main"}});
    let job = json!({"id":99,"run_id":run,"run_attempt":1,"head_sha":source,"head_branch":"main",
        "name":"signed-execution-matrix","status":"completed","conclusion":"success",
        "started_at":"2026-11-09T10:00:00Z","completed_at":"2026-11-09T10:02:00Z"});
    let log = format!(
        "2026-11-09T10:00:30Z {}\n",
        json!({"component":"native-current-qualification","identity":identity,
        "report_sha256":sha(&q),"qualification_verified":true})
    )
    .into_bytes();
    (identity, artifact, bytes, job, log)
}

#[test]
fn retirement_authenticates_original_qualification_artifact_and_successful_exact_attempt_log() {
    let (identity, artifact, bytes, job, logs) = qualified(&"a".repeat(40), 8);
    let report =
        retirement::authenticate_qualification(&artifact, &bytes, &job, &logs, &identity).unwrap();
    assert_eq!(report["source_sha"], identity["source_sha"]);
    assert_eq!(report["retirement_authorized"], false);
    for mutation in [
        "metadata",
        "bytes",
        "source",
        "attempt",
        "failed",
        "log",
        "duplicate-log",
        "artifact-time",
        "self-declared",
    ] {
        let mut artifact = artifact.clone();
        let mut bytes = bytes.clone();
        let mut job = job.clone();
        let mut logs = logs.clone();
        match mutation {
            "metadata" => artifact = json!({"qualification_verified":true}),
            "bytes" => bytes.push(0),
            "source" => job["head_sha"] = json!("b".repeat(40)),
            "attempt" => job["run_attempt"] = json!(2),
            "failed" => job["conclusion"] = json!("failure"),
            "log" => logs.clear(),
            "duplicate-log" => logs.extend_from_within(..),
            "artifact-time" => artifact["created_at"] = json!("2026-11-09T09:59:59Z"),
            "self-declared" => {
                let entries = matrix::artifact_entries(
                    &artifact,
                    &bytes,
                    &identity,
                    "native-signed-execution-matrix",
                    &std::collections::BTreeSet::from([
                        "current-qualification.json".into(),
                        "signed-execution-matrix.json".into(),
                    ]),
                )
                .unwrap();
                let mut report: Value =
                    serde_json::from_slice(&entries["current-qualification.json"]).unwrap();
                report["runtime_cases"] = json!(469);
                let q = serde_json::to_vec(&report).unwrap();
                let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
                for (name, data) in [
                    ("current-qualification.json", q.clone()),
                    (
                        "signed-execution-matrix.json",
                        entries["signed-execution-matrix.json"].clone(),
                    ),
                ] {
                    writer
                        .start_file(name, zip::write::SimpleFileOptions::default())
                        .unwrap();
                    writer.write_all(&data).unwrap();
                }
                bytes = writer.finish().unwrap().into_inner();
                artifact["size_in_bytes"] = json!(bytes.len());
                artifact["digest"] = json!(format!("sha256:{}", sha(&bytes)));
                logs = format!(
                    "2026-11-09T10:00:30Z {}\n",
                    json!({"component":"native-current-qualification","identity":identity,
                    "report_sha256":sha(&q),"qualification_verified":true})
                )
                .into_bytes();
            }
            _ => unreachable!(),
        }
        assert!(
            retirement::authenticate_qualification(&artifact, &bytes, &job, &logs, &identity)
                .is_err(),
            "{mutation}"
        );
    }
    let (other, _, _, _, _) = qualified(&"b".repeat(40), 9);
    assert!(
        retirement::authenticate_qualification(&artifact, &bytes, &job, &logs, &other).is_err(),
        "Q evidence is not R qualification"
    );
}

#[test]
fn optional_retirement_job_waits_for_signed_collector_without_receiving_signing_inputs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let workflow = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    for input in [
        "retirement-parent-run-id:",
        "retirement-parent-run-attempt:",
    ] {
        let section = workflow
            .split_once(input)
            .unwrap()
            .1
            .split_once("type: string")
            .unwrap()
            .1;
        assert!(
            section
                .split_once("jobs:")
                .unwrap_or((section, ""))
                .0
                .contains("required: false")
        );
    }
    let job = workflow
        .split_once("  retirement-verification:\n")
        .unwrap()
        .1;
    for required in [
        "needs:",
        "signed-execution-matrix",
        "needs.signed-execution-matrix.result == 'success'",
        "github.ref == 'refs/heads/main'",
        "github.event_name == 'workflow_dispatch'",
        "actions: read",
        "contents: read",
        "persist-credentials: false",
        "./.github/actions/native-bootstrap",
        "NUMERA_RETIREMENT_RUN_ID",
        "NUMERA_RETIREMENT_RUN_ATTEMPT",
        "${{ inputs.retirement-parent-run-id }}",
        "${{ inputs.retirement-parent-run-attempt }}",
        "retirement-gate",
    ] {
        assert!(job.contains(required), "{required}");
    }
    for forbidden in [
        "secrets.",
        "environment:",
        "continue-on-error",
        "signing-tools",
        "NUMERA_SIGNING",
    ] {
        assert!(!job.contains(forbidden), "{forbidden}");
    }
}

#[test]
fn retirement_success_follows_all_local_inventory_and_source_checks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let source = std::fs::read_to_string(root.join("tooling/src/inventory.rs")).unwrap();
    let check = source.split_once("pub fn check(").unwrap().1;
    let retirement = check.find("crate::retirement::check(context)?").unwrap();
    assert!(check.find("crate::ci::verify_suite_inventory").unwrap() < retirement);
    assert!(check.find("validate_source(&name").unwrap() < retirement);
    assert!(retirement < check.find("native-inventory").unwrap());
}

#[test]
fn retirement_workflow_id_comes_from_the_canonical_workflow_not_the_run_under_review() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let source = std::fs::read_to_string(root.join("tooling/src/retirement.rs")).unwrap();
    assert!(source.contains("actions/workflows/ci.yml"));
    assert!(source.contains("promotion::validate_run(&metadata, &identity, workflow, completed)"));
    assert!(!source.contains("metadata[\"workflow_id\"]"));
}

#[test]
fn historical_tooling_inventory_has_exactly_51_migrated_paths_including_five_powershell_hashes()
{
    let original = manifest();
    let hashes = inventory::tooling_hashes(&original).unwrap();
    assert_eq!(hashes.len(), 51);
    assert_eq!(hashes.keys().filter(|path| path.ends_with(".ps1")).count(), 5);
    assert_eq!(original["schema_version"], 3);
    assert_eq!(original["rust_migration_count"], 51);
    assert_eq!(original["powershell_retained_count"], 0);
    assert!(original.get("retained_powershell").is_none());
    for mutation in ["revision", "hash", "duplicate", "escape", "powershell", "legacy-field"] {
        let mut changed = original.clone();
        match mutation {
            "revision" => changed["original_revision"] = json!("a".repeat(40)),
            "hash" => changed["entries"][0]["source_sha256"] = json!("invalid"),
            "duplicate" => changed["entries"][1] = changed["entries"][0].clone(),
            "escape" => changed["entries"][0]["source"] = json!("scripts/../outside.py"),
            "powershell" => {
                let row = changed["entries"].as_array_mut().unwrap().iter_mut()
                    .find(|row| row["source"].as_str().unwrap().ends_with(".ps1")).unwrap();
                row["source"] = json!("scripts/extra.py");
            }
            "legacy-field" => changed["retained_powershell"] = Value::Null,
            _ => unreachable!(),
        }
        assert!(inventory::tooling_hashes(&changed).is_err(), "{mutation}");
    }
}
