use std::{fs, path::Path};

#[test]
fn initial_hosted_build_uses_the_qualified_native_owner_in_every_context() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let action = fs::read_to_string(root.join(".github/actions/native-bootstrap/action.yml")).unwrap();
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    assert!(action.contains("using: composite"));
    assert!(action.contains("GITHUB_ACTIONS"));
    assert!(action.contains("GITHUB_REPOSITORY"));
    assert!(action.contains("GITHUB_EVENT_NAME"));
    for target in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc", "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu", "x86_64-apple-darwin", "aarch64-apple-darwin"] {
        assert!(action.contains(target), "missing real bootstrap target: {target}");
    }
    for digest in [
        "0128674d9c1a0aad8eb111dc4e45e7def89119f22369089d73729905a315e658",
        "e132f203a88251f3fa4e22a2bdfd7a25261453bc95af55a39f056c364ac0f8e0",
        "51a089b26e3bf9cdbf0631a56ee4644d96babc3ff3f1e0c1dab8948c53bbd56d",
        "62a07c39d57a18afe9dd144c722300b966b14655141655c60d300b89f5e7ad66",
        "e2143157034ae30d2dbe6954e30fd76c8036c412a3b7407db83441e53b484d00",
        "cb26ee662e19fbc12f33209e988a55817fb7dcbe0d94a1836c3475b8dc55cdc7",
    ] {
        assert!(action.contains(digest));
    }
    for bound in ["--max-time 90", "--connect-timeout 10", "--speed-time 30", "--speed-limit 1024",
        "--max-filesize 16777216", "--proto '=https'", "--proto-redir '=https'", "bounded 600000 180000",
        "bounded 900000", "--no-self-update", "build --locked -p numera-task"] {
        assert!(action.contains(bound), "missing bounded bootstrap contract: {bound}");
    }
    assert!(action.contains("numera-image-gen-mcp-0.2.0-"), "immutable signed bootstrap name is historical");
    assert!(action.contains("1.99.1"));
    assert!(action.contains("nightly-2026-10-10"));
    assert_eq!(workflow.matches("uses: ./.github/actions/native-bootstrap").count(), 5);
    assert!(!workflow.contains("uses: dtolnay/rust-toolchain"));
    assert!(!workflow.contains("run: cargo build"));
    assert!(!workflow.contains("run: cargo +1.99.1 build"));
    assert!(workflow.contains("coverage: 'true'"));
    assert!(workflow.contains("name: native-${{ matrix.target }}"));
    assert!(workflow.contains("  execution-matrix:\n"));
    assert!(workflow.contains("needs: [native, native-coverage, execution-matrix]"));
    assert!(workflow.contains("bounded 900000 60000 --exec ./target/debug/numera-task"));
    assert!(workflow.contains("GH_TOKEN: ${{ github.token }}"));
    for retired in ["python", "node ", "npx ", "sleep ", "continue-on-error", "curl |", "--retry 10"] {
        assert!(!action.contains(retired), "bootstrap must not add an interpreted runner or hidden retry: {retired}");
    }
}

#[test]
fn source_preparation_cannot_build_qualify_or_receive_signing_inputs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let action = fs::read_to_string(root.join(".github/actions/native-bootstrap/action.yml")).unwrap();
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let prepare = workflow.split_once("  source-preparation:\n").unwrap().1
        .split_once("  native:\n").unwrap().0;
    assert!(action.contains("build-task:"));
    assert!(action.contains("test \"$GITHUB_JOB\" = source-preparation"));
    assert!(action.contains("test \"$BOOTSTRAP_COVERAGE\" = false"));
    assert!(prepare.contains("build-task: 'false'"));
    assert!(prepare.contains("inputs.operation == 'source-preparation'"));
    for required in ["generate-lockfile", "fmt --all", "fmt --all -- --check", "git diff --binary -- Cargo.lock '*.rs'",
        "source-preparation.patch", "receipt.json", "SHA256SUMS", "GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT",
        "if-no-files-found: error", "persist-credentials: false"] {
        assert!(prepare.contains(required), "missing preparation boundary: {required}");
    }
    for forbidden in ["secrets.", "environment:", "cargo build", "cargo test", "numera-task", "sign '", "continue-on-error"] {
        assert!(!prepare.contains(forbidden), "preparation cannot qualify or sign: {forbidden}");
    }
    assert_eq!(workflow.matches("if: inputs.operation == 'qualify'\n").count(), 3);
    assert!(workflow.contains("if: inputs.operation == 'qualify' && github.repository == 'KiaroSama/Numera-Gen-MCP' && github.ref == 'refs/heads/main'"));
}
