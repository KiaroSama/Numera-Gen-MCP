use crate::{
    context::{fail, sha},
    fixtures::{BASELINE, SOURCE},
    ledger,
    ledger_inputs::REFERENCE_SHA256,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

pub const REVIEW_FILE: &str = "docs/testing/current-assertion-review.json";

fn text<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    value[key]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| fail("Current assertion review field is missing."))
}
fn array<'a>(value: &'a Value, key: &str) -> io::Result<&'a Vec<Value>> {
    value[key]
        .as_array()
        .ok_or_else(|| fail("Current assertion review inventory is missing."))
}
fn source(path: &str) -> bool {
    path.len() <= 1024
        && path.ends_with(".rs")
        && ["native/", "tests/rust/", "tooling/"]
            .iter()
            .any(|prefix| path.starts_with(prefix))
        && !path
            .bytes()
            .any(|byte| byte == b'\\' || byte == b':' || byte.is_ascii_control())
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

/// The caller authenticates the historical rebind and execution provenance independently.
/// This validates the recorded primary source review, never test execution or signatures.
pub fn verify(
    rebind: &Value,
    review: &Value,
    files: &BTreeMap<String, Vec<u8>>,
) -> io::Result<Value> {
    for value in [rebind, review] {
        if value["schema_version"] != 1
            || value["baseline"] != BASELINE
            || value["historical_source_sha"] != SOURCE
            || value["historical_reference_sha256"] != REFERENCE_SHA256
        {
            return Err(fail("Current assertion review historical identity differs."));
        }
    }
    if rebind["component"] != "native-case-source-rebind"
        || rebind["original_evidence_rewritten"] != false
        || review["component"] != "native-current-assertion-review"
        || review["reviewer"] != "primary"
        || review["scope"] != "source_assertions_only"
        || review["current_execution_claimed"] != false
        || review["qualification_verified"] != false
    {
        return Err(fail(
            "Source review cannot inherit execution or independent approval.",
        ));
    }
    let revision = text(rebind, "current_source_sha")?;
    if revision.len() != 40
        || review
            .get("current_source_sha")
            .is_some_and(|value| value != revision)
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(fail(
            "Current source review requires an exact source revision.",
        ));
    }
    let rows = array(rebind, "cases")?
        .iter()
        .map(|case| case["historical"].clone())
        .collect::<Vec<_>>();
    ledger::validate_rows(&rows)?;
    if rows.iter().any(|row| {
        row["assertions"].as_array().is_none_or(Vec::is_empty)
            || row["native"].as_array().is_none_or(Vec::is_empty)
    }) {
        return Err(fail(
            "Every original case requires mapped assertion obligations.",
        ));
    }
    let aggregate = rows
        .iter()
        .map(|row| json!({"id":row["id"],"assertions":row["assertions"]}))
        .collect::<Vec<_>>();
    if review["historical_assertions_sha256"] != ledger::assertion_digest(&json!(aggregate))? {
        return Err(fail(
            "Original assertion aggregate differs from the reviewed obligations.",
        ));
    }
    let expected_files = rebind["current_source_files"]
        .as_object()
        .filter(|files| !files.is_empty() && files.len() <= 4096)
        .ok_or_else(|| fail("Current source review dependencies are missing."))?;
    if review["current_source_files"] != rebind["current_source_files"]
        || files.keys().map(String::as_str).collect::<BTreeSet<_>>()
            != expected_files.keys().map(String::as_str).collect()
        || files.iter().any(|(path, bytes)| {
            (!source(path) && path != "docs/testing/sdk-tooling-migration.json")
                || bytes.len() > 16 * 1024 * 1024
                || expected_files[path] != sha(bytes)
        })
    {
        return Err(fail(
            "Reviewed assertion or helper source bytes are stale or incomplete.",
        ));
    }
    let mut obligations = BTreeMap::new();
    let mut records = 0usize;
    for row in &rows {
        for assertion in array(row, "assertions")? {
            let digest = ledger::assertion_digest(&json!([assertion]))?;
            if let Some(previous) = obligations.insert(digest, assertion)
                && previous != assertion
            {
                return Err(fail("Conflicting original assertion identity."));
            }
            records += 1;
        }
    }
    let mut assertions = array(review, "assertions")?.clone();
    if let Some(groups) = review.get("file_reviews") {
        let groups = groups
            .as_array()
            .ok_or_else(|| fail("File reviews must be an array."))?;
        let mut seen_files = BTreeSet::new();
        for group in groups {
            let file = text(group, "historical_native_file")?;
            let originals = obligations
                .iter()
                .filter(|(_, original)| original["native_file"] == file)
                .collect::<Vec<_>>();
            if !source(file)
                || !seen_files.insert(file)
                || originals.is_empty()
                || group["historical_assertions_sha256"]
                    != ledger::assertion_digest(&json!(
                        originals
                            .iter()
                            .map(|(_, original)| *original)
                            .collect::<Vec<_>>()
                    ))?
            {
                return Err(fail(
                    "File review changes or duplicates its original obligations.",
                ));
            }
            let bytes = files
                .get(file)
                .ok_or_else(|| fail("Reviewed file is missing."))?;
            let lines = std::str::from_utf8(bytes)
                .map_err(|_| fail("Reviewed file must be UTF-8."))?
                .lines()
                .count();
            let ranges = array(group, "ranges")?;
            if ranges.len() != 1
                || ranges[0]["file"] != file
                || ranges[0]["lines"] != json!([1, lines])
            {
                return Err(fail(
                    "A file review must cover the complete current file.",
                ));
            }
            for (digest, _) in originals {
                let mut assertion = group.clone();
                assertion["historical_assertion_sha256"] = json!(digest);
                assertions.push(assertion);
            }
        }
    }
    if obligations.is_empty() || assertions.len() != obligations.len() {
        return Err(fail(
            "Every original assertion requires exactly one current source review.",
        ));
    }
    let sdk = array(&rebind["sdk_assertion_traceability"], "bindings")?;
    let mut seen = BTreeSet::new();
    for assertion in &assertions {
        let digest = text(assertion, "historical_assertion_sha256")?;
        let original = obligations
            .get(digest)
            .ok_or_else(|| fail("Source review introduces an unknown assertion."))?;
        let original_file = text(original, "native_file")?;
        let translated = original_file.ends_with(".mjs");
        if !translated && !source(original_file) {
            return Err(fail(
                "Original assertion has no supported Rust or SDK source.",
            ));
        }
        let basis = text(assertion, "basis")?;
        let supported_basis = if translated {
            basis == "reviewed_sdk_translation"
        } else {
            [
                "reviewed_source_continuity",
                "reviewed_authorized_adaptation",
            ]
            .contains(&basis)
        };
        if !seen.insert(digest)
            || assertion["disposition"] != "equivalent"
            || text(assertion, "rationale")?.len() > 4096
            || !supported_basis
        {
            return Err(fail(
                "Assertion review is duplicated, pending or unsupported.",
            ));
        }
        let ranges = array(assertion, "ranges")?;
        if ranges.is_empty() || ranges.len() > 16 {
            return Err(fail("Reviewed assertion ranges are missing or oversized."));
        }
        if translated {
            let bindings = sdk
                .iter()
                .filter(|binding| binding["historical_assertion_sha256"] == digest)
                .collect::<Vec<_>>();
            if bindings.len() != 1 || bindings[0]["ranges"] != assertion["ranges"] {
                return Err(fail(
                    "Translated assertion review differs from explicit SDK traceability.",
                ));
            }
        }
        let mut seen_ranges = BTreeSet::new();
        for range in ranges {
            let path = text(range, "file")?;
            if !source(path) || (!translated && path != original_file) {
                return Err(fail(
                    "Reviewed assertion range is outside its current Rust source.",
                ));
            }
            let bytes = files
                .get(path)
                .ok_or_else(|| fail("Reviewed assertion source missing."))?;
            let source = std::str::from_utf8(bytes)
                .map_err(|_| fail("Reviewed source must be UTF-8."))?;
            let lines = source.lines().collect::<Vec<_>>();
            let bounds = array(range, "lines")?;
            let start = bounds
                .first()
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0);
            let end = bounds
                .get(1)
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0);
            if bounds.len() != 2
                || start == 0
                || end < start
                || end > lines.len()
                || !seen_ranges.insert((path, start, end))
            {
                return Err(fail(
                    "Reviewed assertion source bounds are invalid or duplicated.",
                ));
            }
            let excerpt = lines[start - 1..end].join("\n");
            if excerpt.trim().is_empty() || range["excerpt_sha256"] != sha(excerpt.as_bytes()) {
                return Err(fail("Reviewed assertion excerpt bytes differ."));
            }
        }
    }
    Ok(json!({"schema_version":1,"component":"native-current-assertion-review-verification",
        "cases":rows.len(),"assertion_records":records,"unique_assertions":seen.len(),
        "current_assertion_review_verified":true,"reviewer":"primary",
        "current_execution_claimed":false,"qualification_verified":false}))
}

