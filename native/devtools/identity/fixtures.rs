use super::{crypto::Crypto, failure};
use crate::devtools::{artifact, private_files};
use serde_json::json;
use std::{io, path::Path};

/// Explicit historical-format fixture authoring. Existing fixtures are never replaced or refreshed.
pub(super) fn write(root: &Path, crypto: &Crypto, pfx: &[u8], password: &[u8]) -> io::Result<()> {
    const TARGETS: [&str; 6] = [
        "x86_64-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ];
    let logo =
        artifact::read(root, "docs/assets/logo.png", 8 * 1024 * 1024).map_err(|_| failure())?;
    // Validate every destination first; failure cannot partially overwrite an immutable fixture set.
    for variant in ["approved", "update"] {
        for target in TARGETS {
            for suffix in ["json", "sig", "wrong.sig"] {
                match std::fs::symlink_metadata(root.join(format!(
                    "tests/fixtures/native-signing/{variant}/{target}.{suffix}"
                ))) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                    _ => return Err(failure()),
                }
            }
        }
    }
    for parent in [
        "tests",
        "tests/fixtures",
        "tests/fixtures/native-signing",
        "tests/fixtures/native-signing/approved",
        "tests/fixtures/native-signing/update",
    ] {
        let path = root.join(parent);
        if !path.try_exists()? {
            private_files::create_directory(&path)?;
        }
        private_files::verify_ancestors(&path)?;
    }
    let _approved =
        private_files::lock_parents(&root.join("tests/fixtures/native-signing/approved/marker"))?;
    let _update =
        private_files::lock_parents(&root.join("tests/fixtures/native-signing/update/marker"))?;
    let signer = crypto.fixture_signer(pfx, password)?;
    for variant in ["approved", "update"] {
        for target in TARGETS {
            let binary = if target.contains("windows") {
                "bin/numera-image-gen-mcp.exe"
            } else {
                "bin/numera-image-gen-mcp"
            };
            let bytes = if variant == "update" {
                b"updated synthetic install fixture; never executed".as_slice()
            } else {
                b"synthetic install fixture; never executed".as_slice()
            };
            let payload = json!({"version":if variant == "update" { "0.2.0" } else { "0.1.0" },
                "target":target,"source_sha":"fc7da25c3b470199949b1b4c67127d77afa0c34a","publisher":"KiaroSama",
                "binary_sha256":crate::compat::hash::sha256(bytes),"files":[
                    {"path":binary,"sha256":crate::compat::hash::sha256(bytes),"bytes":bytes.len()},
                    {"path":"docs/assets/logo.png","sha256":crate::compat::hash::sha256(&logo),"bytes":logo.len()}]});
            let payload = serde_json::to_vec(&payload).map_err(|_| failure())?;
            let (signature, wrong) = signer.signatures(&payload)?;
            for (suffix, bytes) in [("json", payload), ("sig", signature), ("wrong.sig", wrong)] {
                private_files::write_new(
                    &root.join(format!(
                        "tests/fixtures/native-signing/{variant}/{target}.{suffix}"
                    )),
                    &bytes,
                )?;
            }
        }
    }
    Ok(())
}
