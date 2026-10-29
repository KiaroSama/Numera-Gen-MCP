use crate::{context::{Context, fail, sha}, evidence, fixtures::{BASELINE, INPUTS, SOURCE}, ledger, qualification, test_execution};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io::{self, Cursor, Read}};

const LIMIT: usize = 16 * 1024 * 1024;
const REPOSITORY: &str = "KiaroSama/Numera-Gen-MCP";
const REPOSITORY_ID: u64 = 1400814502;

pub fn contexts() -> BTreeMap<&'static str, (&'static str, &'static str)> {
    BTreeMap::from([
        ("x86_64-pc-windows-msvc", ("x86_64-pc-windows-msvc", "built")),
        ("aarch64-pc-windows-msvc", ("aarch64-pc-windows-msvc", "built")),
        ("x86_64-unknown-linux-gnu", ("x86_64-unknown-linux-gnu", "built")),
        ("aarch64-unknown-linux-gnu", ("aarch64-unknown-linux-gnu", "built")),
        ("x86_64-apple-darwin", ("x86_64-apple-darwin", "built")),
        ("aarch64-apple-darwin", ("aarch64-apple-darwin", "built")),
        ("coverage", ("x86_64-unknown-linux-gnu", "coverage")),
    ])
}
pub fn verify_contexts(observed: &BTreeMap<String, Value>) -> io::Result<()> {
    let expected = contexts();
    if observed.len() != expected.len() { return Err(fail("Exact six-native plus coverage execution matrix is incomplete.")); }
    let root = observed.get("x86_64-unknown-linux-gnu").ok_or_else(|| fail("Native matrix source context missing."))?;
    let number = |key: &str| root[key].as_u64().filter(|value| *value > 0).map(|value| value.to_string())
        .ok_or_else(|| fail("Native matrix run and attempt must be positive identities."));
    let mut environment = json!({"GITHUB_REPOSITORY":root["repository"],"GITHUB_EVENT_NAME":root["event"],
        "GITHUB_SHA":root["source_sha"],"GITHUB_REF":root["reference"],
        "GITHUB_RUN_ID":number("run_id")?,"GITHUB_RUN_ATTEMPT":number("run_attempt")?});
    for (label, (target, phase)) in expected {
        environment["GITHUB_JOB"] = json!(if phase == "coverage" { "native-coverage" } else { "native" });
        let identity = test_execution::execution_identity(&environment, phase, target)?;
        if observed.get(label) != Some(&identity) { return Err(fail("Native matrix contexts differ in source, run, attempt or supported identity.")); }
    }
    Ok(())
}
fn decode(bytes: &[u8], expected: &BTreeSet<String>) -> io::Result<BTreeMap<String, Vec<u8>>> {
    if bytes.is_empty() || bytes.len() > LIMIT || expected.is_empty() || expected.len() > 32 {
        return Err(fail("Execution artifact exceeds its inventory or byte bounds."));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| fail("Invalid execution artifact ZIP."))?;
    if archive.len() != expected.len() { return Err(fail("Execution artifact inventory differs.")); }
    let mut entries = BTreeMap::new(); let mut total = 0usize;
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|_| fail("Unreadable execution artifact member."))?;
        let mode = file.unix_mode().unwrap_or(0) & 0o170000;
        if !file.is_file() || !matches!(mode, 0 | 0o100000) || !expected.contains(file.name())
            || file.size() > LIMIT as u64 || entries.contains_key(file.name()) {
            return Err(fail("Unexpected, linked or oversized execution artifact member."));
        }
        total = total.checked_add(file.size() as usize).filter(|total| *total <= 2 * LIMIT)
            .ok_or_else(|| fail("Execution artifact expanded bytes exceed bound."))?;
        let name = file.name().to_owned(); let size = file.size(); let mut data = Vec::new();
        file.take(size + 1).read_to_end(&mut data)?;
        if data.len() as u64 != size { return Err(fail("Truncated execution artifact member.")); }
        entries.insert(name, data);
    }
    Ok(entries)
}
fn authenticate_artifact(metadata: &Value, bytes: &[u8], identity: &Value, name: &str) -> io::Result<()> {
    let run = &metadata["workflow_run"];
    let branch = identity["reference"].as_str().and_then(|value| value.strip_prefix("refs/heads/"))
        .ok_or_else(|| fail("Artifact branch identity missing."))?;
    if bytes.is_empty() || bytes.len() > LIMIT
        || metadata["id"].as_u64().is_none_or(|id| id == 0) || metadata["name"] != name || metadata["expired"] != false
        || metadata["size_in_bytes"].as_u64() != Some(bytes.len() as u64)
        || metadata["digest"] != format!("sha256:{}", sha(bytes)) || run["id"] != identity["run_id"]
        || run["repository_id"] != REPOSITORY_ID || run["head_repository_id"] != REPOSITORY_ID
        || run["head_sha"] != identity["source_sha"] || run["head_branch"] != branch {
        return Err(fail("Artifact repository, source, run, branch, size or digest differs."));
    }
    Ok(())
}
pub fn artifact_entries(metadata: &Value, bytes: &[u8], identity: &Value, name: &str,
    expected: &BTreeSet<String>) -> io::Result<BTreeMap<String, Vec<u8>>> {
    authenticate_artifact(metadata, bytes, identity, name)?;
    decode(bytes, expected)
}
pub fn verify_job(job: &Value, logs: &[u8], identity: &Value, digest: &str) -> io::Result<()> {
    let name = if identity["phase"] == "coverage" { "native-coverage".to_owned() }
        else { format!("native-{}", identity["target"].as_str().unwrap_or("")) };
    let branch = identity["reference"].as_str().and_then(|value| value.strip_prefix("refs/heads/"));
    if job["id"].as_u64().is_none_or(|id| id == 0) || job["name"] != name || job["run_id"] != identity["run_id"]
        || job["head_sha"] != identity["source_sha"] || job["head_branch"].as_str() != branch
        || job["status"] != "completed" || job["conclusion"] != "success" || logs.len() > LIMIT {
        return Err(fail("Native context job is not an exact successful source/run."));
    }
    let text = std::str::from_utf8(logs).map_err(|_| fail("Job log must be UTF-8."))?;
    let mut found = 0usize;
    for line in text.lines() {
        let Some((timestamp, message)) = line.split_once(' ') else { continue; };
        if chrono::DateTime::parse_from_rfc3339(timestamp).is_err() || !message.starts_with('{') { continue; }
        let Ok(row) = ledger::parse(message.as_bytes()) else { continue; };
        if row["component"] != "native-current-named-case-execution" { continue; }
        if row["identity"] != *identity || row["report_sha256"] != digest || row["cases"] != 470 || row["suites"] != 50
            || row["current_named_execution_verified"] != true || row["semantic_parity_claimed"] != false
            || row["qualification_verified"] != false {
            return Err(fail("Named-case report log binding differs."));
        }
        found += 1;
    }
    if found != 1 { return Err(fail("Exactly one original named-case report digest is required in its job log.")); }
    Ok(())
}
fn api(context: &Context, endpoint: &str) -> io::Result<Vec<u8>> {
    let mut command = context.command("gh");
    command.args(["api", "--hostname", "github.com", "--method", "GET", "-H", "Accept: application/vnd.github+json",
        "-H", "X-GitHub-Api-Version: 2026-03-10", endpoint])
        .env("GH_PROMPT_DISABLED", "1").env("GH_PAGER", "cat").env_remove("GH_DEBUG");
    if endpoint.ends_with("/zip") || endpoint.ends_with("/logs") { command.arg("--allow-escape-sequences"); }
    let bytes = context.capture(&mut command, 30000, 15000, LIMIT)?;
    println!("{}", json!({"component":"native-matrix-collection","received_bytes":bytes.len()}));
    Ok(bytes)
}
fn pages(context: &Context, endpoint: &str, key: &str) -> io::Result<Vec<Value>> {
    let mut result = Vec::new(); let mut total = None;
    for page in 1..=10 {
        let value = ledger::parse(&api(context, &format!("{endpoint}?per_page=100&page={page}"))?)?;
        let count = value["total_count"].as_u64().filter(|count| *count <= 1000)
            .ok_or_else(|| fail("GitHub evidence inventory exceeds its bound."))?;
        if total.replace(count).is_some_and(|previous| previous != count) { return Err(fail("GitHub evidence inventory changed during pagination.")); }
        let rows = value[key].as_array().filter(|rows| rows.len() <= 100)
            .ok_or_else(|| fail("GitHub evidence page inventory missing."))?;
        result.extend(rows.iter().cloned());
        if result.len() as u64 == count { return Ok(result); }
        if result.len() as u64 > count || rows.len() < 100 { break; }
    }
    Err(fail("GitHub evidence inventory is truncated or incomplete."))
}
fn artifact(context: &Context, artifacts: &[Value], identity: &Value, name: &str,
    expected: &BTreeSet<String>) -> io::Result<(BTreeMap<String, Vec<u8>>, Value)> {
    let found = artifacts.iter().filter(|row| row["name"] == name).collect::<Vec<_>>();
    if found.len() != 1 { return Err(fail("Artifact context is missing or duplicated.")); }
    let row = found[0]; let id = row["id"].as_u64().filter(|id| *id > 0).ok_or_else(|| fail("Artifact ID missing."))?;
    let bytes = api(context, &format!("repos/{REPOSITORY}/actions/artifacts/{id}/zip"))?;
    let entries = artifact_entries(row, &bytes, identity, name, expected)?;
    Ok((entries, json!({"id":id,"name":name,"sha256":sha(&bytes),"bytes":bytes.len()})))
}
fn proposal_members(bytes: &[u8], coverage: bool) -> io::Result<BTreeSet<String>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| fail("Invalid execution ZIP inventory."))?;
    if archive.len() > 5 { return Err(fail("Execution ZIP inventory exceeds bound.")); }
    let mut proposals = Vec::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|_| fail("Unreadable execution ZIP inventory."))?;
        if let Some(id) = file.name().strip_prefix("current-source-case-rebind-").and_then(|name| name.strip_suffix(".json"))
            && uuid::Uuid::parse_str(id).is_ok() { proposals.push(file.name().to_owned()); }
    }
    if proposals.len() != 1 { return Err(fail("Exactly one explicit source-rebind artifact is required.")); }
    let mut expected = BTreeSet::from(["test-execution.json".into(), "current-case-execution.json".into(), proposals.remove(0)]);
    if !coverage {
        expected.insert("sdk-execution-release.json".into()); expected.insert("sdk-execution-extracted-intermediate.json".into());
    }
    Ok(expected)
}

