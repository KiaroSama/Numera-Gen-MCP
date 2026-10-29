use crate::{archives::{self, MAX_BYTES, validate_name}, context::{Context, fail, sha}};
use numera_gen_mcp::devtools::artifact;
use serde_json::{Value, json};
use std::{collections::{BTreeMap, HashSet}, fs, io, path::Path};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ASSETS: &[&str] = &["LICENSE", "THIRD_PARTY_NOTICES.md", "README.md", ".env.example",
    "schemas/config.schema.json", "schemas/simple-config.schema.json", "schemas/image-request.schema.json", "schemas/media-request.schema.json",
    "examples/claude-desktop.json", "examples/claude-project.json", "examples/codex.toml", "examples/fal.json",
    "docs/CONFIGURATION.md", "docs/CLAUDE.md", "docs/CODEX.md", "docs/ARCHITECTURE.md", "docs/HTTP.md",
    "docs/WINDOWS-WSL.md", "docs/COMPATIBILITY.md", "docs/IMAGE-EDITING.md", "docs/LINUX-MACOS.md",
    "docs/REQUIREMENTS.md", "docs/REFERENCES.md", "SECURITY.md", "docs/TESTING.md", "docs/PRACTICAL-TESTING.md",
    "docs/TEST-RESULTS.md", "docs/testing/MIGRATION-MATRIX.md", "docs/testing/native-parity.json",
    "docs/testing/native-cases/index.json", "docs/testing/native-cases/unit.jsonl", "docs/testing/native-cases/contract.jsonl",
    "docs/testing/native-cases/integration.jsonl", "docs/assets/logo.png", "docs/DEVELOPER-TOOLS.md", "docs/SIGNING.md",
    "docs/signing/publisher.json", "docs/signing/publisher-key.der", "docs/signing/publisher-cert.der",
    "docs/licenses/supplements.json", "docs/licenses/alloc-stdlib-0.3.0/LICENSE", "docs/licenses/rmcp-3.5.1/LICENSE"];

pub fn target(value: &str) -> io::Result<()> {
    if ["x86_64-pc-windows-msvc","aarch64-pc-windows-msvc","x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu","x86_64-apple-darwin","aarch64-apple-darwin"].contains(&value) { Ok(()) }
    else { Err(fail("Unsupported distribution target.")) }
}
pub fn prefix(target: &str, intermediate: bool) -> io::Result<String> {
    self::target(target)?;
    Ok(format!("{}numera-gen-mcp-{VERSION}-{target}", if intermediate {"unsigned-intermediate-"} else {""}))
}

fn collect(directory: &Path, root: &Path, entries: &mut BTreeMap<String,Vec<u8>>) -> io::Result<()> {
    let mut total = entries.values().try_fold(0usize, |total, data| total.checked_add(data.len()))
        .filter(|total| *total <= MAX_BYTES).ok_or_else(|| fail("Distribution exceeds bound."))?;
    let mut pending = vec![(directory.to_path_buf(), 0usize)];
    let mut scanned = 0usize;
    let mut names = entries.keys().map(|name| name.to_lowercase()).collect::<HashSet<_>>();
    while let Some((directory, depth)) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?; let info = entry.file_type()?;
            scanned += 1;
            if scanned > archives::MAX_MEMBERS * 2 || info.is_symlink() {
                return Err(fail("Linked or oversized dependency notice tree."));
            }
            if info.is_dir() {
                if depth >= 32 { return Err(fail("Dependency notice nesting exceeds bound.")); }
                pending.push((entry.path(), depth + 1));
            } else if info.is_file() {
                let name = format!("licenses/{}", entry.path().strip_prefix(root)
                    .map_err(|_| fail("Escaping notice."))?.to_string_lossy().replace('\\', "/"));
                validate_name(&name)?;
                if entries.len() >= archives::MAX_MEMBERS || !names.insert(name.to_lowercase()) {
                    return Err(fail("Duplicate or oversized notice inventory."));
                }
                let data = read_regular(&entry.path(), (1024 * 1024).min(MAX_BYTES - total))?;
                total = total.checked_add(data.len()).filter(|size| *size <= MAX_BYTES)
                    .ok_or_else(|| fail("Distribution exceeds bound."))?;
                entries.insert(name, data);
            } else { return Err(fail("Nonregular dependency notice.")); }
        }
    }
    Ok(())
}

