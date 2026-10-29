use crate::{
    acceptance::{Scratch, public_environment},
    archives::MAX_BYTES,
    context::{Context, fail, sha},
    distribution::{self, read_regular, read_tree, validate_entries, write_tree},
    private_files::Secret,
};
use numera_gen_mcp::devtools::artifact;
use serde_json::{Value, json};
use std::{fs, io::{self, Write}, path::{Path, PathBuf}, process::Command};

pub fn require_rejection(code: i32, reason: Option<&str>, cleanup_verified: bool) -> io::Result<()> {
    if code <= 0 || reason.is_some() || !cleanup_verified {
        return Err(fail("Signature negative case did not complete with a verified rejection."));
    }
    Ok(())
}

pub fn binary_members(manifest: &Value, target: &str) -> io::Result<[String; 2]> {
    distribution::target(target)?;
    let suffix = if target.contains("windows") { ".exe" } else { "" };
    let expected = [format!("bin/numera-gen-mcp{suffix}"), format!("bin/numera-dev{suffix}")];
    let rows = manifest["files"].as_array().ok_or_else(|| fail("Signature inventory missing."))?;
    if rows.len() > 4096 || rows.iter().any(|row| !row["path"].is_string()) {
        return Err(fail("Signature inventory exceeds bound or contains invalid paths."));
    }
    let actual = rows.iter().filter_map(|row| row["path"].as_str())
        .filter(|name| name.starts_with("bin/")).collect::<Vec<_>>();
    if actual.len() != 2 || expected.iter().any(|name| !actual.contains(&name.as_str())) {
        return Err(fail("Both distinct native executable signatures must be tested."));
    }
    Ok(expected)
}

fn tool(name: &str) -> io::Result<PathBuf> {
    let path = std::env::var_os(format!("NUMERA_{}", name.to_uppercase()))
        .map(PathBuf::from).ok_or_else(|| fail("Qualified signing tool missing."))?;
    if !path.is_absolute() { return Err(fail("Qualified signing tool path must be absolute.")); }
    read_regular(&path, MAX_BYTES)?;
    path.canonicalize()
}

fn command(context: &Context, program: &Path) -> Command {
    crate::signing::private_command(context, program)
}

fn completed(context: &Context, command: &mut Command, reject: bool) -> io::Result<Vec<u8>> {
    let codesign = command.get_program() == std::ffi::OsStr::new("/usr/bin/codesign");
    let outcome = context.outcome(command, 30000, 15000, 1024 * 1024)?;
    if reject {
        require_rejection(outcome.code, outcome.reason.as_deref(), outcome.cleanup_verified)?;
    } else if outcome.code != 0 || outcome.reason.is_some() || !outcome.cleanup_verified {
        if codesign && outcome.code != 0 && outcome.reason.is_none() && outcome.cleanup_verified {
            return Err(crate::signing::codesign_failure(&outcome.stderr));
        }
        return Err(fail("Required signature acceptance command failed."));
    }
    Ok(outcome.stdout)
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn rejected<T>(result: numera_gen_mcp::error::Result<T>) -> io::Result<()> {
    if result.is_ok() { return Err(fail("Invalid native signature was accepted.")); }
    Ok(())
}

#[cfg(target_os = "macos")]
fn keychains(context: &Context) -> io::Result<[Vec<u8>; 2]> {
    Ok([
        completed(context, command(context, Path::new("/usr/bin/security")).args(["list-keychains", "-d", "user"]), false)?,
        completed(context, command(context, Path::new("/usr/bin/security")).args(["default-keychain", "-d", "user"]), false)?,
    ])
}

fn fixture_identity(context: &Context, scratch: &Path, openssl: &Path) -> io::Result<(PathBuf, PathBuf, Secret)> {
    let key = scratch.join("wrong-key.pem");
    let cert = scratch.join("wrong-cert.pem");
    let pfx = scratch.join("wrong.pfx");
    // Precreate private outputs instead of changing the process-wide umask across parallel tests.
    for path in [&key, &cert, &pfx] { write_new(path, b"")?; }
    let password = Secret(format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple()).into_bytes());
    let password_file = scratch.join("fixture-password.txt");
    write_new(&password_file, &password.0)?;
    let config = scratch.join("fixture-openssl.cnf");
    write_new(&config, b"[req]\ndistinguished_name=dn\nprompt=no\n[dn]\nCN=Unrelated fixture signer\n")?;
    let mut request = command(context, openssl);
    request.args(["req", "-x509", "-newkey", "rsa:2048", "-noenc", "-sha256", "-days", "1", "-batch", "-config"])
        .arg(&config).args(["-subj", "/CN=Unrelated fixture signer", "-addext", "extendedKeyUsage=codeSigning", "-addext", "basicConstraints=critical,CA:FALSE", "-addext", "keyUsage=critical,digitalSignature", "-keyout"])
        .arg(&key).arg("-out").arg(&cert);
    completed(context, &mut request, false)?;
    let pass = format!("file:{}", password_file.to_str().ok_or_else(|| fail("Fixture password path is not Unicode."))?);
    completed(context, command(context, openssl).args(["pkcs12", "-export", "-inkey"]).arg(&key)
        .arg("-in").arg(&cert).arg("-out").arg(&pfx).arg("-passout").arg(pass), false)?;
    Ok((key, cert, password))
}

