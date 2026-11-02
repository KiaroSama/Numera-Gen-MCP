use crate::{
    context::{fail, sha},
    execution_evidence, inventory, ledger,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
};

pub const MANIFEST_FILE: &str = "docs/testing/tooling-migration.json";

fn paths<'a>(row: &'a Value, field: &str) -> io::Result<Vec<&'a str>> {
    row[field]
        .as_array()
        .filter(|items| !items.is_empty() && items.len() <= 128)
        .ok_or_else(|| fail("Tooling source inventory is missing or oversized."))?
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|path| !path.is_empty() && path.len() <= 1024)
                .ok_or_else(|| fail("Tooling source path is invalid."))
        })
        .collect()
}

fn harness(path: &str) -> io::Result<&str> {
    if path == "native/devtools/process_unix.rs" {
        return Ok("native/lib.rs");
    }
    if path.starts_with("tests/rust/sdk_acceptance/") && path.ends_with(".rs") {
        return Ok("tests/rust/sdk_acceptance.rs");
    }
    if path.ends_with(".rs")
        && matches!(Path::new(path).parent(), Some(parent)
            if parent == Path::new("tests/rust") || parent == Path::new("tooling/tests"))
    {
        return Ok(path);
    }
    Err(fail("Tooling test has no supported execution harness."))
}

fn platforms(mapping: &str) -> &'static [&'static str] {
    match mapping {
        "native/devtools/process_unix.rs::denied_group_kill_requires_closed_leader_and_definitive_live_group_absence"
        | "native/devtools/process_unix.rs::missing_group_does_not_probe_and_closed_observation_errors_never_mean_absence" => &["linux", "darwin"],
        "tests/rust/devtool_supervisor.rs::actual_developer_cli_preserves_full_windows_child_exit_status"
        | "tests/rust/devtool_setup.rs::unsigned_update_is_rejected_before_replacing_a_locked_installation" => &["win32"],
        "tests/rust/artifact_signatures.rs::approved_manifest_does_not_authorize_unsigned_text_for_activation" => &["win32", "darwin"],
        _ => &["win32", "linux", "darwin"],
    }
}