pub fn read_regular(file: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let parent=file.parent().ok_or_else(||fail("Regular input parent missing."))?.canonicalize()?;
    let name=file.file_name().and_then(|s|s.to_str()).ok_or_else(||fail("Regular input filename must be Unicode."))?;
    artifact::read(&parent,name,limit as u64).map_err(|_|fail("Input is linked, changed or exceeds its bound."))
}

pub fn validate_entries(entries: &BTreeMap<String,Vec<u8>>, target: &str, source: &str) -> io::Result<Value> {
    self::target(target)?;
    let bytes=entries.get("manifest.json").ok_or_else(|| fail("Distribution manifest missing."))?;
    if bytes.len()>1024*1024 {return Err(fail("Distribution manifest exceeds bound."));}
    let manifest: artifact::Manifest=serde_json::from_slice(bytes).map_err(|_| fail("Invalid distribution manifest."))?;
    if manifest.version!=VERSION || manifest.target!=target || manifest.source_sha!=source || manifest.publisher!="KiaroSama"
        || manifest.files.is_empty() || manifest.files.len()>4096 {return Err(fail("Distribution identity differs."));}
    let mut names=HashSet::from(["manifest.json".to_owned()]); let mut folded=HashSet::new();let mut total=0usize;
    for row in &manifest.files {
        validate_name(&row.path)?;
        if matches!(row.path.as_str(),"manifest.json"|"manifest.sig") || !names.insert(row.path.clone()) || !folded.insert(row.path.to_lowercase()) {
            return Err(fail("Duplicate or self-referencing manifest member."));
        }
        let data=entries.get(&row.path).ok_or_else(|| fail("Manifest member missing."))?;
        total=total.checked_add(data.len()).filter(|n| *n<=MAX_BYTES).ok_or_else(|| fail("Distribution exceeds bound."))?;
        if data.len() as u64!=row.bytes || sha(data)!=row.sha256 {return Err(fail("Distribution member changed."));}
    }
    if entries.contains_key("manifest.sig") {names.insert("manifest.sig".to_owned());}
    if entries.keys().cloned().collect::<HashSet<_>>()!=names {return Err(fail("Unlisted distribution member."));}
    let suffix=if target.contains("windows"){".exe"}else{""};
    let binary=format!("bin/numera-gen-mcp{suffix}");let developer=format!("bin/numera-dev{suffix}");
    if entries.get(&binary).map(|v| sha(v)).as_deref()!=Some(manifest.binary_sha256.as_str())
        || !entries.contains_key(&developer) || entries.get("docs/assets/logo.png").map(Vec::as_slice)!=Some(numera_gen_mcp::build_identity::LOGO) {
        return Err(fail("Distribution binary, developer tool or logo differs."));
    }
    serde_json::from_slice(bytes).map_err(|_| fail("Invalid manifest JSON."))
}

pub fn authenticate_archive(
    bytes: &[u8], sidecar: &[u8], target: &str, source: &str, intermediate: bool,
) -> io::Result<BTreeMap<String, Vec<u8>>> {
    let prefix = prefix(target, intermediate)?;
    let name = format!("{prefix}.{}", if target.contains("windows") { "zip" } else { "tar.gz" });
    let text = std::str::from_utf8(sidecar).map_err(|_| fail("Invalid checksum encoding."))?;
    if text.split_whitespace().collect::<Vec<_>>() != [sha(bytes), name].iter().map(String::as_str).collect::<Vec<_>>() {
        return Err(fail("Archive checksum or filename differs."));
    }
    let entries = archives::decode(bytes, &prefix, target.contains("windows"))?;
    if entries.contains_key("manifest.sig") == intermediate {
        return Err(fail("Archive signing phase differs."));
    }
    validate_entries(&entries, target, source)?;
    Ok(entries)
}

