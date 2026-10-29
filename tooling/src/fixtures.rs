use crate::context::{Context, fail, sha};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, fs, io, path::Path};

pub const BASELINE: &str = "fc7da25c3b470199949b1b4c67127d77afa0c34a";
pub const SOURCE: &str = "f2535577c2e3145da03e2d5c123d7de061656860";
pub const INPUTS: &[&str] = &["config.json", "policy.json", "normalization-acceptance.json", "canonical.json",
    "adapters.json", "base64.json", "image-acceptance.json", "images.json", "animated-webp.json",
    "output-requirements.json", "stream-acceptance.json", "streams.json", "raw-json.json",
    "raw-js-schema.json", "sqlite-utf16-text.json"];

pub fn parse_json(bytes: &[u8]) -> io::Result<Value> {
    let text = std::str::from_utf8(bytes).map_err(|_|fail("Fixture JSON must be UTF-8."))?;
    numera_gen_mcp::compat::value::Value::parse_with_limit(text, 16 * 1024 * 1024)
        .and_then(|value|value.to_serde()).map_err(|_|fail("Invalid bounded fixture JSON."))
}

pub fn validate_bundle(manifest: &Value, name: &str, bytes: &[u8]) -> io::Result<()> {
    if manifest["schema_version"] != 1 || manifest["source_sha"] != SOURCE || manifest["baseline"] != BASELINE
        || !INPUTS.contains(&name) || bytes.len() > 16 * 1024 * 1024
        || manifest["inputs"][name]["bytes"].as_u64() != Some(bytes.len() as u64)
        || manifest["inputs"][name]["sha256"] != sha(bytes) {
        return Err(fail("Independent golden fixture provenance mismatch."));
    }
    parse_json(bytes)?;
    Ok(())
}

fn text_relocate(text: &str, old: &str, new: &str) -> String {
    let mut out = String::new();
    let mut start = 0;
    for (index, _) in text.match_indices(old) {
        let end = index + old.len();
        let boundary = text[end..].chars().next();
        if boundary.is_none_or(|c| matches!(c, '/' | '\\' | '"' | '\'' | ' ')) {
            out.push_str(&text[start..index]); out.push_str(new); start = end;
        }
    }
    out.push_str(&text[start..]); out
}

pub fn relocate(value: &Value, old: &str, new: &str) -> io::Result<Value> {
    if old.is_empty() || new.is_empty() { return Err(fail("Explicit fixture roots required.")); }
    Ok(match value {
        Value::String(text) => {
            let direct = text_relocate(text, old, new);
            let escaped_old = serde_json::to_string(old).map_err(|_| fail("Cannot encode fixture path."))?;
            let escaped_new = serde_json::to_string(new).map_err(|_| fail("Cannot encode fixture path."))?;
            Value::String(text_relocate(&direct, &escaped_old[1..escaped_old.len()-1], &escaped_new[1..escaped_new.len()-1]))
        }
        Value::Array(values) => Value::Array(values.iter().map(|v| relocate(v, old, new)).collect::<io::Result<_>>()?),
        Value::Object(values) => Value::Object(values.iter().map(|(key, value)|
            relocate(value, old, new).map(|v| (key.clone(), v))).collect::<io::Result<_>>()?),
        other => other.clone(),
    })
}

