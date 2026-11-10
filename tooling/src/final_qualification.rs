use crate::{context::fail, matrix, signed_execution, signed_matrix};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io};

fn digest(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.len() == 64
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

/// Inputs are reconstructed by the collectors from authenticated artifacts and original job logs.
/// This join does not authenticate arbitrary caller-supplied reports or authorize source removal.
pub fn bind(
    original: &Value,
    signing_identity: &Value,
    signed: &BTreeMap<String, Value>,
    runtime_review_sha256: &str,
    tooling_review_sha256: &str,
) -> io::Result<Value> {
    if original["schema_version"] != 1
        || original["component"] != "native-seven-context-execution"
        || original["source_sha"] != signing_identity["source_sha"]
        || original["cases"] != 470
        || original["suites"] != 50
        || original["current_named_execution_verified"] != true
        || original["current_assertion_review_verified"] != true
        || original["source_review_sha256"] != runtime_review_sha256
        || original["tooling_source_review_sha256"] != tooling_review_sha256
        || !digest(&json!(runtime_review_sha256))
        || !digest(&json!(tooling_review_sha256))
        || original["semantic_parity_claimed"] != false
        || original["qualification_verified"] != false
        || original["original_evidence_rewritten"] != false
    {
        return Err(fail(
            "Final qualification requires exact current reviewed execution.",
        ));
    }
    signed_matrix::verify_contexts(signed, signing_identity)?;
    let contexts = original["contexts"]
        .as_object()
        .ok_or_else(|| fail("Final qualification context inventory missing."))?;
    let identities = contexts
        .iter()
        .map(|(name, context)| {
            (
                name.clone(),
                context["reviewed_named_execution"]["identity"].clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    matrix::verify_contexts(&identities)?;
    for (name, context) in contexts {
        let runtime = &context["reviewed_named_execution"];
        let identity = &runtime["identity"];
        if identity["source_sha"] != original["source_sha"]
            || identity["run_id"] != original["run_id"]
            || identity["run_attempt"] != original["run_attempt"]
            || runtime["cases"] != 470
            || runtime["suites"] != 50
            || runtime["current_assertion_review_verified"] != true
            || runtime["current_named_execution_verified"] != true
            || runtime["source_review"]["cases"] != 470
            || runtime["source_review"]["assertion_records"] != 606
            || runtime["source_review"]["unique_assertions"] != 411
            || runtime["source_review"]["current_assertion_review_verified"] != true
            || runtime["source_review"]["reviewer"] != "primary"
        {
            return Err(fail(
                "Final runtime assertion or execution coverage is incomplete.",
            ));
        }
        if name != "coverage" {
            let tooling = &context["reviewed_tooling_execution"];
            if tooling["identity"] != *identity
                || tooling["source_review_sha256"] != tooling_review_sha256
                || tooling["current_tooling_assertions_and_execution_verified"] != true
            {
                return Err(fail(
                    "Final tooling assertion or execution coverage is incomplete.",
                ));
            }
            signed_execution::verify_sdk_names(context, &signed[name])?;
        }
    }
    let coverage = &contexts["coverage"]["coverage_evidence"];
    if !digest(&coverage["report_sha256"]) {
        return Err(fail("Final measured coverage identity missing."));
    }
    let metrics = coverage["metrics"]
        .as_array()
        .filter(|metrics| metrics.len() == 2)
        .ok_or_else(|| fail("Final measured coverage inventory differs."))?;
    crate::inventory::verify_coverage_metrics(metrics)?;
    Ok(
        json!({"schema_version":1,"component":"native-current-qualification",
        "source_sha":original["source_sha"],"qualification_run_id":original["run_id"],
        "qualification_run_attempt":original["run_attempt"],"signing_identity":signing_identity,
        "source_review_sha256":runtime_review_sha256,"tooling_source_review_sha256":tooling_review_sha256,
        "runtime_cases":470,"runtime_suites":50,"native_contexts":6,"coverage_contexts":1,
        "signed_contexts":signed.len(),"coverage":coverage,
        "reviewer":"primary","independent_review_claimed":false,
        "qualification_verified":true,"retirement_authorized":false,"owner_host_changed":false,
        "original_evidence_rewritten":false}),
    )
}
