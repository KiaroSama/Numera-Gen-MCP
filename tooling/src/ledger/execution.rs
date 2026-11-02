use super::mapping::SDK;
use super::{PLATFORMS, array, hex, parse, string, validate_rows};
use crate::context::{fail, sha};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
};

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        other => other.clone(),
    }
}
pub fn assertion_digest(value: &Value) -> io::Result<String> {
    // Historical binders used Python json.dumps(sort_keys=True,separators=(',',':')) with
    // ensure_ascii=True. Preserve its exact Unicode escaping rather than hashing serde's UTF-8 form.
    let text = serde_json::to_string(&canonical(value))
        .map_err(|_| fail("Cannot encode assertion identity."))?;
    let mut ascii = String::new();
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii.push(ch);
        } else {
            let mut units = [0u16; 2];
            for unit in ch.encode_utf16(&mut units) {
                ascii.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    Ok(sha(ascii.as_bytes()))
}

pub fn validate_execution_binding(
    rows: &[Value],
    bytes: &[u8],
    expected_digest: &str,
) -> io::Result<Value> {
    execution_binding(rows, bytes, expected_digest, false)
}
pub fn validate_reference_binding(rows: &[Value], bytes: &[u8], digest: &str) -> io::Result<Value> {
    execution_binding(rows, bytes, digest, true)
}
pub(super) fn execution_binding(
    rows: &[Value],
    bytes: &[u8],
    expected_digest: &str,
    reference: bool,
) -> io::Result<Value> {
    validate_rows(rows)?;
    if !hex(expected_digest, 64) || sha(bytes) != expected_digest {
        return Err(fail("Execution binding digest differs."));
    }
    let binding = parse(bytes)?;
    if binding["baseline"] != crate::fixtures::BASELINE
        || !hex(string(&binding, "source_sha")?, 40)
        || binding["run_id"].as_u64().is_none_or(|n| n == 0)
        || binding["run_attempt"].as_u64().is_none_or(|n| n == 0)
        || binding["native_execution_locally"] != false
        || binding["canonical_promotions"] != 0
    {
        return Err(fail("Execution binding source/run identity differs."));
    }
    let cases = array(&binding, "cases")?;
    let mut indexed = BTreeMap::new();
    for case in cases {
        let id = string(case, "id")?;
        if !hex(id, 64) || indexed.insert(id, case).is_some() {
            return Err(fail("Execution binding duplicates or invalidates a case."));
        }
    }
    if indexed.len() != 470
        || rows
            .iter()
            .any(|row| !indexed.contains_key(row["id"].as_str().unwrap_or("")))
    {
        return Err(fail("Execution binding must cover all470cases."));
    }
    let targets = binding["targets"]
        .as_object()
        .ok_or_else(|| fail("Execution target evidence missing."))?;
    if targets.len() != 7 || !targets.contains_key("coverage") {
        return Err(fail(
            "Allsixnative targets and coverage execution are required.",
        ));
    }
    let actual_targets = targets
        .iter()
        .map(|(label, target)| {
            (
                target["platform"].as_str().unwrap_or(""),
                target["target"].as_str().unwrap_or(""),
                label == "coverage",
            )
        })
        .collect::<BTreeSet<_>>();
    let expected_targets = BTreeSet::from([
        ("win32", "x86_64-pc-windows-msvc", false),
        ("win32", "aarch64-pc-windows-msvc", false),
        ("linux", "x86_64-unknown-linux-gnu", false),
        ("linux", "aarch64-unknown-linux-gnu", false),
        ("darwin", "x86_64-apple-darwin", false),
        ("darwin", "aarch64-apple-darwin", false),
        ("linux", "coverage", true),
    ]);
    if actual_targets != expected_targets {
        return Err(fail("Execution target matrix is incomplete or duplicated."));
    }
    let mut bound = 0;
    for row in rows {
        let Some(case) = indexed.get(string(row, "id")?) else {
            return Err(fail("Execution binding omits mapped source case."));
        };
        let selected = reference || row["evidence"]["execution_binding_sha256"] == expected_digest;
        if !selected {
            continue;
        }
        for key in ["source", "source_sha256", "platform_modes", "native"] {
            if case[key] != row[key] {
                return Err(fail("Execution binding source or mapping differs."));
            }
        }
        if case["assertions_sha256"] != assertion_digest(&row["assertions"])? {
            return Err(fail("Execution assertion source ranges changed."));
        }
        if !reference
            && (row["evidence"]["sha"] != binding["source_sha"]
                || row["evidence"]["run_id"] != binding["run_id"]
                || row["evidence"]["run_attempt"] != binding["run_attempt"])
        {
            return Err(fail(
                "Case evidence does not bind its execution revision/run.",
            ));
        }
        let source = string(row, "source")?;
        if binding["frozen_git_files"][source] != row["source_sha256"] {
            return Err(fail("Executed frozen source bytes differ."));
        }
        for value in array(row, "native")? {
            let value = value
                .as_str()
                .ok_or_else(|| fail("Invalid executed mapping."))?;
            let (file, symbol) = value
                .split_once("::")
                .ok_or_else(|| fail("Executed mapping requires symbol."))?;
            let hash = string(&case["native_sha256"], file)?;
            if !hex(hash, 64)
                || binding["current_git_files"][file] != hash
                || (!reference && row["evidence"]["native_sha256"][file] != hash)
            {
                return Err(fail("Executed native source hash differs."));
            }
            for platform in PLATFORMS {
                if row["platform_modes"][platform] != "run" {
                    continue;
                }
                let mut architectures = BTreeSet::new();
                for (label, target) in targets {
                    if target["platform"] != platform {
                        continue;
                    }
                    let triple = string(target, "target")?;
                    let suffix = match platform {
                        "win32" => "pc-windows-msvc",
                        "darwin" => "apple-darwin",
                        _ => "unknown-linux-gnu",
                    };
                    if label == "coverage" {
                        if platform != "linux" || triple != "coverage" {
                            return Err(fail("Coverage execution target differs."));
                        }
                    } else if triple == format!("x86_64-{suffix}") {
                        architectures.insert("x86_64");
                    } else if triple == format!("aarch64-{suffix}") {
                        architectures.insert("aarch64");
                    } else {
                        return Err(fail("Execution platform and architecture differ."));
                    }
                    if target["baseline_execution_files"][source] != row["source_sha256"] {
                        return Err(fail("Target execution source provenance differs."));
                    }
                    for key in [
                        "download_sha256",
                        "payload_sha256",
                        "provenance_sha256",
                        "log_sha256",
                    ] {
                        if !hex(string(target, key)?, 64) {
                            return Err(fail("Target execution receipt hash missing."));
                        }
                    }
                    if target["artifact_id"].as_u64().is_none_or(|n| n == 0) {
                        return Err(fail("Target execution artifact identity missing."));
                    }
                    let named = &case["named_execution"][label][value];
                    if file.ends_with(".rs") {
                        let harness = if file.starts_with("native/") {
                            "native/lib.rs"
                        } else {
                            file
                        };
                        let actual = string(named, "exact_test_name")?;
                        if named["harness"] != harness
                            || !(actual == symbol || actual.ends_with(&format!("::{symbol}")))
                        {
                            return Err(fail("Exact named native test identity differs."));
                        }
                        let evidence = &target["native_harnesses"][harness];
                        let passed = array(evidence, "passed")?;
                        let matches = passed
                            .iter()
                            .filter_map(Value::as_str)
                            .filter(|name| {
                                *name == symbol || name.ends_with(&format!("::{symbol}"))
                            })
                            .collect::<Vec<_>>();
                        if evidence["started"] != true
                            || matches.len() != 1
                            || matches[0] != actual
                            || !string(evidence, "summary")?.starts_with("ok.")
                        {
                            return Err(fail(
                                "Named native test did not execute successfully or is ambiguous.",
                            ));
                        }
                    } else {
                        if !SDK
                            .iter()
                            .any(|(path, name)| *path == file && *name == symbol)
                        {
                            return Err(fail("Unregistered historical SDK execution mapping."));
                        }
                        let component = format!(
                            "native-{}-acceptance",
                            Path::new(file)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("")
                                .trim_start_matches("native-")
                                .trim_end_matches("-acceptance.mjs")
                        );
                        if named["component"] != component
                            || (label != "coverage" && named["built_and_extracted"] != true)
                            || target["sdk_events"][&component]
                                .as_object()
                                .is_none_or(|v| {
                                    v.is_empty()
                                        || v.values().any(|n| {
                                            n.as_u64().unwrap_or(0)
                                                < if label == "coverage" { 1 } else { 2 }
                                        })
                                })
                        {
                            return Err(fail("Historical SDK built/extracted execution missing."));
                        }
                    }
                }
                if architectures != BTreeSet::from(["x86_64", "aarch64"]) {
                    return Err(fail("Applicable native architecture execution missing."));
                }
            }
        }
        bound += 1;
    }
    if bound == 0 {
        return Err(fail("Execution receipt is not referenced by any case."));
    }
    Ok(
        json!({"cases":470,"bound_cases":bound,"execution_binding_verified":true,"historical_source_sha":binding["source_sha"],"current_execution_claimed":false}),
    )
}
