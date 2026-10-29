use crate::{context::{Context, fail, sha}, distribution::read_regular, fixtures::{BASELINE, INPUTS, SOURCE}};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io::{self, Cursor, Read, Write}};

const LIMIT: usize = 16 * 1024 * 1024;
pub(crate) fn dependencies(context: &Context) -> io::Result<BTreeMap<String, String>> {
    let paths = context.capture(context.command("git").args(["ls-files", "-z"]), 10000, 8000, LIMIT)?;
    let paths = std::str::from_utf8(&paths).map_err(|_| fail("Source inventory is not UTF-8."))?;
    let mut hashes = BTreeMap::new();
    for name in paths.split('\0').filter(|s| !s.is_empty()) {
        if crate::distribution::ASSETS.contains(&name)
            || ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "skills/numera-gen/SKILL.md", ".github/workflows/ci.yml",
            ".github/actions/native-bootstrap/action.yml", "docs/assets/logo.png",
            "docs/signing/publisher.json", "docs/signing/publisher-key.der", "docs/signing/publisher-cert.der",
            "docs/testing/tooling-migration.json", "docs/testing/sdk-tooling-migration.json", "docs/testing/native-parity.json"].contains(&name)
            || ["native/", "tests/rust/", "tooling/", "schemas/", "tests/fixtures/", "scripts/", "tests/tooling/", "docs/testing/native-cases/"]
                .iter().any(|prefix| name.starts_with(prefix)) {
            if hashes.len() >= 4096 { return Err(fail("Evidence dependency inventory exceeds bound.")); }
            hashes.insert(name.to_owned(), sha(&read_regular(&context.relative(name)?, LIMIT)?));
        }
    }
    for required in ["Cargo.toml", "Cargo.lock", "native/main.rs"] {
        if !hashes.contains_key(required) { return Err(fail("Required tracked evidence source missing.")); }
    }
    Ok(hashes)
}
fn inputs(context: &Context) -> io::Result<BTreeMap<String, Vec<u8>>> {
    INPUTS.iter().map(|name| Ok(((*name).to_owned(), read_regular(&context.relative(&format!(".ci-work/parity/{name}"))?, LIMIT)?))).collect()
}
fn materialization(context: &Context) -> io::Result<Value> {
    serde_json::from_slice(&read_regular(&context.relative(".ci-work/native/fixture-materialization.json")?, LIMIT)?)
        .map_err(|_| fail("Invalid fixture provenance."))
}
fn identity(source: &str, relocation: &Value) -> io::Result<()> {
    if source.len() != 40 || !source.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || relocation["current_source"] != source || relocation["historical_source"] != SOURCE
        || relocation["baseline"] != BASELINE || relocation["expectations_generated_by_target"] != false {
        return Err(fail("Fixture provenance identity differs."));
    }
    Ok(())
}
fn input_inventory(payloads: &BTreeMap<String, Vec<u8>>) -> io::Result<Value> {
    if payloads.keys().map(String::as_str).collect::<BTreeSet<_>>() != INPUTS.iter().copied().collect() {
        return Err(fail("Evidence input inventory differs."));
    }
    let mut records = serde_json::Map::new();
    for (name, bytes) in payloads {
        if bytes.len() > LIMIT { return Err(fail("Evidence input exceeds bound.")); }
        crate::fixtures::parse_json(bytes)?;
        records.insert(name.clone(), json!({"bytes":bytes.len(),"sha256":sha(bytes)}));
    }
    Ok(Value::Object(records))
}
pub fn bundle(source: &str, dependencies: &BTreeMap<String, String>, relocation: &Value,
    payloads: &BTreeMap<String, Vec<u8>>) -> io::Result<Vec<u8>> {
    identity(source, relocation)?;
    let manifest = json!({"schema_version":2,"source_sha":source,"baseline":BASELINE,
        "historical_source":SOURCE,"platform":std::env::consts::OS,"machine":std::env::consts::ARCH,
        "inputs":input_inventory(payloads)?,"dependencies":dependencies,"materialization":relocation,
        "expectations_generated_by_target":false});
    let metadata = serde_json::to_vec_pretty(&manifest).map_err(|_| fail("Cannot encode evidence."))?;
    if metadata.len() > LIMIT { return Err(fail("Evidence manifest exceeds bound.")); }
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in payloads.iter().map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .chain(std::iter::once(("provenance.json", metadata.as_slice()))) {
        archive.start_file(name, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::default())).map_err(|_| fail("Cannot create evidence archive."))?;
        archive.write_all(bytes)?;
    }
    archive.finish().map(|archive| archive.into_inner()).map_err(|_| fail("Cannot finish evidence archive."))
}
pub fn verify_bundle(bytes: &[u8], source: &str, dependencies: &BTreeMap<String, String>, relocation: &Value,
    payloads: &BTreeMap<String, Vec<u8>>) -> io::Result<Value> {
    verify_platform_bundle(bytes, source, dependencies, relocation, payloads, std::env::consts::OS, std::env::consts::ARCH)
}
pub fn verify_platform_bundle(bytes: &[u8], source: &str, dependencies: &BTreeMap<String, String>, relocation: &Value,
    payloads: &BTreeMap<String, Vec<u8>>, os: &str, architecture: &str) -> io::Result<Value> {
    identity(source, relocation)?;
    if !["windows", "linux", "macos"].contains(&os) || !["x86_64", "aarch64"].contains(&architecture) {
        return Err(fail("Unsupported evidence platform or architecture."));
    }
    let expected_inputs = input_inventory(payloads)?;
    if bytes.is_empty() || bytes.len() > (INPUTS.len()+1)*LIMIT { return Err(fail("Evidence archive exceeds bound.")); }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| fail("Invalid evidence archive."))?;
    let expected = INPUTS.iter().copied().chain(["provenance.json"]).collect::<BTreeSet<_>>();
    let mut entries = BTreeMap::new();
    if archive.len() != expected.len() { return Err(fail("Evidence archive member count differs.")); }
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|_| fail("Unreadable evidence member."))?;
        let mode = file.unix_mode().unwrap_or(0) & 0o170000;
        if !file.is_file() || !matches!(mode, 0 | 0o100000) || !expected.contains(file.name())
            || file.size() > LIMIT as u64 || entries.contains_key(file.name()) {
            return Err(fail("Evidence archive contains unexpected or duplicate members."));
        }
        let name = file.name().to_owned(); let size = file.size(); let mut data = Vec::new();
        file.take(size+1).read_to_end(&mut data)?;
        if data.len() as u64 != size { return Err(fail("Evidence member length differs.")); }
        entries.insert(name, data);
    }
    let manifest = crate::ledger::parse(&entries["provenance.json"])?;
    if manifest["schema_version"] != 2 || manifest["source_sha"] != source
        || manifest["baseline"] != BASELINE || manifest["historical_source"] != SOURCE
        || manifest["expectations_generated_by_target"] != false || manifest["materialization"] != *relocation
        || manifest["platform"] != os || manifest["machine"] != architecture {
        return Err(fail("Evidence identity changed."));
    }
    if manifest["inputs"] != expected_inputs || INPUTS.iter().any(|name| entries[*name] != payloads[*name]) {
        return Err(fail("Evidence input changed."));
    }
    if manifest["dependencies"] != serde_json::to_value(dependencies).map_err(|_| fail("Cannot compare evidence dependencies."))? {
        return Err(fail("Evidence dependency changed."));
    }
    Ok(manifest)
}
pub fn capture(context: &Context) -> io::Result<()> {
    context.hosted()?;
    let source = context.source_sha()?;
    let actual = context.capture(context.command("git").args(["rev-parse", "HEAD"]), 8000, 5000, 1024)?;
    if std::str::from_utf8(&actual).map_err(|_| fail("Invalid checkout identity."))?.trim() != source {
        return Err(fail("Evidence checkout differs from hosted SHA."));
    }
    let bytes = bundle(&source, &dependencies(context)?, &materialization(context)?, &inputs(context)?)?;
    context.write_new(".ci-work/native/oracle-evidence.zip", &bytes)?;
    verify(context)
}
#[derive(PartialEq, Eq)]
pub struct Snapshot {
    pub sha256: String,
    pub dependencies: BTreeMap<String, String>,
}
pub fn snapshot(context: &Context) -> io::Result<Snapshot> {
    let bytes = read_regular(&context.relative(".ci-work/native/oracle-evidence.zip")?, (INPUTS.len()+1)*LIMIT)?;
    let dependencies = dependencies(context)?;
    verify_bundle(&bytes, &context.source_sha()?, &dependencies, &materialization(context)?, &inputs(context)?)?;
    Ok(Snapshot { sha256: sha(&bytes), dependencies })
}
pub fn verify(context: &Context) -> io::Result<()> {
    snapshot(context)?;
    println!("{}", json!({"component":"native-oracle-evidence","source_sha":context.source_sha()?,"inputs":INPUTS.len(),"verified":true,"historical_source":SOURCE}));
    Ok(())
}
