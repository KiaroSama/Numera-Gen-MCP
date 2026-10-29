use crate::context::{Context, fail, sha};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, fs, io, path::{Component, Path}};
use serde::{Deserialize, Deserializer, de::{MapAccess, SeqAccess, Visitor}};

const LIMIT: usize = 16 * 1024 * 1024;
const PLATFORMS: [&str; 3] = ["win32", "linux", "darwin"];
const SDK: [(&str, &str); 10] = [
    ("scripts/native-tools-acceptance.mjs", "toolsAcceptance"),
    ("scripts/native-two-host-acceptance.mjs", "twoHostAcceptance"),
    ("scripts/native-compact-acceptance.mjs", "compactAcceptance"),
    ("scripts/native-env-acceptance.mjs", "envAcceptance"),
    ("scripts/native-negotiation-acceptance.mjs", "negotiationAcceptance"),
    ("scripts/native-config-reload-acceptance.mjs", "configReloadAcceptance"),
    ("scripts/native-terminal-acceptance.mjs", "terminalAcceptance"),
    ("scripts/native-races-acceptance.mjs", "racesAcceptance"),
    ("scripts/native-lifecycle-acceptance.mjs", "lifecycleAcceptance"),
    ("scripts/native-transport-acceptance.mjs", "transportAcceptance"),
];
#[derive(Clone, Copy)]
enum MappingPolicy { Rust, Historical, Reference }
fn hex(value: &str, length: usize) -> bool { value.len() == length && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn string<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> { value[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| fail("Required ledger identity is missing.")) }
fn array<'a>(value: &'a Value, key: &str) -> io::Result<&'a Vec<Value>> { value[key].as_array().ok_or_else(|| fail("Required ledger array is missing.")) }
fn relative(name: &str) -> io::Result<()> {
    if name.is_empty() || name.bytes().any(|byte|byte==b'\\'||byte==b':'||byte==0) || Path::new(name).components().any(|c| !matches!(c, Component::Normal(_))) || name.split('/').any(|p| p.is_empty() || p == "." || p == "..") { Err(fail("Ledger path is not a contained public file.")) } else { Ok(()) }
}
pub(crate) fn read(root: &Path, name: &str) -> io::Result<Vec<u8>> {
    relative(name)?;
    let root = root.canonicalize()?; let mut path = root.clone();
    for part in name.split('/') { path.push(part); let info = fs::symlink_metadata(&path)?;
        #[cfg(windows)] { use std::os::windows::fs::MetadataExt; if info.file_attributes() & 0x400 != 0 { return Err(fail("Linked ledger member rejected.")); } }
        if info.is_symlink() { return Err(fail("Linked ledger member rejected.")); }
    }
    if !path.canonicalize()?.starts_with(&root) { return Err(fail("Ledger member escapes root.")); }
    // Reuse the maintained handle-verified regular-file reader, including per-platform link,
    // nonblocking/no-follow, one-link, identity and bounded-read checks.
    crate::distribution::read_regular(&path, LIMIT)
}
fn text(bytes: &[u8]) -> io::Result<&str> { std::str::from_utf8(bytes).map_err(|_| fail("Ledger evidence must be UTF-8.")) }
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Strict;
        impl<'de> Visitor<'de> for Strict {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("bounded JSON without duplicate keys") }
            fn visit_bool<E: serde::de::Error>(self,v:bool)->Result<Unique,E>{Ok(Unique(json!(v)))}
            fn visit_i64<E: serde::de::Error>(self,v:i64)->Result<Unique,E>{Ok(Unique(json!(v)))}
            fn visit_u64<E: serde::de::Error>(self,v:u64)->Result<Unique,E>{Ok(Unique(json!(v)))}
            fn visit_f64<E: serde::de::Error>(self,v:f64)->Result<Unique,E>{serde_json::Number::from_f64(v).map(|v|Unique(Value::Number(v))).ok_or_else(||E::custom("nonfinite JSON number"))}
            fn visit_str<E: serde::de::Error>(self,v:&str)->Result<Unique,E>{Ok(Unique(json!(v)))}
            fn visit_string<E: serde::de::Error>(self,v:String)->Result<Unique,E>{Ok(Unique(json!(v)))}
            fn visit_unit<E: serde::de::Error>(self)->Result<Unique,E>{Ok(Unique(Value::Null))}
            fn visit_none<E: serde::de::Error>(self)->Result<Unique,E>{Ok(Unique(Value::Null))}
            fn visit_seq<A: SeqAccess<'de>>(self,mut sequence:A)->Result<Unique,A::Error>{let mut values=Vec::new();while let Some(value)=sequence.next_element::<Unique>()?{values.push(value.0);}Ok(Unique(Value::Array(values)))}
            fn visit_map<A: MapAccess<'de>>(self,mut map:A)->Result<Unique,A::Error>{let mut values=serde_json::Map::new();while let Some(key)=map.next_key::<String>()?{if values.contains_key(&key){return Err(serde::de::Error::custom("duplicate JSON key"));}values.insert(key,map.next_value::<Unique>()?.0);}Ok(Unique(Value::Object(values)))}
        }
        deserializer.deserialize_any(Strict)
    }
}
pub(crate) fn parse(bytes: &[u8]) -> io::Result<Value> {
    if bytes.len()>LIMIT{return Err(fail("Ledger JSON exceeds bound."));}
    serde_json::from_slice::<Unique>(text(bytes)?.as_bytes()).map(|v|v.0).map_err(|_|fail("Invalid or duplicate-key ledger JSON."))
}
pub fn load(root: &Path) -> io::Result<(Value, Vec<Value>)> {
    let index = parse(&read(root, "index.json")?)?;
    if index["baseline"] != crate::fixtures::BASELINE || index["source_cases"] != 470 || index["source_suites"] != 50 { return Err(fail("Frozen case index identity differs.")); }
    let mut files = BTreeSet::new(); let mut rows = Vec::new();
    for name in array(&index, "files")? {
        let name = name.as_str().ok_or_else(|| fail("Invalid ledger filename."))?;
        if !["unit.jsonl", "contract.jsonl", "integration.jsonl"].contains(&name) || !files.insert(name) { return Err(fail("Unexpected or duplicate case file.")); }
        for line in text(&read(root, name)?)?.lines() { rows.push(parse(line.as_bytes())?); }
    }
    if files.len() != 3 { return Err(fail("Every source case group is required.")); } Ok((index, rows))
}

