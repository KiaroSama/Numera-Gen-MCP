use crate::{
    context::{Context, fail, sha},
    distribution, ledger, matrix, signed_execution, test_execution,
};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io};

const REPOSITORY: &str = "KiaroSama/Numera-Gen-MCP";
const REPOSITORY_ID: u64 = 1400814502;
pub const RECEIPT: &str = ".ci-work/native/signing-source.json";

fn hash(value: &Value) -> bool {
    value.as_str().is_some_and(|text| text.len() == 64
        && text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
}

pub fn validate_run(run: &Value, identity: &Value, workflow: u64, completed: bool) -> io::Result<()> {
    let branch = identity["reference"].as_str().and_then(|name| name.strip_prefix("refs/heads/"));
    if run["id"] != identity["run_id"] || run["run_attempt"] != identity["run_attempt"]
        || run["head_sha"] != identity["source_sha"] || run["head_branch"].as_str() != branch
        || run["event"] != "workflow_dispatch" || workflow == 0 || run["workflow_id"] != workflow
        || run["path"] != ".github/workflows/ci.yml" || run["repository"]["id"] != REPOSITORY_ID
        || run["repository"]["full_name"] != REPOSITORY || run["head_repository"]["id"] != REPOSITORY_ID
        || (completed && (run["status"] != "completed" || run["conclusion"] != "success"))
    {
        return Err(fail("Signing source requires the exact successful original workflow/source/attempt."));
    }
    Ok(())
}

pub fn verify_execution(execution: &Value, identity: &Value) -> io::Result<()> {
    if execution["schema_version"] != 1 || execution["component"] != "native-seven-context-execution"
        || execution["source_sha"] != identity["source_sha"] || execution["run_id"] != identity["run_id"]
        || execution["run_attempt"] != identity["run_attempt"] || execution["cases"] != 470
        || execution["suites"] != 50 || execution["current_named_execution_verified"] != true
        || execution["current_assertion_review_verified"] != true || execution["semantic_parity_claimed"] != false
        || execution["qualification_verified"] != false || execution["original_evidence_rewritten"] != false
    {
        return Err(fail("Signing source execution matrix differs or claims unsupported qualification."));
    }
    let contexts = execution["contexts"].as_object().ok_or_else(|| fail("Original contexts missing."))?;
    let identities = contexts.iter().map(|(name, row)|
        (name.clone(), row["reviewed_named_execution"]["identity"].clone())).collect::<BTreeMap<_, _>>();
    matrix::verify_contexts(&identities)?;
    if identities.get("x86_64-unknown-linux-gnu") != Some(identity) {
        return Err(fail("Original matrix source identity differs."));
    }
    for context in contexts.iter().filter(|(target, _)| target.as_str() != "coverage").map(|(_, row)| row) {
        if !hash(&context["tooling_harness_execution"]["sdk_tested_binary_sha256"]) {
            return Err(fail("Original native executed binary identity missing."));
        }
    }
    Ok(())
}

fn verify_job(job: &Value, identity: &Value, name: &str) -> io::Result<()> {
    let branch = identity["reference"].as_str().and_then(|name| name.strip_prefix("refs/heads/"));
    if job["id"].as_u64().is_none_or(|id| id == 0) || job["name"] != name
        || job["run_id"] != identity["run_id"] || job["head_sha"] != identity["source_sha"]
        || job["head_branch"].as_str() != branch || job["status"] != "completed" || job["conclusion"] != "success"
        || (!job["run_attempt"].is_null() && job["run_attempt"] != identity["run_attempt"])
    {
        return Err(fail("Original producing job is not the exact successful source/attempt."));
    }
    Ok(())
}

pub fn verify_intermediate(artifact: &Value, job: &Value, identity: &Value) -> io::Result<()> {
    let target = identity["target"].as_str().ok_or_else(|| fail("Original target missing."))?;
    verify_job(job, identity, &format!("native-{target}"))?;
    let time = |value: &Value| value.as_str().and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
        .ok_or_else(|| fail("Original artifact/job timestamp missing."));
    let created = time(&artifact["created_at"])?;
    let run = &artifact["workflow_run"];
    let branch = identity["reference"].as_str().and_then(|name| name.strip_prefix("refs/heads/"));
    if artifact["id"].as_u64().is_none_or(|id| id == 0)
        || artifact["name"] != format!("native-unsigned-intermediate-{target}") || artifact["expired"] != false
        || artifact["size_in_bytes"].as_u64().is_none_or(|size| size == 0 || size > 2 * crate::archives::MAX_BYTES as u64)
        || !artifact["digest"].as_str().and_then(|text| text.strip_prefix("sha256:"))
            .is_some_and(|digest| hash(&json!(digest)))
        || created < time(&job["started_at"])? || created > time(&job["completed_at"])?
        || run["id"] != identity["run_id"] || run["repository_id"] != REPOSITORY_ID
        || run["head_repository_id"] != REPOSITORY_ID || run["head_sha"] != identity["source_sha"]
        || run["head_branch"].as_str() != branch
    {
        return Err(fail("Original intermediate metadata differs from its successful producing job."));
    }
    Ok(())
}

pub fn package_digest(logs: &[u8], name: &str) -> io::Result<String> {
    if logs.len() > 16 * 1024 * 1024 { return Err(fail("Original package log exceeds bound.")); }
    let text = std::str::from_utf8(logs).map_err(|_| fail("Original package log must be UTF-8."))?;
    let mut found = None;
    for line in text.lines() {
        let Some((timestamp, message)) = line.split_once(' ') else { continue; };
        if chrono::DateTime::parse_from_rfc3339(timestamp).is_err() || !message.starts_with('{') { continue; }
        let Ok(row) = ledger::parse(message.as_bytes()) else { continue; };
        if row["component"] != "native-package" || row["archive"] != name { continue; }
        if row["internal_intermediate"] != true || !hash(&row["sha256"])
            || found.replace(row["sha256"].as_str().unwrap().to_owned()).is_some()
        { return Err(fail("Original package digest is invalid or duplicated.")); }
    }
    found.ok_or_else(|| fail("Original intermediate package digest missing."))
}

pub(crate) struct Original {
    pub identity: Value,
    pub execution: Value,
    pub matrix_artifact: Value,
    pub matrix_sha256: String,
    artifacts: Vec<Value>,
    jobs: Vec<Value>,
}
fn unique_job<'a>(jobs: &'a [Value], name: &str) -> io::Result<&'a Value> {
    let found = jobs.iter().filter(|row| row["name"] == name).collect::<Vec<_>>();
    if found.len() != 1 { return Err(fail("Original required job missing or duplicated.")); }
    Ok(found[0])
}
fn positive(value: &str) -> io::Result<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(fail("Explicit original run/attempt must be positive decimal integers."));
    }
    value.parse::<u64>().ok().filter(|value| *value > 0)
        .ok_or_else(|| fail("Explicit original run/attempt must be positive decimal integers."))
}

