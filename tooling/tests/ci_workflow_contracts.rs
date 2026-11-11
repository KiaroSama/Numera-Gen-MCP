use std::{fs, path::Path};

fn source(path: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    fs::read_to_string(root.join(path)).unwrap()
}

// Inspect the maintained block-style workflow without adding a YAML dependency.
fn block<'a>(text: &'a str, key: &str, indent: usize) -> &'a str {
    let heading = format!("{key}:");
    let mut start = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let depth = line.len() - line.trim_start().len();
        if let Some(start) = start {
            if !trimmed.is_empty() && !trimmed.starts_with('#') && depth <= indent {
                return &text[start..offset];
            }
        } else if depth == indent && trimmed == heading {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    &text[start.unwrap_or_else(|| panic!("Missing workflow block: {key}"))..]
}

fn compact(text: &str) -> String {
    text.split_whitespace().collect()
}

#[test]
fn only_manual_branch_source_runs_qualify_without_cancelling_original_evidence() {
    let workflow = source(".github/workflows/ci.yml");
    let events = block(&workflow, "on", 0);
    let initiators: Vec<_> = events
        .lines()
        .filter(|line| line.len() - line.trim_start().len() == 2)
        .map(str::trim)
        .collect();
    assert_eq!(initiators, ["workflow_dispatch:"]);
    let concurrency = compact(block(&workflow, "concurrency", 0));
    assert!(concurrency.contains("github.ref"));
    assert!(concurrency.contains("inputs.operation"));
    assert!(concurrency.contains("cancel-in-progress:false"));
    assert!(workflow.contains("run-name:"));
    for label in ["'Tests'", "'Source preparation'", "'Signing'"] {
        assert!(
            workflow.contains(label),
            "Missing operation display: {label}"
        );
    }
    assert!(workflow.contains("NUMERA_BUILD_SHA: ${{ github.sha }}"));
    let checkouts: Vec<_> = workflow
        .split("- uses: actions/checkout@")
        .skip(1)
        .collect();
    assert_eq!(checkouts.len(), 7);
    for checkout in checkouts {
        let checkout = checkout.split("\n      - ").next().unwrap();
        assert!(checkout.contains("ref: ${{ github.sha }}"));
        assert!(checkout.contains("persist-credentials: false"));
    }
    for job in ["native", "native-coverage", "execution-matrix"] {
        let job = compact(block(&workflow, job, 2));
        assert!(
            job.contains("if:github.event_name=='workflow_dispatch'&&inputs.operation=='qualify'")
        );
        assert!(!job.contains("secrets."));
        assert!(!job.contains("environment:"));
    }
    let action = source(".github/actions/native-bootstrap/action.yml");
    assert!(action.contains("github.event_name != 'workflow_dispatch'"));
    for automatic in ["'push'", "'pull_request'"] {
        assert!(!action.contains(automatic));
    }
    assert!(!workflow.contains("continue-on-error"));
}

