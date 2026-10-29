use numera_task::{context::sha, validation::{self, Prepared}};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture() -> BTreeMap<String, Vec<u8>> {
    let package = json!({"private":true,"dependencies":{"zod":"4.6.5","undici":"8.11.2","unchanged":"1.0.0"}});
    let lock = json!({"lockfileVersion":3,"packages":{
        "":package,
        "node_modules/zod":{"version":"4.6.5","resolved":"original-zod","integrity":"original","license":"MIT"},
        "node_modules/undici":{"version":"8.11.2","resolved":"original-undici","integrity":"original-undici","license":"MIT","engines":{"node":">=22.19.0"}},
        "node_modules/unchanged":{"version":"1.0.0","integrity":"same"}
    }});
    BTreeMap::from([
        ("package.json".into(), serde_json::to_vec(&package).unwrap()),
        ("package-lock.json".into(), serde_json::to_vec(&lock).unwrap()),
        ("scripts/bounded.mjs".into(), b"original supervisor\n".to_vec()),
        ("tests/integration/setup.test.ts".into(), format!("const fixture = `{}`;\nexpect(result.code, result.stderr).toBe(0);\n", validation::SYNC_FIXTURE).into_bytes()),
        ("tests/exact.test.ts".into(), "const label = \"تصویر\";\n".as_bytes().to_vec()),
        ("src/exact.ts".into(), b"export const fixture = 1;\n".to_vec()),
    ])
}
fn parse(prepared: &Prepared, name: &str) -> Value {
    serde_json::from_slice(&prepared.files[name]).unwrap()
}

#[test]
fn historical_validation_changes_only_reviewed_dependencies_supervisor_and_child_fixture() {
    let original = fixture(); let preserved = original.clone();
    let prepared = validation::prepare(&original, b"maintained supervisor\n").unwrap();
    assert_eq!(original, preserved);
    assert_eq!(prepared.files["tests/exact.test.ts"], original["tests/exact.test.ts"]);
    assert_eq!(prepared.files["src/exact.ts"], original["src/exact.ts"]);
    assert_eq!(prepared.files["scripts/bounded.mjs"], b"maintained supervisor\n");
    let package = parse(&prepared, "package.json");
    assert_eq!(package["dependencies"], json!({"zod":"4.6.7","undici":"8.12.3","unchanged":"1.0.0"}));
    let lock = parse(&prepared, "package-lock.json");
    assert_eq!(lock["packages"]["node_modules/undici"]["version"], "8.12.3");
    assert_eq!(lock["packages"]["node_modules/undici"]["engines"], json!({"node":">=22.19.0"}));
    assert_eq!(lock["packages"]["node_modules/zod"]["license"], "MIT");
    assert_eq!(lock["packages"]["node_modules/unchanged"], json!({"version":"1.0.0","integrity":"same"}));
    assert_eq!(lock["packages"]["node_modules/zod"]["integrity"], "sha512-g8W2D2kU6a8untc053fX1uVBKzM7yqAxW/3zwk83+6RyF2+SKOuV+c0wBDhUwzV1PaGapmZH+rmcUzPf+3CGwg==");
    assert_eq!(lock["packages"]["node_modules/undici"]["integrity"], "sha512-+fdndUTwBP86cVjihTp21hZHo7d6oj1c/47XPA3lL8mM0XHs+C4EkcjmgRsFnFk2f/UONGDy9ItBItmXsLOQHw==");
    let after = std::str::from_utf8(&prepared.files["tests/integration/setup.test.ts"]).unwrap();
    assert_eq!(after.replace(validation::STREAM_FIXTURE, validation::SYNC_FIXTURE).as_bytes(), original["tests/integration/setup.test.ts"]);
    assert!(after.contains("setTimeout(()=>child.kill('SIGKILL'),10000)"));
    assert_eq!(after.matches("clearTimeout(timer)").count(), 2);
    assert!(!after.contains("spawnSync"));
    assert!(after.contains("child.stdout.pipe(process.stdout);child.stderr.pipe(process.stderr)"));
    assert_eq!(prepared.receipt["changes"].as_object().unwrap().len(), 4);
    assert_eq!(prepared.receipt["unchanged_tests"]["tests/exact.test.ts"], sha(&original["tests/exact.test.ts"]));
    assert!(prepared.receipt["unchanged_tests"].get("tests/integration/setup.test.ts").is_none());
    assert_eq!(prepared.receipt["immutable_oracle_modified"], false);
    assert_eq!(prepared.receipt["interpreted_execution"], false);
    validation::verify(&original, b"maintained supervisor\n", &prepared).unwrap();
}

#[test]
fn validation_refuses_unreviewed_source_paths_versions_fixture_and_any_later_byte_drift() {
    let original = fixture();
    for change in ["traversal", "duplicate-spelling", "zod", "undici", "fixture", "missing", "unchanged-supervisor"] {
        let mut changed = original.clone();
        match change {
            "traversal" => { changed.insert("../outside".into(), vec![1]); },
            "duplicate-spelling" => { changed.insert("./src/exact.ts".into(), vec![1]); },
            "zod" | "undici" => {
                let mut package: Value = serde_json::from_slice(&changed["package.json"]).unwrap();
                package["dependencies"][change] = json!("unexpected");
                changed.insert("package.json".into(), serde_json::to_vec(&package).unwrap());
            },
            "fixture" => { changed.insert("tests/integration/setup.test.ts".into(), b"unreviewed child fixture".to_vec()); },
            "missing" => { changed.remove("package-lock.json"); },
            _ => {},
        }
        let supervisor = if change == "unchanged-supervisor" { original["scripts/bounded.mjs"].as_slice() } else { b"maintained supervisor\n" };
        assert!(validation::prepare(&changed, supervisor).is_err(), "{change}");
    }
    let mut prepared = validation::prepare(&original, b"maintained supervisor\n").unwrap();
    prepared.files.get_mut("tests/exact.test.ts").unwrap().push(b' ');
    assert!(validation::verify(&original, b"maintained supervisor\n", &prepared).is_err());
    let mut prepared = validation::prepare(&original, b"maintained supervisor\n").unwrap();
    prepared.receipt["immutable_oracle_modified"] = json!(true);
    assert!(validation::verify(&original, b"maintained supervisor\n", &prepared).is_err());
}