pub fn read_tree(directory: &Path, target: &str, source: &str) -> io::Result<BTreeMap<String, Vec<u8>>> {
    let root = directory.canonicalize()?;
    let bytes = artifact::read(&root, "manifest.json", 1024 * 1024).map_err(|_| fail("Manifest read failed."))?;
    let manifest: artifact::Manifest = serde_json::from_slice(&bytes).map_err(|_| fail("Invalid manifest."))?;
    if manifest.files.len() > 4096 { return Err(fail("Manifest inventory exceeds bound.")); }
    let mut total = bytes.len() as u64;
    let mut names = HashSet::from(["manifest.json".to_owned()]);
    let mut entries = BTreeMap::from([("manifest.json".to_owned(), bytes)]);
    for row in manifest.files {
        validate_name(&row.path)?;
        total = total.checked_add(row.bytes).filter(|size| *size <= MAX_BYTES as u64)
            .ok_or_else(|| fail("Prepared aggregate exceeds bound."))?;
        if entries.contains_key(&row.path) || !names.insert(row.path.to_lowercase()) { return Err(fail("Invalid manifest member.")); }
        let bytes = artifact::read(&root, &row.path, row.bytes).map_err(|_| fail("Manifest member read failed."))?;
        entries.insert(row.path, bytes);
    }
    if directory.join("manifest.sig").try_exists()? {
        entries.insert("manifest.sig".to_owned(), artifact::read(&root, "manifest.sig", 1024).map_err(|_| fail("Signature read failed."))?);
    }
    validate_entries(&entries, target, source)?;
    let mut pending = vec![root.clone()]; let mut scanned = 0usize; let mut found = HashSet::new();
    while let Some(directory) = pending.pop() {
        for row in fs::read_dir(directory)? {
            let row = row?; let info = row.file_type()?; scanned += 1;
            if scanned > archives::MAX_MEMBERS * 2 || info.is_symlink() {
                return Err(fail("Linked or oversized prepared tree."));
            }
            #[cfg(windows)] {
                use std::os::windows::fs::MetadataExt;
                if row.metadata()?.file_attributes() & 0x400 != 0 { return Err(fail("Reparse point in prepared tree.")); }
            }
            let relative = row.path().strip_prefix(&root).map_err(|_| fail("Escaping prepared member."))?
                .to_str().ok_or_else(|| fail("Prepared member must be Unicode."))?.replace('\\', "/");
            if info.is_dir() {
                if !entries.keys().any(|name| name.starts_with(&format!("{relative}/"))) {
                    return Err(fail("Unlisted prepared directory."));
                }
                pending.push(row.path());
            } else if info.is_file() && entries.contains_key(&relative) { found.insert(relative); }
            else { return Err(fail("Unlisted or nonregular prepared member.")); }
        }
    }
    if found != entries.keys().cloned().collect::<HashSet<_>>() { return Err(fail("Prepared inventory changed.")); }
    Ok(entries)
}

pub fn stage(context: &Context, target: &str) -> io::Result<()> {
    context.hosted()?;
    let download = context.relative(".ci-work/signing-download")?;
    let prefix = prefix(target, true)?;
    let name = format!("{prefix}.{}", if target.contains("windows") { "zip" } else { "tar.gz" });
    let actual = fs::read_dir(&download)?.map(|row| row.map(|e| e.file_name())).collect::<io::Result<HashSet<_>>>()?;
    let expected = HashSet::from([std::ffi::OsString::from(&name), std::ffi::OsString::from(format!("{name}.sha256"))]);
    if actual != expected { return Err(fail("Unexpected signing download inventory.")); }
    let bytes = read_regular(&download.join(&name), MAX_BYTES)?;
    let checksum = read_regular(&download.join(format!("{name}.sha256")), 1024)?;
    let entries = authenticate_archive(&bytes, &checksum, target, &context.source_sha()?, true)?;
    let destination = context.relative(&format!(".ci-work/prepared/{target}"))?;
    fs::create_dir_all(destination.parent().ok_or_else(|| fail("Prepared parent missing."))?)?;
    write_tree(&destination, &entries)?;
    println!("{}", json!({"component":"native-signing-stage","intermediate_authenticated":true,"signed":false,"target":target}));
    Ok(())
}