pub fn validate_rows(rows: &[Value]) -> io::Result<()> {
    if rows.len() != 470 { return Err(fail("All470source cases required.")); }
    let (mut ids, mut sources) = (BTreeSet::new(), BTreeSet::new());
    for row in rows {
        let id = string(row, "id")?;
        if !hex(id, 64) || !ids.insert(id) { return Err(fail("Missing or duplicate case identity.")); }
        let source = string(row, "source")?; relative(source)?;
        if !source.starts_with("tests/") || !source.ends_with(".test.ts") { return Err(fail("Invalid frozen source path.")); } sources.insert(source);
        if !hex(string(row, "source_sha256")?, 64) || string(row, "name")?.trim().is_empty() { return Err(fail("Case source identity incomplete.")); }
        let native = array(row, "native")?; let assertions = array(row, "assertions")?;
        if !matches!(row["status"].as_str(), Some("pending" | "verified")) { return Err(fail("Unsupported case disposition; silent retirement is forbidden.")); }
        if row["status"] == "verified" && (native.is_empty() || assertions.is_empty()) { return Err(fail("Case assertions are pending.")); }
        let modes = row["platform_modes"].as_object().ok_or_else(|| fail("Case applicability missing."))?;
        if modes.len() != 3 || PLATFORMS.iter().any(|p| !matches!(modes[*p].as_str(), Some("run" | "skip" | "todo"))) { return Err(fail("Invalid source applicability.")); }
    }
    if sources.len() != 50 { return Err(fail("All50source suites required.")); } Ok(())
}