#[test]
fn qualification_preserves_six_native_targets_coverage_and_original_report_collector() {
    let workflow = source(".github/workflows/ci.yml");
    let native = block(&workflow, "native", 2);
    let signing = block(&workflow, "signing", 2);
    let expected = [
        ("windows-latest", "x86_64-pc-windows-msvc"),
        ("windows-11-arm", "aarch64-pc-windows-msvc"),
        ("ubuntu-latest", "x86_64-unknown-linux-gnu"),
        ("ubuntu-24.04-arm", "aarch64-unknown-linux-gnu"),
        ("macos-15-intel", "x86_64-apple-darwin"),
        ("macos-latest", "aarch64-apple-darwin"),
    ];
    for job in [native, signing] {
        let matrix = compact(block(job, "matrix", 6));
        assert_eq!(matrix.matches("-os:").count(), expected.len());
        for (os, target) in expected {
            assert!(matrix.contains(&format!("-os:{os}target:{target}")));
        }
        assert!(job.contains("fail-fast: false"));
    }
    assert!(native.contains("name: native-${{ matrix.target }}"));
    for operation in [
        " stable",
        " prepare ",
        " extract-intermediate ",
        " case-execution",
    ] {
        assert!(native.contains(operation));
    }
    let coverage = block(&workflow, "native-coverage", 2);
    assert!(coverage.contains("coverage: 'true'"));
    assert!(coverage.contains(" coverage"));
    assert!(coverage.contains(" case-execution"));
    for (job, suffix, retained) in [
        (native, "${{ matrix.target }}", 2),
        (coverage, "coverage", 3),
    ] {
        for report in [
            "test-execution.json",
            "sdk-execution-*.json",
            "current-case-execution.json",
            "current-source-case-rebind-*.json",
            "oracle-evidence.zip",
        ] {
            assert!(job.contains(report), "Missing original report: {report}");
        }
        assert!(job.contains(&format!("name: native-execution-{suffix}")));
        assert!(job.contains(&format!("name: native-oracle-{suffix}")));
        assert_eq!(job.matches("if: always()").count(), retained);
        assert!(job.contains("if-no-files-found: error"));
    }
    assert!(coverage.contains("path: .ci-work/native/coverage.json"));
    let collector = compact(block(&workflow, "execution-matrix", 2));
    assert!(collector.contains("needs:[native,native-coverage]"));
    assert!(collector.contains("bounded90000060000--exec"));
    assert!(collector.contains("execution-matrix'"));
    assert!(collector.contains("name:native-execution-matrix"));
    assert!(collector.contains("path:.ci-work/native/execution-matrix.json"));
    assert!(collector.contains("if-no-files-found:error"));
    assert!(!collector.contains("always()"));
}

#[test]
fn preparation_and_same_source_signing_remain_isolated_from_native_test_reruns() {
    let workflow = source(".github/workflows/ci.yml");
    let preparation = block(&workflow, "source-preparation", 2);
    assert!(preparation.contains("inputs.operation == 'source-preparation'"));
    assert!(preparation.contains("build-task: 'false'"));
    for forbidden in [
        "secrets.",
        "environment:",
        " stable",
        " coverage",
        " sign '",
    ] {
        assert!(!preparation.contains(forbidden));
    }
    let signing = block(&workflow, "signing", 2);
    let signed = block(&workflow, "signed-execution-matrix", 2);
    for job in [signing, signed] {
        let job = compact(job);
        for gate in [
            "!cancelled()",
            "github.ref=='refs/heads/main'",
            "github.event_name=='workflow_dispatch'",
            "inputs.operation=='qualify'&&needs.native.result=='success'",
            "needs.native-coverage.result=='success'&&needs.execution-matrix.result=='success'",
            "inputs.operation=='sign-qualified'&&needs.native.result=='skipped'",
            "needs.native-coverage.result=='skipped'&&needs.execution-matrix.result=='skipped'",
        ] {
            assert!(job.contains(gate), "Missing signing gate: {gate}");
        }
        assert!(!job.contains("source-preparation"));
    }
    assert!(signing.contains("environment: numera-signing"));
    assert!(signing.contains("artifact-ids: ${{ steps.source.outputs.artifact-id }}"));
    assert!(signing.contains("run-id: ${{ steps.source.outputs.run-id }}"));
    assert!(signing.contains("digest-mismatch: error"));
    assert!(signing.find(" signing-source ").unwrap() < signing.find("secrets.").unwrap());
    for forbidden in [
        " stable",
        " coverage",
        " case-execution",
        "continue-on-error",
    ] {
        assert!(!signing.contains(forbidden));
    }
    assert!(signed.contains("needs: [native, native-coverage, execution-matrix, signing]"));
    assert!(signed.contains("needs.signing.result == 'success'"));
    assert!(signed.contains("current-qualification.json"));
}
