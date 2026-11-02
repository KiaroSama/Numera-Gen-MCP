use crate::{
    context::{Context, fail, sha},
    evidence, fixtures::INPUTS, ledger, matrix, promotion, signed_execution, test_execution,
};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io};

const REPOSITORY: &str = "KiaroSama/Numera-Gen-MCP";
const REPOSITORY_ID: u64 = 1400814502;

/// Input records must already be reconstructed from authenticated original stage reports.
pub fn verify_contexts(records: &BTreeMap<String, Value>, root: &Value) -> io::Result<()> {
    let targets = matrix::contexts().keys().filter(|target| **target != "coverage")
        .map(|target| (*target).to_owned()).collect::<BTreeSet<_>>();
    if records.keys().cloned().collect::<BTreeSet<_>>() != targets
        || root["reference"] != "refs/heads/main"
    {
        return Err(fail("Signed execution requires every exact native target on main."));
    }
    let number = |key: &str| root[key].as_u64().filter(|value| *value > 0)
        .map(|value| value.to_string()).ok_or_else(|| fail("Signed matrix run identity missing."));
    let environment = json!({"GITHUB_REPOSITORY":root["repository"], "GITHUB_EVENT_NAME":root["event"],
        "GITHUB_SHA":root["source_sha"], "GITHUB_REF":root["reference"], "GITHUB_JOB":"signing",
        "GITHUB_RUN_ID":number("run_id")?, "GITHUB_RUN_ATTEMPT":number("run_attempt")?});
    for target in targets {
        let identity = test_execution::execution_identity(&environment, "extracted-signed", &target)?;
        let record = &records[&target];
        if record["schema_version"] != 1 || record["component"] != "native-signed-stage-execution"
            || record["identity"] != identity || record["signed_stage_execution_verified"] != true
            || record["semantic_parity_claimed"] != false || record["qualification_verified"] != false
            || record["retirement_authorized"] != false
        {
            return Err(fail("Signed contexts differ in execution, source, run, attempt or target."));
        }
    }
    Ok(())
}

fn run_identity(context: &Context) -> io::Result<(Value, Value)> {
    context.hosted()?;
    if std::env::var("GITHUB_JOB").as_deref() != Ok("signed-execution-matrix")
        || !matches!(std::env::var("NUMERA_CI_OPERATION").as_deref(), Ok("qualify" | "sign-qualified"))
    {
        return Err(fail("Signed matrix belongs to its dedicated qualification job."));
    }
    crate::acceptance::public_environment(context)?;
    let mut environment = ["GITHUB_REPOSITORY", "GITHUB_EVENT_NAME", "GITHUB_SHA", "GITHUB_REF",
        "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"].into_iter().map(|key| (key.to_owned(),
            std::env::var(key).map(Value::String).unwrap_or(Value::Null)))
        .collect::<serde_json::Map<_, _>>();
    environment.insert("GITHUB_JOB".into(), json!("signing"));
    let environment = Value::Object(environment);
    let identity = test_execution::execution_identity(&environment, "extracted-signed", "x86_64-unknown-linux-gnu")?;
    if identity["reference"] != "refs/heads/main" || identity["source_sha"] != context.source_sha()? {
        return Err(fail("Signed matrix requires the current approved main revision."));
    }
    let run = ledger::parse(&matrix::api(context, &format!(
        "repos/{REPOSITORY}/actions/runs/{}/attempts/{}", identity["run_id"], identity["run_attempt"]
    ))?)?;
    if run["id"] != identity["run_id"] || run["run_attempt"] != identity["run_attempt"]
        || run["head_sha"] != identity["source_sha"] || run["head_branch"] != "main"
        || run["event"] != "workflow_dispatch" || run["repository"]["id"] != REPOSITORY_ID
        || run["head_repository"]["id"] != REPOSITORY_ID || run["repository"]["full_name"] != REPOSITORY
    {
        return Err(fail("Signed matrix run, attempt, repository or source differs."));
    }
    Ok((environment, identity))
}

