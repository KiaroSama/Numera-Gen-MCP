use numera_task::{context::Context, test_execution::{compiled_harnesses, harness_command, listing, results, selected_sources, verify_harness_inventory, execution_identity}};
use serde_json::json;
use std::{collections::BTreeSet, fs, path::{Path, PathBuf}};

#[test]
fn listing_requires_unique_complete_test_names_and_never_treats_benchmarks_as_tests() {
    assert_eq!(listing(b"module::one: test\nmodule::two: test\n").unwrap(),
        BTreeSet::from(["module::one".to_owned(), "module::two".to_owned()]));
    assert!(listing(b"").unwrap().is_empty());
    for invalid in [
        "one: test\none: test\n", "one: test", "one: benchmark\n", "unexpected output\n",
        "../path: test\n", "one\u{1b}[0m: test\n", " : test\n",
    ] { assert!(listing(invalid.as_bytes()).is_err(), "{invalid:?}"); }
    assert!(listing(&[0xff]).is_err());
}

#[test]
fn exact_results_require_every_listed_name_and_an_unfiltered_zero_failure_summary() {
    let expected = BTreeSet::from(["one".to_owned(), "module::two".to_owned()]);
    let good = "\nrunning 2 tests\ntest one ... ok\ntest module::two ... ok\n\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\n";
    let value = results(good.as_bytes(), &expected).unwrap();
    assert_eq!(value["passed"], json!(["module::two", "one"]));
    assert_eq!(value["count"], 2);
    let panic = good.replace("test one ... ok", "test one - should panic ... ok").replace('\n', "\r\n");
    assert_eq!(results(panic.as_bytes(), &expected).unwrap()["passed"], value["passed"]);
    let slow = good.replace("test one ... ok", "test one has been running for over 60 seconds\ntest one ... ok");
    assert_eq!(results(slow.as_bytes(), &expected).unwrap()["slow_tests"], json!(["one"]));
    for changed in [
        slow.replace("one has been running", "unknown has been running"),
        slow.replace("60 seconds", "120 seconds"),
        good.replace("test one ... ok\n", ""),
        good.replace("test one ... ok", "test unknown ... ok"),
        good.replace("test one ... ok", "test one ... FAILED"),
        good.replace("test one ... ok", "test one ... ignored"),
        good.replace("running 2 tests", "running 1 test"),
        good.replace("2 passed", "1 passed"),
        good.replace("0 failed", "1 failed"),
        good.replace("0 ignored", "1 ignored"),
        good.replace("0 filtered out", "1 filtered out"),
        good.replace("test one ... ok", "test one ... ok\ntest one ... ok"),
        good.replace("test result: ok.", "test result: FAILED."),
        good.replace("test result: ok.", "untrusted prefix test result: ok."),
        format!("{good}{good}"), good.trim_end().to_owned(),
    ] { assert!(results(changed.as_bytes(), &expected).is_err(), "{changed}"); }
    let empty = b"running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
    assert_eq!(results(empty, &BTreeSet::new()).unwrap()["count"], 0);
    assert!(results(good.as_bytes(), &BTreeSet::new()).is_err());
    assert!(results(b"", &expected).is_err());
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.ci-work/tooling-tests")
            .join(format!("execution {}", uuid::Uuid::new_v4()));
        for name in ["native", "tooling/src", "target/debug/deps", "tests/rust"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        for name in ["Cargo.toml", "tooling/Cargo.toml", "native/lib.rs", "tests/rust/sdk_acceptance.rs",
            "target/debug/deps/library", "target/debug/deps/sdk"] { fs::write(root.join(name), b"synthetic bytes").unwrap(); }
        Self(root.canonicalize().unwrap())
    }
    fn artifact(&self, source: &str, executable: &str) -> serde_json::Value {
        json!({"reason":"compiler-artifact","manifest_path":self.0.join("Cargo.toml"),
            "profile":{"test":true},"target":{"src_path":self.0.join(source)},
            "executable":self.0.join(executable)})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            if std::thread::panicking() { eprintln!("Execution fixture cleanup failed: {:?}", error.kind()); }
            else { panic!("Execution fixture cleanup failed: {:?}", error.kind()); }
        }
    }
}
#[test]
fn standalone_harness_keeps_package_cwd_profile_override_and_removal_without_copying_private_env() {
    use std::{ffi::OsString, process::Command};
    let fixture = Fixture::new();
    let context = Context {
        root: fixture.0.clone(),
        logger: numera_gen_mcp::logging::Logger::new(&fixture.0.join("logs"), "ERROR"),
    };
    let target = fixture.0.join("target/llvm-cov-target");
    let profile = target.join("原本-%p-%m.profraw");
    let mut compile = Command::new("cargo");
    compile.env("LLVM_PROFILE_FILE", &profile).env("CARGO_TARGET_DIR", &target)
        .env("NUMERA_SIGNING_FIXTURE", "synthetic-not-a-secret")
        .env("NUMERA_NATIVE_BINARY", fixture.0.join("target/release/numera-gen-mcp"));
    let executable = fixture.0.join("target/debug/deps/sdk");
    for (source, cwd) in [("tests/rust/sdk_acceptance.rs", fixture.0.clone()),
        ("tooling/tests/test_execution_contracts.rs", fixture.0.join("tooling"))] {
        let command = harness_command(&context, &compile, source, &executable);
        assert_eq!(command.get_current_dir(), Some(cwd.as_path()));
        let overrides = command.get_envs().map(|(key, value)|
            (key.to_os_string(), value.map(ToOwned::to_owned))).collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(overrides[&OsString::from("LLVM_PROFILE_FILE")], Some(profile.clone().into_os_string()));
        assert_eq!(overrides[&OsString::from("CARGO_TARGET_DIR")], Some(target.clone().into_os_string()));
        assert!(!overrides.contains_key(&OsString::from("NUMERA_SIGNING_FIXTURE")));
        assert_eq!(overrides[&OsString::from("RUST_TEST_NOCAPTURE")], None);
        assert_eq!(overrides[&OsString::from("NUMERA_NATIVE_BINARY")], None);
    }
    compile.env_remove("LLVM_PROFILE_FILE");
    let command = harness_command(&context, &compile, "native/lib.rs", &executable);
    assert!(command.get_envs().any(|(key, value)| key == "LLVM_PROFILE_FILE" && value.is_none()));
}

