use crate::{archives::MAX_BYTES, context::{Context, fail, sha}, distribution::{self, read_regular}};
use numera_gen_mcp::devtools::artifact;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, io, path::{Path, PathBuf}};

pub struct Scratch { pub path: PathBuf }
impl Scratch {
    pub fn new(context: &Context, purpose: &str) -> io::Result<Self> {
        if purpose.is_empty() || !purpose.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
            return Err(fail("Invalid owned scratch purpose."));
        }
        let parent = context.relative(&format!(".ci-work/{purpose}"))?;
        fs::create_dir_all(&parent)?;
        let path = parent.join(uuid::Uuid::new_v4().to_string());
        crate::private_files::create_directory(&path)?;
        Ok(Self { path })
    }
    pub fn cleanup(self) -> io::Result<()> {
        fs::remove_dir_all(&self.path)?;
        if self.path.try_exists()? { return Err(fail("Owned acceptance scratch remains.")); }
        Ok(())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.path.exists() && fs::remove_dir_all(&self.path).is_err() {
            eprintln!("ERROR: Owned acceptance scratch cleanup remains unverified.");
        }
    }
}

pub fn public_environment(context: &Context) -> io::Result<()> {
    if context.relative(".ci-work/signing")?.try_exists()?
        || std::env::vars_os().any(|(key, _)| key.to_string_lossy().starts_with("NUMERA_SIGNING_")) {
        return Err(fail("Publisher inputs must be absent before acceptance."));
    }
    Ok(())
}
fn binary(root: &Path, developer: bool) -> PathBuf {
    root.join("bin").join(format!("{}{}", if developer { "numera-dev" } else { "numera-gen-mcp" }, if cfg!(windows) { ".exe" } else { "" }))
}
fn setup(context: &Context, developer: &Path, target: &Path, action: &str,
    prepared: &Path, host: &Path, config: &Path, reject: bool) -> io::Result<Value> {
    let mut command = context.command(developer);
    command.args(["setup", action, "--root"]).arg(target);
    if matches!(action, "Install" | "Update") {
        command.arg("--artifact").arg(prepared);
    } else {
        command.args(["--client", "ClaudeDesktop", "--host-config"]).arg(host).arg("--config").arg(config);
    }
    let outcome = context.outcome(&mut command, 20000, 10000, 1024 * 1024)?;
    if !outcome.cleanup_verified || outcome.reason.is_some() || outcome.code != if reject { 1 } else { 0 } {
        return Err(fail("Actual setup CLI outcome differs."));
    }
    if reject { return Ok(Value::Null); }
    serde_json::from_slice(&outcome.stdout).map_err(|_| fail("Setup CLI returned invalid JSON."))
}
fn installed(root: &Path, manifest: &artifact::Manifest) -> io::Result<BTreeMap<String, Vec<u8>>> {
    manifest.files.iter().map(|row| Ok((row.path.clone(), read_regular(&root.join(&row.path), MAX_BYTES)?))).collect()
}
fn unchanged(target: &Path, original: &BTreeMap<String, Vec<u8>>, manifest: &artifact::Manifest) -> io::Result<()> {
    if installed(target, manifest)? != *original
        || fs::read(target.join("config.local.json"))? != b"private fixture unchanged"
        || target.join(".numera-install.lock").try_exists()?
        || fs::read_dir(target)?.map(|row| row.map(|entry| entry.file_name().to_string_lossy().starts_with(".numera-staging-")))
            .collect::<io::Result<Vec<_>>>()?.into_iter().any(|staging| staging) {
        return Err(fail("Activation failed preservation or cleanup postconditions."));
    }
    Ok(())
}

pub fn activation(context: &Context, prepared: &Path) -> io::Result<()> {
    context.hosted()?; public_environment(context)?;
    let authenticated = artifact::verify_activation(prepared).map_err(|_| fail("Signed activation candidate rejected."))?;
    if authenticated.manifest.source_sha != context.source_sha()? { return Err(fail("Activation source differs.")); }
    let scratch = Scratch::new(context, "activation")?;
    let target = scratch.path.join("installation with spaces"); fs::create_dir(&target)?;
    let host = scratch.path.join("host.json"); let config = scratch.path.join("config.json");
    fs::write(target.join("config.local.json"), b"private fixture unchanged")?;
    let developer = binary(prepared, true);
    if setup(context, &developer, &target, "Install", prepared, &host, &config, false)?["installed"] != true {
        return Err(fail("Signed installation did not complete."));
    }
    let original = installed(&target, &authenticated.manifest)?;
    if original != installed(prepared, &authenticated.manifest)? { return Err(fail("Installed original bytes differ.")); }
    unchanged(&target, &original, &authenticated.manifest)?;
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        let second = authenticated.manifest.files.get(1).ok_or_else(|| fail("Rollback second member missing."))?;
        // No write/delete sharing: replacement must fail after the first member and roll back.
        let held = fs::OpenOptions::new().read(true).share_mode(1).open(target.join(&second.path))?;
        setup(context, &developer, &target, "Update", prepared, &host, &config, true)?;
        unchanged(&target, &original, &authenticated.manifest)?;
        drop(held);
    }
    setup(context, &developer, &target, "Update", prepared, &host, &config, false)?;
    unchanged(&target, &original, &authenticated.manifest)?;
    let before = json!({"retained":true,"mcpServers":{"other":{"command":"unchanged"}}});
    fs::write(&host, serde_json::to_vec(&before).map_err(|_| fail("Cannot encode isolated host fixture."))?)?;
    fs::write(&config, br#"{"api_endpoint":"https://example.invalid/v1","api_key":"fixture-not-used","models":[{"id":"fixture","name":"Fixture"}]}"#)?;
    setup(context, &developer, &target, "Register", prepared, &host, &config, false)?;
    let registered: Value = serde_json::from_slice(&fs::read(&host)?).map_err(|_| fail("Invalid registered fixture."))?;
    let expected = binary(&target, false).canonicalize()?;
    if registered["retained"] != true || registered["mcpServers"]["other"] != before["mcpServers"]["other"]
        || registered["mcpServers"]["numera-gen"]["args"] != json!(["bridge"])
        || registered["mcpServers"]["numera-gen"]["command"].as_str().map(Path::new).and_then(|p|p.canonicalize().ok()).as_ref() != Some(&expected) {
        return Err(fail("Entry-scoped registration differs."));
    }
    setup(context, &developer, &target, "Remove", prepared, &host, &config, false)?;
    let after: Value = serde_json::from_slice(&fs::read(&host)?).map_err(|_| fail("Invalid removed fixture."))?;
    if before != after { return Err(fail("Removal changed unrelated fixture settings.")); }
    unchanged(&target, &original, &authenticated.manifest)?;
    scratch.cleanup()?;
    println!("{}", json!({"component":"native-activation","target":authenticated.manifest.target,"signed_install_update_register_remove":true,"private_files_preserved":true,"fixture_cleanup":true,"owner_host_changed":false,"public_os_trust":false}));
    Ok(())
}

