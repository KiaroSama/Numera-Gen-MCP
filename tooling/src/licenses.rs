use crate::{
    context::{Context, fail, sha},
    distribution::read_regular,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

fn candidates(directory: &Path, root: &Path, found: &mut BTreeSet<PathBuf>) -> io::Result<()> {
    let mut pending = vec![(directory.to_path_buf(), 0usize)];
    let mut scanned = 0usize;
    while let Some((directory, depth)) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            scanned += 1;
            if scanned > 16384 {
                return Err(fail("Dependency notice inventory exceeds bound."));
            }
            let info = entry.file_type()?;
            if info.is_symlink() {
                return Err(fail("Linked registry notice rejected."));
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if entry.metadata()?.file_attributes() & 0x400 != 0 {
                    return Err(fail("Linked registry notice rejected."));
                }
            }
            let path = entry.path().canonicalize()?;
            if !path.starts_with(root) {
                return Err(fail("Escaping dependency notice."));
            }
            if info.is_dir() {
                if depth >= 32 {
                    return Err(fail("Dependency notice nesting exceeds bound."));
                }
                pending.push((path, depth + 1));
            } else if info.is_file() {
                let name = entry.file_name().to_string_lossy().to_uppercase();
                if [
                    "LICENSE",
                    "LICENCE",
                    "COPYING",
                    "COPYRIGHT",
                    "NOTICE",
                    "UNLICENSE",
                ]
                .iter()
                .any(|prefix| name.starts_with(prefix))
                {
                    found.insert(path);
                }
            } else {
                return Err(fail("Nonregular registry member rejected."));
            }
        }
    }
    Ok(())
}

