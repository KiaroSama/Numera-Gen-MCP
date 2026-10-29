use crate::compat::value::{Value, json};
use crate::{
    error::{NumeraError, Result},
    files::{
        images::{Image, inspect},
        paths::{output_directory, safe_relative},
    },
    jobs::{Receipt, Store},
};
use std::{fs, io::Read, path::Path};
use uuid::Uuid;

struct Temporary(std::path::PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub struct Publication<'a> {
    pub id: &'a str,
    pub index: u32,
    pub subdir: Option<&'a str>,
    pub prefix: Option<&'a str>,
    pub deviations: &'a [String],
    pub expected: Option<&'a Receipt>,
    pub stable: bool,
}

pub fn save(
    image: &Image,
    config: &Value,
    store: &mut Store,
    publication: Publication<'_>,
) -> Result<Value> {
    publish(&image.bytes, &image.sha256, &image.extension,
        json!({"mime_type":image.mime,"width":image.width,"height":image.height,"has_alpha":image.alpha}),
        config, store, publication)
}

pub fn save_media(
    kind: crate::media::Kind, role: &str, bytes: &[u8], config: &Value,
    store: &mut Store, publication: Publication<'_>,
) -> Result<Value> {
    let verified = crate::media::inspect(kind, bytes,
        config["files"]["maxOutputBytes"].as_u64().unwrap_or(50 * 1024 * 1024) as usize,
        config["files"]["maxPixels"].as_u64().unwrap_or(64000000))?;
    let hash = crate::compat::hash::sha256(bytes);
    let metadata = Value::from_serde(&serde_json::json!({"kind":kind.as_str(),"role":role,
        "mime_type":verified.mime,"format":verified.format,"media":verified.metadata}))?;
    publish(bytes, &hash, &verified.format, metadata, config, store, publication)
}

