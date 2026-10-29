use numera_task::{context::sha, matrix, test_execution::execution_identity};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io::{Cursor, Write}};

fn identity() -> Value {
    execution_identity(&json!({"GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP","GITHUB_EVENT_NAME":"workflow_dispatch",
        "GITHUB_SHA":"a".repeat(40),"GITHUB_REF":"refs/heads/main","GITHUB_RUN_ID":"7","GITHUB_RUN_ATTEMPT":"2",
        "GITHUB_JOB":"native"}), "built", "x86_64-unknown-linux-gnu").unwrap()
}
fn artifact(bytes: &[u8]) -> Value {
    json!({"id":42,"name":"native-execution-x86_64-unknown-linux-gnu","size_in_bytes":bytes.len(),
        "digest":format!("sha256:{}",sha(bytes)),"expired":false,"workflow_run":{"id":7,
        "repository_id":1400814502u64,"head_repository_id":1400814502u64,"head_sha":"a".repeat(40),"head_branch":"main"}})
}
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer.start_file(*name,zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn artifact_is_bound_to_exact_repository_run_source_branch_size_digest_and_allowlisted_regular_members() {
    let bytes = zip(&[("test-execution.json",b"{}"),("current-case-execution.json",b"{\"case\":1}")]);
    let metadata = artifact(&bytes);
    let expected = BTreeSet::from(["test-execution.json".into(),"current-case-execution.json".into()]);
    let entries = matrix::artifact_entries(&metadata,&bytes,&identity(),metadata["name"].as_str().unwrap(),&expected).unwrap();
    assert_eq!(entries["current-case-execution.json"],b"{\"case\":1}");
    for mutation in ["repo","run","sha","branch","digest","size","expired","name","id"] {
        let mut changed = metadata.clone();
        match mutation {
            "repo" => changed["workflow_run"]["head_repository_id"] = json!(1),
            "run" => changed["workflow_run"]["id"] = json!(8),
            "sha" => changed["workflow_run"]["head_sha"] = json!("b".repeat(40)),
            "branch" => changed["workflow_run"]["head_branch"] = json!("other"),
            "digest" => changed["digest"] = json!(format!("sha256:{}","0".repeat(64))),
            "size" => changed["size_in_bytes"] = json!(bytes.len()+1),
            "expired" => changed["expired"] = json!(true),
            "name" => changed["name"] = json!("native-execution-coverage"),
            "id" => changed["id"] = json!(0),
            _ => unreachable!(),
        }
        assert!(matrix::artifact_entries(&changed,&bytes,&identity(),metadata["name"].as_str().unwrap(),&expected).is_err(),"{mutation}");
    }
    for invalid in [zip(&[("../test-execution.json",b"{}"),("current-case-execution.json",b"{}")]),zip(&[("test-execution.json",b"{}")]),
        zip(&[("test-execution.json",b"{}"),("current-case-execution.json",b"{}"),("extra",b"x")])] {
        let metadata = artifact(&invalid);
        assert!(matrix::artifact_entries(&metadata,&invalid,&identity(),metadata["name"].as_str().unwrap(),&expected).is_err());
    }
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer.add_symlink("test-execution.json", "elsewhere", zip::write::SimpleFileOptions::default()).unwrap();
    writer.start_file("current-case-execution.json", zip::write::SimpleFileOptions::default()).unwrap();
    writer.write_all(b"{}").unwrap();
    let linked = writer.finish().unwrap().into_inner(); let metadata = artifact(&linked);
    assert!(matrix::artifact_entries(&metadata,&linked,&identity(),metadata["name"].as_str().unwrap(),&expected).is_err());
}

#[test]
fn named_case_report_digest_must_appear_once_in_its_successful_exact_job_log() {
    let identity = identity();
    let report = json!({"component":"native-current-named-case-execution","identity":identity,
        "report_sha256":"1".repeat(64),"cases":470,"suites":50,"current_named_execution_verified":true,
        "semantic_parity_claimed":false,"qualification_verified":false});
    let log = format!("2026-10-29T10:00:00.0000000Z {report}\n");
    let job = json!({"id":99,"run_id":7,"head_sha":"a".repeat(40),"head_branch":"main",
        "name":"native-x86_64-unknown-linux-gnu","status":"completed","conclusion":"success"});
    matrix::verify_job(&job,log.as_bytes(),&identity,&"1".repeat(64)).unwrap();
    for mutation in ["failed","sha","run","name","branch","status","id"] {
        let mut changed = job.clone();
        match mutation {
            "failed" => changed["conclusion"] = json!("failure"),
            "sha" => changed["head_sha"] = json!("b".repeat(40)),
            "run" => changed["run_id"] = json!(8),
            "name" => changed["name"] = json!("unrelated"),
            "branch" => changed["head_branch"] = json!("other"),
            "status" => changed["status"] = json!("in_progress"),
            "id" => changed["id"] = json!(0),
            _ => unreachable!(),
        }
        assert!(matrix::verify_job(&changed,log.as_bytes(),&identity,&"1".repeat(64)).is_err());
    }
    for invalid in [String::new(),format!("{log}{log}"),log.replace(&"1".repeat(64),&"2".repeat(64)),
        log.replace("\"run_attempt\":2","\"run_attempt\":1"),format!("untrusted-prefix {report}\n")] {
        assert!(matrix::verify_job(&job,invalid.as_bytes(),&identity,&"1".repeat(64)).is_err());
    }
}

#[test]
fn matrix_requires_six_native_targets_and_separate_instrumented_linux_without_duplicates() {
    let expected = matrix::contexts();
    assert_eq!(expected.len(),7);
    assert_eq!(expected["coverage"],("x86_64-unknown-linux-gnu","coverage"));
    let observed = expected.iter().map(|(label,(target,phase))| {
        let value = execution_identity(&json!({"GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP",
            "GITHUB_EVENT_NAME":"workflow_dispatch","GITHUB_SHA":"a".repeat(40),"GITHUB_REF":"refs/heads/main",
            "GITHUB_RUN_ID":"7","GITHUB_RUN_ATTEMPT":"2",
            "GITHUB_JOB":if *phase == "coverage" {"native-coverage"} else {"native"}}),phase,target).unwrap();
        ((*label).to_owned(),value)
    }).collect::<BTreeMap<_,_>>();
    matrix::verify_contexts(&observed).unwrap();
    let mut missing = observed.clone();missing.remove("coverage");assert!(matrix::verify_contexts(&missing).is_err());
    for (key,value) in [("target",json!("aarch64-unknown-linux-gnu")),("source_sha",json!("b".repeat(40))),
        ("reference",json!("refs/heads/other")),("run_id",json!(8)),("run_attempt",json!(1)),
        ("job",json!("native")),("platform",json!("darwin")),("event",json!("push")),
        ("repository",json!("other/repository"))] {
        let mut wrong = observed.clone(); wrong.get_mut("coverage").unwrap()[key] = value;
        assert!(matrix::verify_contexts(&wrong).is_err(),"{key}");
    }
    let mut extra = observed.clone(); extra.insert("unrelated".into(),identity());
    assert!(matrix::verify_contexts(&extra).is_err());
    let mut invalid = observed; invalid.get_mut("x86_64-unknown-linux-gnu").unwrap()["run_id"] = json!(0);
    assert!(matrix::verify_contexts(&invalid).is_err());
}
