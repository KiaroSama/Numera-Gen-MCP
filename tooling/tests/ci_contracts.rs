use numera_task::{context::Context, inventory};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../.ci-work/tooling-tests")
            .join(format!("ci {}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join(".ci-work/native")).unwrap();
        Self { root }
    }
    fn context(&self) -> Context {
        Context {
            root: self.root.canonicalize().unwrap(),
            logger: numera_gen_mcp::logging::Logger::new(&self.root.join("logs"), "ERROR"),
        }
    }
    fn report(&self, value: &Value) {
        fs::write(
            self.root.join(".ci-work/native/coverage.json"),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            if std::thread::panicking() {
                eprintln!("CI fixture cleanup failed: {:?}", error.kind());
            } else {
                panic!("CI fixture cleanup failed: {:?}", error.kind());
            }
        }
    }
}
fn coverage(regions: (u64, u64), branches: (u64, u64)) -> Value {
    json!({"data":[{"totals":{"regions":{"count":regions.0,"covered":regions.1},"branches":{"count":branches.0,"covered":branches.1}}}]})
}

#[test]
fn runtime_coverage_requires_both_eighty_five_region_and_eighty_branch_floors() {
    let fixture = Fixture::new();
    let context = fixture.context();
    fixture.report(&coverage((100, 85), (100, 80)));
    inventory::coverage(&context).unwrap();
    for report in [
        coverage((100, 84), (100, 80)),
        coverage((100, 85), (100, 79)),
        coverage((0, 0), (100, 80)),
        coverage((100, 85), (0, 0)),
    ] {
        fixture.report(&report);
        assert!(
            inventory::coverage(&context).is_err(),
            "both real denominators and floors are required"
        );
    }
}

#[test]
fn coverage_aggregates_counts_not_unweighted_percentages_and_never_accepts_impossible_totals() {
    let fixture = Fixture::new();
    let context = fixture.context();
    fixture.report(&json!({"data":[
        {"totals":{"regions":{"count":10,"covered":0},"branches":{"count":10,"covered":0}}},
        {"totals":{"regions":{"count":90,"covered":90},"branches":{"count":90,"covered":90}}}
    ]}));
    inventory::coverage(&context).unwrap();
    for metric in ["regions", "branches"] {
        let mut impossible = coverage((100, 85), (100, 80));
        impossible["data"][0]["totals"][metric]["covered"] = json!(101);
        fixture.report(&impossible);
        assert!(inventory::coverage(&context).is_err());
        let mut missing = coverage((100, 85), (100, 80));
        missing["data"][0]["totals"][metric]
            .as_object_mut()
            .unwrap()
            .remove("covered");
        fixture.report(&missing);
        assert!(inventory::coverage(&context).is_err());
    }
    fixture.report(&json!({"data":[
        {"totals":{"regions":{"count":u64::MAX,"covered":0},"branches":{"count":100,"covered":80}}},
        {"totals":{"regions":{"count":1,"covered":1},"branches":{"count":100,"covered":80}}}
    ]}));
    assert!(
        inventory::coverage(&context).is_err(),
        "counter overflow cannot wrap into a passing percentage"
    );
}

#[test]
fn malformed_empty_negative_and_noninteger_coverage_reports_fail_closed() {
    let fixture = Fixture::new();
    let context = fixture.context();
    for report in [
        json!({}),
        json!({"data":[]}),
        json!({"data":{}}),
        json!({"data":[{}]}),
        json!({"data":[{"totals":{"regions":{"count":-1,"covered":0},"branches":{"count":100,"covered":80}}}]}),
        json!({"data":[{"totals":{"regions":{"count":100.5,"covered":90},"branches":{"count":100,"covered":80}}}]}),
    ] {
        fixture.report(&report);
        assert!(inventory::coverage(&context).is_err());
    }
    fs::write(
        fixture.root.join(".ci-work/native/coverage.json"),
        b"not-json",
    )
    .unwrap();
    assert!(inventory::coverage(&context).is_err());
}