pub fn sdk(context: &Context, executable: &Path, phase: &str) -> io::Result<()> {
    let identity = crate::test_execution::hosted_identity(context, phase)?;
    if !["release", "extracted-intermediate", "extracted-signed"].contains(&phase) {
        return Err(fail("Unsupported SDK execution phase."));
    }
    let executable = executable.canonicalize()?;
    if !executable.starts_with(&context.root) { return Err(fail("Tested SDK binary escapes the workspace.")); }
    let tested_hash = sha(&read_regular(&executable, MAX_BYTES)?);
    let provenance = crate::evidence::snapshot(context)?;
    let mut harnesses = BTreeMap::new();
    let mut failures = Vec::new();
    for name in ["sdk_acceptance", "media_acceptance"] {
        let source = format!("tests/rust/{name}.rs");
        let operation = (|| {
            let test = crate::ci::test_executable(context, name)?;
            let command = || {
                let mut command = crate::test_execution::harness_command(context, &context.command("cargo"), &source, &test);
                command.env("NUMERA_NATIVE_BINARY", &executable);
                command
            };
            crate::test_execution::execute_harness(context, command, &source,
                std::time::Instant::now() + std::time::Duration::from_millis(180000), 45000)
        })();
        match operation {
            Ok(report) => { harnesses.insert(source, report); },
            Err(error) => {
                failures.push(format!("harness:{source}"));
                harnesses.insert(source, json!({"execution_verified":false,"error":crate::context::diagnostic(&error)}));
            }
        }
    }
    if tested_hash != sha(&read_regular(&executable, MAX_BYTES)?) { failures.push("tested-binary-changed".into()); }
    if crate::evidence::snapshot(context)? != provenance { failures.push("execution-provenance-changed".into()); }
    let report = json!({"schema_version":1,"component":"native-exact-sdk-execution","identity":identity,
        "provenance_sha256":provenance.sha256,"tested_binary_sha256":tested_hash,
        "harnesses":harnesses,"required_failed_gates":failures,"semantic_parity_claimed":false,"qualification_verified":false});
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| fail("Cannot encode exact SDK execution evidence."))?;
    if failures.is_empty() {
        let expected = ["tests/rust/sdk_acceptance.rs", "tests/rust/media_acceptance.rs"]
            .into_iter().map(str::to_owned).collect();
        crate::execution_evidence::verify_report(&bytes, &identity, &provenance.sha256,
            &provenance.dependencies, &expected)?;
    }
    context.write_new(&format!(".ci-work/native/sdk-execution-{phase}.json"), &bytes)?;
    if failures.is_empty() { Ok(()) } else { Err(fail("Required exact SDK execution failed.")) }
}
pub fn extract(context: &Context, target: &str, intermediate: bool) -> io::Result<()> {
    context.hosted()?; public_environment(context)?;
    let prefix = distribution::prefix(target, intermediate)?;
    let name = format!("{prefix}.{}", if target.contains("windows") { "zip" } else { "tar.gz" });
    let parent = context.relative(if intermediate { ".ci-work/intermediate" } else { ".ci-work/release" })?;
    let bytes = read_regular(&parent.join(&name), MAX_BYTES)?;
    let checksum = read_regular(&parent.join(format!("{name}.sha256")), 1024)?;
    let entries = distribution::authenticate_archive(&bytes, &checksum, target, &context.source_sha()?, intermediate)?;
    let scratch = Scratch::new(context, "extracted")?;
    let installed = scratch.path.join(&prefix); distribution::write_tree(&installed, &entries)?;
    if !intermediate {
        artifact::verify_activation(&installed).map_err(|_| fail("Extracted signature rejected."))?;
        activation(context, &installed)?;
    }
    for developer in [false, true] {
        let output = context.capture(context.command(binary(&installed, developer)).arg("--version"), 10000, 5000, 1024 * 1024)?;
        let version = std::str::from_utf8(&output).map_err(|_| fail("Version is not UTF-8."))?;
        if !version.contains(&context.source_sha()?) || !version.contains(distribution::VERSION) || !version.contains("KiaroSama") {
            return Err(fail("Extracted build identity differs."));
        }
    }
    sdk(context, &binary(&installed, false), if intermediate { "extracted-intermediate" } else { "extracted-signed" })?;
    scratch.cleanup()?;
    println!("{}", json!({"component":"native-extract","archive":name,"verified":true,"completed_signed_build":!intermediate,"internal_intermediate":intermediate,"sha256":sha(&bytes)}));
    Ok(())
}
