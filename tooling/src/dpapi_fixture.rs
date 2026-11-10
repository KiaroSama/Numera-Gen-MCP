use crate::context::Context;
#[cfg(windows)]
use crate::context::fail;
use std::io;

/// Hosted synthetic input only; no caller-supplied secret or private credential lookup.
pub fn create(context: &Context) -> io::Result<()> {
    context.hosted()?;
    #[cfg(not(windows))]
    {
        Err(io::Error::other(
            "The synthetic CurrentUser DPAPI fixture requires Windows.",
        ))
    }
    #[cfg(windows)]
    {
        use numera_gen_mcp::devtools::{
            private_files::{self, Secret},
            protect_secret,
        };
        let directory = context.relative(".ci-work/parity")?;
        if !directory.is_dir() {
            return Err(fail("Materialized parity input directory is required."));
        }
        let file = directory.join("current-user-dpapi.txt");
        let config = directory.join("config-dpapi.json");
        for path in [&file, &config] {
            match std::fs::symlink_metadata(path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
                Ok(_) => {
                    return Err(fail(
                        "Synthetic DPAPI fixture already exists; no overwrite is performed.",
                    ));
                }
            }
        }
        protect_secret::protect_with(&file, false, || {
            Ok(Secret(b"synthetic-same-user-credential".to_vec()))
        })?;
        let record = serde_json::json!({"file":file.canonicalize()?.to_str().ok_or_else(|| fail("Synthetic fixture path must be Unicode."))?,
            "expected":{"Authorization":"Bearer synthetic-same-user-credential"}});
        let bytes = serde_json::to_vec(&record)
            .map_err(|_| fail("Cannot encode synthetic DPAPI fixture."))?;
        private_files::write_new(&config, &bytes)?;
        Ok(())
    }
}