#[derive(Clone)]
struct Function { name: String, test: bool }
// Mask comments and literals while preserving byte offsets/newlines. It prevents commented-out
// tests and strings containing fake attributes/functions from satisfying mapping registration.
fn code(text: &str) -> io::Result<String> {
    let bytes = text.as_bytes(); let mut out = bytes.to_vec(); let mut n = 0;
    while n < bytes.len() {
        let start = n;
        if bytes[n..].starts_with(b"//") { while n < bytes.len() && bytes[n] != b'\n' { n += 1; } }
        else if bytes[n..].starts_with(b"/*") { n += 2; let mut depth = 1; while n < bytes.len() && depth > 0 { if bytes[n..].starts_with(b"/*") { depth += 1; n += 2; } else if bytes[n..].starts_with(b"*/") { depth -= 1; n += 2; } else { n += 1; } } if depth != 0 { return Err(fail("Unterminated native mapping comment.")); } }
        else {
            let raw_start = if bytes[n] == b'r' { Some(n + 1) } else if bytes[n..].starts_with(b"br") || bytes[n..].starts_with(b"cr") { Some(n + 2) } else { None };
            let mut raw = None;
            if let Some(mut end) = raw_start { let hashes = end; while end < bytes.len() && bytes[end] == b'#' { end += 1; } if bytes.get(end) == Some(&b'"') { raw = Some((end + 1, end - hashes)); } }
            if let Some((mut end, hashes)) = raw { let mut closed = false; while end < bytes.len() { if bytes[end] == b'"' && bytes.get(end + 1..end + 1 + hashes).is_some_and(|s| s.iter().all(|b| *b == b'#')) { n = end + 1 + hashes; closed = true; break; } end += 1; } if !closed { return Err(fail("Unterminated native mapping literal.")); } }
            else if bytes[n] == b'"' || (bytes[n] == b'\'' && (bytes.get(n + 2) == Some(&b'\'') || bytes.get(n + 1) == Some(&b'\\'))) { let quote = bytes[n]; n += 1; let mut closed = false; while n < bytes.len() { if bytes[n] == b'\\' { n = (n + 2).min(bytes.len()); } else if bytes[n] == quote { n += 1; closed = true; break; } else { n += 1; } } if !closed { return Err(fail("Unterminated native mapping literal.")); } }
            else { n += 1; continue; }
        }
        for byte in &mut out[start..n] { if *byte != b'\n' && *byte != b'\r' { *byte = b' '; } }
    }
    String::from_utf8(out).map_err(|_| fail("Native mapping source must be UTF-8."))
}
fn tokens(text: &str) -> Vec<String> {
    let mut result = Vec::new(); let mut word = String::new();
    for ch in text.chars() { if ch.is_ascii_alphanumeric() || ch == '_' { word.push(ch); } else { if !word.is_empty() { result.push(std::mem::take(&mut word)); } if !ch.is_whitespace() { result.push(ch.to_string()); } } }
    if !word.is_empty() { result.push(word); } result
}
fn functions(text: &str) -> io::Result<Vec<Function>> {
    let tokens = tokens(&code(text)?); let mut functions = Vec::new(); let mut attributes = Vec::<Vec<String>>::new(); let mut n = 0;
    let mut scopes=Vec::<bool>::new();let mut module_brace=false;
    // Test functions must be real module items. A matching token inside a function, impl,
    // macro definition/invocation or arbitrary block is not a registered test declaration.
    while n < tokens.len() {
        if tokens[n]=="{"{scopes.push(module_brace);module_brace=false;attributes.clear();n+=1;continue;}
        if tokens[n]=="}"{scopes.pop();attributes.clear();n+=1;continue;}
        if scopes.iter().any(|module|!*module){attributes.clear();n+=1;continue;}
        if tokens[n]=="mod" && tokens.get(n+2).is_some_and(|t|t=="{"){module_brace=true;}
        if tokens[n] == "#" && tokens.get(n + 1).is_some_and(|s| s == "[") { let mut end = n + 2; let mut depth = 1; while end < tokens.len() && depth > 0 { if tokens[end] == "[" { depth += 1; } if tokens[end] == "]" { depth -= 1; } end += 1; } if depth != 0 { return Err(fail("Invalid native test attribute.")); } attributes.push(tokens[n + 2..end - 1].to_vec()); n = end; continue; }
        if tokens[n] == "fn" { if let Some(name) = tokens.get(n + 1) { if tokens.get(n + 2).is_some_and(|s| s == "(") { let test = attributes.iter().any(|a| (a.len()==1 && a[0]=="test") || (a.iter().take(4).map(String::as_str).eq(["tokio",":",":","test"]) && (a.len()==4 || a.get(4).is_some_and(|token|token=="(")))); let ignored = attributes.iter().any(|a| a.first().is_some_and(|s| s == "ignore")); if ignored && test { return Err(fail("Ignored native mapping cannot prove execution.")); } functions.push(Function { name: name.clone(), test }); } } attributes.clear(); n += 1; continue; }
        if !["pub", "async", "unsafe", "extern", "(", ")", "crate", "super", "self"].contains(&tokens[n].as_str()) && !tokens[n].starts_with("cfg") { attributes.clear(); }
        n += 1;
    }
    Ok(functions)
}
fn mapping<'a>(project: &Path, path: &str, separator: &str, symbol: &str, policy: MappingPolicy, cache: &'a mut BTreeMap<String, String>) -> io::Result<&'a str> {
    if separator != "::" || symbol.is_empty() || !symbol.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') { return Err(fail("Native mapping requires an exact test symbol.")); }
    if !cache.contains_key(path) { cache.insert(path.into(), text(&read(project, path)?)?.into()); }
    let source = &cache[path];
    if let Some((_, registered)) = SDK.iter().find(|(file, _)| *file == path) {
        if !matches!(policy, MappingPolicy::Historical | MappingPolicy::Reference) || *registered != symbol || !source.contains(&format!("export async function {symbol}(")) { return Err(fail("Interpreted SDK mapping requires historical evidence, not maintained Rust execution.")); }
        let caller = text(&read(project, "scripts/native-interop.mjs")?)?.to_owned();
        let name = Path::new(path).file_name().and_then(|s| s.to_str()).ok_or_else(|| fail("Invalid SDK fixture path."))?;
        if !caller.contains(&format!("import {{ {symbol} }} from \"./{name}\";")) || !caller.contains(&format!("await {symbol}(")) { return Err(fail("Historical SDK fixture is not executed.")); }
        for entry in ["scripts/native-ci.py", "scripts/native-extract-check.py"] { if !text(&read(project, entry)?)?.contains("scripts/native-interop.mjs") { return Err(fail("Historical SDK fixture lacks hosted/extracted execution.")); } }
    } else {
        if !path.ends_with(".rs") || !(path.starts_with("native/") || path.starts_with("tests/rust/")) { return Err(fail("Native mapping must identify an actual Rust test.")); }
        let found = functions(source)?.into_iter().filter(|f| f.name == symbol).collect::<Vec<_>>();
        if found.len() != 1 || !found[0].test { return Err(fail("Mapped native symbol is missing, ambiguous or not an attributed test.")); }
    }
    Ok(&cache[path])
}
pub fn mapping_source(project: &Path, path: &str, separator: &str, symbol: &str) -> io::Result<String> { Ok(mapping(project, path, separator, symbol, MappingPolicy::Rust, &mut BTreeMap::new())?.into()) }
pub fn historical_mapping_source(project: &Path, path: &str, separator: &str, symbol: &str) -> io::Result<String> { Ok(mapping(project, path, separator, symbol, MappingPolicy::Historical, &mut BTreeMap::new())?.into()) }
fn bounds(value: &Value, count: usize) -> io::Result<()> { let values = value.as_array().ok_or_else(|| fail("Assertion line evidence missing."))?; let a = values.first().and_then(Value::as_u64).unwrap_or(0); let b = values.get(1).and_then(Value::as_u64).unwrap_or(0); if values.len() != 2 || a == 0 || b < a || b > count as u64 { Err(fail("Assertion line evidence is invalid.")) } else { Ok(()) } }
fn location(value: &Value, source: &str) -> io::Result<()> { let row = value.as_object().ok_or_else(|| fail("Case declaration location missing."))?; if row.len() != 2 || row.get("line").and_then(Value::as_u64).is_none_or(|n| n == 0 || n > source.lines().count() as u64) || row.get("column").and_then(Value::as_u64).is_none_or(|n| n == 0) { Err(fail("Case declaration location is invalid.")) } else { Ok(()) } }