#[test]
fn execution_identity_requires_exact_manual_repository_source_positive_run_attempt_and_actual_target() {
    let environment = json!({"GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP","GITHUB_EVENT_NAME":"workflow_dispatch",
        "GITHUB_SHA":"a".repeat(40),"GITHUB_REF":"refs/heads/main","GITHUB_RUN_ID":"42","GITHUB_RUN_ATTEMPT":"2","GITHUB_JOB":"native"});
    let actual = numera_gen_mcp::devtools::artifact::target();
    let value = execution_identity(&environment, "built", actual).unwrap();
    assert_eq!(value["run_id"], 42); assert_eq!(value["run_attempt"], 2);
    assert_eq!(value["target"], actual); assert_eq!(value["phase"], "built");
    for (key, wrong) in [("GITHUB_REPOSITORY","other/project"),("GITHUB_EVENT_NAME","push"),
        ("GITHUB_SHA","bad"),("GITHUB_REF","refs/tags/v0.3.0"),("GITHUB_RUN_ID","0"),
        ("GITHUB_RUN_ID","+42"),("GITHUB_RUN_ID","18446744073709551616"),("GITHUB_RUN_ATTEMPT",""),
        ("GITHUB_JOB","other")] {
        let mut changed = environment.clone(); changed[key] = json!(wrong);
        assert!(execution_identity(&changed, "built", actual).is_err(), "{key}");
    }
    assert!(execution_identity(&environment, "unknown", actual).is_err());
    assert!(execution_identity(&environment, "coverage", actual).is_err());
    assert!(execution_identity(&environment, "built", "unsupported").is_err());
    let mut coverage = environment.clone(); coverage["GITHUB_JOB"] = json!("native-coverage");
    assert!(execution_identity(&coverage, "coverage", "x86_64-unknown-linux-gnu").is_ok());
    assert!(execution_identity(&coverage, "coverage", "aarch64-unknown-linux-gnu").is_err());
    let mut signing = environment; signing["GITHUB_JOB"] = json!("signing");
    assert!(execution_identity(&signing, "extracted-signed", actual).is_ok());
    assert!(execution_identity(&signing, "release", actual).is_err());
}

fn messages(values: &[serde_json::Value]) -> Vec<u8> {
    values.iter().map(|value| value.to_string() + "\n").collect::<String>().into_bytes()
}

