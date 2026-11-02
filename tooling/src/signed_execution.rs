use crate::{
    context::{Context, fail, sha},
    execution_evidence, ledger, test_execution,
};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io};

pub const SIGNATURE_REPORT: &str = "signature-execution.json";
pub const EXTRACTION_REPORT: &str = "signed-extraction-execution.json";

fn hash(value: &Value) -> bool {
    value.as_str().is_some_and(|text| text.len() == 64
        && text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
}

/// The caller authenticates original artifacts and successful job logs independently.
/// These joins establish executed stages, never semantic parity or retirement permission.
pub fn bind(
    signature_bytes: &[u8], extraction_bytes: &[u8], sdk_bytes: &[u8],
    identity: &Value, provenance: &str, dependencies: &BTreeMap<String, String>,
) -> io::Result<Value> {
    if identity["phase"] != "extracted-signed" || identity["reference"] != "refs/heads/main" {
        return Err(fail("Signed execution requires the approved main signing context."));
    }
    let sources = BTreeSet::from([
        "tests/rust/sdk_acceptance.rs".to_owned(),
        "tests/rust/media_acceptance.rs".to_owned(),
    ]);
    let sdk = execution_evidence::verify_report(sdk_bytes, identity, provenance, dependencies, &sources)?;
    let signature = ledger::parse(signature_bytes)?;
    let extraction = ledger::parse(extraction_bytes)?;
    for (report, component) in [
        (&signature, "native-signed-signature-execution"),
        (&extraction, "native-signed-extraction-execution"),
    ] {
        if report["schema_version"] != 1 || report["component"] != component
            || report["identity"] != *identity || report["provenance_sha256"] != provenance
            || !hash(&report["manifest_sha256"])
            || report["tested_binary_sha256"] != sdk["tested_binary_sha256"]
            || report["cleanup_verified"] != true || report["public_os_trust"] != false
            || report["semantic_parity_claimed"] != false || report["qualification_verified"] != false
        {
            return Err(fail("Signed stage identity, bytes, cleanup or evidence boundary differs."));
        }
    }
    if signature["manifest_sha256"] != extraction["manifest_sha256"]
        || signature["positive_negative_verified"] != true || signature["publisher_inputs_absent"] != true
        || extraction["activation_verified"] != true || extraction["owner_host_changed"] != false
        || extraction["sdk_report_sha256"] != sha(sdk_bytes) || !hash(&extraction["archive_sha256"])
    {
        return Err(fail("Signed signature, activation, archive and SDK stages are not the same artifact."));
    }
    Ok(json!({"schema_version":1, "component":"native-signed-stage-execution", "identity":identity,
        "provenance_sha256":provenance, "manifest_sha256":signature["manifest_sha256"],
        "tested_binary_sha256":sdk["tested_binary_sha256"], "archive_sha256":extraction["archive_sha256"],
        "signature_report_sha256":sha(signature_bytes), "extraction_report_sha256":sha(extraction_bytes),
        "sdk_report_sha256":sha(sdk_bytes), "sdk_harnesses":sdk["harnesses"],
        "signed_stage_execution_verified":true,
        "semantic_parity_claimed":false, "qualification_verified":false, "retirement_authorized":false}))
}

pub fn verify_sdk_names(original: &Value, signed: &Value) -> io::Result<()> {
    let tooling = &original["tooling_harness_execution"];
    if tooling["current_named_test_execution_verified"] != true
        || tooling["sdk_artifact_phase_execution_verified"] != true
    {
        return Err(fail("Original tooling SDK execution is not verified."));
    }
    for source in ["tests/rust/sdk_acceptance.rs", "tests/rust/media_acceptance.rs"] {
        let names = |value: &Value| -> io::Result<BTreeSet<String>> {
            let values = value.as_array().filter(|values| !values.is_empty() && values.len() <= 20000)
                .ok_or_else(|| fail("SDK named inventory is missing or oversized."))?;
            let names = values.iter().map(|name| name.as_str().map(str::to_owned)
                .ok_or_else(|| fail("SDK test name must be a string.")))
                .collect::<io::Result<BTreeSet<_>>>()?;
            if names.len() != values.len() { return Err(fail("SDK test names are duplicated.")); }
            Ok(names)
        };
        if names(&tooling["harnesses"][source])? != names(&signed["sdk_harnesses"][source]["passed"])? {
            return Err(fail("Signed SDK execution differs from the original native test inventory."));
        }
    }
    Ok(())
}

pub(crate) fn verify_logs(logs: &[u8], identity: &Value, reports: &[(&str, String)]) -> io::Result<()> {
    if logs.len() > 16 * 1024 * 1024 || reports.is_empty() {
        return Err(fail("Signed job logs or receipt inventory exceed bounds."));
    }
    let text = std::str::from_utf8(logs).map_err(|_| fail("Signed job log must be UTF-8."))?;
    for (component, digest) in reports {
        if !hash(&json!(digest)) {
            return Err(fail("Signed receipt digest is invalid."));
        }
        let mut count = 0;
        for line in text.lines() {
            let Some((timestamp, message)) = line.split_once(' ') else { continue; };
            if chrono::DateTime::parse_from_rfc3339(timestamp).is_err() || !message.starts_with('{') { continue; }
            let Ok(row) = ledger::parse(message.as_bytes()) else { continue; };
            if row["component"] != *component { continue; }
            if row["identity"] != *identity || row["report_sha256"] != *digest {
                return Err(fail("Signed receipt log identity or digest differs."));
            }
            count += 1;
        }
        if count != 1 {
            return Err(fail("Exactly one original signed receipt digest is required in its job log."));
        }
    }
    Ok(())
}

pub fn verify_job(job: &Value, logs: &[u8], identity: &Value, reports: &[(&str, String)]) -> io::Result<()> {
    let name = format!("signing-{}", identity["target"].as_str().unwrap_or(""));
    if job["id"].as_u64().is_none_or(|id| id == 0) || job["run_id"] != identity["run_id"]
        || job["head_sha"] != identity["source_sha"] || job["head_branch"] != "main"
        || job["name"] != name || job["status"] != "completed" || job["conclusion"] != "success"
    {
        return Err(fail("Signed context job is not the exact successful source/run/target."));
    }
    verify_logs(logs, identity, reports)
}

fn common(context: &Context, component: &str, manifest: &str, binary: &str) -> io::Result<Value> {
    context.hosted()?;
    crate::acceptance::public_environment(context)?;
    let identity = test_execution::hosted_identity(context, "extracted-signed")?;
    if identity["reference"] != "refs/heads/main" {
        return Err(fail("Signed stage receipt requires approved main authority."));
    }
    let provenance = crate::evidence::snapshot(context)?;
    if !hash(&json!(manifest)) || !hash(&json!(binary)) {
        return Err(fail("Signed stage byte identity is invalid."));
    }
    Ok(json!({"schema_version":1, "component":component, "identity":identity,
        "provenance_sha256":provenance.sha256, "manifest_sha256":manifest,
        "tested_binary_sha256":binary, "cleanup_verified":true, "public_os_trust":false,
        "semantic_parity_claimed":false, "qualification_verified":false}))
}
fn write(context: &Context, name: &str, report: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(report).map_err(|_| fail("Cannot encode signed execution receipt."))?;
    context.write_new(&format!(".ci-work/native/{name}"), &bytes)?;
    println!("{}", json!({"component":report["component"], "identity":report["identity"],
        "report_sha256":sha(&bytes)}));
    Ok(())
}

/// Called only after genuine positive/negative verification and successful fixture cleanup.
pub fn signature(context: &Context, manifest: &str, binary: &str) -> io::Result<()> {
    let mut report = common(context, "native-signed-signature-execution", manifest, binary)?;
    report["positive_negative_verified"] = json!(true);
    report["publisher_inputs_absent"] = json!(true);
    write(context, SIGNATURE_REPORT, &report)
}

/// Called after signed extraction, actual activation/SDK execution and successful scratch cleanup.
pub fn extraction(context: &Context, manifest: &str, binary: &str, archive: &str) -> io::Result<()> {
    let mut report = common(context, "native-signed-extraction-execution", manifest, binary)?;
    let sdk = ledger::read(&context.root, ".ci-work/native/sdk-execution-extracted-signed.json")?;
    report["archive_sha256"] = json!(archive);
    report["sdk_report_sha256"] = json!(sha(&sdk));
    report["activation_verified"] = json!(true);
    report["owner_host_changed"] = json!(false);
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| fail("Cannot encode signed extraction receipt."))?;
    let signature = ledger::read(&context.root, &format!(".ci-work/native/{SIGNATURE_REPORT}"))?;
    let provenance = crate::evidence::snapshot(context)?;
    bind(&signature, &bytes, &sdk, &report["identity"], &provenance.sha256, &provenance.dependencies)?;
    write(context, EXTRACTION_REPORT, &report)
}