pub fn validate(project: &Path, source: &Path, collection: &Value, ledger_root: &Path, acceptance: bool) -> io::Result<Value> { validate_with(project, source, collection, ledger_root, acceptance, MappingPolicy::Rust) }
pub fn validate_historical(project: &Path, source: &Path, collection: &Value, ledger_root: &Path, acceptance: bool) -> io::Result<Value> { validate_with(project, source, collection, ledger_root, acceptance, MappingPolicy::Historical) }
fn validate_with(project: &Path, source: &Path, collection: &Value, ledger_root: &Path, acceptance: bool, policy: MappingPolicy) -> io::Result<Value> {
    let (index, rows) = load(ledger_root)?; validate_rows(&rows)?;
    if collection["baseline"] != crate::fixtures::BASELINE || collection["source_cases"] != 470 || collection["source_suites"] != 50 || collection["test_bodies_executed"] != false { return Err(fail("Expanded frozen collection identity differs.")); }
    let platform = string(collection, "platform")?; if !PLATFORMS.contains(&platform) { return Err(fail("Unsupported source collection platform.")); }
    let mut expected = BTreeMap::new();
    for case in array(collection, "cases")? { if expected.insert(string(case, "id")?, case).is_some() { return Err(fail("Expanded case identities are duplicated.")); } }
    if expected.len() != 470 || rows.iter().any(|row| !expected.contains_key(row["id"].as_str().unwrap_or(""))) { return Err(fail("Every expanded source case must appear exactly once.")); }
    let (mut sources, mut mappings, mut native_hashes) = (BTreeMap::<String, String>::new(), BTreeMap::<String, String>::new(), BTreeMap::<String, String>::new());
    let mut registered=BTreeSet::<String>::new();
    let (mut pending, mut mapped, mut historical) = (0, 0, false);
    for row in &rows {
        let case = expected[string(row, "id")?];
        for key in ["source", "source_sha256", "name", "location"] { if row.get(key).is_none() || row[key] != case[key] { return Err(fail("Case identity differs from independent runtime collection.")); } }
        let path = string(row, "source")?;
        if !sources.contains_key(path) { sources.insert(path.into(), text(&read(source, path)?)?.into()); }
        let frozen = &sources[path];
        if row["source_sha256"] != sha(frozen.as_bytes()) { return Err(fail("Frozen source assertion identity is stale.")); } location(&row["location"], frozen)?;
        if row["platform_modes"][platform] != case["mode"] { return Err(fail("Case platform applicability differs from collection.")); }
        let mut native_files = BTreeSet::new();let mut row_mappings=BTreeSet::new();
        for value in array(row, "native")? { let value = value.as_str().ok_or_else(|| fail("Invalid native mapping."))?;if !row_mappings.insert(value){return Err(fail("Duplicate native case mapping."));} let (file, symbol) = value.split_once("::").ok_or_else(|| fail("Native mapping requires a symbol."))?; if registered.insert(value.into()){mapping(project, file, "::", symbol, policy, &mut mappings)?;} historical |= file.ends_with(".mjs"); native_files.insert(file); if !native_hashes.contains_key(file) { native_hashes.insert(file.into(), sha(mappings[file].as_bytes())); } }
        for assertion in array(row, "assertions")? { let file = string(assertion, "native_file")?; if !native_files.contains(file) || string(assertion, "claim")?.trim().is_empty() { return Err(fail("Assertion must reference its mapped native test.")); } bounds(&assertion["source_lines"], frozen.lines().count())?; bounds(&assertion["native_lines"], mappings[file].lines().count())?; }
        if !native_files.is_empty() && !array(row, "assertions")?.is_empty() { mapped += 1; }
        if row["status"] == "pending" { pending += 1; } else {
            if row["gaps"].as_array().is_none_or(|a| !a.is_empty()) || row["review"].as_object().is_none_or(|r| r.is_empty()) || row["review"]["source_assertions"]!="equivalent" || row["review"]["confirmed_gaps"]!=0 { return Err(fail("Verified case has incomplete assertion review.")); }
            if matches!(policy, MappingPolicy::Reference) { continue; }
            let evidence = &row["evidence"];
            if !hex(string(evidence, "sha")?, 40) || evidence["run_id"].as_u64().is_none_or(|n| n == 0) || evidence["run_attempt"].as_u64().is_none_or(|n| n == 0) || !hex(string(evidence, "execution_binding_sha256")?, 64) { return Err(fail("Verified case has no exact native execution identity.")); }
            if evidence.get("source_only_rebinding").is_some_and(|v|!v.is_null()) && evidence["source_only_rebinding"]["current_tree_execution_claimed"]!=false{return Err(fail("Source-only rebinding cannot claim current execution."));}
            let targets = array(evidence, "passed_targets")?;
            let unique=targets.iter().filter_map(Value::as_str).collect::<BTreeSet<_>>();if unique.len()!=targets.len()||unique.iter().any(|p|!PLATFORMS.contains(p)){return Err(fail("Invalid or duplicate passed platform evidence."));}
            for file in native_files { if evidence["native_sha256"][file] != native_hashes[file] { return Err(fail("Native assertion evidence is stale.")); } }
            for platform in PLATFORMS { if row["platform_modes"][platform] == "run" && !targets.iter().any(|p| p == platform) { return Err(fail("Applicable native case execution is missing.")); } }
        }
    }
    if sources.len() != 50 { return Err(fail("Case ledger suite totals differ.")); }
    let discovered=inventory(source)?;
    let actual_suites=array(&discovered,"suites")?.iter().map(|row|row["source"].as_str().unwrap()).collect::<BTreeSet<_>>();
    if sources.keys().map(String::as_str).collect::<BTreeSet<_>>()!=actual_suites{return Err(fail("Frozen source suite discovery differs from case collection."));}
    if acceptance && (pending != 0 || index["mapping_complete"] != true) { return Err(fail("Complete native assertion parity is still pending.")); }
    if acceptance { let mut digests = BTreeSet::new(); for row in &rows { digests.insert(string(&row["evidence"], "execution_binding_sha256")?); } for digest in digests { let bytes = read(ledger_root, &format!("execution/{digest}.json"))?; validate_execution_binding(&rows, &bytes, digest)?; } }
    Ok(json!({"component":"native-case-ledger","cases":rows.len(),"suites":sources.len(),"assertion_assignments":mapped,"pending":pending,"structural_verified":true,"acceptance_verified":acceptance && !historical,"historical_acceptance_verified":acceptance && historical,"current_execution_claimed":false}))
}