pub fn write_tree(directory: &Path, entries: &BTreeMap<String,Vec<u8>>) -> io::Result<()> {
    fs::create_dir(directory)?;
    let operation=(||{
        for (name,bytes) in entries {
            validate_name(name)?;let file=directory.join(name);
            if let Some(parent)=file.parent(){fs::create_dir_all(parent)?;}
            use std::io::Write;
            let mut options=fs::OpenOptions::new();options.create_new(true).write(true);
            #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(if name.starts_with("bin/"){0o755}else{0o644});}
            let mut output=options.open(file)?;output.write_all(bytes)?;output.sync_all()?;
        }
        Ok(())
    })();
    if operation.is_err(){fs::remove_dir_all(directory)?;}
    operation
}

pub fn prepare(context:&Context,target:&str,intermediate:bool)->io::Result<()> {
    context.hosted()?;self::target(target)?;
    let suffix=if target.contains("windows"){".exe"}else{""};let mut entries=BTreeMap::new();
    for name in ["numera-gen-mcp","numera-dev"]{
        entries.insert(format!("bin/{name}{suffix}"),read_regular(&context.root.join(format!("target/release/{name}{suffix}")),MAX_BYTES)?);
    }
    for name in ASSETS{entries.insert((*name).to_owned(),read_regular(&context.relative(name)?,MAX_BYTES)?);}
    let licenses=context.relative(".ci-work/native/licenses")?;
    if !licenses.join("inventory.json").is_file(){return Err(fail("Resolved license inventory required."));}
    collect(&licenses,&licenses,&mut entries)?;
    let source=context.source_sha()?;let binary=format!("bin/numera-gen-mcp{suffix}");
    let manifest=json!({"version":VERSION,"target":target,"source_sha":source,"publisher":"KiaroSama",
        "binary_sha256":sha(&entries[&binary]),"files":entries.iter().map(|(name,bytes)|json!({"path":name,"bytes":bytes.len(),"sha256":sha(bytes)})).collect::<Vec<_>>()});
    let mut bytes=serde_json::to_vec_pretty(&manifest).map_err(|_| fail("Cannot encode manifest."))?;bytes.push(b'\n');entries.insert("manifest.json".to_owned(),bytes);
    validate_entries(&entries,target,&source)?;
    let prepared=context.relative(&format!(".ci-work/prepared/{target}"))?;
    fs::create_dir_all(prepared.parent().ok_or_else(||fail("Invalid prepared root."))?)?;
    write_tree(&prepared,&entries)?;
    if intermediate{archive(context,&entries,target,true)?;}
    Ok(())
}

pub fn archive(context:&Context,entries:&BTreeMap<String,Vec<u8>>,target:&str,intermediate:bool)->io::Result<()> {
    if !intermediate && !entries.contains_key("manifest.sig"){return Err(fail("Unsigned final distribution forbidden."));}
    validate_entries(entries,target,&context.source_sha()?)?;
    let prefix=prefix(target,intermediate)?;let zip=target.contains("windows");
    let bytes=archives::encode(&prefix,entries,zip)?;
    let name=format!("{prefix}.{}",if zip{"zip"}else{"tar.gz"});let dir=if intermediate{"intermediate"}else{"release"};
    context.write_new(&format!(".ci-work/{dir}/{name}"),&bytes)?;
    context.write_new(&format!(".ci-work/{dir}/{name}.sha256"),format!("{}  {name}\n",sha(&bytes)).as_bytes())?;
    println!("{}",json!({"component":"native-package","archive":name,"sha256":sha(&bytes),"files":entries.len(),"internal_intermediate":intermediate}));
    Ok(())
}
