use crate::{
    context::{Context, fail, sha},
    inventory, ledger, matrix, promotion, signed_execution, signed_matrix, test_execution,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    process::Command,
};

pub type Tree = BTreeMap<String, String>;
const REPOSITORY: &str = "KiaroSama/Numera-Gen-MCP";
const LIMIT: usize = 16 * 1024 * 1024;

fn hexadecimal(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn parse_tree(bytes: &[u8]) -> io::Result<Tree> {
    if bytes.is_empty() || bytes.len() > LIMIT || bytes.last() != Some(&0) {
        return Err(fail("Retirement tree inventory is missing or truncated."));
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| fail("Retirement tree paths must be UTF-8."))?;
    let mut tree = Tree::new();
    for member in text[..text.len() - 1].split('\0') {
        let (object, path) = member
            .split_once('\t')
            .ok_or_else(|| fail("Invalid retirement tree record."))?;
        let fields = object.split(' ').collect::<Vec<_>>();
        if fields.len() != 3
            || !hexadecimal(fields[2], 40)
            || !matches!(
                (fields[0], fields[1]),
                ("100644" | "100755" | "120000", "blob") | ("160000", "commit")
            )
            || path.len() > 1024
            || path
                .bytes()
                .any(|byte| byte == b'\\' || byte == b':' || byte.is_ascii_control())
            || path.split('/').any(|part| matches!(part, "" | "." | ".."))
            || tree.len() >= 10000
            || tree.insert(path.to_owned(), object.to_owned()).is_some()
        {
            return Err(fail(
                "Retirement tree contains invalid or duplicate members.",
            ));
        }
    }
    Ok(tree)
}

/// Q qualification only permits this source transition; R still requires its own complete run.
pub fn verify_transition(
    manifest: &Value,
    qualified: &str,
    current: &str,
    parents: &[String],
    before: &Tree,
    after: &Tree,
) -> io::Result<()> {
    inventory::tooling_hashes(manifest)?;
    if !hexadecimal(qualified, 40)
        || !hexadecimal(current, 40)
        || qualified == current
        || parents != [qualified]
    {
        return Err(fail("Retirement requires one exact qualified parent."));
    }
    let mut expected = before.clone();
    for row in manifest["entries"].as_array().unwrap() {
        let path = row["source"].as_str().unwrap();
        if !expected.remove(path).is_some_and(|object| {
            object.starts_with("100644 blob ") || object.starts_with("100755 blob ")
        }) {
            return Err(fail(
                "Qualified parent lacks an original regular tooling file.",
            ));
        }
    }
    if expected != *after
        || !after.get("native/main.rs").is_some_and(|object| {
            object.starts_with("100644 blob ") || object.starts_with("100755 blob ")
        })
    {
        return Err(fail(
            "Retirement must delete exactly 51 originals and preserve the current Rust tree.",
        ));
    }
    Ok(())
}

/// Metadata, original collector log and its exact ZIP authenticate this existing qualification.
pub fn authenticate_qualification(
    metadata: &Value,
    bytes: &[u8],
    job: &Value,
    logs: &[u8],
    identity: &Value,
) -> io::Result<Value> {
    let entries = matrix::artifact_entries(
        metadata,
        bytes,
        identity,
        "native-signed-execution-matrix",
        &BTreeSet::from([
            "current-qualification.json".into(),
            "signed-execution-matrix.json".into(),
        ]),
    )?;
    let branch = identity["reference"]
        .as_str()
        .and_then(|name| name.strip_prefix("refs/heads/"));
    let time = |value: &Value| {
        value
            .as_str()
            .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
            .ok_or_else(|| fail("Qualified collector timestamps are missing."))
    };
    let created = time(&metadata["created_at"])?;
    let environment = json!({"GITHUB_REPOSITORY":identity["repository"],"GITHUB_EVENT_NAME":identity["event"],
        "GITHUB_SHA":identity["source_sha"],"GITHUB_REF":identity["reference"],"GITHUB_JOB":"signing",
        "GITHUB_RUN_ID":identity["run_id"].as_u64().unwrap_or(0).to_string(),
        "GITHUB_RUN_ATTEMPT":identity["run_attempt"].as_u64().unwrap_or(0).to_string()});
    if *identity
        != test_execution::execution_identity(
            &environment,
            "extracted-signed",
            "x86_64-unknown-linux-gnu",
        )?
    {
        return Err(fail(
            "Qualified collector identity differs from the exact supported main context.",
        ));
    }
    if job["id"].as_u64().is_none_or(|id| id == 0)
        || job["name"] != "signed-execution-matrix"
        || job["run_id"] != identity["run_id"]
        || job["head_sha"] != identity["source_sha"]
        || job["head_branch"].as_str() != branch
        || job["status"] != "completed"
        || job["conclusion"] != "success"
        || (!job["run_attempt"].is_null() && job["run_attempt"] != identity["run_attempt"])
        || created < time(&job["started_at"])?
        || created > time(&job["completed_at"])?
    {
        return Err(fail(
            "Qualification artifact is not from its exact successful collector attempt.",
        ));
    }
    let qualification_bytes = &entries["current-qualification.json"];
    signed_execution::verify_logs(
        logs,
        identity,
        &[("native-current-qualification", sha(qualification_bytes))],
    )?;
    let qualification = ledger::parse(qualification_bytes)?;
    let signed = ledger::parse(&entries["signed-execution-matrix.json"])?;
    let records = signed["contexts"]
        .as_object()
        .ok_or_else(|| fail("Qualified signed contexts missing."))?
        .iter()
        .map(|(target, record)| (target.clone(), record.clone()))
        .collect::<BTreeMap<_, _>>();
    signed_matrix::verify_contexts(&records, identity)?;
    if signed["schema_version"] != 1
        || signed["component"] != "native-signed-six-context-execution"
        || signed["source_sha"] != identity["source_sha"]
        || signed["run_id"] != identity["run_id"]
        || signed["run_attempt"] != identity["run_attempt"]
        || signed["current_signed_stage_execution_verified"] != true
        || signed["semantic_parity_claimed"] != false
        || signed["qualification_verified"] != false
        || signed["retirement_authorized"] != false
        || signed["original_evidence_rewritten"] != false
        || signed["original_execution_identity"]["source_sha"] != identity["source_sha"]
        || qualification["qualification_run_id"] != signed["original_execution_identity"]["run_id"]
        || qualification["qualification_run_attempt"]
            != signed["original_execution_identity"]["run_attempt"]
        || qualification["schema_version"] != 1
        || qualification["component"] != "native-current-qualification"
        || qualification["source_sha"] != identity["source_sha"]
        || qualification["signing_identity"] != *identity
        || qualification["qualification_run_id"]
            .as_u64()
            .is_none_or(|id| id == 0)
        || qualification["qualification_run_attempt"]
            .as_u64()
            .is_none_or(|id| id == 0)
        || qualification["runtime_cases"] != 470
        || qualification["runtime_suites"] != 50
        || qualification["native_contexts"] != 6
        || qualification["coverage_contexts"] != 1
        || qualification["signed_contexts"] != 6
        || qualification["reviewer"] != "primary"
        || qualification["independent_review_claimed"] != false
        || qualification["qualification_verified"] != true
        || qualification["retirement_authorized"] != false
        || qualification["owner_host_changed"] != false
        || qualification["original_evidence_rewritten"] != false
        || ["source_review_sha256", "tooling_source_review_sha256"]
            .iter()
            .any(|key| {
                qualification[*key]
                    .as_str()
                    .is_none_or(|text| !hexadecimal(text, 64))
            })
        || qualification["coverage"]["report_sha256"]
            .as_str()
            .is_none_or(|text| !hexadecimal(text, 64))
    {
        return Err(fail(
            "Authenticated qualification is incomplete or belongs to another source.",
        ));
    }
    let metrics = qualification["coverage"]["metrics"]
        .as_array()
        .ok_or_else(|| fail("Qualified coverage missing."))?;
    inventory::verify_coverage_metrics(metrics)?;
    Ok(qualification)
}

fn git(context: &Context) -> Command {
    let mut command = context.command("git");
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| key.starts_with("GIT_")) {
            command.env_remove(key);
        }
    }
    command
        .args([
            "--no-pager",
            "--no-replace-objects",
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
        ])
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    command
}

