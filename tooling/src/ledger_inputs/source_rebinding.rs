use super::{LIMIT, REFERENCE_SHA256, field, hash, list};
use crate::{
    context::{fail, sha},
    fixtures::{BASELINE, SOURCE},
    ledger,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
};

const SDK_REBINDS: [(&str, &str, &str); 10] = [
    (
        "scripts/native-tools-acceptance.mjs",
        "toolsAcceptance",
        "native_sdk_all_ten_tools_and_exact_shared_two_host_request",
    ),
    (
        "scripts/native-two-host-acceptance.mjs",
        "twoHostAcceptance",
        "native_sdk_all_ten_tools_and_exact_shared_two_host_request",
    ),
    (
        "scripts/native-compact-acceptance.mjs",
        "compactAcceptance",
        "native_sdk_compact_defaults_adjacent_discovery_and_private_dotenv_reload",
    ),
    (
        "scripts/native-env-acceptance.mjs",
        "envAcceptance",
        "native_sdk_compact_defaults_adjacent_discovery_and_private_dotenv_reload",
    ),
    (
        "scripts/native-negotiation-acceptance.mjs",
        "negotiationAcceptance",
        "native_sdk_three_protocol_eras_and_failed_handshake_ownership",
    ),
    (
        "scripts/native-config-reload-acceptance.mjs",
        "configReloadAcceptance",
        "native_sdk_three_saved_configuration_reload_schedules",
    ),
    (
        "scripts/native-terminal-acceptance.mjs",
        "terminalAcceptance",
        "native_sdk_terminal_rounds_preserve_authoritative_outcomes_and_capacity",
    ),
    (
        "scripts/native-races-acceptance.mjs",
        "racesAcceptance",
        "native_sdk_seven_actual_two_host_races_and_restarts",
    ),
    (
        "scripts/native-lifecycle-acceptance.mjs",
        "lifecycleAcceptance",
        "native_sdk_eighteen_literal_terminal_lifecycle_scenarios",
    ),
    (
        "scripts/native-transport-acceptance.mjs",
        "transportAcceptance",
        "native_sdk_thirteen_transport_boundary_scenarios",
    ),
];
/// Locate whole lines only. Even an exact match is navigation, not a semantic review or execution.
pub fn assertion_range_proposal(
    historical: &str,
    current: &str,
    range: &Value,
) -> io::Result<Value> {
    if historical.len() > LIMIT || current.len() > LIMIT {
        return Err(fail("Assertion source exceeds bound."));
    }
    let lines = historical.lines().collect::<Vec<_>>();
    let range = range
        .as_array()
        .filter(|value| value.len() == 2)
        .ok_or_else(|| fail("Assertion requires a two-line bound."))?;
    let start = range[0]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| fail("Assertion start is invalid."))?;
    let end = range[1]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value >= start && *value <= lines.len())
        .ok_or_else(|| fail("Assertion end is invalid."))?;
    let excerpt = lines[start - 1..end].join("\n");
    if excerpt.trim().is_empty() {
        return Err(fail("Empty assertion excerpt cannot be rebound."));
    }
    let current_lines = current.lines().collect::<Vec<_>>();
    let mut result = json!({"range_match":"changed","native_lines":Value::Null,
        "historical_excerpt_sha256":sha(excerpt.as_bytes()),"current_excerpt_sha256":Value::Null,
        "assertion_review":"pending","current_execution_claimed":false});
    let adapted = excerpt.replace("numera_image_gen_mcp", "numera_gen_mcp");
    for (kind, candidate) in [
        ("exact_lines", excerpt.as_str()),
        ("crate_namespace_only", adapted.as_str()),
    ] {
        if kind == "crate_namespace_only" && adapted == excerpt {
            break;
        }
        let expected = candidate.split('\n').collect::<Vec<_>>();
        if expected.len() > current_lines.len() {
            continue;
        }
        let matches = current_lines
            .windows(expected.len())
            .enumerate()
            .filter_map(|(index, window)| (window == expected.as_slice()).then_some(index))
            .take(2)
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            result["range_match"] = json!("ambiguous");
            return Ok(result);
        }
        if let Some(index) = matches.first() {
            result["range_match"] = json!(kind);
            result["native_lines"] = json!([index + 1, index + expected.len()]);
            result["current_excerpt_sha256"] = json!(sha(candidate.as_bytes()));
            return Ok(result);
        }
    }
    Ok(result)
}