pub(crate) fn load(context: &Context) -> io::Result<Original> {
    context.hosted()?;
    crate::acceptance::public_environment(context)?;
    if std::env::var("GITHUB_REF").as_deref() != Ok("refs/heads/main")
        || !matches!(std::env::var("GITHUB_JOB").as_deref(), Ok("signing" | "signed-execution-matrix"))
    { return Err(fail("Signing source authentication belongs to approved main jobs.")); }
    let mut environment = ["GITHUB_REPOSITORY", "GITHUB_EVENT_NAME", "GITHUB_SHA", "GITHUB_REF",
        "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"].into_iter().map(|key|
            (key.to_owned(), std::env::var(key).map(Value::String).unwrap_or(Value::Null)))
        .collect::<serde_json::Map<_, _>>();
    environment.insert("GITHUB_JOB".into(), json!("signing"));
    let mut environment = Value::Object(environment);
    let current = test_execution::execution_identity(&environment, "extracted-signed", "x86_64-unknown-linux-gnu")?;
    if current["source_sha"] != context.source_sha()? { return Err(fail("Current signing source differs.")); }
    let current_run = current["run_id"].as_u64().ok_or_else(|| fail("Current run missing."))?;
    let current_attempt = current["run_attempt"].as_u64().ok_or_else(|| fail("Current attempt missing."))?;
    let operation = std::env::var("NUMERA_CI_OPERATION").unwrap_or_default();
    let (run, attempt) = match operation.as_str() {
        "qualify" => (current_run, current_attempt),
        "sign-qualified" => {
            let run = positive(&std::env::var("NUMERA_QUALIFICATION_RUN_ID").unwrap_or_default())?;
            let attempt = positive(&std::env::var("NUMERA_QUALIFICATION_RUN_ATTEMPT").unwrap_or_default())?;
            if run == current_run { return Err(fail("Signing promotion requires a distinct original completed run.")); }
            (run, attempt)
        }
        _ => return Err(fail("Operation cannot authenticate signing inputs.")),
    };
    let get = |run: u64, attempt: u64| ledger::parse(&matrix::api(context,
        &format!("repos/{REPOSITORY}/actions/runs/{run}/attempts/{attempt}"))?);
    let active = get(current_run, current_attempt)?;
    let workflow = active["workflow_id"].as_u64().filter(|id| *id > 0)
        .ok_or_else(|| fail("Current workflow identity missing."))?;
    validate_run(&active, &current, workflow, false)?;
    let original = if run == current_run { active } else { get(run, attempt)? };
    environment["GITHUB_JOB"] = json!("native");
    environment["GITHUB_REF"] = json!(format!("refs/heads/{}", original["head_branch"].as_str().unwrap_or("")));
    environment["GITHUB_RUN_ID"] = json!(run.to_string());
    environment["GITHUB_RUN_ATTEMPT"] = json!(attempt.to_string());
    let identity = test_execution::execution_identity(&environment, "built", "x86_64-unknown-linux-gnu")?;
    validate_run(&original, &identity, workflow, run != current_run)?;
    let jobs = matrix::pages(context, &format!("repos/{REPOSITORY}/actions/runs/{run}/attempts/{attempt}/jobs"), "jobs")?;
    for (label, (target, phase)) in matrix::contexts() {
        environment["GITHUB_JOB"] = json!(if phase == "coverage" { "native-coverage" } else { "native" });
        let identity = test_execution::execution_identity(&environment, phase, target)?;
        let name = if label == "coverage" { "native-coverage".to_owned() } else { format!("native-{target}") };
        verify_job(unique_job(&jobs, &name)?, &identity, &name)?;
    }
    let job = unique_job(&jobs, "execution-matrix")?;
    verify_job(job, &identity, "execution-matrix")?;
    let artifacts = matrix::pages(context, &format!("repos/{REPOSITORY}/actions/runs/{run}/artifacts"), "artifacts")?;
    let (entries, matrix_artifact) = matrix::artifact(context, &artifacts, &identity,
        "native-execution-matrix", &BTreeSet::from(["execution-matrix.json".into()]))?;
    let bytes = &entries["execution-matrix.json"];
    let execution = ledger::parse(bytes)?;
    verify_execution(&execution, &identity)?;
    let logs = matrix::api(context, &format!("repos/{REPOSITORY}/actions/jobs/{}/logs", job["id"]))?;
    signed_execution::verify_logs(&logs, &identity, &[("native-seven-context-execution", sha(bytes))])?;
    Ok(Original { identity, execution, matrix_artifact, matrix_sha256:sha(bytes), artifacts, jobs })
}