pub fn inventory(source: &Path) -> io::Result<Value> {
    fn walk(root: &Path, current: &Path, found: &mut Vec<Value>) -> io::Result<()> { if found.len()>50 || current.components().count().saturating_sub(root.components().count())>16{return Err(fail("Frozen source inventory exceeds bound."));}for (visited,entry) in fs::read_dir(current)?.enumerate() { if visited>=4096{return Err(fail("Frozen source directory exceeds bound."));}let entry = entry?; let info = entry.file_type()?; if info.is_symlink() { return Err(fail("Linked frozen source suite rejected.")); } if info.is_dir() { walk(root, &entry.path(), found)?; } else if entry.file_name().to_string_lossy().ends_with(".test.ts") { let path = entry.path().strip_prefix(root).map_err(|_| fail("Escaping frozen suite."))?.to_string_lossy().replace('\\', "/"); found.push(json!({"source":path,"sha256":sha(&read(root,&path)?)})); } } Ok(()) }
    let mut suites = Vec::new(); walk(source, &source.join("tests"), &mut suites)?;
    if suites.len() != 50 { return Err(fail("Pinned baseline suite count differs from50.")); } suites.sort_by(|a,b| a["source"].as_str().cmp(&b["source"].as_str()));
    Ok(json!({"baseline":crate::fixtures::BASELINE,"baseline_case_count":470,"suites":suites,"current_execution_claimed":false}))
}
pub fn validate_parity(project: &Path, source: &Path, collection: &Value, ledger_root: &Path, manifest: &Value, acceptance: bool) -> io::Result<Value> {
    let report = validate(project,source,collection,ledger_root,acceptance)?; let expected = inventory(source)?;
    if manifest["baseline"] != crate::fixtures::BASELINE || manifest["baseline_case_count"] != 470 || (acceptance && manifest["case_mapping_complete"] != true) { return Err(fail("Suite parity identity or expanded case mapping is pending.")); }
    let hashes = array(&expected,"suites")?.iter().map(|s|(s["source"].as_str().unwrap(), &s["sha256"])).collect::<BTreeMap<_,_>>(); let mut seen = BTreeSet::new();
    for row in array(manifest,"suites")? { let path = string(row,"source")?; if !seen.insert(path) || hashes.get(path).is_none_or(|hash| row["sha256"].as_str() != hash.as_str()) { return Err(fail("Every baseline suite needs one exact source mapping.")); } if row["status"] != "verified" || row["evidence"].is_null() || array(row,"native")?.is_empty() { return Err(fail("Suite contracts remain unverified.")); } for value in array(row,"native")? { let (file,symbol) = if let Some(value)=value.as_str() { value.split_once("::").ok_or_else(||fail("Suite mapping requires test identity."))? } else { (string(value,"file")?,string(value,"test")?) }; mapping_source(project,file,"::",symbol)?; } }
    if seen.len()!=50 { return Err(fail("Every baseline suite requires an explicit mapping.")); } Ok(report)
}

