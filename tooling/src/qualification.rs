use crate::{context::{Context, fail, sha}, execution_evidence, fixtures::{BASELINE, SOURCE}, ledger, ledger_inputs, test_execution};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io};

fn field<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    value[key].as_str().filter(|text| !text.is_empty()).ok_or_else(|| fail("Named case binding identity is missing."))
}
fn array<'a>(value: &'a Value, key: &str) -> io::Result<&'a Vec<Value>> {
    value[key].as_array().ok_or_else(|| fail("Named case binding inventory is missing."))
}

/// Bind successful names to authenticated historical proposals, without approving their semantics.
/// Artifact/run authentication and the final seven-context decision remain separate requirements.
pub fn bind_cases(rebind: &Value, identity: &Value, provenance: &str,
    dependencies: &BTreeMap<String, String>, sources: &BTreeSet<String>, reports: &BTreeMap<String, Vec<u8>>) -> io::Result<Value> {
    let coverage = identity["phase"] == "coverage";
    if !coverage && identity["phase"] != "built" { return Err(fail("Case binding requires a built or coverage context.")); }
    if rebind["schema_version"] != 1 || rebind["component"] != "native-case-source-rebind"
        || rebind["baseline"] != BASELINE || rebind["historical_source_sha"] != SOURCE
        || rebind["historical_reference_sha256"] != ledger_inputs::REFERENCE_SHA256
        || rebind["current_source_sha"] != identity["source_sha"] || rebind["original_evidence_rewritten"] != false
        || rebind["current_assertion_review"] != "pending" || rebind["current_execution_claimed"] != false
        || rebind["acceptance_verified"] != false {
        return Err(fail("Current case proposals cannot inherit historical acceptance."));
    }
    let files = rebind["current_source_files"].as_object().ok_or_else(|| fail("Current assertion source inventory missing."))?;
    if files.is_empty() || files.iter().any(|(path, digest)| dependencies.get(path)
        .is_none_or(|actual| digest.as_str() != Some(actual.as_str()))) {
        return Err(fail("Current assertion or helper source differs from verified provenance."));
    }
    let phases: &[&str] = if coverage { &["coverage"] } else { &["built", "release", "extracted-intermediate"] };
    if reports.keys().map(String::as_str).collect::<BTreeSet<_>>() != phases.iter().copied().collect() {
        return Err(fail("Every required exact execution phase must be present once."));
    }
    let sdk_sources = BTreeSet::from(["tests/rust/sdk_acceptance.rs".to_owned(), "tests/rust/media_acceptance.rs".to_owned()]);
    let mut verified = BTreeMap::new(); let mut report_hashes = BTreeMap::new();
    for phase in phases {
        let mut expected = identity.clone(); expected["phase"] = json!(phase);
        let report = execution_evidence::verify_report(&reports[*phase], &expected, provenance,
            dependencies, if matches!(*phase, "built" | "coverage") { sources } else { &sdk_sources })?;
        report_hashes.insert(*phase, sha(&reports[*phase])); verified.insert(*phase, report);
    }
    let cases = array(rebind, "cases")?;
    let historical = cases.iter().map(|case| case["historical"].clone()).collect::<Vec<_>>();
    ledger::validate_rows(&historical)?;
    let platform = field(identity, "platform")?;
    let mut bound = Vec::new();
    for (case, original) in cases.iter().zip(&historical) {
        if case["id"] != original["id"] || case["current"]["status"] != "pending"
            || case["current"]["assertion_review"] != "pending" || case["current"]["current_execution_claimed"] != false {
            return Err(fail("Current case state or original identity differs."));
        }
        let old_mappings = array(original, "native")?.iter().map(|value| value.as_str()
            .ok_or_else(|| fail("Historical native mapping must be a string."))).collect::<io::Result<BTreeSet<_>>>()?;
        let proposals = array(&case["current"], "proposed_native")?;
        let mut mapped = BTreeSet::new(); let mut origins = BTreeSet::new();
        for proposal in proposals {
            let origin = field(proposal, "historical_mapping")?;
            let mapping = field(proposal, "proposed_mapping")?;
            let (file, _) = mapping.split_once("::").ok_or_else(|| fail("Current mapping lacks its test symbol."))?;
            if !old_mappings.contains(origin) || !origins.insert(origin)
                || !files.contains_key(file) || dependencies.get(file).is_none_or(|digest| proposal["current_sha256"] != *digest)
                || proposal["assertion_ranges_rebound"] != false || proposal["assertion_review"] != "pending"
                || proposal["current_execution_claimed"] != false {
                return Err(fail("Current mapped assertion is duplicated, stale or implicitly approved."));
            }
            mapped.insert(mapping);
        }
        if old_mappings.len() != array(original, "native")?.len() || origins != old_mappings || mapped.is_empty() { return Err(fail("Every original mapping requires its current Rust proposal.")); }
        let mode = field(&original["platform_modes"], platform)?;
        let mut named = serde_json::Map::new();
        if mode == "run" {
            for phase in phases {
                let mut results = serde_json::Map::new();
                for mapping in &mapped {
                    // Non-SDK tests execute in the built/instrumented harness, not a release client tier.
                    if !matches!(*phase, "built" | "coverage") && !mapping.starts_with("tests/rust/sdk_acceptance.rs::") { continue; }
                    results.insert((*mapping).to_owned(), execution_evidence::named_test(&verified[*phase], mapping)?);
                }
                if !results.is_empty() { named.insert((*phase).to_owned(), Value::Object(results)); }
            }
        }
        bound.push(json!({"id":original["id"],"source":original["source"],"source_sha256":original["source_sha256"],
            "platform_mode":mode,"historical_native":original["native"],"current_native":mapped,
            "historical_assertions_sha256":ledger::assertion_digest(&original["assertions"])?,
            "assertion_review":"pending","named_execution":named}));
    }
    Ok(json!({"schema_version":1,"component":"native-current-named-case-execution","identity":identity,
        "baseline":BASELINE,"historical_source_sha":SOURCE,"historical_reference_sha256":ledger_inputs::REFERENCE_SHA256,
        "provenance_sha256":provenance,"phase_reports_sha256":report_hashes,"cases":bound,"suites":50,
        "current_named_execution_verified":true,"semantic_parity_claimed":false,"qualification_verified":false,
        "original_evidence_rewritten":false}))
}