pub(crate) fn receipt(context: &Context, original: &Original, identity: &Value) -> io::Result<Value> {
    let target = identity["target"].as_str().ok_or_else(|| fail("Signing target missing."))?;
    let mut source_identity = original.identity.clone();
    source_identity["target"] = identity["target"].clone();
    source_identity["platform"] = identity["platform"].clone();
    let name = format!("native-unsigned-intermediate-{target}");
    let found = original.artifacts.iter().filter(|row| row["name"] == name).collect::<Vec<_>>();
    if found.len() != 1 { return Err(fail("Original intermediate missing or duplicated.")); }
    let artifact = found[0];
    let job = unique_job(&original.jobs, &format!("native-{target}"))?;
    verify_intermediate(artifact, job, &source_identity)?;
    let logs = matrix::api(context, &format!("repos/{REPOSITORY}/actions/jobs/{}/logs", job["id"]))?;
    let archive = format!("{}.{}", distribution::prefix(target, true)?,
        if target.contains("windows") { "zip" } else { "tar.gz" });
    let digest = package_digest(&logs, &archive)?;
    let binary = &original.execution["contexts"][target]["tooling_harness_execution"]["sdk_tested_binary_sha256"];
    if !hash(binary) { return Err(fail("Original extracted intermediate binary hash missing.")); }
    Ok(json!({"schema_version":1,"component":"native-authenticated-signing-source",
        "identity":identity,"original_identity":source_identity,"artifact":artifact,
        "matrix_artifact":original.matrix_artifact,"matrix_sha256":original.matrix_sha256,
        "original_job_id":job["id"],"original_log_sha256":sha(&logs),
        "archive":archive,"archive_sha256":digest,"unsigned_binary_sha256":binary,
        "qualification_verified":false,"retirement_authorized":false}))
}