fn canonical(value: &Value) -> Value { match value { Value::Object(values) => Value::Object(values.iter().map(|(k,v)|(k.clone(),canonical(v))).collect::<BTreeMap<_,_>>().into_iter().collect()), Value::Array(values) => Value::Array(values.iter().map(canonical).collect()), other => other.clone() } }
pub fn assertion_digest(value: &Value) -> io::Result<String> {
    // Historical binders used Python json.dumps(sort_keys=True,separators=(',',':')) with
    // ensure_ascii=True. Preserve its exact Unicode escaping rather than hashing serde's UTF-8 form.
    let text = serde_json::to_string(&canonical(value)).map_err(|_|fail("Cannot encode assertion identity."))?; let mut ascii=String::new();
    for ch in text.chars() { if ch.is_ascii() { ascii.push(ch); } else { let mut units=[0u16;2]; for unit in ch.encode_utf16(&mut units) { ascii.push_str(&format!("\\u{unit:04x}")); } } } Ok(sha(ascii.as_bytes()))
}

pub fn validate_execution_binding(rows: &[Value], bytes: &[u8], expected_digest: &str) -> io::Result<Value> {
    execution_binding(rows, bytes, expected_digest, false)
}
pub fn validate_reference_binding(rows: &[Value], bytes: &[u8], digest: &str) -> io::Result<Value> {
    execution_binding(rows,bytes,digest,true)
}
fn execution_binding(rows: &[Value], bytes: &[u8], expected_digest: &str, reference: bool) -> io::Result<Value> {
    validate_rows(rows)?;
    if !hex(expected_digest,64) || sha(bytes)!=expected_digest { return Err(fail("Execution binding digest differs.")); } let binding=parse(bytes)?;
    if binding["baseline"]!=crate::fixtures::BASELINE || !hex(string(&binding,"source_sha")?,40) || binding["run_id"].as_u64().is_none_or(|n|n==0) || binding["run_attempt"].as_u64().is_none_or(|n|n==0) || binding["native_execution_locally"]!=false || binding["canonical_promotions"]!=0 { return Err(fail("Execution binding source/run identity differs.")); }
    let cases=array(&binding,"cases")?; let mut indexed=BTreeMap::new(); for case in cases { let id=string(case,"id")?; if !hex(id,64)||indexed.insert(id,case).is_some() { return Err(fail("Execution binding duplicates or invalidates a case.")); } } if indexed.len()!=470 || rows.iter().any(|row|!indexed.contains_key(row["id"].as_str().unwrap_or(""))) { return Err(fail("Execution binding must cover all470cases.")); }
    let targets=binding["targets"].as_object().ok_or_else(||fail("Execution target evidence missing."))?;
    if targets.len()!=7 || !targets.contains_key("coverage") {return Err(fail("Allsixnative targets and coverage execution are required."));}
    let actual_targets=targets.iter().map(|(label,target)|(target["platform"].as_str().unwrap_or(""),target["target"].as_str().unwrap_or(""),label=="coverage")).collect::<BTreeSet<_>>();
    let expected_targets=BTreeSet::from([("win32","x86_64-pc-windows-msvc",false),("win32","aarch64-pc-windows-msvc",false),("linux","x86_64-unknown-linux-gnu",false),("linux","aarch64-unknown-linux-gnu",false),("darwin","x86_64-apple-darwin",false),("darwin","aarch64-apple-darwin",false),("linux","coverage",true)]);
    if actual_targets!=expected_targets{return Err(fail("Execution target matrix is incomplete or duplicated."));}
    let mut bound=0;
    for row in rows { let Some(case)=indexed.get(string(row,"id")?) else { return Err(fail("Execution binding omits mapped source case.")); };
        let selected=reference || row["evidence"]["execution_binding_sha256"]==expected_digest;
        if !selected{continue;}
        for key in["source","source_sha256","platform_modes","native"] { if case[key]!=row[key] { return Err(fail("Execution binding source or mapping differs.")); } } if case["assertions_sha256"]!=assertion_digest(&row["assertions"])? { return Err(fail("Execution assertion source ranges changed.")); }
        if !reference && (row["evidence"]["sha"]!=binding["source_sha"] || row["evidence"]["run_id"]!=binding["run_id"] || row["evidence"]["run_attempt"]!=binding["run_attempt"]) { return Err(fail("Case evidence does not bind its execution revision/run.")); }
        let source=string(row,"source")?; if binding["frozen_git_files"][source]!=row["source_sha256"] { return Err(fail("Executed frozen source bytes differ.")); }
        for value in array(row,"native")? { let value=value.as_str().ok_or_else(||fail("Invalid executed mapping."))?; let(file,symbol)=value.split_once("::").ok_or_else(||fail("Executed mapping requires symbol."))?;
            let hash=string(&case["native_sha256"],file)?; if !hex(hash,64) || binding["current_git_files"][file]!=hash || (!reference && row["evidence"]["native_sha256"][file]!=hash) { return Err(fail("Executed native source hash differs.")); }
            for platform in PLATFORMS { if row["platform_modes"][platform]!="run" {continue;} let mut architectures=BTreeSet::new();
                for(label,target)in targets { if target["platform"]!=platform {continue;} let triple=string(target,"target")?;
                    let suffix=match platform{"win32"=>"pc-windows-msvc","darwin"=>"apple-darwin",_=>"unknown-linux-gnu"};
                    if label=="coverage"{if platform!="linux"||triple!="coverage"{return Err(fail("Coverage execution target differs."));}}
                    else if triple==format!("x86_64-{suffix}"){architectures.insert("x86_64");}else if triple==format!("aarch64-{suffix}"){architectures.insert("aarch64");}else{return Err(fail("Execution platform and architecture differ."));}
                    if target["baseline_execution_files"][source]!=row["source_sha256"] {return Err(fail("Target execution source provenance differs."));}
                    for key in["download_sha256","payload_sha256","provenance_sha256","log_sha256"] {if !hex(string(target,key)?,64){return Err(fail("Target execution receipt hash missing."));}}
                    if target["artifact_id"].as_u64().is_none_or(|n|n==0){return Err(fail("Target execution artifact identity missing."));}
                    let named=&case["named_execution"][label][value]; if file.ends_with(".rs") { let harness=if file.starts_with("native/"){"native/lib.rs"}else{file}; let actual=string(named,"exact_test_name")?; if named["harness"]!=harness || !(actual==symbol || actual.ends_with(&format!("::{symbol}"))) {return Err(fail("Exact named native test identity differs."));} let evidence=&target["native_harnesses"][harness]; let passed=array(evidence,"passed")?;let matches=passed.iter().filter_map(Value::as_str).filter(|name|*name==symbol||name.ends_with(&format!("::{symbol}"))).collect::<Vec<_>>();if evidence["started"]!=true || matches.len()!=1 || matches[0]!=actual || !string(evidence,"summary")?.starts_with("ok."){return Err(fail("Named native test did not execute successfully or is ambiguous."));} }
                    else { if !SDK.iter().any(|(path,name)|*path==file&&*name==symbol){return Err(fail("Unregistered historical SDK execution mapping."));}let component=format!("native-{}-acceptance",Path::new(file).file_name().and_then(|n|n.to_str()).unwrap_or("").trim_start_matches("native-").trim_end_matches("-acceptance.mjs")); if named["component"]!=component || (label!="coverage" && named["built_and_extracted"]!=true) || target["sdk_events"][&component].as_object().is_none_or(|v|v.is_empty()||v.values().any(|n|n.as_u64().unwrap_or(0)<if label=="coverage"{1}else{2})){return Err(fail("Historical SDK built/extracted execution missing."));} }
                }
                if architectures!=BTreeSet::from(["x86_64","aarch64"]){return Err(fail("Applicable native architecture execution missing."));}
            }
        }
        bound+=1;
    }
    if bound==0{return Err(fail("Execution receipt is not referenced by any case."));}
    Ok(json!({"cases":470,"bound_cases":bound,"execution_binding_verified":true,"historical_source_sha":binding["source_sha"],"current_execution_claimed":false}))
}