#[test]
fn harness_inventory_requires_every_selected_package_target_and_rejects_omissions_or_extras() {
    let fixture = Fixture::new();
    let library = fixture.0.join("native/lib.rs");
    let sdk = fixture.0.join("tests/rust/sdk_acceptance.rs");
    let task = fixture.0.join("tooling/src/lib.rs");
    fs::write(&task, b"synthetic task source").unwrap();
    let metadata = json!({"workspace_members":["runtime", "task"],"packages":[
        {"id":"runtime","name":"numera-gen-mcp","targets":[
            {"test":true,"kind":["lib"],"src_path":library},
            {"test":true,"kind":["test"],"src_path":sdk},
            {"test":false,"kind":["example"],"src_path":"unused"}]},
        {"id":"task","name":"numera-task","targets":[{"test":true,"kind":["lib"],"src_path":task}]},
        {"id":"dependency","name":"dependency","targets":[]}]});
    let runtime = std::collections::BTreeMap::from([
        ("native/lib.rs".into(), fixture.0.join("target/debug/deps/library")),
        ("tests/rust/sdk_acceptance.rs".into(), fixture.0.join("target/debug/deps/sdk"))]);
    assert_eq!(selected_sources(&fixture.0, &metadata, &["numera-gen-mcp"]).unwrap(), runtime.keys().cloned().collect::<BTreeSet<_>>());
    verify_harness_inventory(&fixture.0, &metadata, &["numera-gen-mcp"], &runtime).unwrap();
    assert!(verify_harness_inventory(&fixture.0, &metadata, &["numera-gen-mcp", "numera-task"], &runtime).is_err());
    let mut all = runtime.clone();
    all.insert("tooling/src/lib.rs".into(), fixture.0.join("target/debug/deps/task"));
    verify_harness_inventory(&fixture.0, &metadata, &["numera-gen-mcp", "numera-task"], &all).unwrap();
    assert!(verify_harness_inventory(&fixture.0, &metadata, &["numera-gen-mcp"], &all).is_err());
    for selected in [vec![], vec!["missing"], vec!["numera-gen-mcp", "numera-gen-mcp"], vec!["dependency"]] {
        assert!(verify_harness_inventory(&fixture.0, &metadata, &selected, &runtime).is_err());
    }
    let mut missing = runtime.clone(); missing.remove("native/lib.rs");
    assert!(verify_harness_inventory(&fixture.0, &metadata, &["numera-gen-mcp"], &missing).is_err());
    let mut duplicate = metadata.clone();
    duplicate["packages"][0]["targets"].as_array_mut().unwrap().push(metadata["packages"][0]["targets"][0].clone());
    assert!(verify_harness_inventory(&fixture.0, &duplicate, &["numera-gen-mcp"], &runtime).is_err());
}

#[test]
fn compiler_inventory_binds_source_and_executable_but_never_claims_execution() {
    let fixture = Fixture::new();
    let library = fixture.artifact("native/lib.rs", "target/debug/deps/library");
    let sdk = fixture.artifact("tests/rust/sdk_acceptance.rs", "target/debug/deps/sdk");
    let done = json!({"reason":"build-finished","success":true});
    let output = messages(&[library.clone(), sdk.clone(), done.clone()]);
    let actual = compiled_harnesses(&fixture.0, &output).unwrap();
    assert_eq!(actual.len(), 2);
    assert_eq!(actual["native/lib.rs"], fixture.0.join("target/debug/deps/library"));
    assert_eq!(actual["tests/rust/sdk_acceptance.rs"], fixture.0.join("target/debug/deps/sdk"));
    assert!(compiled_harnesses(&fixture.0, &messages(&[sdk.clone()])).is_err());
    let mut not_test = library.clone(); not_test["profile"]["test"] = json!(false);
    assert!(compiled_harnesses(&fixture.0, &messages(&[not_test, done.clone()])).is_err());
    let mut duplicate = library.clone(); duplicate["executable"] = sdk["executable"].clone();
    assert!(compiled_harnesses(&fixture.0, &messages(&[library.clone(), duplicate, done.clone()])).is_err());
    let mut outside = sdk.clone(); outside["executable"] = json!(fixture.0.join("../outside"));
    assert!(compiled_harnesses(&fixture.0, &messages(&[outside, done.clone()])).is_err());
    let mut unowned = library.clone(); unowned["manifest_path"] = json!(fixture.0.join("dependency/Cargo.toml"));
    assert!(compiled_harnesses(&fixture.0, &messages(&[unowned, done.clone()])).is_err());
    let mut dependency = library.clone(); dependency["profile"]["test"] = json!(false);
    dependency["manifest_path"] = json!(fixture.0.join("dependency/Cargo.toml"));
    dependency["executable"] = serde_json::Value::Null;
    assert_eq!(compiled_harnesses(&fixture.0, &messages(&[dependency, sdk.clone(), done.clone()])).unwrap().len(), 1);
    assert!(compiled_harnesses(&fixture.0, &messages(&[library, json!({"reason":"build-finished","success":false})])).is_err());
}