pub fn check(context: &Context, target: &str) -> io::Result<()> {
    distribution::target(target)?;
    if std::env::var("GITHUB_JOB").as_deref() != Ok("signing") {
        return Err(fail("Only the native signing job can select its source artifact."));
    }
    let original = load(context)?;
    let identity = test_execution::hosted_identity(context, "extracted-signed")?;
    if identity["target"] != target { return Err(fail("Signing source requires its actual native target.")); }
    let receipt = receipt(context, &original, &identity)?;
    context.write_new(RECEIPT, &serde_json::to_vec_pretty(&receipt).map_err(|_| fail("Cannot encode signing source."))?)?;
    println!("{}", json!({"component":"native-authenticated-signing-source","identity":identity,
        "report_sha256":sha(&ledger::read(&context.root, RECEIPT)?)}));
    let output = std::env::var_os("GITHUB_OUTPUT").ok_or_else(|| fail("Hosted step output missing."))?;
    let output = std::path::Path::new(&output).canonicalize()?;
    let runner_temp = std::env::var_os("RUNNER_TEMP").ok_or_else(|| fail("Hosted step output root missing."))?;
    if !output.starts_with(std::path::Path::new(&runner_temp).canonicalize()?) {
        return Err(fail("Hosted step output escapes runner command storage."));
    }
    use std::io::Write;
    writeln!(std::fs::OpenOptions::new().append(true).open(output)?, "artifact-id={}\nrun-id={}",
        receipt["artifact"]["id"], original.identity["run_id"])?;
    Ok(())
}

pub(crate) fn selected(context: &Context, target: &str) -> io::Result<Value> {
    let receipt = ledger::parse(&ledger::read(&context.root, RECEIPT)?)?;
    let identity = test_execution::hosted_identity(context, "extracted-signed")?;
    if identity["target"] != target || receipt["schema_version"] != 1
        || receipt["component"] != "native-authenticated-signing-source" || receipt["identity"] != identity
        || !hash(&receipt["archive_sha256"]) || !hash(&receipt["unsigned_binary_sha256"])
        || receipt["qualification_verified"] != false || receipt["retirement_authorized"] != false
    { return Err(fail("Authenticated original signing source receipt differs.")); }
    Ok(receipt)
}

pub(crate) fn verify_prepared(context: &Context, target: &str) -> io::Result<()> {
    let receipt = selected(context, target)?;
    let source = context.source_sha()?;
    let entries = distribution::read_tree(&context.relative(&format!(".ci-work/prepared/{target}"))?,
        target, &source)?;
    let name = receipt["archive"].as_str().ok_or_else(|| fail("Original archive name missing."))?;
    let download = context.relative(".ci-work/signing-download")?;
    let expected = format!("{}.{}", distribution::prefix(target, true)?,
        if target.contains("windows") { "zip" } else { "tar.gz" });
    if name != expected {
        return Err(fail("Original signing archive filename differs."));
    }
    let bytes = distribution::read_regular(&download.join(name), crate::archives::MAX_BYTES)?;
    let checksum = distribution::read_regular(&download.join(format!("{name}.sha256")), 1024)?;
    let original = distribution::authenticate_archive(&bytes, &checksum, target, &source, true)?;
    let suffix = if target.contains("windows") { ".exe" } else { "" };
    if entries != original || receipt["archive_sha256"] != sha(&bytes)
        || receipt["unsigned_binary_sha256"] != sha(&entries[&format!("bin/numera-gen-mcp{suffix}")])
    {
        return Err(fail("Prepared signing tree is not the original executed intermediate."));
    }
    Ok(())
}