/// Exact source traceability is deliberately separate from semantic review and execution.
pub fn sdk_assertion_traceability(
    project: &Path,
    rows: &[Value],
    manifest: &Value,
) -> io::Result<Value> {
    ledger::validate_rows(rows)?;
    let review = &manifest["assertion_traceability"];
    if manifest["schema_version"] != 1
        || manifest["feature"] != "003-rust-tooling"
        || review["schema_version"] != 1
        || review["status"] != "source_traceability_only"
        || review["semantic_parity_claimed"] != false
        || review["current_execution_claimed"] != false
    {
        return Err(fail(
            "SDK assertion traceability cannot inherit semantic or execution approval.",
        ));
    }
    let mut obligations = BTreeMap::new();
    let mut records = 0usize;
    for row in rows {
        for assertion in list(row, "assertions")? {
            let file = field(assertion, "native_file")?;
            if !file.ends_with(".mjs") {
                continue;
            }
            let mappings = list(row, "native")?
                .iter()
                .filter_map(|value| {
                    value
                        .as_str()
                        .and_then(|value| value.split_once("::"))
                        .filter(|(path, _)| *path == file)
                })
                .collect::<Vec<_>>();
            if mappings.len() != 1 {
                return Err(fail(
                    "Original SDK assertion requires one explicit historical mapping.",
                ));
            }
            let (_, _, symbol) = SDK_REBINDS
                .iter()
                .find(|(path, name, _)| *path == mappings[0].0 && *name == mappings[0].1)
                .ok_or_else(|| fail("SDK assertion has no registered Rust mapping."))?;
            let mapping = format!("tests/rust/sdk_acceptance.rs::{symbol}");
            let digest = ledger::assertion_digest(&json!([assertion]))?;
            if let Some((previous, previous_mapping)) =
                obligations.insert(digest, (assertion, mapping.clone()))
                && (previous != assertion || previous_mapping != mapping)
            {
                return Err(fail(
                    "Original SDK assertion identity has conflicting mappings.",
                ));
            }
            records += 1;
        }
    }
    let mut sources = BTreeMap::new();
    let mut files = BTreeMap::new();
    for path in std::iter::once("tests/rust/sdk_acceptance.rs".to_owned()).chain(
        [
            "host",
            "provider",
            "fixtures",
            "catalog",
            "configuration",
            "reload",
            "recovery",
            "lifecycle",
            "transport",
            "terminal",
            "races",
            "http",
            "contracts",
            "cleanup",
            "cleanup_contracts",
        ]
        .into_iter()
        .map(|module| format!("tests/rust/sdk_acceptance/{module}.rs")),
    ) {
        let bytes = ledger::read(project, &path)?;
        files.insert(path.clone(), sha(&bytes));
        sources.insert(
            path,
            String::from_utf8(bytes).map_err(|_| fail("SDK source must be UTF-8."))?,
        );
    }
    let bindings = list(review, "bindings")?;
    if obligations.is_empty() || bindings.len() != obligations.len() {
        return Err(fail(
            "Every original SDK assertion needs exactly one source binding.",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut checked = BTreeSet::new();
    for binding in bindings {
        let digest = field(binding, "historical_assertion_sha256")?;
        let (original, expected_mapping) = obligations
            .get(digest)
            .ok_or_else(|| fail("SDK source binding has an unknown historical obligation."))?;
        let mapping = field(binding, "current_mapping")?;
        if !seen.insert(digest)
            || mapping != expected_mapping
            || binding["historical_file"] != original["native_file"]
            || binding["historical_lines"] != original["native_lines"]
            || binding["source_lines"] != original["source_lines"]
        {
            return Err(fail(
                "SDK source binding duplicates or changes its original obligation.",
            ));
        }
        if checked.insert(mapping) {
            let (file, symbol) = mapping
                .split_once("::")
                .ok_or_else(|| fail("SDK binding test symbol missing."))?;
            ledger::mapping_source(project, file, "::", symbol)?;
        }
        let entry = list(manifest, "entries")?
            .iter()
            .find(|entry| entry["source"] == original["native_file"])
            .ok_or_else(|| fail("SDK source binding has no scenario entry."))?;
        let modules = list(entry, "modules")?;
        let ranges = list(binding, "ranges")?;
        if ranges.is_empty() || ranges.len() > 16 {
            return Err(fail("SDK source ranges are missing or oversized."));
        }
        let mut seen_ranges = BTreeSet::new();
        for range in ranges {
            let file = field(range, "file")?;
            if !modules.iter().any(|module| module.as_str() == Some(file)) {
                return Err(fail("SDK source range is outside its scenario modules."));
            }
            let source = sources
                .get(file)
                .ok_or_else(|| fail("SDK source range is not a maintained acceptance module."))?;
            let lines = source.lines().collect::<Vec<_>>();
            let bounds = list(range, "lines")?;
            let start = bounds
                .first()
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0);
            let end = bounds
                .get(1)
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0);
            if bounds.len() != 2
                || start == 0
                || end < start
                || end > lines.len()
                || !seen_ranges.insert((file, start, end))
            {
                return Err(fail("SDK source range is invalid or duplicated."));
            }
            let excerpt = lines[start - 1..end].join("\n");
            if excerpt.trim().is_empty()
                || !hash(field(range, "excerpt_sha256")?)
                || range["excerpt_sha256"] != sha(excerpt.as_bytes())
            {
                return Err(fail(
                    "SDK source excerpt differs from its reviewed traceability range.",
                ));
            }
        }
    }
    Ok(
        json!({"schema_version":1,"component":"sdk-assertion-source-traceability",
        "unique_obligations":seen.len(),"original_records":records,"bindings":bindings,"current_source_files":files,
        "semantic_parity_claimed":false,"current_execution_claimed":false}),
    )
}

/// Proposals are source navigation, never transplanted assertion ranges or current execution proof.
/// Every original row (including old status/review/evidence) is retained verbatim as historical data.
pub fn current_rebind_map(
    project: &Path,
    rows: &[Value],
    reference: &Value,
    current_sha: &str,
) -> io::Result<Value> {
    ledger::validate_rows(rows)?;
    if reference["source_sha"] != SOURCE || reference["baseline"] != BASELINE {
        return Err(fail(
            "Current rebind requires the immutable historical reference.",
        ));
    }
    let mut reference_cases = BTreeMap::new();
    for case in list(reference, "cases")? {
        if reference_cases.insert(field(case, "id")?, case).is_some() {
            return Err(fail("Historical reference duplicates a case."));
        }
    }
    if reference_cases.len() != 470 {
        return Err(fail(
            "Current rebind requires all original reference cases.",
        ));
    }
    if current_sha.len() != 40
        || !current_sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(fail("Current source revision identity is required."));
    }
    let mut checked = BTreeMap::<String, String>::new();
    let mut files = BTreeMap::<String, String>::new();
    let mut cases = Vec::new();
    for row in rows {
        let original = reference_cases
            .get(field(row, "id")?)
            .ok_or_else(|| fail("Current rebind source case is not in historical reference."))?;
        for key in ["source", "source_sha256", "platform_modes", "native"] {
            if original[key] != row[key] {
                return Err(fail(
                    "Current rebind cannot alter historical case identity.",
                ));
            }
        }
        if original["assertions_sha256"] != ledger::assertion_digest(&row["assertions"])? {
            return Err(fail("Current rebind cannot relabel historical assertions."));
        }
        let mut proposed = Vec::new();
        for old in list(row, "native")? {
            let old = old
                .as_str()
                .ok_or_else(|| fail("Historical native mapping is invalid."))?;
            let (old_file, old_symbol) = old
                .split_once("::")
                .ok_or_else(|| fail("Historical native mapping lacks symbol."))?;
            let (file, symbol) = if old_file.ends_with(".mjs") {
                let (_,_,symbol)=SDK_REBINDS.iter().find(|(file,symbol,_)| *file==old_file && *symbol==old_symbol)
                    .ok_or_else(||fail("Historical interpreted mapping has no explicit Rust migration proposal."))?;
                ("tests/rust/sdk_acceptance.rs", *symbol)
            } else {
                (old_file, old_symbol)
            };
            let historical_hash = field(&reference["current_git_files"], old_file)?;
            if !hash(historical_hash) || original["native_sha256"][old_file] != historical_hash {
                return Err(fail(
                    "Historical mapping bytes lack the retained execution binding.",
                ));
            }
            let mapping = format!("{file}::{symbol}");
            if !checked.contains_key(&mapping) {
                let source = ledger::mapping_source(project, file, "::", symbol)?;
                files.insert(file.into(), sha(source.as_bytes()));
                checked.insert(mapping.clone(), sha(source.as_bytes()));
            }
            proposed.push(json!({"historical_mapping":old,"historical_sha256":reference["current_git_files"][old_file],
                "proposed_mapping":mapping,"current_sha256":checked[&mapping],
                "same_file_bytes":old_file==file && reference["current_git_files"][old_file]==checked[&mapping],
                "assertion_ranges_rebound":false,"assertion_review":"pending","current_execution_claimed":false}));
        }
        cases.push(json!({"id":row["id"],"historical":row,"current":{"status":"pending","proposed_native":proposed,
            "assertions":Value::Null,"assertion_review":"pending","current_execution_claimed":false}}));
    }
    let manifest_path = "docs/testing/sdk-tooling-migration.json";
    let manifest_bytes = ledger::read(project, manifest_path)?;
    let traceability = sdk_assertion_traceability(project, rows, &ledger::parse(&manifest_bytes)?)?;
    for (path, digest) in traceability["current_source_files"]
        .as_object()
        .ok_or_else(|| fail("SDK traceability source inventory missing."))?
    {
        let digest = digest
            .as_str()
            .filter(|value| hash(value))
            .ok_or_else(|| fail("SDK traceability source digest missing."))?;
        files.insert(path.clone(), digest.to_owned());
    }
    files.insert(manifest_path.into(), sha(&manifest_bytes));
    Ok(
        json!({"schema_version":1,"component":"native-case-source-rebind","baseline":BASELINE,"historical_source_sha":SOURCE,
        "historical_reference_sha256":REFERENCE_SHA256,"current_source_sha":current_sha,"cases":cases,"current_source_files":files,
        "sdk_assertion_traceability":traceability,
        "original_evidence_rewritten":false,"current_assertion_review":"pending","current_execution_claimed":false,"acceptance_verified":false}),
    )
}
