use super::{Options, crypto::Crypto, failure};
use crate::devtools::{
    artifact,
    private_files::{self, Secret},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

const REGISTRY_LIMIT: usize = 1048576;
fn absent(path: &Path) -> io::Result<()> {
    private_files::verify_ancestors(path)?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        _ => Err(failure()),
    }
}
fn public(root: &Path, name: &str, limit: u64) -> io::Result<Vec<u8>> {
    artifact::read(root, &format!("docs/signing/{name}"), limit).map_err(|_| failure())
}
fn inspect(root: &Path) -> io::Result<Value> {
    let registry = root.join("secrets.md");
    let security = private_files::inspect_private(&registry, false)?;
    if fs::symlink_metadata(&registry)?.len() > REGISTRY_LIMIT as u64 {
        return Err(failure());
    }
    Ok(
        json!({"registry_present":true,"registry_owner_only":security.owner_only,
        "private_identity_exists":root.join(".runtime/signing").try_exists()?,
        "public_identity_exists":root.join("docs/signing/publisher-key.der").try_exists()?}),
    )
}
fn preflight(root: &Path) -> io::Result<()> {
    private_files::verify_ancestors(&root.join("secrets.md"))?;
    let security = private_files::inspect_private(&root.join("secrets.md"), false)?;
    if !security.owner_current_user
        || fs::symlink_metadata(root.join("secrets.md"))?.len() > REGISTRY_LIMIT as u64
    {
        return Err(failure());
    }
    for path in [
        ".runtime/signing",
        "docs/signing/publisher-key.der",
        "docs/signing/publisher-cert.der",
        "docs/signing/publisher.json",
    ] {
        match fs::symlink_metadata(root.join(path)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            _ => return Err(failure()),
        }
    }
    for path in [
        root.join(".runtime"),
        root.join("docs"),
        root.join("docs/signing"),
    ] {
        if path.try_exists()? {
            private_files::verify_ancestors(&path)?;
        }
    }
    Ok(())
}
fn create(root: &Path, options: &Options) -> io::Result<Value> {
    preflight(root)?;
    if options.dry_run {
        return Ok(
            json!({"dry_run":true,"private_custody_created":false,"registry_updated":false,
            "os_trust_changed":false,"remote_secret_changed":false}),
        );
    }
    let crypto = Crypto::open(
        options.library.as_ref().ok_or_else(failure)?,
        options.library_sha256.as_deref().ok_or_else(failure)?,
    )?;
    let mut registry =
        private_files::open_registry(&root.join("secrets.md"), REGISTRY_LIMIT as u64)?;
    let mut original = Secret(Vec::new());
    (&mut registry.file)
        .take(REGISTRY_LIMIT as u64 + 1)
        .read_to_end(&mut original.0)?;
    let text = std::str::from_utf8(&original.0).map_err(|_| failure())?;
    if original.0.len() > REGISTRY_LIMIT || text.contains("NUMERA_SIGNING_PFX_PASSWORD") {
        return Err(failure());
    }
    for parent in [
        root.join(".runtime"),
        root.join("docs"),
        root.join("docs/signing"),
    ] {
        if !parent.try_exists()? {
            private_files::create_directory(&parent)?;
        }
        private_files::verify_ancestors(&parent)?;
    }
    let private = root.join(".runtime/signing");
    let _private_parents = private_files::lock_parents(&private)?;
    let _public_parents =
        private_files::lock_parents(&root.join("docs/signing/publisher-key.der"))?;
    absent(&private)?;
    private_files::create_directory(&private)?;
    let _custody = private_files::lock_parents(&private.join("publisher.pfx"))?;
    // Persistent partial custody is deliberately retained on failure, never automatically replaced.
    private_files::write_new(
        &private.join("registry-before.sddl"),
        registry.original_sddl.as_bytes(),
    )?;
    private_files::restrict_registry(&registry)?;
    let material = crypto.create()?;
    private_files::write_new(&private.join("publisher.pfx"), &material.pfx.0)?;
    private_files::write_new(
        &private.join("publisher-password.txt"),
        &material.password.0,
    )?;
    let password = std::str::from_utf8(&material.password.0).map_err(|_| failure())?;
    let entry = Secret(format!("\n## Native artifact signing\n- Secret: NUMERA_SIGNING_PFX_PASSWORD\n- Purpose: approved KiaroSama self-signed native artifact signing only.\n- Service: protected final hosted signing; not configured remotely by this command.\n- Source: owner-authorized project identity creation.\n- Updated UTC: {}\n- Private binary custody: .runtime/signing/publisher.pfx (encrypted; preserve for recovery).\n- Password value: {password}\n- Recovery: use this same PFX/password; never rotate on a failed build. No OS trust installation.\n", chrono::Utc::now().to_rfc3339()).into_bytes());
    registry.file.seek(SeekFrom::End(0))?;
    registry.file.write_all(&entry.0)?;
    registry.file.sync_all()?;
    for (name, bytes) in [
        ("publisher-key.der", material.key.as_slice()),
        ("publisher-cert.der", material.certificate.as_slice()),
    ] {
        private_files::write_new(&root.join("docs/signing").join(name), bytes)?;
    }
    let mut metadata = serde_json::to_vec_pretty(&material.metadata).map_err(|_| failure())?;
    metadata.push(b'\n');
    private_files::write_new(&root.join("docs/signing/publisher.json"), &metadata)?;
    Ok(
        json!({"publisher":"KiaroSama","public_key_sha256":material.metadata["public_key_sha256"],
        "private_custody_created":true,"registry_updated":true,"os_trust_changed":false,"remote_secret_changed":false}),
    )
}
fn check(root: &Path, options: &Options) -> io::Result<Value> {
    let crypto = Crypto::open(
        options.library.as_ref().ok_or_else(failure)?,
        options.library_sha256.as_deref().ok_or_else(failure)?,
    )?;
    let _custody = private_files::lock_parents(&root.join(".runtime/signing/publisher.pfx"))?;
    let _public = private_files::lock_parents(&root.join("docs/signing/publisher-key.der"))?;
    let pfx = private_files::read_private(&root.join(".runtime/signing/publisher.pfx"), 32768)?;
    let password =
        private_files::read_private(&root.join(".runtime/signing/publisher-password.txt"), 4095)?;
    let key = public(root, "publisher-key.der", 2048)?;
    let certificate = public(root, "publisher-cert.der", 16384)?;
    let metadata: Value =
        serde_json::from_slice(&public(root, "publisher.json", 65536)?).map_err(|_| failure())?;
    crypto.check(&pfx.0, &password.0, &key, &certificate, &metadata)?;
    let registry = private_files::read_private(&root.join("secrets.md"), REGISTRY_LIMIT)?;
    let expected = Secret(
        format!(
            "- Password value: {}",
            std::str::from_utf8(&password.0).map_err(|_| failure())?
        )
        .into_bytes(),
    );
    let registry = std::str::from_utf8(&registry.0).map_err(|_| failure())?;
    let expected = std::str::from_utf8(&expected.0).map_err(|_| failure())?;
    if !registry.lines().any(|line| line == expected) {
        return Err(failure());
    }
    if options.write_fixtures {
        super::fixtures::write(root, &crypto, &pfx.0, &password.0)?;
    }
    Ok(
        json!({"identity_recovered":true,"public_binding":true,"unicode_roundtrip":true,"tamper_rejected":true,
        "wrong_signer_rejected":true,"local_recovery_verified":true,"artifact_signed":false,"os_trust_changed":false}),
    )
}
pub(super) fn execute(root: &Path, options: &Options) -> io::Result<Value> {
    private_files::verify_ancestors(root)?;
    let root = root.canonicalize()?;
    let _parents = private_files::lock_parents(&root.join("secrets.md"))?;
    match options.action.as_str() {
        "inspect" => inspect(&root),
        "create" => create(&root, options),
        "check" => check(&root, options),
        _ => Err(failure()),
    }
}
