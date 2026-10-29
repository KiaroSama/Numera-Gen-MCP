//! Inspect the exact historical validation delta without executing a retired interpreter.
use crate::{context::{fail, sha}, fixtures::BASELINE};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io};

const LIMIT: usize = 128 * 1024 * 1024;
pub const SYNC_FIXTURE: &str = "import { spawnSync } from 'node:child_process';\\nconst r=spawnSync('pwsh',['-NoProfile','-NonInteractive','-Command',process.argv[2]],{windowsHide:true,encoding:'utf8',timeout:10000,stdio:['ignore','pipe','pipe']});process.stdout.write(r.stdout??'');process.stderr.write(r.stderr??'');process.exitCode=r.status??1;";
pub const STREAM_FIXTURE: &str = "import { spawn } from 'node:child_process';\\nprocess.stderr.write('PowerShell fixture: launch\\\\n');const child=spawn('pwsh',['-NoProfile','-NonInteractive','-Command',process.argv[2]],{windowsHide:true,stdio:['ignore','pipe','pipe']});const timer=setTimeout(()=>child.kill('SIGKILL'),10000);child.stdout.pipe(process.stdout);child.stderr.pipe(process.stderr);child.once('error',()=>{clearTimeout(timer);process.stderr.write('PowerShell fixture: launch failed\\\\n');process.exitCode=1});child.once('close',(code)=>{clearTimeout(timer);process.exitCode=code??1});";
const CHANGED: [&str; 4] = ["package.json", "package-lock.json", "scripts/bounded.mjs", "tests/integration/setup.test.ts"];

pub struct Prepared {
    pub files: BTreeMap<String, Vec<u8>>,
    pub receipt: Value,
}
fn text(bytes: &[u8]) -> io::Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| fail("Historical validation text must be UTF-8."))
}
fn source_names(files: &BTreeMap<String, Vec<u8>>) -> io::Result<()> {
    if files.is_empty() || files.len() > 4096 { return Err(fail("Historical validation inventory exceeds bound.")); }
    let mut total = 0usize;
    let mut folded = BTreeSet::new();
    for (name, bytes) in files {
        numera_gen_mcp::files::paths::safe_relative(name).map_err(|_| fail("Invalid historical validation source path."))?;
        if name.contains('\\') || !(name.starts_with("src/") || name.starts_with("tests/")
            || ["package.json", "package-lock.json", "scripts/bounded.mjs"].contains(&name.as_str()))
            || !folded.insert(name.to_lowercase()) {
            return Err(fail("Unreviewed or duplicate historical validation member."));
        }
        total = total.checked_add(bytes.len()).filter(|size| *size <= LIMIT)
            .ok_or_else(|| fail("Historical validation bytes exceed aggregate bound."))?;
    }
    Ok(())
}
fn get<'a>(files: &'a BTreeMap<String, Vec<u8>>, name: &str) -> io::Result<&'a [u8]> {
    files.get(name).map(Vec::as_slice).ok_or_else(|| fail("Required historical validation source missing."))
}
fn encode(value: &Value) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| fail("Cannot encode historical validation metadata."))?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn prepare(original: &BTreeMap<String, Vec<u8>>, supervisor: &[u8]) -> io::Result<Prepared> {
    source_names(original)?;
    if supervisor.is_empty() || supervisor.len() > 1024 * 1024 {
        return Err(fail("Historical supervisor exceeds bound."));
    }
    text(supervisor)?;
    let mut package = crate::ledger::parse(get(original, "package.json")?)?;
    let mut lock = crate::ledger::parse(get(original, "package-lock.json")?)?;
    for (name, old) in [("zod", "4.6.5"), ("undici", "8.11.2")] {
        let key = format!("node_modules/{name}");
        if package["dependencies"][name] != old || lock["lockfileVersion"] != 3
            || lock["packages"][""]["dependencies"][name] != old
            || lock["packages"][key.as_str()]["version"] != old {
            return Err(fail("Unexpected frozen validation dependency contract."));
        }
    }
    for (name, version, resolved, integrity) in [
        ("zod", "4.6.7", "https://registry.npmjs.org/zod/-/zod-4.6.7.tgz", "sha512-g8W2D2kU6a8untc053fX1uVBKzM7yqAxW/3zwk83+6RyF2+SKOuV+c0wBDhUwzV1PaGapmZH+rmcUzPf+3CGwg=="),
        ("undici", "8.12.3", "https://registry.npmjs.org/undici/-/undici-8.12.3.tgz", "sha512-+fdndUTwBP86cVjihTp21hZHo7d6oj1c/47XPA3lL8mM0XHs+C4EkcjmgRsFnFk2f/UONGDy9ItBItmXsLOQHw=="),
    ] {
        package["dependencies"][name] = json!(version);
        lock["packages"][""]["dependencies"][name] = json!(version);
        let key = format!("node_modules/{name}");
        let entry = lock["packages"][key.as_str()].as_object_mut()
            .ok_or_else(|| fail("Frozen dependency metadata must be an object."))?;
        entry.extend([("version".into(), json!(version)), ("resolved".into(), json!(resolved)), ("integrity".into(), json!(integrity))]);
    }
    let setup = text(get(original, "tests/integration/setup.test.ts")?)?;
    if setup.matches(SYNC_FIXTURE).count() != 1 {
        return Err(fail("Unexpected frozen PowerShell child fixture contract."));
    }
    let mut files = original.clone();
    files.insert("package.json".into(), encode(&package)?);
    files.insert("package-lock.json".into(), encode(&lock)?);
    files.insert("scripts/bounded.mjs".into(), supervisor.to_vec());
    files.insert("tests/integration/setup.test.ts".into(), setup.replacen(SYNC_FIXTURE, STREAM_FIXTURE, 1).into_bytes());
    let changed = original.iter().filter(|(name, bytes)| files.get(*name) != Some(*bytes))
        .map(|(name, _)| name.as_str()).collect::<BTreeSet<_>>();
    if changed != CHANGED.into_iter().collect() || !original.keys().any(|name| name.starts_with("tests/") && !changed.contains(name.as_str())) {
        return Err(fail("Historical validation delta differs from the reviewed four-file allowlist."));
    }
    let changes = changed.iter().map(|name| ((*name).to_owned(), json!({"baseline":sha(&original[*name]),"validation":sha(&files[*name])})))
        .collect::<BTreeMap<_, _>>();
    let unchanged = original.iter().filter(|(name, _)| name.starts_with("tests/") && !changed.contains(name.as_str()))
        .map(|(name, bytes)| (name.clone(), sha(bytes))).collect::<BTreeMap<_, _>>();
    let hashes = files.iter().map(|(name, bytes)| (name.clone(), sha(bytes))).collect::<BTreeMap<_, _>>();
    let receipt = json!({"schema_version":1,"baseline":BASELINE,"changes":changes,"unchanged_tests":unchanged,
        "validation_files":hashes,"assertion_preserving_test_updates":{"tests/integration/setup.test.ts":"Only buffered child fixture replaced; original assertions and time bounds unchanged."},
        "immutable_oracle_modified":false,"interpreted_execution":false});
    Ok(Prepared { files, receipt })
}

pub fn verify(original: &BTreeMap<String, Vec<u8>>, supervisor: &[u8], prepared: &Prepared) -> io::Result<()> {
    let expected = prepare(original, supervisor)?;
    if expected.files != prepared.files || expected.receipt != prepared.receipt {
        return Err(fail("Historical validation bytes or provenance changed after preparation."));
    }
    Ok(())
}