pub fn check(context: &Context) -> io::Result<()> {
    let phase = match std::env::var("GITHUB_JOB").as_deref() {
        Ok("native") => "built", Ok("native-coverage") => "coverage",
        _ => return Err(fail("Named case binding belongs to native or coverage jobs only.")),
    };
    let identity = test_execution::hosted_identity(context, phase)?;
    let provenance = crate::evidence::snapshot(context)?;
    let historical = ledger_inputs::check(context)?;
    let bytes = ledger::read(&context.root, field(&historical, "current_source_rebind")?)?;
    if historical["rebind_sha256"] != sha(&bytes) { return Err(fail("Current source proposal bytes changed.")); }
    let rebind = ledger::parse(&bytes)?;
    let metadata = crate::ci::suite_metadata(context)?;
    let selected: &[&str] = if phase == "coverage" { &["numera-gen-mcp"] } else { &["numera-gen-mcp", "numera-task"] };
    let sources = test_execution::selected_sources(&context.root, &metadata, selected)?;
    let phases: &[&str] = if phase == "coverage" { &["coverage"] } else { &["built", "release", "extracted-intermediate"] };
    let mut reports = BTreeMap::new();
    for phase in phases {
        let path = if matches!(*phase, "built" | "coverage") { ".ci-work/native/test-execution.json".to_owned() }
            else { format!(".ci-work/native/sdk-execution-{phase}.json") };
        reports.insert((*phase).to_owned(), ledger::read(&context.root, &path)?);
    }
    let mut report = bind_cases(&rebind, &identity, &provenance.sha256, &provenance.dependencies, &sources, &reports)?;
    if crate::evidence::snapshot(context)? != provenance { return Err(fail("Case binding provenance changed.")); }
    report["source_rebind_sha256"] = json!(sha(&bytes));
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| fail("Cannot encode named case execution evidence."))?;
    context.write_new(".ci-work/native/current-case-execution.json", &bytes)?;
    println!("{}", json!({"component":"native-current-named-case-execution","cases":470,"suites":50,
        "identity":identity,"report_sha256":sha(&bytes),
        "current_named_execution_verified":true,"semantic_parity_claimed":false,"qualification_verified":false}));
    Ok(())
}