pub fn check(context: &Context) -> io::Result<()> {
    context.hosted()?;
    if std::env::var("GITHUB_JOB").as_deref() != Ok("execution-matrix") { return Err(fail("Execution matrix belongs to its dedicated hosted job.")); }
    let environment = ["GITHUB_REPOSITORY", "GITHUB_EVENT_NAME", "GITHUB_SHA", "GITHUB_REF", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"]
        .into_iter().map(|key| (key.to_owned(), std::env::var(key).map(Value::String).unwrap_or(Value::Null))).collect::<serde_json::Map<_, _>>();
    let mut environment = Value::Object(environment); environment["GITHUB_JOB"] = json!("native");
    let root_identity = test_execution::execution_identity(&environment, "built", "x86_64-unknown-linux-gnu")?;
    let run_id = root_identity["run_id"].as_u64().ok_or_else(|| fail("Matrix run ID missing."))?;
    let attempt = root_identity["run_attempt"].as_u64().ok_or_else(|| fail("Matrix attempt missing."))?;
    let run = ledger::parse(&api(context, &format!("repos/{REPOSITORY}/actions/runs/{run_id}/attempts/{attempt}"))?)?;
    if run["id"] != run_id || run["run_attempt"] != attempt || run["event"] != "workflow_dispatch"
        || run["head_sha"] != context.source_sha()? || run["repository"]["id"] != REPOSITORY_ID
        || run["repository"]["full_name"] != REPOSITORY || run["head_repository"]["id"] != REPOSITORY_ID
        || run["head_branch"].as_str() != root_identity["reference"].as_str().and_then(|value| value.strip_prefix("refs/heads/")) {
        return Err(fail("Matrix source run/attempt/repository identity differs."));
    }
    let artifacts = pages(context, &format!("repos/{REPOSITORY}/actions/runs/{run_id}/artifacts"), "artifacts")?;
    let jobs = pages(context, &format!("repos/{REPOSITORY}/actions/runs/{run_id}/attempts/{attempt}/jobs"), "jobs")?;
    let dependencies = evidence::dependencies(context)?;
    let historical = crate::ledger_inputs::check(context)?;
    let proposal_bytes = ledger::read(&context.root, historical["current_source_rebind"].as_str()
        .ok_or_else(|| fail("Matrix source proposals missing."))?)?;
    if historical["rebind_sha256"] != sha(&proposal_bytes) { return Err(fail("Matrix source proposal bytes changed.")); }
    let expected_rebind = ledger::parse(&proposal_bytes)?;
    let metadata = crate::ci::suite_metadata(context)?;
    let mut identities = BTreeMap::new(); let mut records = BTreeMap::new();
    for (label, (target, phase)) in contexts() {
        environment["GITHUB_JOB"] = json!(if phase == "coverage" { "native-coverage" } else { "native" });
        let identity = test_execution::execution_identity(&environment, phase, target)?;
        let execution_name = format!("native-execution-{label}");
        let matched = artifacts.iter().filter(|row| row["name"] == execution_name).collect::<Vec<_>>();
        if matched.len() != 1 { return Err(fail("Native execution artifact missing or duplicated.")); }
        let id = matched[0]["id"].as_u64().filter(|id| *id > 0).ok_or_else(|| fail("Native execution artifact ID missing."))?;
        let raw = api(context, &format!("repos/{REPOSITORY}/actions/artifacts/{id}/zip"))?;
        authenticate_artifact(matched[0], &raw, &identity, &execution_name)?;
        let members = proposal_members(&raw, phase == "coverage")?;
        let execution = artifact_entries(matched[0], &raw, &identity, &execution_name, &members)?;
        let proposal = execution.iter().find(|(name, _)| name.starts_with("current-source-case-rebind-"))
            .ok_or_else(|| fail("Source rebind member missing."))?;
        let rebind = ledger::parse(proposal.1)?;
        if rebind != expected_rebind { return Err(fail("Matrix context differs from authenticated current case proposals.")); }
        let oracle_name = format!("native-oracle-{label}");
        let (oracle, oracle_artifact) = artifact(context, &artifacts, &identity, &oracle_name, &BTreeSet::from(["oracle-evidence.zip".into()]))?;
        let provenance_bytes = &oracle["oracle-evidence.zip"];
        let payload_names = INPUTS.iter().map(|name| (*name).to_owned()).chain(["provenance.json".into()]).collect();
        let mut payloads = decode(provenance_bytes, &payload_names)?;
        let manifest = ledger::parse(&payloads.remove("provenance.json").ok_or_else(|| fail("Oracle provenance missing."))?)?;
        let os = if target.contains("windows") { "windows" } else if target.contains("apple") { "macos" } else { "linux" };
        let arch = if target.starts_with("aarch64") { "aarch64" } else { "x86_64" };
        evidence::verify_platform_bundle(provenance_bytes, &context.source_sha()?, &dependencies, &manifest["materialization"], &payloads, os, arch)?;
        crate::fixtures::verify_materialized(&context.root, os, &context.source_sha()?, &manifest["materialization"], &payloads)?;
        let selected: &[&str] = if phase == "coverage" { &["numera-gen-mcp"] } else { &["numera-gen-mcp", "numera-task"] };
        let sources = test_execution::selected_sources(&context.root, &metadata, selected)?;
        let mut reports = BTreeMap::from([(phase.to_owned(), execution["test-execution.json"].clone())]);
        if phase != "coverage" { for phase in ["release", "extracted-intermediate"] {
            reports.insert(phase.into(), execution[&format!("sdk-execution-{phase}.json")].clone());
        } }
        let mut reconstructed = qualification::bind_cases(&rebind, &identity, &sha(provenance_bytes), &dependencies, &sources, &reports)?;
        reconstructed["source_rebind_sha256"] = json!(sha(proposal.1));
        let case_bytes = &execution["current-case-execution.json"];
        if ledger::parse(case_bytes)? != reconstructed { return Err(fail("Matrix named-case report differs from original phase results.")); }
        let job_name = if phase == "coverage" { "native-coverage".to_owned() } else { format!("native-{target}") };
        let found = jobs.iter().filter(|job| job["name"] == job_name).collect::<Vec<_>>();
        if found.len() != 1 { return Err(fail("Exact native context job missing or duplicated.")); }
        let job_id = found[0]["id"].as_u64().filter(|id| *id > 0).ok_or_else(|| fail("Native context job ID missing."))?;
        let logs = api(context, &format!("repos/{REPOSITORY}/actions/jobs/{job_id}/logs"))?;
        verify_job(found[0], &logs, &identity, &sha(case_bytes))?;
        records.insert(label, json!({"execution_artifact_id":id,"execution_artifact_sha256":sha(&raw),
            "oracle_artifact":oracle_artifact,"job_id":job_id,"log_sha256":sha(&logs),"case_report_sha256":sha(case_bytes)}));
        identities.insert(label.to_owned(), identity);
    }
    verify_contexts(&identities)?;
    if evidence::dependencies(context)? != dependencies { return Err(fail("Matrix source dependencies changed during collection.")); }
    let report = json!({"schema_version":1,"component":"native-seven-context-execution","source_sha":context.source_sha()?,
        "run_id":run_id,"run_attempt":attempt,"baseline":BASELINE,"historical_source_sha":SOURCE,
        "contexts":records,"cases":470,"suites":50,"current_named_execution_verified":true,
        "semantic_parity_claimed":false,"qualification_verified":false,"original_evidence_rewritten":false});
    context.write_new(".ci-work/native/execution-matrix.json", &serde_json::to_vec_pretty(&report).map_err(|_| fail("Cannot encode matrix evidence."))?)?;
    println!("{}", json!({"component":"native-seven-context-execution","contexts":7,"cases":470,
        "current_named_execution_verified":true,"semantic_parity_claimed":false,"qualification_verified":false}));
    Ok(())
}
