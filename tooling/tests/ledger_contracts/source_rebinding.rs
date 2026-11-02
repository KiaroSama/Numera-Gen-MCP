use numera_task::context::sha;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

#[test]
fn retained_original_collections_and_final_reference_bind_all470_historical_rows_without_rewriting_evidence()
 {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let (_, rows) = numera_task::ledger::load(&project.join("docs/testing/native-cases")).unwrap();
    let original = rows.clone();
    let mut reference_cases = None;
    for (label, platform, _, _) in numera_task::ledger_inputs::COLLECTIONS {
        let bytes =
            fs::read(project.join(format!("tests/fixtures/oracle/{label}/source-cases.json")))
                .expect(
                    "Exact retained collection fixture must be committed; no generated substitute",
                );
        let collection = numera_task::ledger_inputs::validate_collection(&bytes, platform).unwrap();
        assert_eq!(collection["cases"].as_array().unwrap().len(), 470);
        assert_eq!(collection["suites"].as_array().unwrap().len(), 50);
        let ids = collection["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap().to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(expected) = &reference_cases {
            assert_eq!(&ids, expected);
        } else {
            reference_cases = Some(ids);
        }
        let independent = collection["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v["id"].as_str().unwrap(), v))
            .collect::<BTreeMap<_, _>>();
        for row in &rows {
            let case = independent[row["id"].as_str().unwrap()];
            for key in ["source", "source_sha256", "name", "location"] {
                assert_eq!(row[key], case[key]);
            }
            assert_eq!(row["platform_modes"][platform], case["mode"]);
        }
        let mut corrupted = bytes;
        corrupted.push(b' ');
        assert!(numera_task::ledger_inputs::validate_collection(&corrupted, platform).is_err());
    }
    let bytes = fs::read(project.join(numera_task::ledger_inputs::REFERENCE_FILE))
        .expect("Exact retained historical reference fixture must be committed");
    let receipt = numera_task::ledger_inputs::validate_reference(&bytes).unwrap();
    assert_eq!(receipt["source_sha"], numera_task::fixtures::SOURCE);
    let report = numera_task::ledger::validate_reference_binding(
        &rows,
        &bytes,
        numera_task::ledger_inputs::REFERENCE_SHA256,
    )
    .unwrap();
    assert_eq!(report["bound_cases"], 470);
    assert_eq!(report["current_execution_claimed"], false);
    assert_eq!(rows, original);
    let mut changed = rows.clone();
    changed[0]["assertions"][0]["claim"] = json!("silently relabelled original assertion");
    assert!(
        numera_task::ledger::validate_reference_binding(
            &changed,
            &bytes,
            numera_task::ledger_inputs::REFERENCE_SHA256
        )
        .is_err()
    );
}

fn source_tar(entries: &[(&str, &[u8], tar::EntryType)], padding: usize) -> Vec<u8> {
    let mut writer = tar::Builder::new(Vec::new());
    for (path, bytes, kind) in entries {
        let mut header = tar::Header::new_ustar();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_entry_type(*kind);
        if kind.is_symlink() || kind.is_hard_link() {
            header.set_link_name("outside").unwrap();
        }
        header.set_cksum();
        writer.append_data(&mut header, path, *bytes).unwrap();
    }
    let mut bytes = writer.into_inner().unwrap();
    bytes.resize(bytes.len() + padding, 0);
    bytes
}

#[test]
fn bounded_git_archive_decoder_requires_exact_independent_regular_bytes_and_rejects_links_duplicates_and_extras()
 {
    let expected = BTreeMap::from([(
        "tests/unit/original.test.ts".into(),
        sha(b"independent original Unicode \xd8\xaa\xd8\xb5\xd9\x88\xdb\x8c\xd8\xb1\n"),
    )]);
    let source: &[u8] = b"independent original Unicode \xd8\xaa\xd8\xb5\xd9\x88\xdb\x8c\xd8\xb1\n";
    let path = "tests/unit/original.test.ts";
    let good = source_tar(&[(path, source, tar::EntryType::Regular)], 8192);
    let decoded = numera_task::ledger_inputs::decode_archive(&good, &expected).unwrap();
    assert_eq!(decoded[path].as_slice(), source);
    for archive in [
        source_tar(&[(path, b"changed", tar::EntryType::Regular)], 0),
        source_tar(
            &[
                (path, source, tar::EntryType::Regular),
                (path, source, tar::EntryType::Regular),
            ],
            0,
        ),
        source_tar(
            &[
                (path, source, tar::EntryType::Regular),
                (
                    "tests/unit/unrequested.test.ts",
                    b"extra",
                    tar::EntryType::Regular,
                ),
            ],
            0,
        ),
        source_tar(&[(path, b"", tar::EntryType::Symlink)], 0),
        source_tar(&[(path, b"", tar::EntryType::Link)], 0),
        source_tar(
            &[
                ("unrequested", b"", tar::EntryType::Directory),
                (path, source, tar::EntryType::Regular),
            ],
            0,
        ),
        source_tar(&[(path, source, tar::EntryType::Regular)], 11_000),
    ] {
        assert!(numera_task::ledger_inputs::decode_archive(&archive, &expected).is_err());
    }
    for key in [
        "../outside",
        "/absolute",
        "tests//empty",
        "tests/./dot",
        "tests\\backslash",
        "tests/stream:ads",
    ] {
        assert!(
            numera_task::ledger_inputs::decode_archive(
                &good,
                &BTreeMap::from([(key.into(), sha(source))])
            )
            .is_err()
        );
    }
    let mut changed = good.clone();
    changed.push(1);
    assert!(numera_task::ledger_inputs::decode_archive(&changed, &expected).is_err());
    let mut truncated = good;
    truncated.truncate(1024);
    assert!(numera_task::ledger_inputs::decode_archive(&truncated, &expected).is_err());
    truncated.truncate(520);
    assert!(numera_task::ledger_inputs::decode_archive(&truncated, &expected).is_err());
    assert!(
        numera_task::ledger_inputs::decode_archive(&vec![0; 16 * 1024 * 1024 + 1], &expected)
            .is_err()
    );
}

#[test]
fn assertion_ranges_follow_unique_whole_lines_and_never_guess_changed_or_duplicate_source() {
    use numera_task::ledger_inputs::assertion_range_proposal;
    let historical = "// original\n#[test]\nfn preserves_original() {\n    assert_eq!(\"تصویر\", \"تصویر\");\n}\n";
    let current = format!("// added\n{historical}");
    let proposal = assertion_range_proposal(historical, &current, &json!([2, 5])).unwrap();
    assert_eq!(proposal["range_match"], "exact_lines");
    assert_eq!(proposal["native_lines"], json!([3, 6]));
    assert_eq!(proposal["assertion_review"], "pending");
    assert_eq!(proposal["current_execution_claimed"], false);
    assert_eq!(
        proposal["historical_excerpt_sha256"],
        proposal["current_excerpt_sha256"]
    );
    let renamed = assertion_range_proposal(
        "use numera_image_gen_mcp::jobs;\n",
        "// new\nuse numera_gen_mcp::jobs;\n",
        &json!([1, 1]),
    )
    .unwrap();
    assert_eq!(renamed["range_match"], "crate_namespace_only");
    assert_eq!(renamed["native_lines"], json!([2, 2]));
    assert_ne!(
        renamed["historical_excerpt_sha256"],
        renamed["current_excerpt_sha256"]
    );
    for (candidate, expected) in [
        (format!("{historical}{historical}"), "ambiguous"),
        (historical.replace("assert_eq!", "assert_ne!"), "changed"),
        (
            "fn preserves_original() {\n    assert_eq!(\"تصویر\", \"تصویر\");\n}".into(),
            "changed",
        ),
    ] {
        let result = assertion_range_proposal(historical, &candidate, &json!([2, 5])).unwrap();
        assert_eq!(result["range_match"], expected);
        assert!(result["native_lines"].is_null());
    }
    assert_eq!(
        assertion_range_proposal("assert!(ok);", "prefix assert!(ok); suffix", &json!([1, 1]))
            .unwrap()["range_match"],
        "changed"
    );
    assert_eq!(
        assertion_range_proposal(
            "assert!(ok);\r\n",
            "// new\r\nassert!(ok);\r\n",
            &json!([1, 1])
        )
        .unwrap()["native_lines"],
        json!([2, 2])
    );
    for invalid in [
        json!([]),
        json!([0, 1]),
        json!([2, 1]),
        json!([1, 99]),
        json!([1]),
        json!([1, 1, 1]),
        json!([1.5, 2]),
    ] {
        assert!(assertion_range_proposal(historical, &current, &invalid).is_err());
    }
    assert!(assertion_range_proposal("\n", "\n", &json!([1, 1])).is_err());
}

#[test]
fn sdk_assertion_traceability_requires_every_original_obligation_and_exact_current_source_without_approval()
 {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let (_, rows) = numera_task::ledger::load(&project.join("docs/testing/native-cases")).unwrap();
    let manifest: Value = serde_json::from_slice(
        &fs::read(project.join("docs/testing/sdk-tooling-migration.json")).unwrap(),
    )
    .unwrap();
    let check = |value: &Value| {
        numera_task::ledger_inputs::sdk_assertion_traceability(project, &rows, value)
    };
    let report = check(&manifest).unwrap();
    assert_eq!(report["unique_obligations"], 26);
    assert_eq!(report["original_records"], 82);
    assert_eq!(report["semantic_parity_claimed"], false);
    assert_eq!(report["current_execution_claimed"], false);
    assert!(report["current_source_files"]["tests/rust/sdk_acceptance/fixtures.rs"].is_string());
    for mutation in [
        "missing",
        "duplicate",
        "extra",
        "historical",
        "mapping",
        "range",
        "hash",
        "module",
        "duplicate-range",
        "approval",
        "execution",
    ] {
        let mut changed = manifest.clone();
        let review = &mut changed["assertion_traceability"];
        match mutation {
            "missing" => {
                review["bindings"].as_array_mut().unwrap().pop();
            }
            "duplicate" => review["bindings"][25] = review["bindings"][0].clone(),
            "extra" => {
                let extra = review["bindings"][0].clone();
                review["bindings"].as_array_mut().unwrap().push(extra);
            }
            "historical" => {
                review["bindings"][0]["historical_assertion_sha256"] = json!("0".repeat(64))
            }
            "mapping" => {
                review["bindings"][0]["current_mapping"] = json!(
                    "tests/rust/sdk_acceptance.rs::native_sdk_all_ten_tools_and_exact_shared_two_host_request"
                )
            }
            "range" => review["bindings"][0]["ranges"][0]["lines"] = json!([0, 1]),
            "hash" => review["bindings"][0]["ranges"][0]["excerpt_sha256"] = json!("0".repeat(64)),
            "module" => {
                let path = "tests/rust/sdk_acceptance/catalog.rs";
                let source = fs::read_to_string(project.join(path)).unwrap();
                let excerpt = source
                    .lines()
                    .skip(26)
                    .take(16)
                    .collect::<Vec<_>>()
                    .join("\n");
                review["bindings"][0]["ranges"][0]["file"] = json!(path);
                review["bindings"][0]["ranges"][0]["excerpt_sha256"] =
                    json!(sha(excerpt.as_bytes()));
            }
            "duplicate-range" => {
                let range = review["bindings"][0]["ranges"][0].clone();
                review["bindings"][0]["ranges"]
                    .as_array_mut()
                    .unwrap()
                    .push(range);
            }
            "approval" => review["semantic_parity_claimed"] = json!(true),
            "execution" => review["current_execution_claimed"] = json!(true),
            _ => unreachable!(),
        }
        assert!(check(&changed).is_err(), "{mutation}");
    }
    let mut changed = rows.clone();
    let assertion = changed
        .iter_mut()
        .flat_map(|row| row["assertions"].as_array_mut().unwrap())
        .find(|assertion| assertion["native_file"].as_str().unwrap().ends_with(".mjs"))
        .unwrap();
    assertion["claim"] = json!("changed historical obligation");
    assert!(
        numera_task::ledger_inputs::sdk_assertion_traceability(project, &changed, &manifest)
            .is_err()
    );
}

#[test]
fn separate_current_source_proposals_preserve_every_historical_row_and_never_copy_old_assertion_ranges()
 {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let (_, rows) = numera_task::ledger::load(&project.join("docs/testing/native-cases")).unwrap();
    let bytes = fs::read(project.join(numera_task::ledger_inputs::REFERENCE_FILE)).unwrap();
    let reference = numera_task::ledger_inputs::validate_reference(&bytes).unwrap();
    let map =
        numera_task::ledger_inputs::current_rebind_map(project, &rows, &reference, &"a".repeat(40))
            .unwrap();
    assert_eq!(map["current_execution_claimed"], false);
    assert_eq!(map["acceptance_verified"], false);
    assert_eq!(map["original_evidence_rewritten"], false);
    let cases = map["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 470);
    for (row, case) in rows.iter().zip(cases) {
        assert_eq!(case["historical"], *row);
        assert_eq!(case["current"]["status"], "pending");
        assert!(case["current"]["assertions"].is_null());
        for proposal in case["current"]["proposed_native"].as_array().unwrap() {
            assert!(
                proposal["proposed_mapping"]
                    .as_str()
                    .unwrap()
                    .split_once("::")
                    .unwrap()
                    .0
                    .ends_with(".rs")
            );
            assert_eq!(proposal["assertion_ranges_rebound"], false);
            assert_eq!(proposal["assertion_review"], "pending");
            assert_eq!(proposal["current_execution_claimed"], false);
            assert_eq!(proposal["current_sha256"].as_str().unwrap().len(), 64);
        }
    }
    assert_eq!(
        rows,
        cases
            .iter()
            .map(|v| v["historical"].clone())
            .collect::<Vec<_>>()
    );
    assert!(map["current_source_files"]["tests/rust/sdk_acceptance/host.rs"].is_string());
    let manifest = fs::read(project.join("docs/testing/sdk-tooling-migration.json")).unwrap();
    assert_eq!(
        map["current_source_files"]["docs/testing/sdk-tooling-migration.json"],
        sha(&manifest)
    );
    assert_eq!(map["sdk_assertion_traceability"]["unique_obligations"], 26);
    assert_eq!(map["sdk_assertion_traceability"]["original_records"], 82);
    assert_eq!(
        map["sdk_assertion_traceability"]["semantic_parity_claimed"],
        false
    );
    let mut changed = rows;
    changed[0]["assertions"][0]["claim"] = json!("unapproved current assertion relabelling");
    assert!(
        numera_task::ledger_inputs::current_rebind_map(
            project,
            &changed,
            &reference,
            &"a".repeat(40)
        )
        .is_err()
    );
}