/// Validate a separate historical reference without relabelling the ledger's original evidence.
/// The caller authenticates the reference digest independently. This never proves current acceptance.
pub fn validate_reference(project: &Path, source: &Path, collections: &[Value], ledger_root: &Path, bytes: &[u8], digest: &str) -> io::Result<Value> {
    if collections.len()!=3 || collections.iter().filter_map(|v|v["platform"].as_str()).collect::<BTreeSet<_>>()!=BTreeSet::from(PLATFORMS){return Err(fail("Independent collections for every original platform are required."));}
    for collection in collections { validate_with(project,source,collection,ledger_root,false,MappingPolicy::Reference)?; }
    let (_,rows)=load(ledger_root)?; let execution=execution_binding(&rows,bytes,digest,true)?; let binding=parse(bytes)?;
    let mut checked=BTreeSet::new();
    for row in &rows { for value in array(row,"native")? { let(file,_)=value.as_str().and_then(|v|v.split_once("::")).ok_or_else(||fail("Invalid historical mapping."))?; if checked.insert(file){if binding["current_git_files"][file]!=sha(&read(project,file)?){return Err(fail("Materialized historical native bytes differ from execution reference."));}} } }
    Ok(json!({"cases":470,"suites":50,"platforms":3,"historical_reference_verified":true,"historical_execution":execution,"original_evidence_rewritten":false,"acceptance_verified":false,"current_execution_claimed":false}))
}