pub fn verify_materialized(root: &Path, platform: &str, source: &str, report: &Value,
    payloads: &BTreeMap<String, Vec<u8>>) -> io::Result<()> {
    if !["windows", "linux", "macos"].contains(&platform) || source.len() != 40
        || !source.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || report["schema"] != 1 || report["historical_source"] != SOURCE || report["baseline"] != BASELINE
        || report["current_source"] != source || report["platform"] != platform
        || report["expectations_generated_by_target"] != false
        || report["normalization"] != "Only explicit historical checkout path relocation in config.json; other bytes unchanged."
        || payloads.keys().map(String::as_str).collect::<BTreeSet<_>>() != INPUTS.iter().copied().collect() {
        return Err(fail("Materialized fixture identity or inventory differs."));
    }
    let provenance = crate::ledger::parse(&crate::ledger::read(root,
        &format!("tests/fixtures/oracle/{platform}/provenance.json"))?)?;
    let records = report["inputs"].as_array().filter(|rows| rows.len() == INPUTS.len())
        .ok_or_else(|| fail("Materialized fixture records missing."))?;
    let mut names = BTreeSet::new();
    for record in records {
        let name = record["name"].as_str().filter(|name| INPUTS.contains(name))
            .ok_or_else(|| fail("Unexpected materialized fixture record."))?;
        if !names.insert(name) { return Err(fail("Duplicated materialized fixture record.")); }
        let directory = if name == "config.json" { platform } else { "common" };
        let original = crate::ledger::read(root, &format!("tests/fixtures/oracle/{directory}/{name}"))?;
        validate_bundle(&provenance, name, &original)?;
        if record["original_sha256"] != sha(&original) || record["materialized_sha256"] != sha(&payloads[name]) {
            return Err(fail("Materialized fixture record hashes differ."));
        }
        let expected = if name == "config.json" {
            let config = parse_json(&original)?; let current = parse_json(&payloads[name])?;
            let suffix = if platform == "windows" { "\\.ci-work\\parity\\config.json" } else { "/.ci-work/parity/config.json" };
            let checkout = |value: &Value| -> io::Result<String> {
                value["parse"][0]["file"].as_str().and_then(|file| file.strip_suffix(suffix))
                    .filter(|path| !path.is_empty() && !path.chars().any(char::is_control))
                    .map(str::to_owned).ok_or_else(|| fail("Explicit fixture checkout relocation missing."))
            };
            serde_json::to_vec(&relocate(&config, &checkout(&config)?, &checkout(&current)?)?)
                .map_err(|_| fail("Cannot compare relocated fixture bytes."))?
        } else { original };
        if expected != payloads[name] { return Err(fail("Fixture changed beyond explicit historical config path relocation.")); }
    }
    Ok(())
}

pub fn materialize(context: &Context) -> io::Result<()> {
    context.hosted()?;
    let platform = match std::env::consts::OS { "windows" => "windows", "linux" => "linux", "macos" => "macos", _ => return Err(fail("Unsupported fixture platform.")) };
    let base = context.root.join("tests/fixtures/oracle");
    let provenance: Value = serde_json::from_slice(&fs::read(base.join(platform).join("provenance.json"))?)
        .map_err(|_| fail("Invalid historical fixture provenance."))?;
    let config = parse_json(&fs::read(base.join(platform).join("config.json"))?)?;
    let file = config["parse"][0]["file"].as_str().ok_or_else(|| fail("Historical fixture root missing."))?;
    let separator = if platform == "windows" { "\\.ci-work\\parity\\config.json" } else { "/.ci-work/parity/config.json" };
    let old = file.strip_suffix(separator).ok_or_else(|| fail("Historical fixture root differs."))?;
    let new = context.root.to_str().ok_or_else(|| fail("Fixture workspace must be Unicode."))?;
    let mut records = Vec::new();
    for name in INPUTS {
        let directory = if *name == "config.json" { base.join(platform) } else { base.join("common") };
        let bytes = fs::read(directory.join(name))?;
        validate_bundle(&provenance, name, &bytes)?;
        let materialized = if *name == "config.json" {
            let input = parse_json(&bytes)?;
            serde_json::to_vec(&relocate(&input, old, new)?).map_err(|_| fail("Cannot relocate fixture paths."))?
        } else { bytes.clone() };
        context.write_new(&format!(".ci-work/parity/{name}"), &materialized)?;
        records.push(json!({"name":name,"original_sha256":sha(&bytes),"materialized_sha256":sha(&materialized)}));
    }
    let report = json!({"schema":1,"historical_source":SOURCE,"baseline":BASELINE,
        "current_source":context.source_sha()?,"platform":platform,"inputs":records,
        "normalization":"Only explicit historical checkout path relocation in config.json; other bytes unchanged.",
        "expectations_generated_by_target":false});
    context.write_new(".ci-work/native/fixture-materialization.json", &serde_json::to_vec_pretty(&report).map_err(|_| fail("Cannot record fixture provenance."))?)
}