pub fn check(context: &Context) -> io::Result<()> {
    let (environment, root) = run_identity(context)?;
    let run = root["run_id"].as_u64().ok_or_else(|| fail("Signed matrix run missing."))?;
    let attempt = root["run_attempt"].as_u64().ok_or_else(|| fail("Signed matrix attempt missing."))?;
    let artifacts = matrix::pages(context, &format!("repos/{REPOSITORY}/actions/runs/{run}/artifacts"), "artifacts")?;
    let jobs = matrix::pages(context, &format!("repos/{REPOSITORY}/actions/runs/{run}/attempts/{attempt}/jobs"), "jobs")?;
    let dependencies = evidence::dependencies(context)?;
    let original = promotion::load(context)?;
    let mut records = BTreeMap::new();
    let mut evidence_records = BTreeMap::new();
    for target in matrix::contexts().keys().filter(|target| **target != "coverage") {
        let identity = test_execution::execution_identity(&environment, "extracted-signed", target)?;
        let name = format!("native-execution-signed-{target}");
        let (entries, artifact) = matrix::artifact(context, &artifacts, &identity, &name, &BTreeSet::from([
            signed_execution::SIGNATURE_REPORT.into(), signed_execution::EXTRACTION_REPORT.into(),
            "sdk-execution-extracted-signed.json".into(), "oracle-evidence.zip".into(),
            "signing-source.json".into(),
        ]))?;
        let source = ledger::parse(&entries["signing-source.json"])?;
        if source != promotion::receipt(context, &original, &identity)? {
            return Err(fail("Signed stage did not consume the exact original executed intermediate."));
        }
        let provenance_bytes = &entries["oracle-evidence.zip"];
        let expected = INPUTS.iter().map(|name| (*name).to_owned()).chain(["provenance.json".into()]).collect();
        let mut payloads = matrix::decode(provenance_bytes, &expected)?;
        let manifest = ledger::parse(&payloads.remove("provenance.json")
            .ok_or_else(|| fail("Signed oracle provenance missing."))?)?;
        let os = if target.contains("windows") { "windows" } else if target.contains("apple") { "macos" } else { "linux" };
        let arch = if target.starts_with("aarch64") { "aarch64" } else { "x86_64" };
        evidence::verify_platform_bundle(provenance_bytes, &context.source_sha()?, &dependencies,
            &manifest["materialization"], &payloads, os, arch)?;
        crate::fixtures::verify_materialized(&context.root, os, &context.source_sha()?,
            &manifest["materialization"], &payloads)?;
        let record = signed_execution::bind(&entries[signed_execution::SIGNATURE_REPORT],
            &entries[signed_execution::EXTRACTION_REPORT], &entries["sdk-execution-extracted-signed.json"],
            &identity, &sha(provenance_bytes), &dependencies)?;
        let name = format!("signing-{target}");
        let matching = jobs.iter().filter(|job| job["name"] == name).collect::<Vec<_>>();
        if matching.len() != 1 { return Err(fail("Signed native job missing or duplicated.")); }
        let job = matching[0];
        let id = job["id"].as_u64().filter(|id| *id > 0).ok_or_else(|| fail("Signed job ID missing."))?;
        let logs = matrix::api(context, &format!("repos/{REPOSITORY}/actions/jobs/{id}/logs"))?;
        signed_execution::verify_job(job, &logs, &identity, &[
            ("native-authenticated-signing-source", sha(&entries["signing-source.json"])),
            ("native-signed-signature-execution", record["signature_report_sha256"].as_str().unwrap().to_owned()),
            ("native-signed-extraction-execution", record["extraction_report_sha256"].as_str().unwrap().to_owned()),
        ])?;
        evidence_records.insert((*target).to_owned(), json!({"artifact":artifact,"job_id":id,"log_sha256":sha(&logs),
            "signing_source_sha256":sha(&entries["signing-source.json"]),"original_intermediate":source["artifact"]}));
        records.insert((*target).to_owned(), record);
    }
    verify_contexts(&records, &root)?;
    let contexts = original.execution["contexts"].as_object()
        .ok_or_else(|| fail("Original execution contexts missing."))?;
    if contexts.keys().map(String::as_str).collect::<BTreeSet<_>>() != matrix::contexts().keys().copied().collect() {
        return Err(fail("Original seven-context inventory differs."));
    }
    for (target, record) in &records {
        let original = contexts.get(target).ok_or_else(|| fail("Original native SDK context missing."))?;
        signed_execution::verify_sdk_names(original, record)?;
    }
    if evidence::dependencies(context)? != dependencies { return Err(fail("Signed matrix dependencies changed during collection.")); }
    let report = json!({"schema_version":1,"component":"native-signed-six-context-execution",
        "source_sha":root["source_sha"],"run_id":run,"run_attempt":attempt,"contexts":records,
        "evidence":evidence_records,"execution_matrix_artifact":original.matrix_artifact,
        "original_execution_identity":original.identity,"execution_matrix_sha256":original.matrix_sha256,
        "current_signed_stage_execution_verified":true,
        "semantic_parity_claimed":false,"qualification_verified":false,"retirement_authorized":false,
        "original_evidence_rewritten":false});
    context.write_new(".ci-work/native/signed-execution-matrix.json", &serde_json::to_vec_pretty(&report)
        .map_err(|_| fail("Cannot encode signed matrix receipt."))?)?;
    println!("{}", json!({"component":"native-signed-six-context-execution","contexts":6,
        "current_signed_stage_execution_verified":true,"qualification_verified":false}));
    Ok(())
}
