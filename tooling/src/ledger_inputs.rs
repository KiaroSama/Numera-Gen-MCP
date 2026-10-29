use crate::{context::{Context, fail, sha}, fixtures::{BASELINE, SOURCE}, ledger};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, fs, io::{self, Cursor, Read}, path::{Component, Path}};

const LIMIT: usize = 16 * 1024 * 1024;
pub const REFERENCE_SHA256: &str = "7218c0632c1e83b8735e3a15a026e9118aeea758c697ffc899218ce20949bf52";
pub const REFERENCE_FILE: &str = "tests/fixtures/oracle/common/case-execution-binding.json";
// These are the retained collectTests artifacts, not reconstructed ledger expectations. Collection
// executed declarations only; the separate final native binder never promotes them to TS execution.
pub const COLLECTIONS: [(&str, &str, &str, &str); 3] = [
    ("linux", "linux", "d7800801a609388e0a382a044ad1a7dbe20a4b133860e1cf2b8759532e946dc5", "3e03ea2f79c5db0bd56ca80712d08079a231dbf0d6cde9c64cc13198ca2ce9cc"),
    ("windows", "win32", "55a18593073ab7cb3d35e738a569c11e6714149db2328bcbd0f63545a2c2d132", "fff0ac307db8cf022c8b977484cee5ebfecc17099b07a4c0bfbff11fc47fc3ba"),
    ("macos", "darwin", "2edc7e2b2c37906110529fa2bf1f67ec6b1fc72c8c43e7fb3a1ccc2690f94c72", "5225347413d591add19a089a7834ae42b5a90138e1900210aee67ee200a7c41a"),
];
const SDK_REBINDS: [(&str, &str, &str); 10] = [
    ("scripts/native-tools-acceptance.mjs", "toolsAcceptance", "native_sdk_all_ten_tools_and_exact_shared_two_host_request"),
    ("scripts/native-two-host-acceptance.mjs", "twoHostAcceptance", "native_sdk_all_ten_tools_and_exact_shared_two_host_request"),
    ("scripts/native-compact-acceptance.mjs", "compactAcceptance", "native_sdk_compact_defaults_adjacent_discovery_and_private_dotenv_reload"),
    ("scripts/native-env-acceptance.mjs", "envAcceptance", "native_sdk_compact_defaults_adjacent_discovery_and_private_dotenv_reload"),
    ("scripts/native-negotiation-acceptance.mjs", "negotiationAcceptance", "native_sdk_three_protocol_eras_and_failed_handshake_ownership"),
    ("scripts/native-config-reload-acceptance.mjs", "configReloadAcceptance", "native_sdk_three_saved_configuration_reload_schedules"),
    ("scripts/native-terminal-acceptance.mjs", "terminalAcceptance", "native_sdk_terminal_rounds_preserve_authoritative_outcomes_and_capacity"),
    ("scripts/native-races-acceptance.mjs", "racesAcceptance", "native_sdk_seven_actual_two_host_races_and_restarts"),
    ("scripts/native-lifecycle-acceptance.mjs", "lifecycleAcceptance", "native_sdk_eighteen_literal_terminal_lifecycle_scenarios"),
    ("scripts/native-transport-acceptance.mjs", "transportAcceptance", "native_sdk_thirteen_transport_boundary_scenarios"),
];
fn field<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    value[key].as_str().filter(|v| !v.is_empty()).ok_or_else(|| fail("Original ledger input field is missing."))
}
fn list<'a>(value: &'a Value, key: &str) -> io::Result<&'a Vec<Value>> {
    value[key].as_array().ok_or_else(|| fail("Original ledger input array is missing."))
}
fn hash(value: &str) -> bool { value.len()==64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn name(value: &str) -> io::Result<()> {
    if value.is_empty() || value.len()>512 || value.bytes().any(|b| b==0 || b==b'\\' || b==b':')
        || value.split('/').any(|p| p.is_empty() || p=="." || p=="..")
        || Path::new(value).components().any(|c| !matches!(c,Component::Normal(_))) {
        return Err(fail("Historical source path is not a contained public member."));
    }
    Ok(())
}

pub fn validate_collection(bytes: &[u8], platform: &str) -> io::Result<Value> {
    let (_, expected_platform, digest, _) = COLLECTIONS.iter().find(|(label,p,_,_)| *label==platform || *p==platform)
        .ok_or_else(|| fail("Unsupported original collection platform."))?;
    if sha(bytes)!=*digest { return Err(fail("Original source-case collection bytes changed.")); }
    let value=ledger::parse(bytes)?;
    let architecture=if *expected_platform=="darwin" {"arm64"} else {"x64"};
    if value["baseline"]!=BASELINE || value["runtime"]!="v24.21.0" || value["platform"]!=*expected_platform
        || value["architecture"]!=architecture || value["source_suites"]!=50 || value["source_cases"]!=470
        || value["test_bodies_executed"]!=false || value["case_mapping_complete"]!=false {
        return Err(fail("Original runtime collection provenance differs."));
    }
    let mut suites=BTreeMap::new(); let mut suite_counts=BTreeMap::<String,usize>::new();
    for suite in list(&value,"suites")? {
        let source=field(suite,"source")?;name(source)?;
        if !source.starts_with("tests/") || !source.ends_with(".test.ts") || !hash(field(suite,"sha256")?)
            || suite["cases"].as_u64().is_none_or(|n|n==0 || n>470) || suites.insert(source,suite).is_some() {
            return Err(fail("Original source suite identity is invalid or duplicated."));
        }
    }
    let mut ids=BTreeSet::new();
    for case in list(&value,"cases")? {
        let id=field(case,"id")?;let source=field(case,"source")?;
        let suite=suites.get(source).ok_or_else(||fail("Original case refers to an uncollected suite."))?;
        if !hash(id) || !ids.insert(id) || case["source_sha256"]!=suite["sha256"] || field(case,"name")?.trim().is_empty()
            || !matches!(case["mode"].as_str(),Some("run"|"skip"|"todo"))
            || case["collection_state"]!=(if case["mode"]=="run" {"pending"} else {"skipped"})
            || case["location"].as_object().is_none_or(|v|v.len()!=2)
            || case["location"]["line"].as_u64().is_none_or(|n|n==0)
            || case["location"]["column"].as_u64().is_none_or(|n|n==0) {
            return Err(fail("Original expanded source-case identity differs."));
        }
        *suite_counts.entry(source.into()).or_default()+=1;
    }
    if ids.len()!=470 || suites.len()!=50 || suites.iter().any(|(path,suite)|suite["cases"].as_u64()!=suite_counts.get(*path).map(|n|*n as u64)) {
        return Err(fail("Original470case/50suite collection totals differ."));
    }
    Ok(value)
}

/// No extraction through TAR paths. Only exact, independently hash-bound regular-file bytes leave
/// this decoder; symlinks, hardlinks, duplicate names, unrequested members and size changes fail.
pub fn decode_archive(bytes: &[u8], expected: &BTreeMap<String,String>) -> io::Result<BTreeMap<String,Vec<u8>>> {
    if bytes.len()<1024 || bytes.len()>LIMIT || bytes.len()%512!=0 || bytes[bytes.len()-1024..].iter().any(|b|*b!=0)
        || expected.is_empty() || expected.len()>512 {
        return Err(fail("Historical Git archive exceeds its bound."));
    }
    for(path,digest)in expected {name(path)?;if !hash(digest){return Err(fail("Historical source hash is missing."));}}
    let mut archive=tar::Archive::new(Cursor::new(bytes));let mut result=BTreeMap::new();let mut members=BTreeSet::new();let mut total=0usize;let mut globals=0;
    for item in archive.entries()? {
        let mut item=item?;
        let path=item.path()?.to_str().ok_or_else(||fail("Historical member path must be UTF-8."))?.to_owned();
        let kind=item.header().entry_type();
        if kind.is_pax_global_extensions() { globals+=1;if globals>1 || !result.is_empty() || item.size()>4096 {return Err(fail("Historical Git metadata exceeds bound."));} continue; }
        let path=if kind.is_dir() {path.strip_suffix('/').unwrap_or(&path)} else {path.as_str()};name(path)?;
        if members.len()>=1024 || !members.insert(path.to_owned()){return Err(fail("Duplicate or oversized historical archive inventory."));}
        if kind.is_dir() {
            if !expected.keys().any(|p|p.starts_with(&format!("{path}/"))) || item.size()!=0 {return Err(fail("Unrequested historical directory."));}
            continue;
        }
        if !kind.is_file() || !expected.contains_key(path) || item.size()>LIMIT as u64 {return Err(fail("Nonregular or unrequested historical source member."));}
        let size=item.size();total=total.checked_add(size as usize).filter(|n|*n<=LIMIT).ok_or_else(||fail("Historical source bytes exceed bound."))?;
        let mut content=Vec::new();(&mut item).take(size+1).read_to_end(&mut content)?;
        if content.len() as u64!=size || sha(&content)!=expected[path] {return Err(fail("Historical Git source bytes differ from original bound hashes."));}
        result.insert(path.to_owned(),content);
    }
    // Git emits 10KiB TAR blocks, including up to one full block of terminal zero padding.
    let mut tail=Vec::new();archive.into_inner().take(10_241).read_to_end(&mut tail)?;
    if tail.len()>10_240 || tail.iter().any(|b|*b!=0) || result.len()!=expected.len(){return Err(fail("Historical source archive is truncated or incomplete."));}
    Ok(result)
}

pub fn validate_reference(bytes: &[u8]) -> io::Result<Value> {
    if sha(bytes)!=REFERENCE_SHA256{return Err(fail("Retained final historical execution reference changed."));}
    let value=ledger::parse(bytes)?;
    if value["source_sha"]!=SOURCE || value["baseline"]!=BASELINE || value["run_id"]!=40_244_894_089u64
        || value["run_attempt"]!=2 || value["full_acceptance"]!=false || value["native_execution_locally"]!=false
        || value["validation_only"]!=false || value["canonical_promotions"]!=0 {
        return Err(fail("Retained final execution reference identity differs."));
    }
    Ok(value)
}
fn source_hashes(collection: &Value) -> io::Result<BTreeMap<String,String>> {
    list(collection,"suites")?.iter().map(|suite|Ok((field(suite,"source")?.into(),field(suite,"sha256")?.into()))).collect()
}
fn historical_hashes(reference: &Value) -> io::Result<BTreeMap<String,String>> {
    let files=reference["current_git_files"].as_object().ok_or_else(||fail("Historical native source inventory missing."))?;
    let mut hashes=BTreeMap::new();
    for(path,digest)in files {
        name(path)?;
        if !(path.starts_with("native/") || path.starts_with("tests/rust/") || path.starts_with("scripts/")
            || path.starts_with("schemas/") || ["Cargo.toml","Cargo.lock","rust-toolchain.toml",".github/workflows/ci.yml"].contains(&path.as_str())) {
            return Err(fail("Historical source inventory contains an unexpected scope."));
        }
        let digest=digest.as_str().filter(|v|hash(v)).ok_or_else(||fail("Historical source digest is invalid."))?;
        hashes.insert(path.clone(),digest.into());
    }
    if hashes.len()!=256{return Err(fail("Historical native source inventory count differs."));}
    Ok(hashes)
}
fn archive(context: &Context, revision: &str, hashes: &BTreeMap<String,String>) -> io::Result<BTreeMap<String,Vec<u8>>> {
    // A fixed full commit plus explicit public paths; no checkout, shell, network, TS interpreter or
    // working-tree attributes. export-subst/export-ignore changes fail the independently pinned hashes.
    if ![BASELINE,SOURCE].contains(&revision){return Err(fail("Only immutable authenticated ledger source revisions are allowed."));}
    let mut command=context.command("git");
    for(key,_)in std::env::vars_os(){if key.to_str().is_some_and(|key|key.starts_with("GIT_")){command.env_remove(key);}}
    command.args(["--no-pager","--no-replace-objects","--literal-pathspecs","-c","core.fsmonitor=false","archive","--format=tar",revision,"--"]);
    command.args(hashes.keys());command.env("GIT_NO_LAZY_FETCH","1").env("GIT_TERMINAL_PROMPT","0");
    decode_archive(&context.capture(&mut command,30_000,15_000,LIMIT)?,hashes)
}
fn write_sources(context: &Context, root: &str, files: &BTreeMap<String,Vec<u8>>) -> io::Result<()> {
    for(path,bytes)in files {context.write_new(&format!("{root}/{path}"),bytes)?;}
    Ok(())
}

/// Locate whole lines only. Even an exact match is navigation, not a semantic review or execution.
pub fn assertion_range_proposal(historical: &str, current: &str, range: &Value) -> io::Result<Value> {
    if historical.len() > LIMIT || current.len() > LIMIT { return Err(fail("Assertion source exceeds bound.")); }
    let lines = historical.lines().collect::<Vec<_>>();
    let range = range.as_array().filter(|value| value.len() == 2)
        .ok_or_else(|| fail("Assertion requires a two-line bound."))?;
    let start = range[0].as_u64().and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0).ok_or_else(|| fail("Assertion start is invalid."))?;
    let end = range[1].as_u64().and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value >= start && *value <= lines.len())
        .ok_or_else(|| fail("Assertion end is invalid."))?;
    let excerpt = lines[start - 1..end].join("\n");
    if excerpt.trim().is_empty() { return Err(fail("Empty assertion excerpt cannot be rebound.")); }
    let current_lines = current.lines().collect::<Vec<_>>();
    let mut result = json!({"range_match":"changed","native_lines":Value::Null,
        "historical_excerpt_sha256":sha(excerpt.as_bytes()),"current_excerpt_sha256":Value::Null,
        "assertion_review":"pending","current_execution_claimed":false});
    let adapted = excerpt.replace("numera_image_gen_mcp", "numera_gen_mcp");
    for (kind, candidate) in [("exact_lines", excerpt.as_str()), ("crate_namespace_only", adapted.as_str())] {
        if kind == "crate_namespace_only" && adapted == excerpt { break; }
        let expected = candidate.split('\n').collect::<Vec<_>>();
        if expected.len() > current_lines.len() { continue; }
        let matches = current_lines.windows(expected.len()).enumerate()
            .filter_map(|(index, window)| (window == expected.as_slice()).then_some(index))
            .take(2).collect::<Vec<_>>();
        if matches.len() > 1 { result["range_match"] = json!("ambiguous"); return Ok(result); }
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
pub fn sdk_assertion_traceability(project: &Path, rows: &[Value], manifest: &Value) -> io::Result<Value> {
    ledger::validate_rows(rows)?;
    let review = &manifest["assertion_traceability"];
    if manifest["schema_version"] != 1 || manifest["feature"] != "003-rust-tooling"
        || review["schema_version"] != 1 || review["status"] != "source_traceability_only"
        || review["semantic_parity_claimed"] != false || review["current_execution_claimed"] != false {
        return Err(fail("SDK assertion traceability cannot inherit semantic or execution approval."));
    }
    let mut obligations = BTreeMap::new(); let mut records = 0usize;
    for row in rows {
        for assertion in list(row, "assertions")? {
            let file = field(assertion, "native_file")?;
            if !file.ends_with(".mjs") { continue; }
            let mappings = list(row, "native")?.iter().filter_map(|value| value.as_str()
                .and_then(|value| value.split_once("::")).filter(|(path, _)| *path == file)).collect::<Vec<_>>();
            if mappings.len() != 1 { return Err(fail("Original SDK assertion requires one explicit historical mapping.")); }
            let (_, _, symbol) = SDK_REBINDS.iter().find(|(path, name, _)| *path == mappings[0].0 && *name == mappings[0].1)
                .ok_or_else(|| fail("SDK assertion has no registered Rust mapping."))?;
            let mapping = format!("tests/rust/sdk_acceptance.rs::{symbol}");
            let digest = ledger::assertion_digest(&json!([assertion]))?;
            if let Some((previous, previous_mapping)) = obligations.insert(digest, (assertion, mapping.clone()))
                && (previous != assertion || previous_mapping != mapping) {
                return Err(fail("Original SDK assertion identity has conflicting mappings."));
            }
            records += 1;
        }
    }
    let mut sources = BTreeMap::new(); let mut files = BTreeMap::new();
    for path in std::iter::once("tests/rust/sdk_acceptance.rs".to_owned()).chain(
        ["host", "provider", "fixtures", "catalog", "configuration", "reload", "recovery", "lifecycle", "transport", "terminal", "races", "http", "contracts"]
            .into_iter().map(|module| format!("tests/rust/sdk_acceptance/{module}.rs"))) {
        let bytes = ledger::read(project, &path)?;
        files.insert(path.clone(), sha(&bytes));
        sources.insert(path, String::from_utf8(bytes).map_err(|_| fail("SDK source must be UTF-8."))?);
    }
    let bindings = list(review, "bindings")?;
    if obligations.is_empty() || bindings.len() != obligations.len() { return Err(fail("Every original SDK assertion needs exactly one source binding.")); }
    let mut seen = BTreeSet::new(); let mut checked = BTreeSet::new();
    for binding in bindings {
        let digest = field(binding, "historical_assertion_sha256")?;
        let (original, expected_mapping) = obligations.get(digest).ok_or_else(|| fail("SDK source binding has an unknown historical obligation."))?;
        let mapping = field(binding, "current_mapping")?;
        if !seen.insert(digest) || mapping != expected_mapping || binding["historical_file"] != original["native_file"]
            || binding["historical_lines"] != original["native_lines"] || binding["source_lines"] != original["source_lines"] {
            return Err(fail("SDK source binding duplicates or changes its original obligation."));
        }
        if checked.insert(mapping) {
            let (file, symbol) = mapping.split_once("::").ok_or_else(|| fail("SDK binding test symbol missing."))?;
            ledger::mapping_source(project, file, "::", symbol)?;
        }
        let entry = list(manifest, "entries")?.iter().find(|entry| entry["source"] == original["native_file"])
            .ok_or_else(|| fail("SDK source binding has no scenario entry."))?;
        let modules = list(entry, "modules")?;
        let ranges = list(binding, "ranges")?;
        if ranges.is_empty() || ranges.len() > 16 { return Err(fail("SDK source ranges are missing or oversized.")); }
        let mut seen_ranges = BTreeSet::new();
        for range in ranges {
            let file = field(range, "file")?;
            if !modules.iter().any(|module| module.as_str() == Some(file)) { return Err(fail("SDK source range is outside its scenario modules.")); }
            let source = sources.get(file).ok_or_else(|| fail("SDK source range is not a maintained acceptance module."))?;
            let lines = source.lines().collect::<Vec<_>>();
            let bounds = list(range, "lines")?;
            let start = bounds.first().and_then(Value::as_u64).and_then(|value| usize::try_from(value).ok()).unwrap_or(0);
            let end = bounds.get(1).and_then(Value::as_u64).and_then(|value| usize::try_from(value).ok()).unwrap_or(0);
            if bounds.len() != 2 || start == 0 || end < start || end > lines.len() || !seen_ranges.insert((file, start, end)) {
                return Err(fail("SDK source range is invalid or duplicated."));
            }
            let excerpt = lines[start - 1..end].join("\n");
            if excerpt.trim().is_empty() || !hash(field(range, "excerpt_sha256")?) || range["excerpt_sha256"] != sha(excerpt.as_bytes()) {
                return Err(fail("SDK source excerpt differs from its reviewed traceability range."));
            }
        }
    }
    Ok(json!({"schema_version":1,"component":"sdk-assertion-source-traceability",
        "unique_obligations":seen.len(),"original_records":records,"bindings":bindings,"current_source_files":files,
        "semantic_parity_claimed":false,"current_execution_claimed":false}))
}

/// Proposals are source navigation, never transplanted assertion ranges or current execution proof.
/// Every original row (including old status/review/evidence) is retained verbatim as historical data.
pub fn current_rebind_map(project: &Path, rows: &[Value], reference: &Value, current_sha: &str) -> io::Result<Value> {
    ledger::validate_rows(rows)?;
    if reference["source_sha"]!=SOURCE || reference["baseline"]!=BASELINE {return Err(fail("Current rebind requires the immutable historical reference."));}
    let mut reference_cases=BTreeMap::new();
    for case in list(reference,"cases")? {if reference_cases.insert(field(case,"id")?,case).is_some(){return Err(fail("Historical reference duplicates a case."));}}
    if reference_cases.len()!=470{return Err(fail("Current rebind requires all original reference cases."));}
    if current_sha.len()!=40 || !current_sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err(fail("Current source revision identity is required."));
    }
    let mut checked=BTreeMap::<String,String>::new();let mut files=BTreeMap::<String,String>::new();let mut cases=Vec::new();
    for row in rows {
        let original=reference_cases.get(field(row,"id")?).ok_or_else(||fail("Current rebind source case is not in historical reference."))?;
        for key in ["source","source_sha256","platform_modes","native"] {if original[key]!=row[key]{return Err(fail("Current rebind cannot alter historical case identity."));}}
        if original["assertions_sha256"]!=ledger::assertion_digest(&row["assertions"])?{return Err(fail("Current rebind cannot relabel historical assertions."));}
        let mut proposed=Vec::new();
        for old in list(row,"native")? {
            let old=old.as_str().ok_or_else(||fail("Historical native mapping is invalid."))?;
            let (old_file,old_symbol)=old.split_once("::").ok_or_else(||fail("Historical native mapping lacks symbol."))?;
            let (file,symbol)=if old_file.ends_with(".mjs") {
                let (_,_,symbol)=SDK_REBINDS.iter().find(|(file,symbol,_)| *file==old_file && *symbol==old_symbol)
                    .ok_or_else(||fail("Historical interpreted mapping has no explicit Rust migration proposal."))?;
                ("tests/rust/sdk_acceptance.rs",*symbol)
            } else {(old_file,old_symbol)};
            let historical_hash=field(&reference["current_git_files"],old_file)?;
            if !hash(historical_hash) || original["native_sha256"][old_file]!=historical_hash{return Err(fail("Historical mapping bytes lack the retained execution binding."));}
            let mapping=format!("{file}::{symbol}");
            if !checked.contains_key(&mapping) {
                let source=ledger::mapping_source(project,file,"::",symbol)?;
                files.insert(file.into(),sha(source.as_bytes()));checked.insert(mapping.clone(),sha(source.as_bytes()));
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
    for (path, digest) in traceability["current_source_files"].as_object()
        .ok_or_else(|| fail("SDK traceability source inventory missing."))? {
        let digest = digest.as_str().filter(|value| hash(value))
            .ok_or_else(|| fail("SDK traceability source digest missing."))?;
        files.insert(path.clone(), digest.to_owned());
    }
    files.insert(manifest_path.into(), sha(&manifest_bytes));
    Ok(json!({"schema_version":1,"component":"native-case-source-rebind","baseline":BASELINE,"historical_source_sha":SOURCE,
        "historical_reference_sha256":REFERENCE_SHA256,"current_source_sha":current_sha,"cases":cases,"current_source_files":files,
        "sdk_assertion_traceability":traceability,
        "original_evidence_rewritten":false,"current_assertion_review":"pending","current_execution_claimed":false,"acceptance_verified":false}))
}

pub fn check(context: &Context) -> io::Result<Value> {
    context.hosted()?;
    let mut collections=Vec::new();let mut collection_records=Vec::new();
    for(label,platform,digest,artifact)in COLLECTIONS {
        let path=format!("tests/fixtures/oracle/{label}/source-cases.json");let bytes=ledger::read(&context.root,&path)?;
        collections.push(validate_collection(&bytes,platform)?);
        collection_records.push(json!({"platform":platform,"sha256":digest,"original_artifact_sha256":artifact,"source_collection_run_id":38_367_053_960u64}));
    }
    let receipt=ledger::read(&context.root,REFERENCE_FILE)?;let reference=validate_reference(&receipt)?;
    let suite_hashes=source_hashes(&collections[0])?;
    for collection in &collections[1..] {if source_hashes(collection)?!=suite_hashes{return Err(fail("Original platform suite inventories differ."));}}
    let mut frozen_hashes=reference["frozen_git_files"].as_object()
        .ok_or_else(||fail("Historical frozen source hashes missing."))?.iter()
        .map(|(path,value)|Ok((path.clone(),value.as_str().filter(|value|hash(value))
            .ok_or_else(||fail("Historical frozen source digest invalid."))?.to_owned())))
        .collect::<io::Result<BTreeMap<_,_>>>()?;
    if frozen_hashes.len()!=102 || suite_hashes.iter().any(|(path,digest)|frozen_hashes.get(path)!=Some(digest)) {
        return Err(fail("Historical frozen source differs from original collected suites."));
    }
    frozen_hashes.insert("scripts/bounded.mjs".into(),"a12ce2c6b5f226ac5f1b0460fa48345171e0a1512b3fd848d91d0a8444b1d47b".into());
    // Authenticate/decode all source bytes before creating the isolated source tree.
    let frozen=archive(context,BASELINE,&frozen_hashes)?;let historical=archive(context,SOURCE,&historical_hashes(&reference)?)?;
    let supervisor=historical.get("scripts/bounded.mjs").ok_or_else(||fail("Historical maintained supervisor missing."))?;
    let validation=crate::validation::prepare(&frozen,supervisor)?;
    crate::validation::verify(&frozen,supervisor,&validation)?;
    let workspace=format!(".ci-work/native/ledger-inputs-{}",uuid::Uuid::new_v4());let path=context.relative(&workspace)?;fs::create_dir(&path)?;
    let operation=(|| {
        write_sources(context,&format!("{workspace}/baseline"),&frozen)?;write_sources(context,&format!("{workspace}/historical"),&historical)?;
        let ledger_root=context.root.join("docs/testing/native-cases");
        let result=ledger::validate_reference(&path.join("historical"),&path.join("baseline"),&collections,&ledger_root,&receipt,REFERENCE_SHA256)?;
        let(_,rows)=ledger::load(&ledger_root)?;
        let parity=ledger::parse(&ledger::read(&context.root,"docs/testing/native-parity.json")?)?;
        let suites=ledger::validate_reference_parity(&path.join("historical"),&path.join("baseline"),&parity,&rows)?;
        let mut rebind=current_rebind_map(&context.root,&rows,&reference,&context.source_sha()?)?;
        let sdk_bindings = list(&rebind["sdk_assertion_traceability"], "bindings")?.iter()
            .map(|binding| Ok((field(binding, "historical_assertion_sha256")?.to_owned(), binding.clone())))
            .collect::<io::Result<BTreeMap<_, _>>>()?;
        let mut current_sources=BTreeMap::<String,String>::new();let mut range_counts=BTreeMap::<String,usize>::new();
        for case in rebind["cases"].as_array_mut().ok_or_else(||fail("Current case proposals missing."))? {
            let mut proposals=Vec::new();
            for assertion in list(&case["historical"],"assertions")? {
                let file=field(assertion,"native_file")?;
                let mut proposal=if file.ends_with(".rs") {
                    if !current_sources.contains_key(file) {
                        let bytes=ledger::read(&context.root,file)?;
                        current_sources.insert(file.into(),String::from_utf8(bytes).map_err(|_|fail("Current assertion source must be UTF-8."))?);
                    }
                    let original=historical.get(file).ok_or_else(||fail("Historical assertion source missing."))?;
                    let original=std::str::from_utf8(original).map_err(|_|fail("Historical assertion source must be UTF-8."))?;
                    assertion_range_proposal(original,&current_sources[file],&assertion["native_lines"])?
                }else{
                    let digest = ledger::assertion_digest(&json!([assertion]))?;
                    let binding = sdk_bindings.get(&digest).ok_or_else(|| fail("Original SDK assertion source binding missing."))?;
                    json!({"range_match":"sdk_source_traceability_only","native_lines":Value::Null,
                        "source_binding":binding,"assertion_review":"pending","current_execution_claimed":false})
                };
                *range_counts.entry(field(&proposal,"range_match")?.into()).or_default()+=1;
                proposal["historical_native_file"]=json!(file);
                proposal["historical_native_lines"]=assertion["native_lines"].clone();
                proposal["source_lines"]=assertion["source_lines"].clone();
                proposal["historical_claim"]=assertion["claim"].clone();
                proposals.push(proposal);
            }
            case["current"]["assertion_proposals"]=json!(proposals);
        }
        rebind["assertion_range_counts"]=json!(range_counts);
        let map_path=format!(".ci-work/native/current-source-case-rebind-{}.json",uuid::Uuid::new_v4());
        let bytes=serde_json::to_vec_pretty(&rebind).map_err(|_|fail("Cannot serialize current source rebind report."))?;
        context.write_new(&map_path,&bytes)?;
        Ok(json!({"component":"native-case-ledger","cases":470,"suites":50,"historical_reference":result,"historical_suite_parity":suites,
            "collections":collection_records,"current_source_rebind":map_path,"rebind_sha256":sha(&bytes),
            "historical_inputs_materialized":true,"historical_validation_delta":validation.receipt,
            "expectations_generated_by_target":false,"test_bodies_executed":false,
            "original_evidence_rewritten":false,"current_execution_claimed":false,"acceptance_verified":false}))
    })();
    if fs::remove_dir_all(&path).is_err(){return Err(fail("Owned historical source materialization cleanup failed."));}
    operation
}