pub fn validate_reference_parity(project: &Path, source: &Path, manifest: &Value, rows: &[Value]) -> io::Result<Value> {
    validate_rows(rows)?;let frozen=inventory(source)?;
    if manifest["baseline"]!=crate::fixtures::BASELINE || manifest["baseline_case_count"]!=470 || manifest["case_mapping_complete"]!=true{return Err(fail("Historical suite parity identity differs."));}
    let hashes=array(&frozen,"suites")?.iter().map(|v|(v["source"].as_str().unwrap(),&v["sha256"])).collect::<BTreeMap<_,_>>();
    let mut seen=BTreeSet::new();let mut cache=BTreeMap::new();
    for suite in array(manifest,"suites")? {let path=string(suite,"source")?;
        if !seen.insert(path) || hashes.get(path).is_none_or(|v|suite["sha256"].as_str()!=v.as_str()) || suite["status"]!="verified" || suite["evidence"].is_null(){return Err(fail("Historical suite source or disposition differs."));}
        let cases=rows.iter().filter(|v|v["source"]==path).collect::<Vec<_>>();
        if suite["case_count"].as_u64()!=Some(cases.len() as u64){return Err(fail("Historical suite expanded case count differs."));}
        let expected=cases.iter().flat_map(|v|v["native"].as_array().into_iter().flatten()).filter_map(Value::as_str).collect::<BTreeSet<_>>();
        let actual=array(suite,"native")?.iter().map(|v|v.as_str().ok_or_else(||fail("Historical suite mapping must name an exact test."))).collect::<io::Result<Vec<_>>>()?;
        if actual.len()!=expected.len() || actual.iter().copied().collect::<BTreeSet<_>>()!=expected{return Err(fail("Historical suite mappings differ from expanded case mappings."));}
        for value in actual {let(file,symbol)=value.split_once("::").ok_or_else(||fail("Historical suite mapping symbol missing."))?;mapping(project,file,"::",symbol,MappingPolicy::Reference,&mut cache)?;}
    }
    if seen.len()!=50{return Err(fail("Historical suite parity omits a frozen suite."));}
    Ok(json!({"suites":50,"cases":470,"historical_suite_mapping_verified":true,"acceptance_verified":false,"current_execution_claimed":false}))
}

pub fn check(context: &Context) -> io::Result<()> {
    let report=crate::ledger_inputs::check(context)?;
    println!("{report}");
    Ok(())
}