fn read_tree(context: &Context, revision: &str) -> io::Result<Tree> {
    if !hexadecimal(revision, 40) {
        return Err(fail("Exact retirement revision required."));
    }
    parse_tree(&context.capture(
        git(context).args(["ls-tree", "-r", "-z", "--full-tree", revision]),
        30000,
        15000,
        LIMIT,
    )?)
}

fn remote_qualification(
    context: &Context,
    run: u64,
    attempt: u64,
    source: &str,
    workflow: u64,
    completed: bool,
) -> io::Result<Value> {
    let identity = test_execution::execution_identity(
        &json!({"GITHUB_REPOSITORY":REPOSITORY,
        "GITHUB_EVENT_NAME":"workflow_dispatch","GITHUB_SHA":source,"GITHUB_REF":"refs/heads/main",
        "GITHUB_JOB":"signing","GITHUB_RUN_ID":run.to_string(),"GITHUB_RUN_ATTEMPT":attempt.to_string()}),
        "extracted-signed",
        "x86_64-unknown-linux-gnu",
    )?;
    let metadata = ledger::parse(&matrix::api(
        context,
        &format!("repos/{REPOSITORY}/actions/runs/{run}/attempts/{attempt}"),
    )?)?;
    promotion::validate_run(&metadata, &identity, workflow, completed)?;
    let artifacts = matrix::pages(
        context,
        &format!("repos/{REPOSITORY}/actions/runs/{run}/artifacts"),
        "artifacts",
    )?;
    let jobs = matrix::pages(
        context,
        &format!("repos/{REPOSITORY}/actions/runs/{run}/attempts/{attempt}/jobs"),
        "jobs",
    )?;
    let unique = |rows: Vec<&Value>| -> io::Result<Value> {
        if rows.len() != 1 {
            return Err(fail("Qualified collector evidence missing or duplicated."));
        }
        Ok(rows[0].clone())
    };
    let artifact = unique(
        artifacts
            .iter()
            .filter(|row| row["name"] == "native-signed-execution-matrix")
            .collect(),
    )?;
    let job = unique(
        jobs.iter()
            .filter(|row| row["name"] == "signed-execution-matrix")
            .collect(),
    )?;
    let id = artifact["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or_else(|| fail("Qualified artifact ID missing."))?;
    let job_id = job["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or_else(|| fail("Qualified job ID missing."))?;
    let bytes = matrix::api(
        context,
        &format!("repos/{REPOSITORY}/actions/artifacts/{id}/zip"),
    )?;
    let logs = matrix::api(
        context,
        &format!("repos/{REPOSITORY}/actions/jobs/{job_id}/logs"),
    )?;
    authenticate_qualification(&artifact, &bytes, &job, &logs, &identity)
}

/// Read-only check: no source deletion, signing, publication or local receipt grants authority.
pub fn check(context: &Context) -> io::Result<()> {
    context.hosted()?;
    crate::acceptance::public_environment(context)?;
    if std::env::var("GITHUB_JOB").as_deref() != Ok("retirement-verification")
        || std::env::var("GITHUB_REF").as_deref() != Ok("refs/heads/main")
        || std::env::var("GITHUB_EVENT_NAME").as_deref() != Ok("workflow_dispatch")
        || std::env::var("GITHUB_REPOSITORY").as_deref() != Ok(REPOSITORY)
    {
        return Err(fail(
            "Retirement authentication belongs to its dedicated main collector.",
        ));
    }
    let source = context.source_sha()?;
    let head = context.capture(
        git(context).args(["rev-parse", "--verify", "HEAD"]),
        10000,
        8000,
        128,
    )?;
    if std::str::from_utf8(&head)
        .map_err(|_| fail("Invalid retirement checkout identity."))?
        .trim()
        != source
    {
        return Err(fail(
            "Retirement checkout differs from the exact current source.",
        ));
    }
    let manifest = ledger::parse(&ledger::read(
        &context.root,
        crate::tooling_execution::MANIFEST_FILE,
    )?)?;
    inventory::tooling_manifest(&context.root, &manifest, false)?;
    crate::ledger_inputs::tooling_originals(context, &manifest)?;
    let commit = context.capture(
        git(context).args(["rev-list", "--parents", "-n", "1", &source]),
        10000,
        8000,
        1024,
    )?;
    let words = std::str::from_utf8(&commit)
        .map_err(|_| fail("Invalid retirement commit identity."))?
        .split_whitespace()
        .collect::<Vec<_>>();
    if words.len() != 2 || words[0] != source || !hexadecimal(words[1], 40) {
        return Err(fail("Retirement requires one exact qualified parent."));
    }
    let parent = words[1];
    let before = read_tree(context, parent)?;
    let after = read_tree(context, &source)?;
    verify_transition(
        &manifest,
        parent,
        &source,
        &[parent.to_owned()],
        &before,
        &after,
    )?;
    let tracked = context.capture(git(context).args(["ls-files", "-z"]), 10000, 8000, LIMIT)?;
    let tracked = std::str::from_utf8(&tracked)
        .map_err(|_| fail("Retirement tracked inventory must be UTF-8."))?;
    inventory::tooling_inventory(&manifest, tracked)?;
    for row in manifest["entries"].as_array().unwrap() {
        match std::fs::symlink_metadata(context.root.join(row["source"].as_str().unwrap())) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            _ => {
                return Err(fail(
                    "Retired tooling originals remain in the working tree.",
                ));
            }
        }
    }
    let working = context.capture(
        git(context).args(["status", "--porcelain", "--untracked-files=no"]),
        10000,
        8000,
        LIMIT,
    )?;
    if !working.is_empty() {
        return Err(fail(
            "Retirement requires the unchanged committed source tree.",
        ));
    }
    let number = |key: &str| -> io::Result<u64> {
        let text = std::env::var(key).unwrap_or_default();
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(fail("Explicit qualification run and attempt required."));
        }
        text.parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| fail("Invalid qualification run or attempt."))
    };
    let parent_run = number("NUMERA_RETIREMENT_RUN_ID")?;
    let parent_attempt = number("NUMERA_RETIREMENT_RUN_ATTEMPT")?;
    let current_run = number("GITHUB_RUN_ID")?;
    let current_attempt = number("GITHUB_RUN_ATTEMPT")?;
    if parent_run == current_run {
        return Err(fail(
            "Retirement requires distinct original and current qualification runs.",
        ));
    }
    let workflow = ledger::parse(&matrix::api(
        context,
        &format!("repos/{REPOSITORY}/actions/workflows/ci.yml"),
    )?)?;
    if workflow["path"] != ".github/workflows/ci.yml" {
        return Err(fail("Canonical qualification workflow path differs."));
    }
    let workflow_id = workflow["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or_else(|| fail("Canonical qualification workflow identity missing."))?;
    let prior = remote_qualification(
        context,
        parent_run,
        parent_attempt,
        parent,
        workflow_id,
        true,
    )?;
    // The downstream job authenticates its completed producer, not the still-running enclosing run.
    let current = remote_qualification(
        context,
        current_run,
        current_attempt,
        &source,
        workflow_id,
        false,
    )?;
    for key in ["source_review_sha256", "tooling_source_review_sha256"] {
        if prior[key] != current[key] {
            return Err(fail("Retirement changed the qualified assertion review."));
        }
    }
    if current["source_review_sha256"]
        != sha(&ledger::read(
            &context.root,
            crate::source_review::REVIEW_FILE,
        )?)
        || current["tooling_source_review_sha256"]
            != sha(&ledger::read(
                &context.root,
                crate::tooling_review::REVIEW_FILE,
            )?)
        || read_tree(context, &source)? != after
        || !context
            .capture(
                git(context).args(["status", "--porcelain", "--untracked-files=no"]),
                10000,
                8000,
                LIMIT,
            )?
            .is_empty()
    {
        return Err(fail(
            "Retirement source or review bytes changed during authentication.",
        ));
    }
    println!(
        "{}",
        json!({"component":"native-tooling-retirement-verification","source_sha":source,
        "qualified_parent":parent,"removed_files":51,
        "current_source_qualification_verified":true,"retirement_verified":true,"source_deleted_by_command":false})
    );
    Ok(())
}