/// Combine source review with an already authenticated, reconstructed bind_cases result.
/// This is not an execution-report authenticator or a tooling-retirement decision.
pub fn bind_execution(
    rebind: &Value,
    review: &Value,
    files: &BTreeMap<String, Vec<u8>>,
    execution: &Value,
) -> io::Result<Value> {
    let checked = verify(rebind, review, files)?;
    if execution["schema_version"] != 1
        || execution["component"] != "native-current-named-case-execution"
        || execution["identity"]["source_sha"] != rebind["current_source_sha"]
        || execution["baseline"] != BASELINE
        || execution["historical_source_sha"] != SOURCE
        || execution["historical_reference_sha256"] != REFERENCE_SHA256
        || execution["current_named_execution_verified"] != true
        || execution["semantic_parity_claimed"] != false
        || execution["qualification_verified"] != false
        || execution["original_evidence_rewritten"] != false
        || execution["suites"] != 50
    {
        return Err(fail("Source review and named execution identities differ."));
    }
    let expected = array(rebind, "cases")?
        .iter()
        .map(|case| Ok((text(&case["historical"], "id")?, &case["historical"])))
        .collect::<io::Result<BTreeMap<_, _>>>()?;
    let cases = array(execution, "cases")?;
    if cases.len() != expected.len() {
        return Err(fail("Named execution omits reviewed original cases."));
    }
    let mut seen = BTreeSet::new();
    for case in cases {
        let id = text(case, "id")?;
        let original = expected
            .get(id)
            .ok_or_else(|| fail("Named execution introduces an unreviewed case."))?;
        if !seen.insert(id)
            || case["source"] != original["source"]
            || case["source_sha256"] != original["source_sha256"]
            || case["historical_native"] != original["native"]
            || case["historical_assertions_sha256"]
                != ledger::assertion_digest(&original["assertions"])?
        {
            return Err(fail(
                "Named execution duplicates or changes a reviewed obligation.",
            ));
        }
    }
    Ok(json!({"schema_version":1,"component":"native-reviewed-named-case-execution",
        "identity":execution["identity"],"source_review":checked,
        "cases":cases.len(),"suites":50,"current_assertion_review_verified":true,
        "current_named_execution_verified":true,"semantic_parity_claimed":false,
        "qualification_verified":false,"original_evidence_rewritten":false}))
}
