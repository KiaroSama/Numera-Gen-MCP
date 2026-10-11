use crate::{
    error::{NumeraError, Result},
    files::{
        images::{Image, inspect},
        paths::{output_directory, safe_relative},
    },
    jobs::{Receipt, Store},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use uuid::Uuid;

struct Temporary(std::path::PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub fn save(
    image: &Image,
    config: &Value,
    store: &mut Store,
    id: &str,
    index: u32,
    subdir: Option<&str>,
    prefix: Option<&str>,
    deviations: &[String],
    expected: Option<&Receipt>,
) -> Result<Value> {
    let error = || {
        NumeraError::new("output_file_error","Cannot save verified image; retained results can be recovered with get_job. Do not resubmit automatically.").stage("storage")
    };
    let root = Path::new(config["outputDir"].as_str().ok_or_else(error)?);
    let directory = output_directory(root, subdir)?;
    if let Some(prefix) = prefix {
        safe_relative(prefix)?;
        if prefix.contains(['/', '\\']) {
            return Err(NumeraError::new(
                "invalid_input",
                "Filename prefix cannot contain directories.",
            ));
        }
    }
    let item = store.publication(
        id,
        index,
        |output_id| {
            Ok(directory
                .join(format!(
                    "{}{}-{}-{}.{}",
                    prefix.map(|s| format!("{s}-")).unwrap_or_default(),
                    id,
                    index,
                    output_id,
                    image.extension
                ))
                .to_string_lossy()
                .into_owned())
        },
        expected,
    )?;
    let target = std::path::PathBuf::from(item.target.as_ref().ok_or_else(error)?);
    let parent = fs::canonicalize(target.parent().ok_or_else(error)?).map_err(|_| error())?;
    if parent != directory || !parent.starts_with(fs::canonicalize(root).map_err(|_| error())?) {
        return Err(NumeraError::new(
            "permission_denied",
            "Retained output target does not match the configured output directory.",
        ));
    }
    if item.sha256.as_deref() != Some(image.sha256.as_str()) {
        return Err(NumeraError::new(
            "output_file_error",
            "Retained output hash does not match image bytes.",
        )
        .stage("storage"));
    }
    if item.phase == "committed" {
        return store.output(&item.output_id);
    }
    let temporary =
        Temporary(directory.join(format!(".{}-{}.tmp", item.output_id, Uuid::new_v4())));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary.0).map_err(|_| error())?;
    file.write_all(&image.bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| error())?;
    drop(file);
    if !store.owns(id, expected)? {
        return Err(NumeraError::new(
            "outcome_unknown",
            "Output writer lease changed before publication.",
        )
        .stage("storage"));
    }
    match fs::hard_link(&temporary.0, &target) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(error()),
    }
    fs::remove_file(&temporary.0).map_err(|_| error())?;
    #[cfg(unix)]
    fs::File::open(&directory)
        .and_then(|f| f.sync_all())
        .map_err(|_| error())?;
    let resolved = fs::canonicalize(&target).map_err(|_| error())?;
    if !same_physical_path(&resolved, &target) || !resolved.starts_with(&parent) {
        return Err(NumeraError::new(
            "permission_denied",
            "Published image path changed during storage.",
        ));
    }
    let saved = read_image(&target, config)?;
    if saved.sha256 != image.sha256 {
        return Err(
            NumeraError::new("output_file_error", "Saved image hash mismatch.").stage("storage"),
        );
    }
    let output = json!({"output_id":item.output_id,"request_id":id,"path":target.to_string_lossy(),
        "mime_type":saved.mime,"width":saved.width,"height":saved.height,"has_alpha":saved.alpha,"bytes":saved.bytes.len(),"sha256":saved.sha256});
    store.commit(id, index, &output, deviations, expected)?;
    Ok(output)
}

fn same_physical_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        let normalize = |path: &Path| {
            let text = path.to_string_lossy();
            if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
                format!("\\\\{unc}")
            } else {
                text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned()
            }
        };
        normalize(left) == normalize(right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn read_image(path: &Path, config: &Value) -> Result<Image> {
    let limit = config["files"]["maxOutputBytes"]
        .as_u64()
        .unwrap_or(50 * 1024 * 1024);
    let mut file = fs::File::open(path)
        .map_err(|_| NumeraError::new("output_file_error", "Owned output file is missing."))?;
    let metadata = file.metadata().map_err(|_| NumeraError::storage())?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(NumeraError::new(
            "output_file_error",
            "Owned output exceeds bounded regular-file limits.",
        ));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| NumeraError::storage())?;
    inspect(bytes, config, None)
}

pub fn owned(config: &Value, store: &Store, id: &str) -> Result<(Value, Image)> {
    let output = store.output(id)?;
    let root = fs::canonicalize(
        config["outputDir"]
            .as_str()
            .ok_or_else(NumeraError::storage)?,
    )
    .map_err(|_| NumeraError::storage())?;
    let path = fs::canonicalize(output["path"].as_str().ok_or_else(NumeraError::storage)?)
        .map_err(|_| NumeraError::new("output_file_error", "Owned output file is missing."))?;
    if !path.starts_with(root) {
        return Err(NumeraError::new(
            "permission_denied",
            "Owned output path escapes its root.",
        ));
    }
    let image = read_image(&path, config)?;
    if Some(image.sha256.as_str()) != output["sha256"].as_str() {
        return Err(NumeraError::new(
            "output_file_error",
            "Owned output bytes changed after storage.",
        ));
    }
    Ok((output, image))
}