/// Binds dispositions to authenticated exact named tests, not scenario assertion parity.
/// The caller independently authenticates artifacts, identity and complete source provenance.
pub fn bind(
    root: &Path,
    manifest_bytes: &[u8],
    identity: &Value,
    provenance: &str,
    dependencies: &BTreeMap<String, String>,
    sources: &BTreeSet<String>,
    reports: &BTreeMap<String, Vec<u8>>,
) -> io::Result<Value> {
    if identity["phase"] != "built"
        || !sources.contains("native/lib.rs")
        || !sources.contains("tooling/src/lib.rs")
    {
        return Err(fail(
            "Tooling binding requires the built workspace execution.",
        ));
    }
    if dependencies.get(MANIFEST_FILE) != Some(&sha(manifest_bytes)) {
        return Err(fail(
            "Tooling dispositions differ from authenticated source provenance.",
        ));
    }
    let manifest = ledger::parse(manifest_bytes)?;
    inventory::tooling_manifest(root, &manifest, false)?;
    if manifest["sdk_scenario_manifest"] != "docs/testing/sdk-tooling-migration.json" {
        return Err(fail("Tooling SDK scenario manifest identity differs."));
    }
    let phases = ["built", "release", "extracted-intermediate"];
    if reports.keys().map(String::as_str).collect::<BTreeSet<_>>()
        != phases.into_iter().collect()
    {
        return Err(fail("Tooling binding requires every exact artifact execution phase."));
    }
    let sdk_sources = BTreeSet::from([
        "tests/rust/sdk_acceptance.rs".to_owned(),
        "tests/rust/media_acceptance.rs".to_owned(),
    ]);
    let mut verified = BTreeMap::new();
    let mut report_hashes = BTreeMap::new();
    for phase in phases {
        let mut expected = identity.clone();
        expected["phase"] = json!(phase);
        verified.insert(phase, execution_evidence::verify_report(
            &reports[phase],
            &expected,
            provenance,
            dependencies,
            if phase == "built" { sources } else { &sdk_sources },
        )?);
        report_hashes.insert(phase, sha(&reports[phase]));
    }
    let sdk_binary = &verified["release"]["tested_binary_sha256"];
    if verified["extracted-intermediate"]["tested_binary_sha256"] != *sdk_binary {
        return Err(fail("Release and extracted tooling SDK execution tested different binaries."));
    }
    let report = &verified["built"];
    let mut required = BTreeSet::from([
        "native/devtools/mod.rs",
        "docs/testing/sdk-tooling-migration.json",
    ]);
    for field in ["entries", "retained_powershell"] {
        for row in manifest[field].as_array().unwrap() {
            required.insert(row["source"].as_str().unwrap());
            if field == "entries" {
                for field in ["rust", "tests"] {
                    required.extend(paths(row, field)?);
                }
            }
        }
    }
    if required.len() > 4096 {
        return Err(fail("Tooling dependency inventory exceeds bound."));
    }
    let mut source_hashes = BTreeMap::new();
    for path in required {
        let digest = sha(&ledger::read(root, path)?);
        if dependencies.get(path) != Some(&digest) {
            return Err(fail(
                "Tooling original, replacement or helper source differs.",
            ));
        }
        source_hashes.insert(path, digest);
    }
    let mut entries = Vec::new();
    let mut harnesses = BTreeMap::new();
    for row in manifest["entries"].as_array().unwrap() {
        let declared = paths(row, "tests")?;
        let mut tests = Vec::new();
        for path in &declared {
            let path = *path;
            let harness = harness(path)?;
            // This helper is compiled by native/devtools/mod.rs only under cfg(unix).
            let applicable =
                path != "native/devtools/process_unix.rs" || identity["platform"] != "win32";
            if applicable {
                if !sources.contains(harness) {
                    return Err(fail("Declared tooling test harness was not selected."));
                }
                let passed = report["harnesses"][harness]["passed"]
                    .as_array()
                    .filter(|names| !names.is_empty())
                    .ok_or_else(|| {
                        fail("Declared tooling test harness has no successful named tests.")
                    })?;
                harnesses.insert(harness, passed.clone());
            }
            tests.push(json!({"source":path,"harness":harness,"applicable":applicable,
                "not_run_reason":if applicable {Value::Null} else {json!("cfg(unix) helper is not compiled on Windows")}}));
        }
        let mut named = BTreeMap::new();
        let mut artifact_phases = BTreeMap::<&str, BTreeMap<&str, Value>>::new();
        for mapping in paths(row, "named_tests")? {
            let (file, symbol) = mapping
                .split_once("::")
                .ok_or_else(|| fail("Tooling execution requires an exact test symbol."))?;
            if !declared.contains(&file) || named.contains_key(mapping) {
                return Err(fail("Tooling test mapping is undeclared or duplicated."));
            }
            ledger::mapping_source(root, file, "::", symbol)?;
            let platform = identity["platform"]
                .as_str()
                .ok_or_else(|| fail("Tooling execution platform missing."))?;
            let result = if platforms(mapping).contains(&platform) {
                let harness = harness(file)?;
                let resolved = format!("{harness}::{symbol}");
                if sdk_sources.contains(harness) {
                    for phase in ["release", "extracted-intermediate"] {
                        let execution = execution_evidence::named_test(&verified[phase], &resolved)?;
                        artifact_phases.entry(phase).or_default().insert(mapping, execution);
                    }
                }
                execution_evidence::named_test(report, &resolved)?
            } else {
                json!({"applicable":false,"not_run_reason":"Source cfg excludes this exact test on this platform"})
            };
            named.insert(mapping, result);
        }
        let scenarios = paths(row, "scenarios")?;
        let declared_scenarios = scenarios.iter().copied().collect::<BTreeSet<_>>();
        let bindings = row["scenario_tests"]
            .as_object()
            .ok_or_else(|| fail("Explicit tooling scenario test bindings are missing."))?;
        if declared_scenarios.len() != scenarios.len()
            || bindings.keys().map(String::as_str).collect::<BTreeSet<_>>() != declared_scenarios
        {
            return Err(fail("Tooling scenario test coverage is incomplete or duplicated."));
        }
        let mut scenario_execution = BTreeMap::new();
        let mut used = BTreeSet::new();
        for scenario in scenarios {
            let mut results = BTreeMap::new();
            for mapping in paths(&row["scenario_tests"], scenario)? {
                let execution = named
                    .get(mapping)
                    .ok_or_else(|| fail("Tooling scenario references an undeclared test."))?;
                if results.insert(mapping, execution).is_some() {
                    return Err(fail("Tooling scenario duplicates a test binding."));
                }
                used.insert(mapping);
            }
            scenario_execution.insert(scenario, results);
        }
        if used != named.keys().copied().collect() {
            return Err(fail("Named tooling test is not assigned to a declared scenario."));
        }
        entries.push(json!({"source":row["source"],"source_sha256":row["source_sha256"],
            "replacements":row["rust"],"scenarios":row["scenarios"],"tests":tests,
            "named_execution":named,"scenario_execution":scenario_execution,
            "artifact_phase_execution":artifact_phases}));
    }
    Ok(json!({"schema_version":1,"component":"native-tooling-harness-execution",
        "identity":identity,"manifest_sha256":sha(manifest_bytes),"provenance_sha256":provenance,
        "execution_report_sha256":sha(&reports["built"]),"phase_reports_sha256":report_hashes,
        "source_files":source_hashes,
        "mapped_python_js":46,"retained_powershell":manifest["retained_powershell"],
        "entries":entries,"harnesses":harnesses,"current_named_harness_execution_verified":true,
        "current_named_test_execution_verified":true,"declared_scenario_execution_verified":true,
        "sdk_artifact_phase_execution_verified":true,"sdk_tested_binary_sha256":sdk_binary,
        "scenario_assertions_verified":false,"semantic_parity_claimed":false,
        "qualification_verified":false,"retirement_authorized":false,"original_evidence_rewritten":false}))
}