fn publish(
    bytes: &[u8], hash: &str, extension: &str, metadata: Value,
    config: &Value, store: &mut Store, publication: Publication<'_>,
) -> Result<Value> {
    let Publication { id, index, subdir, prefix, deviations, expected, stable } = publication;
    let error = || {
        NumeraError::new("output_file_error","Cannot save verified original; retained results can be recovered with get_job. Do not resubmit automatically.").stage("storage")
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
    let filename = |output_id: &str| -> Result<String> {
        Ok(directory
            .join(format!(
                "{}{}-{}-{}.{}",
                prefix.map(|s| format!("{s}-")).unwrap_or_default(),
                id,
                index,
                output_id,
                extension
            ))
            .to_string_lossy()
            .into_owned())
    };
    let item = if stable {
        Some(store.publication(id, index, filename, expected)?)
    } else {
        None
    };
    let output_id = item
        .as_ref()
        .map(|item| item.output_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let target = std::path::PathBuf::from(match &item {
        Some(item) => item.target.as_ref().ok_or_else(error)?.clone(),
        None => filename(&output_id)?,
    });
    let parent = fs::canonicalize(target.parent().ok_or_else(error)?).map_err(|_| error())?;
    if parent != directory || !parent.starts_with(fs::canonicalize(root).map_err(|_| error())?) {
        return Err(NumeraError::new(
            "permission_denied",
            "Retained output target does not match the configured output directory.",
        ));
    }
    if item
        .as_ref()
        .is_some_and(|item| item.sha256.as_deref() != Some(hash))
    {
        return Err(NumeraError::new(
            "output_file_error",
            "Retained output hash does not match image bytes.",
        )
        .stage("storage"));
    }
    if item.as_ref().is_some_and(|item| item.phase == "committed") {
        return store.output(&output_id);
    }
    let temporary = Temporary(directory.join(format!(".{}-{}.tmp", output_id, Uuid::new_v4())));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary.0).map_err(|_| error())?;
    store
        .persistence
        .write(&mut file, bytes)
        .and_then(|_| store.persistence.sync(&file))
        .map_err(|_| error())?;
    drop(file);
    if stable && !store.owns(id, expected)? {
        return Err(NumeraError::new(
            "outcome_unknown",
            "Output writer lease changed before publication.",
        )
        .stage("storage"));
    }
    match store.persistence.link(&temporary.0, &target) {
        Ok(()) => {}
        Err(e) if stable && e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(error()),
    }
    store
        .persistence
        .unlink(&temporary.0)
        .map_err(|_| error())?;
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
    let saved = read_bytes(&target, config).map_err(|_| error())?;
    if crate::compat::hash::sha256(&saved) != hash {
        return Err(
            NumeraError::new("output_file_error", "Saved image hash mismatch.").stage("storage"),
        );
    }
    if metadata.get("kind").is_none() {
        let verified = inspect(saved.clone(), config, None).map_err(|_| error())?;
        if metadata["width"] != verified.width || metadata["height"] != verified.height
            || metadata["mime_type"] != verified.mime.as_str() || metadata["has_alpha"] != verified.alpha {
            return Err(error());
        }
    }
    let mut output = json!({"output_id":output_id,"request_id":id,"path":target.to_str().ok_or_else(error)?});
    for (key, value) in metadata.as_object().ok_or_else(error)? { output[key] = value.clone(); }
    output["bytes"] = json!(saved.len()); output["sha256"] = json!(hash);
    if stable {
        store
            .commit(id, index, &output, deviations, expected)
            .map_err(|_| error())?;
    } else {
        store.add_output(&output).map_err(|_| error())?;
    }
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
    inspect(read_bytes(path, config)?, config, None)
}

fn read_bytes(path: &Path, config: &Value) -> Result<Vec<u8>> {
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
    if bytes.len() as u64 > limit { return Err(NumeraError::new("output_file_error", "Owned output exceeds byte bound.")); }
    Ok(bytes)
}

pub fn owned_media(config: &Value, store: &Store, id: &str) -> Result<(Value, Vec<u8>)> {
    let output = store.output(id)?;
    let root = fs::canonicalize(config["outputDir"].as_str().ok_or_else(NumeraError::storage)?)
        .map_err(|_| NumeraError::storage())?;
    let raw = Path::new(output["path"].as_str().ok_or_else(NumeraError::storage)?);
    let path = fs::canonicalize(raw).map_err(|_| NumeraError::new("output_file_error", "Owned output file is missing."))?;
    if !path.starts_with(&root) || !same_physical_path(raw, &path) {
        return Err(NumeraError::new("permission_denied", "Owned output path changed or escapes its root."));
    }
    let bytes = read_bytes(&path, config)?;
    if output["sha256"].as_str() != Some(crate::compat::hash::sha256(&bytes).as_str()) {
        return Err(NumeraError::new("output_file_error", "Owned output bytes changed after storage."));
    }
    let kind = match output.get("kind") {
        None => crate::media::Kind::Image,
        Some(value) => serde_json::from_str(&value.to_string()).map_err(|_| NumeraError::storage())?,
    };
    let inspected = crate::media::inspect(kind, &bytes,
        config["files"]["maxOutputBytes"].as_u64().unwrap_or(50 * 1024 * 1024) as usize,
        config["files"]["maxPixels"].as_u64().unwrap_or(64000000))?;
    if output["mime_type"].as_str() != Some(inspected.mime.as_str())
        || output["bytes"].as_u64() != Some(bytes.len() as u64) {
        return Err(NumeraError::new("output_file_error", "Owned output metadata differs from its original."));
    }
    if let Some(format) = output.get("format")
        && format.as_str() != Some(inspected.format.as_str()) {
        return Err(NumeraError::new("output_file_error", "Owned media format changed."));
    }
    if let Some(metadata) = output.get("media")
        && metadata.to_serde()? != inspected.metadata {
        return Err(NumeraError::new("output_file_error", "Owned media metadata changed."));
    }
    Ok((output, bytes))
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