#[test]
fn tracked_text_requires_strict_utf8_and_the_exact_eight_hundred_source_line_boundary() {
    for name in ["native/example.rs", "scripts/example.py", "scripts/example.mjs", "scripts/example.ps1"] {
        for ending in ["\n", "\r\n"] {
            let limit = format!("{}last", ending.repeat(799));
            inventory::validate_source(name, limit.as_bytes()).unwrap();
            inventory::validate_source(name, format!("{limit}{ending}").as_bytes()).unwrap();
            let oversized = format!("{limit}{ending}extra");
            assert_eq!(
                inventory::validate_source(name, oversized.as_bytes()).unwrap_err().to_string(),
                "Source exceeds 800 lines."
            );
        }
        inventory::validate_source(name, "تصویر\u{2028}東京\r".as_bytes()).unwrap();
        let unicode = format!("{}تصویر\u{2028}東京\r", "\n".repeat(799));
        inventory::validate_source(name, unicode.as_bytes()).unwrap();
        assert!(inventory::validate_source(name, format!("{unicode}\nextra").as_bytes()).is_err());
        inventory::validate_source(name, b"").unwrap();
        assert!(inventory::validate_source(name, b"invalid\xff").is_err());
    }
    inventory::validate_source("docs/example.md", "text\n".repeat(801).as_bytes()).unwrap();
    assert!(inventory::validate_source("docs/example.md", &[0xff]).is_err());
}

#[test]
fn diagnostics_expose_only_explicit_safe_messages_not_os_or_private_child_text() {
    use numera_task::context::{diagnostic, fail};
    assert_eq!(
        diagnostic(&fail("Required fixture provenance differs.")),
        "Required fixture provenance differs."
    );
    let private = std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "synthetic-password /private/path",
    );
    let result = diagnostic(&private);
    assert!(result.contains("PermissionDenied"));
    assert!(!result.contains("synthetic-password"));
    assert!(!result.contains("/private/path"));
}

#[test]
fn workspace_discovery_includes_every_real_test_file_and_rejects_missing_or_extra_targets() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let first = fixture.root.join("runtime-test.rs");
    let second = fixture.root.join("tooling-test.rs");
    fs::write(&first, b"source only").unwrap();
    fs::write(&second, b"source only").unwrap();
    let first = first.canonicalize().unwrap();
    let second = second.canonicalize().unwrap();
    let actual = std::collections::BTreeSet::from([first.clone(), second.clone()]);
    let metadata = json!({"workspace_members":["runtime","tooling"],"packages":[
        {"id":"runtime","targets":[{"kind":["test"],"src_path":first}]},
        {"id":"tooling","targets":[{"kind":["test"],"src_path":second}]},
        {"id":"dependency","targets":[{"kind":["test"],"src_path":"not-a-workspace-member"}]}
    ]});
    numera_task::ci::verify_suite_inventory(&metadata, &actual).unwrap();
    let mut missing = metadata.clone();
    missing["packages"][1]["targets"] = json!([]);
    assert!(numera_task::ci::verify_suite_inventory(&missing, &actual).is_err());
    let mut extra = actual.clone();
    extra.remove(&second);
    assert!(numera_task::ci::verify_suite_inventory(&metadata, &extra).is_err());
    let output = format!(
        "{}\n{}\n",
        json!({"reason":"compiler-artifact","profile":{"test":true},"target":{"name":"sdk_acceptance"},"executable":first}),
        json!({"reason":"compiler-artifact","profile":{"test":true},"target":{"name":"media_acceptance"},"executable":second})
    );
    numera_task::ci::record_test_executables(&context, output.as_bytes()).unwrap();
    assert_eq!(
        numera_task::ci::test_executable(&context, "sdk_acceptance").unwrap(),
        first
    );
    assert_eq!(
        numera_task::ci::test_executable(&context, "media_acceptance").unwrap(),
        second
    );
    assert!(numera_task::ci::test_executable(&context, "missing").is_err());
}

