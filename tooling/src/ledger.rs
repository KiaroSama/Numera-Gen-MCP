use crate::context::{Context, fail, sha};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::Path,
};

mod execution;
mod input;
mod mapping;

pub use execution::{assertion_digest, validate_execution_binding, validate_reference_binding};
pub(crate) use input::{parse, read};
use input::{relative, text};
pub use mapping::{historical_mapping_source, mapping_source};
use mapping::{MappingPolicy, mapping};

const PLATFORMS: [&str; 3] = ["win32", "linux", "darwin"];
fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn string<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| fail("Required ledger identity is missing."))
}
fn array<'a>(value: &'a Value, key: &str) -> io::Result<&'a Vec<Value>> {
    value[key]
        .as_array()
        .ok_or_else(|| fail("Required ledger array is missing."))
}
pub fn load(root: &Path) -> io::Result<(Value, Vec<Value>)> {
    let index = parse(&read(root, "index.json")?)?;
    if index["baseline"] != crate::fixtures::BASELINE
        || index["source_cases"] != 470
        || index["source_suites"] != 50
    {
        return Err(fail("Frozen case index identity differs."));
    }
    let mut files = BTreeSet::new();
    let mut rows = Vec::new();
    for name in array(&index, "files")? {
        let name = name
            .as_str()
            .ok_or_else(|| fail("Invalid ledger filename."))?;
        if !["unit.jsonl", "contract.jsonl", "integration.jsonl"].contains(&name)
            || !files.insert(name)
        {
            return Err(fail("Unexpected or duplicate case file."));
        }
        for line in text(&read(root, name)?)?.lines() {
            rows.push(parse(line.as_bytes())?);
        }
    }
    if files.len() != 3 {
        return Err(fail("Every source case group is required."));
    }
    Ok((index, rows))
}

pub fn validate_rows(rows: &[Value]) -> io::Result<()> {
    if rows.len() != 470 {
        return Err(fail("All470source cases required."));
    }
    let (mut ids, mut sources) = (BTreeSet::new(), BTreeSet::new());
    for row in rows {
        let id = string(row, "id")?;
        if !hex(id, 64) || !ids.insert(id) {
            return Err(fail("Missing or duplicate case identity."));
        }
        let source = string(row, "source")?;
        relative(source)?;
        if !source.starts_with("tests/") || !source.ends_with(".test.ts") {
            return Err(fail("Invalid frozen source path."));
        }
        sources.insert(source);
        if !hex(string(row, "source_sha256")?, 64) || string(row, "name")?.trim().is_empty() {
            return Err(fail("Case source identity incomplete."));
        }
        let native = array(row, "native")?;
        let assertions = array(row, "assertions")?;
        if !matches!(row["status"].as_str(), Some("pending" | "verified")) {
            return Err(fail(
                "Unsupported case disposition; silent retirement is forbidden.",
            ));
        }
        if row["status"] == "verified" && (native.is_empty() || assertions.is_empty()) {
            return Err(fail("Case assertions are pending."));
        }
        let modes = row["platform_modes"]
            .as_object()
            .ok_or_else(|| fail("Case applicability missing."))?;
        if modes.len() != 3
            || PLATFORMS
                .iter()
                .any(|p| !matches!(modes[*p].as_str(), Some("run" | "skip" | "todo")))
        {
            return Err(fail("Invalid source applicability."));
        }
    }
    if sources.len() != 50 {
        return Err(fail("All50source suites required."));
    }
    Ok(())
}

fn bounds(value: &Value, count: usize) -> io::Result<()> {
    let values = value
        .as_array()
        .ok_or_else(|| fail("Assertion line evidence missing."))?;
    let a = values.first().and_then(Value::as_u64).unwrap_or(0);
    let b = values.get(1).and_then(Value::as_u64).unwrap_or(0);
    if values.len() != 2 || a == 0 || b < a || b > count as u64 {
        Err(fail("Assertion line evidence is invalid."))
    } else {
        Ok(())
    }
}
fn location(value: &Value, source: &str) -> io::Result<()> {
    let row = value
        .as_object()
        .ok_or_else(|| fail("Case declaration location missing."))?;
    if row.len() != 2
        || row
            .get("line")
            .and_then(Value::as_u64)
            .is_none_or(|n| n == 0 || n > source.lines().count() as u64)
        || row
            .get("column")
            .and_then(Value::as_u64)
            .is_none_or(|n| n == 0)
    {
        Err(fail("Case declaration location is invalid."))
    } else {
        Ok(())
    }
}