pub fn preserve_package(
    context: &Context,
    package: &Value,
    output: &Path,
    supplements: &Value,
) -> io::Result<Value> {
    let name = package["name"]
        .as_str()
        .ok_or_else(|| fail("Missing package name."))?;
    let version = package["version"]
        .as_str()
        .ok_or_else(|| fail("Missing package version."))?;
    let identity = format!("{name}@{version}");
    if !identity
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_.+@-".contains(&b))
    {
        return Err(fail("Invalid package identity."));
    }
    let directory = Path::new(
        package["manifest_path"]
            .as_str()
            .ok_or_else(|| fail("Missing package manifest."))?,
    )
    .parent()
    .ok_or_else(|| fail("Invalid package manifest."))?
    .canonicalize()?;
    if !directory.starts_with(context.root.join(".ci-cache/cargo").canonicalize()?) {
        return Err(fail("Dependency registry path escapes workspace."));
    }
    let license = &package["license"];
    let license_file = &package["license_file"];
    if !matches!(license, Value::Null | Value::String(_))
        || !matches!(license_file, Value::Null | Value::String(_))
        || license_file.as_str().is_some_and(|value| value.trim().is_empty())
        || ![license, license_file].iter().any(|value| {
            value.as_str().is_some_and(|value| !value.trim().is_empty())
        })
    {
        return Err(fail("Dependency license declaration missing or invalid."));
    }
    let mut files = BTreeSet::new();
    candidates(&directory, &directory, &mut files)?;
    if let Some(name) = package["license_file"].as_str() {
        let path = directory.join(name);
        if fs::symlink_metadata(&path)?.is_symlink()
            || !path.canonicalize()?.starts_with(&directory)
        {
            return Err(fail("Declared notice escapes package."));
        }
        files.insert(path.canonicalize()?);
    }
    let target = output.join(format!("{name}-{version}"));
    let mut names = Vec::new();
    let mut hashes = BTreeMap::new();
    let mut supplement_provenance = Value::Null;
    let mut retained = BTreeMap::new();
    if files.is_empty() {
        let supplement = &supplements[&identity];
        if supplement.is_null() {
            return Err(fail("Resolved dependency notice missing."));
        }
        let registry_root = context
            .relative(".ci-cache/cargo/registry/src")?
            .canonicalize()?;
        let parts = directory
            .strip_prefix(&registry_root)
            .map_err(|_| fail("Supplement registry source layout differs."))?
            .components()
            .collect::<Vec<_>>();
        if parts.len() != 2
            || parts[1].as_os_str() != std::ffi::OsStr::new(&format!("{name}-{version}"))
        {
            return Err(fail("Supplement registry source layout differs."));
        }
        let registry = parts[0]
            .as_os_str()
            .to_str()
            .ok_or_else(|| fail("Registry identity must be Unicode."))?;
        let archive = context.relative(&format!(
            ".ci-cache/cargo/registry/cache/{registry}/{name}-{version}.crate"
        ))?;
        let archive = read_regular(&archive, 64 * 1024 * 1024)?;
        let vcs = crate::ledger::parse(&read_regular(
            &directory.join(".cargo_vcs_info.json"),
            1024 * 1024,
        )?)?;
        if archive.is_empty()
            || supplement["crate_sha256"] != sha(&archive)
            || supplement["source_revision"].as_str().is_none_or(|revision| {
                revision.len() != 40
                    || !revision
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            || vcs["git"]["sha1"] != supplement["source_revision"]
            || package["license"] != supplement["license"]
        {
            return Err(fail("Supplement source identity differs."));
        }
        let name = supplement["file"]
            .as_str()
            .ok_or_else(|| fail("Supplement filename missing."))?;
        let path = context.relative(name)?;
        if !path
            .canonicalize()?
            .starts_with(context.root.join("docs/licenses").canonicalize()?)
        {
            return Err(fail("Supplement path rejected."));
        }
        let data = read_regular(&path, 1024 * 1024)?;
        if data.is_empty() || supplement["sha256"] != sha(&data) {
            return Err(fail("Supplement hash differs."));
        }
        retained.insert("LICENSE".to_owned(), data);
        supplement_provenance = json!({"crate_sha256":supplement["crate_sha256"],"source_revision":supplement["source_revision"],"source_url":supplement["source_url"]});
    } else {
        let mut total = 0usize;
        for path in files {
            let relative = path
                .strip_prefix(&directory)
                .map_err(|_| fail("Notice path escapes package."))?;
            let data = read_regular(&path, 1024 * 1024)?;
            total = total
                .checked_add(data.len())
                .filter(|total| *total <= 16 * 1024 * 1024)
                .ok_or_else(|| fail("Dependency notice aggregate exceeds bound."))?;
            let name = relative.to_string_lossy().replace('\\', "/");
            retained.insert(name, data);
        }
    }
    for (name, data) in retained {
        let destination = target.join(&name);
        fs::create_dir_all(
            destination
                .parent()
                .ok_or_else(|| fail("Invalid notice path."))?,
        )?;
        fs::write(destination, &data)?;
        names.push(name.clone());
        hashes.insert(name, sha(&data));
    }
    let mut report = json!({"name":name,"version":version,"license":package["license"],"repository":package["repository"],"notices":names,"notice_sha256":hashes});
    if !supplement_provenance.is_null() {
        report["supplement"] = supplement_provenance;
    }
    Ok(report)
}

pub fn preserve(context: &Context, target: &str) -> io::Result<()> {
    context.hosted()?;
    crate::distribution::target(target)?;
    let bytes = context.capture(
        context.command("cargo").args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            target,
        ]),
        120000,
        45000,
        16 * 1024 * 1024,
    )?;
    let metadata: Value =
        serde_json::from_slice(&bytes).map_err(|_| fail("Invalid Cargo metadata."))?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or_else(|| fail("Missing Cargo packages."))?;
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .ok_or_else(|| fail("Missing Cargo dependency graph."))?;
    let root = packages
        .iter()
        .find(|p| p["name"] == "numera-gen-mcp")
        .and_then(|p| p["id"].as_str())
        .ok_or_else(|| fail("Runtime root missing."))?;
    let mut pending = vec![root.to_owned()];
    let mut selected = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !selected.insert(id.clone()) {
            continue;
        }
        let node = nodes
            .iter()
            .find(|n| n["id"] == id)
            .ok_or_else(|| fail("Dependency node missing."))?;
        for dependency in node["deps"]
            .as_array()
            .ok_or_else(|| fail("Dependency edges missing."))?
        {
            if dependency["dep_kinds"]
                .as_array()
                .ok_or_else(|| fail("Dependency kinds missing."))?
                .iter()
                .any(|k| k["kind"].is_null() || k["kind"] == "build")
            {
                pending.push(
                    dependency["pkg"]
                        .as_str()
                        .ok_or_else(|| fail("Invalid dependency identity."))?
                        .to_owned(),
                );
            }
        }
    }
    let output = context.relative(".ci-work/native/licenses")?;
    fs::create_dir_all(&output)?;
    let supplements: Value = serde_json::from_slice(&fs::read(
        context.relative("docs/licenses/supplements.json")?,
    )?)
    .map_err(|_| fail("Invalid notice supplement index."))?;
    let mut reports = Vec::new();
    let mut failures = Vec::new();
    for id in selected.iter().filter(|id| id.as_str() != root) {
        let package = packages
            .iter()
            .find(|p| p["id"] == *id)
            .ok_or_else(|| fail("Resolved package missing."))?;
        match preserve_package(context, package, &output, &supplements) {
            Ok(report) => reports.push(report),
            Err(_) => failures
                .push(json!({"package":package["name"],"code":"notice_verification_failed"})),
        }
    }
    if !failures.is_empty() {
        fs::write(
            output.join("failures.json"),
            serde_json::to_vec_pretty(&failures)
                .map_err(|_| fail("Cannot encode notice failure."))?,
        )?;
        return Err(fail("Resolved dependency notices remain unverified."));
    }
    fs::write(
        output.join("inventory.json"),
        serde_json::to_vec_pretty(&reports).map_err(|_| fail("Cannot encode notice inventory."))?,
    )?;
    println!(
        "{}",
        json!({"component":"native-licenses","resolved_packages":reports.len()})
    );
    Ok(())
}