#[test]
fn original_tooling_manifest_accounts_for_every_file_without_promoting_source_mapping_to_acceptance()
 {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let bytes = fs::read(project.join("docs/testing/tooling-migration.json")).unwrap();
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    let report = inventory::tooling_manifest(project, &manifest, false).unwrap();
    assert_eq!(report["mapped_python_js"], 46);
    assert_eq!(report["retained_powershell"], 5);
    assert_eq!(report["acceptance_verified"], false);
    assert!(
        inventory::tooling_manifest(project, &manifest, true).is_err(),
        "source mappings cannot authorize retirement"
    );
    let tracked = manifest["entries"]
        .as_array()
        .unwrap()
        .iter()
        .chain(manifest["retained_powershell"].as_array().unwrap())
        .map(|row| row["source"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\0")
        + "\0";
    inventory::tooling_inventory(&manifest, &tracked).unwrap();
    assert!(
        inventory::tooling_inventory(&manifest, &(tracked.clone() + "scripts/unmapped.py\0"))
            .is_err()
    );
    assert!(inventory::tooling_inventory(&manifest, tracked.split_once('\0').unwrap().1).is_err());
    // An ignored retired source helper is not part of git ls-files and must not enter the inventory.
    inventory::tooling_inventory(&manifest, &(tracked.clone() + "docs/example.md\0")).unwrap();
    for mutation in [
        "missing",
        "duplicate",
        "hash",
        "replacement",
        "test",
        "powershell",
        "count",
    ] {
        let mut changed = manifest.clone();
        match mutation {
            "missing" => {
                changed["entries"].as_array_mut().unwrap().pop();
            }
            "duplicate" => changed["entries"][1] = changed["entries"][0].clone(),
            "hash" => changed["entries"][0]["source_sha256"] = json!("0".repeat(64)),
            "replacement" => changed["entries"][0]["rust"][0] = json!("../outside.rs"),
            "test" => changed["entries"][0]["tests"][0] = json!("tests/rust/missing-test.rs"),
            "powershell" => {
                changed["retained_powershell"].as_array_mut().unwrap().pop();
            }
            "count" => changed["rust_migration_count"] = json!(45),
            _ => unreachable!(),
        }
        assert!(
            inventory::tooling_manifest(project, &changed, false).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn actual_task_cli_reports_help_invalid_input_and_unauthorized_signing_without_private_mutation() {
    use numera_gen_mcp::devtools::supervisor::{Bounds, run};
    use std::{process::Command, sync::atomic::AtomicBool};

    let fixture = Fixture::new();
    let root = fixture.root.join("task 日本語");
    fs::create_dir_all(root.join("native")).unwrap();
    fs::write(root.join("Cargo.toml"), b"inert fixture; never compiled").unwrap();
    fs::write(root.join("native/lib.rs"), b"inert fixture; never compiled").unwrap();
    fs::write(root.join("retained.txt"), b"unrelated original bytes").unwrap();
    let root = root.canonicalize().unwrap();
    let root_text = root.to_str().unwrap();
    let owner = fixture.context();
    let target = numera_gen_mcp::devtools::artifact::target();
    for (arguments, hosted, expected_code, expected) in [
        (vec!["--help"], false, 0, "Usage: numera-task --root ABSOLUTE_ROOT COMMAND [ARGS]"),
        (vec!["--root", "relative", "inventory"], true, 1, "Task root must be absolute."),
        (vec!["--root", root_text], true, 1, "Explicit owning --root is required."),
        (vec!["--root", root_text, "inventory"], false, 1, "Compilation and verification are hosted CI only."),
        (vec!["--root", root_text, "unrequested"], true, 1, "Unsupported task command; no fallback was executed."),
        (vec!["--root", root_text, "sign", target], true, 1, "Protected hosted signing route not authorized."),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_numera-task"));
        command.args(arguments).env_clear();
        for key in ["PATH", "SystemRoot", "WINDIR", "LLVM_PROFILE_FILE"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        if hosted {
            command
                .env("GITHUB_ACTIONS", "true")
                .env("GITHUB_REPOSITORY", "KiaroSama/Numera-Gen-MCP")
                .env("GITHUB_REF", "refs/heads/chore/tooling-media")
                .env("GITHUB_EVENT_NAME", "workflow_dispatch")
                .env("GITHUB_JOB", "signing")
                .env("NUMERA_CI_OPERATION", "qualify")
                .env("NUMERA_SIGNING_PFX_BASE64", "synthetic-invalid-identity")
                .env("NUMERA_SIGNING_PASSWORD", "synthetic-private-password");
        }
        let outcome = run(
            &mut command,
            &owner.root,
            Bounds::new(5000, 2000).unwrap(),
            &AtomicBool::new(false),
            &owner.logger,
        )
        .unwrap();
        assert_eq!(outcome.code, expected_code);
        assert!(outcome.reason.is_none());
        assert!(outcome.cleanup_verified);
        let stdout = std::str::from_utf8(&outcome.stdout).unwrap();
        let stderr = std::str::from_utf8(&outcome.stderr).unwrap();
        if expected_code == 0 {
            assert!(stdout.contains(expected));
            assert!(stderr.is_empty());
        } else {
            assert!(stdout.is_empty());
            assert_eq!(stderr.trim(), format!("ERROR: {expected}"));
        }
        for private in [root_text, "synthetic-invalid-identity", "synthetic-private-password"] {
            assert!(!stdout.contains(private));
            assert!(!stderr.contains(private));
        }
        for forbidden in [".ci-work/signing", ".ci-work/prepared", ".ci-work/release", "target"] {
            assert!(!root.join(forbidden).exists());
        }
        assert_eq!(fs::read(root.join("retained.txt")).unwrap(), b"unrelated original bytes");
        assert_eq!(fs::read(root.join("Cargo.toml")).unwrap(), b"inert fixture; never compiled");
    }
}

#[test]
fn successful_diagnostics_never_clear_required_format_or_other_gate_failures() {
    numera_task::ci::finish(Vec::new()).unwrap();
    for failures in [vec!["format".to_owned()], vec!["format".to_owned(), "clippy".to_owned()]] {
        let error = numera_task::ci::finish(failures).unwrap_err();
        assert_eq!(error.to_string(), "Required native gates failed; retained coverage or artifacts do not override execution failures.");
    }
}

#[test]
fn comfy_transport_remains_in_services_outside_pure_adapter_mapping() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    assert!(!project.join("native/adapters/comfy.rs").exists());
    let service = fs::read_to_string(project.join("native/services/comfy.rs")).unwrap();
    for entry in ["pub async fn prepare(", "pub async fn submit(", "pub async fn status("] {
        assert!(service.contains(entry), "missing service boundary: {entry}");
    }
}

#[test]
fn client_json_examples_launch_only_native_bridge_and_do_not_embed_provider_credentials() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let desktop: Value =
        serde_json::from_slice(&fs::read(project.join("examples/claude-desktop.json")).unwrap())
            .unwrap();
    let code: Value =
        serde_json::from_slice(&fs::read(project.join("examples/claude-project.json")).unwrap())
            .unwrap();
    assert_eq!(
        desktop
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["mcpServers"]
    );
    assert_eq!(
        code.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["mcpServers"]
    );
    for servers in [&desktop["mcpServers"], &code["mcpServers"]] {
        assert_eq!(
            servers
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["numera-gen-mcp"]
        );
        let entry = &servers["numera-gen-mcp"];
        assert_eq!(entry["args"], json!(["bridge"]));
        assert_eq!(
            entry["command"],
            "C:/tools/numera-gen-mcp/bin/numera-gen-mcp.exe"
        );
        assert_eq!(
            entry["env"],
            json!({"NUMERA_CONFIG":"C:/tools/numera-gen-mcp/.env",
                "TEMP":"C:/tools/numera-gen-mcp/.runtime/temp",
                "TMP":"C:/tools/numera-gen-mcp/.runtime/temp",
                "TMPDIR":"C:/tools/numera-gen-mcp/.runtime/temp"})
        );
    }
    assert!(desktop["mcpServers"]["numera-gen-mcp"].get("type").is_none());
    assert_eq!(code["mcpServers"]["numera-gen-mcp"]["type"], "stdio");
    // The maintained example deliberately uses this exact small TOML surface; unknown lines fail.
    let codex = fs::read_to_string(project.join("examples/codex.toml")).unwrap();
    assert_eq!(
        codex
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>(),
        vec![
            "[mcp_servers.numera-gen-mcp]",
            "command = \"C:/tools/numera-gen-mcp/bin/numera-gen-mcp.exe\"",
            "args = [\"bridge\"]",
            "startup_timeout_sec = 10",
            "tool_timeout_sec = 360",
            "env = { NUMERA_CONFIG = \"C:/tools/numera-gen-mcp/.env\", TEMP = \"C:/tools/numera-gen-mcp/.runtime/temp\", TMP = \"C:/tools/numera-gen-mcp/.runtime/temp\", TMPDIR = \"C:/tools/numera-gen-mcp/.runtime/temp\" }",
        ]
    );
}