pub fn validate(
    project: &Path,
    source: &Path,
    collection: &Value,
    ledger_root: &Path,
    acceptance: bool,
) -> io::Result<Value> {
    validate_with(
        project,
        source,
        collection,
        ledger_root,
        acceptance,
        MappingPolicy::Rust,
    )
}
pub fn validate_historical(
    project: &Path,
    source: &Path,
    collection: &Value,
    ledger_root: &Path,
    acceptance: bool,
) -> io::Result<Value> {
    validate_with(
        project,
        source,
        collection,
        ledger_root,
        acceptance,
        MappingPolicy::Historical,
    )
}
fn validate_with(
    project: &Path,
    source: &Path,
    collection: &Value,
    ledger_root: &Path,
    acceptance: bool,
    policy: MappingPolicy,
) -> io::Result<Value> {
    let (index, rows) = load(ledger_root)?;
    validate_rows(&rows)?;
    if collection["baseline"] != crate::fixtures::BASELINE
        || collection["source_cases"] != 470
        || collection["source_suites"] != 50
        || collection["test_bodies_executed"] != false
    {
        return Err(fail("Expanded frozen collection identity differs."));
    }
    let platform = string(collection, "platform")?;
    if !PLATFORMS.contains(&platform) {
        return Err(fail("Unsupported source collection platform."));
    }
    let mut expected = BTreeMap::new();
    for case in array(collection, "cases")? {
        if expected.insert(string(case, "id")?, case).is_some() {
            return Err(fail("Expanded case identities are duplicated."));
        }
    }
    if expected.len() != 470
        || rows
            .iter()
            .any(|row| !expected.contains_key(row["id"].as_str().unwrap_or("")))
    {
        return Err(fail("Every expanded source case must appear exactly once."));
    }
    let (mut sources, mut mappings, mut native_hashes) = (
        BTreeMap::<String, String>::new(),
        BTreeMap::<String, String>::new(),
        BTreeMap::<String, String>::new(),
    );
    let mut registered = BTreeSet::<String>::new();
    let (mut pending, mut mapped, mut historical) = (0, 0, false);
    for row in &rows {
        let case = expected[string(row, "id")?];
        for key in ["source", "source_sha256", "name", "location"] {
            if row.get(key).is_none() || row[key] != case[key] {
                return Err(fail(
                    "Case identity differs from independent runtime collection.",
                ));
            }
        }
        let path = string(row, "source")?;
        if !sources.contains_key(path) {
            sources.insert(path.into(), text(&read(source, path)?)?.into());
        }
        let frozen = &sources[path];
        if row["source_sha256"] != sha(frozen.as_bytes()) {
            return Err(fail("Frozen source assertion identity is stale."));
        }
        location(&row["location"], frozen)?;
        if row["platform_modes"][platform] != case["mode"] {
            return Err(fail("Case platform applicability differs from collection."));
        }
        let mut native_files = BTreeSet::new();
        let mut row_mappings = BTreeSet::new();
        for value in array(row, "native")? {
            let value = value
                .as_str()
                .ok_or_else(|| fail("Invalid native mapping."))?;
            if !row_mappings.insert(value) {
                return Err(fail("Duplicate native case mapping."));
            }
            let (file, symbol) = value
                .split_once("::")
                .ok_or_else(|| fail("Native mapping requires a symbol."))?;
            if registered.insert(value.into()) {
                mapping(project, file, "::", symbol, policy, &mut mappings)?;
            }
            historical |= file.ends_with(".mjs");
            native_files.insert(file);
            if !native_hashes.contains_key(file) {
                native_hashes.insert(file.into(), sha(mappings[file].as_bytes()));
            }
        }
        for assertion in array(row, "assertions")? {
            let file = string(assertion, "native_file")?;
            if !native_files.contains(file) || string(assertion, "claim")?.trim().is_empty() {
                return Err(fail("Assertion must reference its mapped native test."));
            }
            bounds(&assertion["source_lines"], frozen.lines().count())?;
            bounds(&assertion["native_lines"], mappings[file].lines().count())?;
        }
        if !native_files.is_empty() && !array(row, "assertions")?.is_empty() {
            mapped += 1;
        }
        if row["status"] == "pending" {
            pending += 1;
        } else {
            if row["gaps"].as_array().is_none_or(|a| !a.is_empty())
                || row["review"].as_object().is_none_or(|r| r.is_empty())
                || row["review"]["source_assertions"] != "equivalent"
                || row["review"]["confirmed_gaps"] != 0
            {
                return Err(fail("Verified case has incomplete assertion review."));
            }
            if matches!(policy, MappingPolicy::Reference) {
                continue;
            }
            let evidence = &row["evidence"];
            if !hex(string(evidence, "sha")?, 40)
                || evidence["run_id"].as_u64().is_none_or(|n| n == 0)
                || evidence["run_attempt"].as_u64().is_none_or(|n| n == 0)
                || !hex(string(evidence, "execution_binding_sha256")?, 64)
            {
                return Err(fail(
                    "Verified case has no exact native execution identity.",
                ));
            }
            if evidence
                .get("source_only_rebinding")
                .is_some_and(|v| !v.is_null())
                && evidence["source_only_rebinding"]["current_tree_execution_claimed"] != false
            {
                return Err(fail(
                    "Source-only rebinding cannot claim current execution.",
                ));
            }
            let targets = array(evidence, "passed_targets")?;
            let unique = targets
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>();
            if unique.len() != targets.len() || unique.iter().any(|p| !PLATFORMS.contains(p)) {
                return Err(fail("Invalid or duplicate passed platform evidence."));
            }
            for file in native_files {
                if evidence["native_sha256"][file] != native_hashes[file] {
                    return Err(fail("Native assertion evidence is stale."));
                }
            }
            for platform in PLATFORMS {
                if row["platform_modes"][platform] == "run"
                    && !targets.iter().any(|p| p == platform)
                {
                    return Err(fail("Applicable native case execution is missing."));
                }
            }
        }
    }
    if sources.len() != 50 {
        return Err(fail("Case ledger suite totals differ."));
    }
    let discovered = inventory(source)?;
    let actual_suites = array(&discovered, "suites")?
        .iter()
        .map(|row| row["source"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    if sources.keys().map(String::as_str).collect::<BTreeSet<_>>() != actual_suites {
        return Err(fail(
            "Frozen source suite discovery differs from case collection.",
        ));
    }
    if acceptance && (pending != 0 || index["mapping_complete"] != true) {
        return Err(fail("Complete native assertion parity is still pending."));
    }
    if acceptance {
        let mut digests = BTreeSet::new();
        for row in &rows {
            digests.insert(string(&row["evidence"], "execution_binding_sha256")?);
        }
        for digest in digests {
            let bytes = read(ledger_root, &format!("execution/{digest}.json"))?;
            validate_execution_binding(&rows, &bytes, digest)?;
        }
    }
    Ok(
        json!({"component":"native-case-ledger","cases":rows.len(),"suites":sources.len(),"assertion_assignments":mapped,"pending":pending,"structural_verified":true,"acceptance_verified":acceptance && !historical,"historical_acceptance_verified":acceptance && historical,"current_execution_claimed":false}),
    )
}

pub fn inventory(source: &Path) -> io::Result<Value> {
    fn walk(root: &Path, current: &Path, found: &mut Vec<Value>) -> io::Result<()> {
        if found.len() > 50
            || current
                .components()
                .count()
                .saturating_sub(root.components().count())
                > 16
        {
            return Err(fail("Frozen source inventory exceeds bound."));
        }
        for (visited, entry) in fs::read_dir(current)?.enumerate() {
            if visited >= 4096 {
                return Err(fail("Frozen source directory exceeds bound."));
            }
            let entry = entry?;
            let info = entry.file_type()?;
            if info.is_symlink() {
                return Err(fail("Linked frozen source suite rejected."));
            }
            if info.is_dir() {
                walk(root, &entry.path(), found)?;
            } else if entry.file_name().to_string_lossy().ends_with(".test.ts") {
                let path = entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| fail("Escaping frozen suite."))?
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push(json!({"source":path,"sha256":sha(&read(root,&path)?)}));
            }
        }
        Ok(())
    }
    let mut suites = Vec::new();
    walk(source, &source.join("tests"), &mut suites)?;
    if suites.len() != 50 {
        return Err(fail("Pinned baseline suite count differs from50."));
    }
    suites.sort_by(|a, b| a["source"].as_str().cmp(&b["source"].as_str()));
    Ok(
        json!({"baseline":crate::fixtures::BASELINE,"baseline_case_count":470,"suites":suites,"current_execution_claimed":false}),
    )
}
pub fn validate_parity(
    project: &Path,
    source: &Path,
    collection: &Value,
    ledger_root: &Path,
    manifest: &Value,
    acceptance: bool,
) -> io::Result<Value> {
    let report = validate(project, source, collection, ledger_root, acceptance)?;
    let expected = inventory(source)?;
    if manifest["baseline"] != crate::fixtures::BASELINE
        || manifest["baseline_case_count"] != 470
        || (acceptance && manifest["case_mapping_complete"] != true)
    {
        return Err(fail(
            "Suite parity identity or expanded case mapping is pending.",
        ));
    }
    let hashes = array(&expected, "suites")?
        .iter()
        .map(|s| (s["source"].as_str().unwrap(), &s["sha256"]))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    for row in array(manifest, "suites")? {
        let path = string(row, "source")?;
        if !seen.insert(path)
            || hashes
                .get(path)
                .is_none_or(|hash| row["sha256"].as_str() != hash.as_str())
        {
            return Err(fail("Every baseline suite needs one exact source mapping."));
        }
        if row["status"] != "verified"
            || row["evidence"].is_null()
            || array(row, "native")?.is_empty()
        {
            return Err(fail("Suite contracts remain unverified."));
        }
        for value in array(row, "native")? {
            let (file, symbol) = if let Some(value) = value.as_str() {
                value
                    .split_once("::")
                    .ok_or_else(|| fail("Suite mapping requires test identity."))?
            } else {
                (string(value, "file")?, string(value, "test")?)
            };
            mapping_source(project, file, "::", symbol)?;
        }
    }
    if seen.len() != 50 {
        return Err(fail("Every baseline suite requires an explicit mapping."));
    }
    Ok(report)
}

/// Validate a separate historical reference without relabelling the ledger's original evidence.
/// The caller authenticates the reference digest independently. This never proves current acceptance.
pub fn validate_reference(
    project: &Path,
    source: &Path,
    collections: &[Value],
    ledger_root: &Path,
    bytes: &[u8],
    digest: &str,
) -> io::Result<Value> {
    if collections.len() != 3
        || collections
            .iter()
            .filter_map(|v| v["platform"].as_str())
            .collect::<BTreeSet<_>>()
            != BTreeSet::from(PLATFORMS)
    {
        return Err(fail(
            "Independent collections for every original platform are required.",
        ));
    }
    for collection in collections {
        validate_with(
            project,
            source,
            collection,
            ledger_root,
            false,
            MappingPolicy::Reference,
        )?;
    }
    let (_, rows) = load(ledger_root)?;
    let execution = execution::execution_binding(&rows, bytes, digest, true)?;
    let binding = parse(bytes)?;
    let mut checked = BTreeSet::new();
    for row in &rows {
        for value in array(row, "native")? {
            let (file, _) = value
                .as_str()
                .and_then(|v| v.split_once("::"))
                .ok_or_else(|| fail("Invalid historical mapping."))?;
            if checked.insert(file) {
                if binding["current_git_files"][file] != sha(&read(project, file)?) {
                    return Err(fail(
                        "Materialized historical native bytes differ from execution reference.",
                    ));
                }
            }
        }
    }
    Ok(
        json!({"cases":470,"suites":50,"platforms":3,"historical_reference_verified":true,"historical_execution":execution,"original_evidence_rewritten":false,"acceptance_verified":false,"current_execution_claimed":false}),
    )
}

pub fn validate_reference_parity(
    project: &Path,
    source: &Path,
    manifest: &Value,
    rows: &[Value],
) -> io::Result<Value> {
    validate_rows(rows)?;
    let frozen = inventory(source)?;
    if manifest["baseline"] != crate::fixtures::BASELINE
        || manifest["baseline_case_count"] != 470
        || manifest["case_mapping_complete"] != true
    {
        return Err(fail("Historical suite parity identity differs."));
    }
    let hashes = array(&frozen, "suites")?
        .iter()
        .map(|v| (v["source"].as_str().unwrap(), &v["sha256"]))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    let mut cache = BTreeMap::new();
    for suite in array(manifest, "suites")? {
        let path = string(suite, "source")?;
        if !seen.insert(path)
            || hashes
                .get(path)
                .is_none_or(|v| suite["sha256"].as_str() != v.as_str())
            || suite["status"] != "verified"
            || suite["evidence"].is_null()
        {
            return Err(fail("Historical suite source or disposition differs."));
        }
        let cases = rows
            .iter()
            .filter(|v| v["source"] == path)
            .collect::<Vec<_>>();
        if suite["case_count"].as_u64() != Some(cases.len() as u64) {
            return Err(fail("Historical suite expanded case count differs."));
        }
        let expected = cases
            .iter()
            .flat_map(|v| v["native"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        let actual = array(suite, "native")?
            .iter()
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| fail("Historical suite mapping must name an exact test."))
            })
            .collect::<io::Result<Vec<_>>>()?;
        if actual.len() != expected.len()
            || actual.iter().copied().collect::<BTreeSet<_>>() != expected
        {
            return Err(fail(
                "Historical suite mappings differ from expanded case mappings.",
            ));
        }
        for value in actual {
            let (file, symbol) = value
                .split_once("::")
                .ok_or_else(|| fail("Historical suite mapping symbol missing."))?;
            mapping(
                project,
                file,
                "::",
                symbol,
                MappingPolicy::Reference,
                &mut cache,
            )?;
        }
    }
    if seen.len() != 50 {
        return Err(fail("Historical suite parity omits a frozen suite."));
    }
    Ok(
        json!({"suites":50,"cases":470,"historical_suite_mapping_verified":true,"acceptance_verified":false,"current_execution_claimed":false}),
    )
}

pub fn check(context: &Context) -> io::Result<()> {
    let report = crate::ledger_inputs::check(context)?;
    println!("{report}");
    Ok(())
}