pub fn verify(context: &Context, target: &str) -> io::Result<()> {
    context.hosted()?;
    distribution::target(target)?;
    if target != artifact::target() { return Err(fail("Signature acceptance requires the matching native host.")); }
    public_environment(context)?;
    crate::signing::public_identity()?;
    let source = context.source_sha()?;
    let prepared = context.relative(&format!(".ci-work/prepared/{target}"))?;
    let authenticated = artifact::verify_activation(&prepared).map_err(|_| fail("Prepared native signatures rejected."))?;
    if authenticated.manifest.source_sha != source || authenticated.manifest.target != target {
        return Err(fail("Signature acceptance source or target differs."));
    }
    let entries = read_tree(&prepared, target, &source)?;
    let initial_manifest = sha(&authenticated.manifest_bytes);
    let manifest = validate_entries(&entries, target, &source)?;
    let binaries = binary_members(&manifest, target)?;
    let scratch = Scratch::new(context, "signature-acceptance")?;
    let result = (|| {
        let openssl = tool("openssl")?;
        let (key, cert, password) = fixture_identity(context, &scratch.path, &openssl)?;
        let public = scratch.path.join("approved-public.pem");
        use base64::{Engine, engine::general_purpose::STANDARD};
        let encoded = STANDARD.encode(include_bytes!("../../docs/signing/publisher-key.der"));
        let mut pem = String::from("-----BEGIN RSA PUBLIC KEY-----\n");
        for line in encoded.as_bytes().chunks(64) {
            pem.push_str(std::str::from_utf8(line).map_err(|_| fail("Invalid public key encoding."))?);
            pem.push('\n');
        }
        pem.push_str("-----END RSA PUBLIC KEY-----\n");
        write_new(&public, pem.as_bytes())?;
        completed(context, command(context, &openssl).args(["dgst", "-sha256", "-verify"]).arg(&public)
            .arg("-signature").arg(prepared.join("manifest.sig")).arg(prepared.join("manifest.json")), false)?;
        let wrong_signature = scratch.path.join("wrong-manifest.sig");
        write_new(&wrong_signature, b"")?;
        completed(context, command(context, &openssl).args(["dgst", "-sha256", "-sign"]).arg(&key)
            .arg("-out").arg(&wrong_signature).arg(prepared.join("manifest.json")), false)?;
        completed(context, command(context, &openssl).args(["dgst", "-sha256", "-verify"]).arg(&public)
            .arg("-signature").arg(&wrong_signature).arg(prepared.join("manifest.json")), true)?;
        let mut wrong_tree = entries.clone();
        wrong_tree.insert("manifest.sig".into(), read_regular(&wrong_signature, 1024)?);
        let wrong_root = scratch.path.join("wrong-manifest-signer");
        write_tree(&wrong_root, &wrong_tree)?;
        rejected(artifact::verify(&wrong_root))?;
        fs::remove_dir_all(&wrong_root)?;
        drop(wrong_tree);
        let mut changed = entries.clone();
        changed.get_mut("manifest.json").ok_or_else(|| fail("Manifest missing."))?.push(b' ');
        let changed_root = scratch.path.join("changed-manifest");
        write_tree(&changed_root, &changed)?;
        completed(context, command(context, &openssl).args(["dgst", "-sha256", "-verify"]).arg(&public)
            .arg("-signature").arg(prepared.join("manifest.sig")).arg(changed_root.join("manifest.json")), true)?;
        rejected(artifact::verify(&changed_root))?;
        fs::remove_dir_all(&changed_root)?;
        drop(changed);
        for (index, name) in binaries.iter().enumerate() {
            let original = prepared.join(name);
            let filename = original.file_name().and_then(|name| name.to_str())
                .ok_or_else(|| fail("Executable filename must be Unicode."))?;
            let mut tampered = read_regular(&original, MAX_BYTES)?;
            let middle = tampered.len() / 2;
            let byte = tampered.get_mut(middle).ok_or_else(|| fail("Executable is empty."))?;
            *byte ^= 1;
            let altered = scratch.path.join(format!("tampered-{filename}"));
            write_new(&altered, &tampered)?;
            if target.contains("windows") || target.contains("apple") {
                rejected(artifact::verify_native(&altered))?;
            } else {
                let mut tree = entries.clone(); tree.insert(name.clone(), tampered);
                let root = scratch.path.join(format!("changed-elf-{index}"));
                write_tree(&root, &tree)?;
                rejected(artifact::verify(&root))?;
                fs::remove_dir_all(root)?;
            }
            let unsigned = scratch.path.join(format!("unsigned-{filename}"));
            let wrong = scratch.path.join(format!("wrong-{filename}"));
            if target.contains("windows") {
                let signer = tool("osslsigncode")?;
                completed(context, command(context, &signer).args(["remove-signature", "-in"])
                    .arg(&original).arg("-out").arg(&unsigned), false)?;
                rejected(artifact::verify_native(&unsigned))?;
                completed(context, command(context, &signer).args(["sign", "-pkcs12"])
                    .arg(scratch.path.join("wrong.pfx")).arg("-readpass").arg(scratch.path.join("fixture-password.txt"))
                    .args(["-h", "sha256", "-in"]).arg(&unsigned).arg("-out").arg(&wrong), false)?;
                let certificate = completed(context, command(context, &openssl).args(["x509", "-in"])
                    .arg(&cert).args(["-outform", "DER"]), false)?;
                completed(context, command(context, &signer).args(["verify", "-CAfile"]).arg(&cert)
                    .args(["-ignore-cdp", "-require-leaf-hash"]).arg(format!("sha256:{}", sha(&certificate)))
                    .arg("-in").arg(&wrong), false)?;
                rejected(artifact::verify_native(&wrong))?;
            } else if target.contains("apple") {
                #[cfg(target_os = "macos")] {
                    write_new(&unsigned, &read_regular(&original, MAX_BYTES)?)?;
                    completed(context, command(context, Path::new("/usr/bin/codesign")).arg("--remove-signature").arg(&unsigned), false)?;
                    rejected(artifact::verify_native(&unsigned))?;
                    let certificate = completed(context, command(context, &openssl).args(["x509", "-in"])
                        .arg(&cert).args(["-outform", "DER"]), false)?;
                    let pfx = Secret(read_regular(&scratch.path.join("wrong.pfx"), 32768)?);
                    let before = keychains(context)?;
                    let signed = crate::signing_macos::sign(&pfx.0, &password.0, &sha(&certificate), &[unsigned.as_path()]);
                    if keychains(context)? != before { return Err(fail("Wrong-signer fixture changed keychain preferences.")); }
                    signed?;
                    completed(context, command(context, Path::new("/usr/bin/codesign")).args(["--verify", "--strict"]).arg(&unsigned), false)?;
                    rejected(artifact::verify_native(&unsigned))?;
                }
                #[cfg(not(target_os = "macos"))]
                return Err(fail("Native macOS fixture signer unavailable."));
            }
        }
        let _ = (&cert, &password);
        let final_tree = artifact::verify_activation(&prepared).map_err(|_| fail("Original signatures changed during acceptance."))?;
        if sha(&final_tree.manifest_bytes) != initial_manifest {
            return Err(fail("Original prepared manifest changed during acceptance."));
        }
        Ok(())
    })();
    let cleanup = scratch.cleanup();
    result?;
    cleanup?;
    println!("{}", json!({"component":"native-signing-acceptance","target":target,"source_sha":source,
        "native_positive_negative":true,"publisher_inputs_absent":true,"wrong_signer_rejected":true,
        "tamper_rejected":true,"fixture_cleanup":true,"public_os_trust":false}));
    Ok(())
}
