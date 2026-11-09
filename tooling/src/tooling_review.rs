use crate::{
    context::{fail, sha},
    ledger, tooling_execution,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
};

pub const REVIEW_FILE: &str = "docs/testing/tooling-assertion-review.json";

/// Records a primary source comparison only; the caller authenticates execution separately.
pub fn verify(root: &Path, manifest: &Value, review: &Value) -> io::Result<Value> {
    let manifest_bytes = ledger::read(root, tooling_execution::MANIFEST_FILE)?;
    if ledger::parse(&manifest_bytes)? != *manifest {
        return Err(fail(
            "Tooling review requires the exact tracked dispositions.",
        ));
    }
    crate::inventory::tooling_manifest(root, manifest, false)?;
    if review["schema_version"] != 1
        || review["component"] != "native-tooling-assertion-review"
        || review["reviewer"] != "primary"
        || review["scope"] != "source_assertions_only"
        || review["current_execution_claimed"] != false
        || review["qualification_verified"] != false
        || review["retirement_authorized"] != false
        || review["original_revision"] != manifest["original_revision"]
        || review["manifest_sha256"] != sha(&manifest_bytes)
    {
        return Err(fail(
            "Tooling source review identity or evidence boundary differs.",
        ));
    }
    let entries = manifest["entries"]
        .as_array()
        .ok_or_else(|| fail("Tooling dispositions missing."))?;
    let expected = entries
        .iter()
        .map(|entry| {
            entry["source"]
                .as_str()
                .ok_or_else(|| fail("Tooling original identity missing."))
        })
        .collect::<io::Result<BTreeSet<_>>>()?;
    let decisions = review["entries"]
        .as_object()
        .ok_or_else(|| fail("Tooling assertion review entries missing."))?;
    if decisions.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected {
        return Err(fail(
            "Every original tooling file requires one source review.",
        ));
    }
    let mut sources = BTreeSet::from([
        "native/devtools/mod.rs",
        "native/devtools/artifact.rs",
        "native/devtools/signature_windows.rs",
        "native/devtools/signature_macos.rs",
        "docs/testing/sdk-tooling-migration.json",
        "tooling/src/lib.rs",
        "tooling/src/ledger/execution.rs",
        "tooling/src/ledger/input.rs",
        "tooling/src/ledger/mapping.rs",
        "tooling/src/ledger_inputs/source_rebinding.rs",
        "tooling/src/tooling_review.rs",
        "tooling/tests/tooling_review_contracts.rs",
    ]);
    let mut scenarios = 0usize;
    for entry in entries {
        let decision = &decisions[entry["source"].as_str().unwrap()];
        if !matches!(
            decision["basis"].as_str(),
            Some("reviewed_translation" | "reviewed_adaptation" | "retained_independent_input")
        ) || decision["disposition"] != "equivalent"
            || decision["rationale"]
                .as_str()
                .is_none_or(|text| text.trim().is_empty() || text.len() > 4096)
        {
            return Err(fail("Tooling assertion review is pending or unsupported."));
        }
        scenarios += entry["scenarios"]
            .as_array()
            .ok_or_else(|| fail("Tooling scenarios missing."))?
            .len();
        for key in ["rust", "tests"] {
            for path in entry[key]
                .as_array()
                .ok_or_else(|| fail("Reviewed tooling sources missing."))?
            {
                sources.insert(
                    path.as_str()
                        .ok_or_else(|| fail("Reviewed tooling source path is invalid."))?,
                );
            }
        }
    }
    let files = sources
        .into_iter()
        .map(|path| Ok((path.to_owned(), sha(&ledger::read(root, path)?))))
        .collect::<io::Result<BTreeMap<_, _>>>()?;
    if review["source_file_count"].as_u64() != Some(files.len() as u64)
        || review["source_inventory_sha256"] != ledger::assertion_digest(&json!(files))?
        || review["scenario_count"].as_u64() != Some(scenarios as u64)
    {
        return Err(fail(
            "Reviewed tooling source, helper or scenario bytes are stale.",
        ));
    }
    Ok(json!({"schema_version":1,"component":"native-tooling-source-review-verification",
        "manifest_sha256":review["manifest_sha256"],"source_inventory_sha256":review["source_inventory_sha256"],
        "files":entries.len(),"scenarios":scenarios,"reviewer":"primary",
        "source_assertion_review_verified":true,"current_execution_claimed":false,
        "qualification_verified":false,"retirement_authorized":false}))
}

/// The execution must be reconstructed by tooling_execution::bind from authenticated originals.
pub fn bind_execution(
    root: &Path,
    manifest: &Value,
    review: &Value,
    execution: &Value,
) -> io::Result<Value> {
    let checked = verify(root, manifest, review)?;
    if execution["schema_version"] != 1
        || execution["component"] != "native-tooling-harness-execution"
        || execution["manifest_sha256"] != review["manifest_sha256"]
        || execution["mapped_python_js"] != 46
        || execution["retained_powershell"] != manifest["retained_powershell"]
        || execution["current_named_test_execution_verified"] != true
        || execution["declared_scenario_execution_verified"] != true
        || execution["sdk_artifact_phase_execution_verified"] != true
        || execution["scenario_assertions_verified"] != false
        || execution["semantic_parity_claimed"] != false
        || execution["qualification_verified"] != false
        || execution["retirement_authorized"] != false
        || execution["original_evidence_rewritten"] != false
    {
        return Err(fail("Reviewed tooling and reconstructed execution differ."));
    }
    let actual = execution["entries"]
        .as_array()
        .filter(|rows| rows.len() == 46)
        .ok_or_else(|| fail("Reviewed tooling execution inventory missing."))?;
    for (source, executed) in manifest["entries"].as_array().unwrap().iter().zip(actual) {
        if source["source"] != executed["source"]
            || source["source_sha256"] != executed["source_sha256"]
            || source["scenarios"] != executed["scenarios"]
        {
            return Err(fail("Tooling execution changes reviewed scenario identities."));
        }
    }
    Ok(json!({"schema_version":1,"component":"native-reviewed-tooling-execution",
        "identity":execution["identity"],"source_review":checked,
        "source_review_sha256":sha(&ledger::read(root, REVIEW_FILE)?),
        "manifest_sha256":review["manifest_sha256"],
        "current_tooling_assertions_and_execution_verified":true,
        "qualification_verified":false,"retirement_authorized":false}))
}
