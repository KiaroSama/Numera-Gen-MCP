use numera_task::archives::{decode, encode, validate_name};
use std::collections::BTreeMap;

#[test]
fn sorted_archive_members_round_trip_exact_bytes_in_both_formats() {
    let entries = BTreeMap::from([
        (
            "bin/numera-gen-mcp".to_owned(),
            b"synthetic; never executed".to_vec(),
        ),
        (
            "docs/東京.txt".to_owned(),
            "تصویر 日本語\n".as_bytes().to_vec(),
        ),
    ]);
    for zip in [true, false] {
        let first = encode("fixture-x86_64", &entries, zip).expect("bounded archive");
        assert_eq!(
            first,
            encode("fixture-x86_64", &entries, zip).expect("deterministic archive")
        );
        assert_eq!(
            decode(&first, "fixture-x86_64", zip).expect("regular files"),
            entries
        );
        let expected_names = [
            "fixture-x86_64/bin/numera-gen-mcp",
            "fixture-x86_64/docs/東京.txt",
        ];
        if zip {
            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&first)).unwrap();
            assert_eq!(archive.len(), expected_names.len());
            for (index, expected) in expected_names.iter().enumerate() {
                let file = archive.by_index(index).unwrap();
                assert_eq!(file.name().unwrap().into_owned(), *expected);
                assert!(file.is_file());
                assert_eq!(file.compression(), zip::CompressionMethod::Deflated);
                assert_eq!(file.last_modified(), Some(zip::DateTime::default()));
                assert_eq!(
                    file.unix_mode().unwrap() & 0o177777,
                    if index == 0 { 0o100755 } else { 0o100644 }
                );
            }
        } else {
            let decoder = flate2::read::GzDecoder::new(first.as_slice());
            let mut archive = tar::Archive::new(decoder);
            let members = archive
                .entries()
                .unwrap()
                .collect::<std::io::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(members.len(), expected_names.len());
            for (index, file) in members.iter().enumerate() {
                assert_eq!(
                    file.path().unwrap().to_str().unwrap(),
                    expected_names[index]
                );
                assert!(file.header().entry_type().is_file());
                assert_eq!(
                    file.header().mode().unwrap(),
                    if index == 0 { 0o755 } else { 0o644 }
                );
                assert_eq!(file.header().mtime().unwrap(), 0);
                assert_eq!(file.header().uid().unwrap(), 0);
                assert_eq!(file.header().gid().unwrap(), 0);
            }
        }
        assert!(decode(&first, "another", zip).is_err());
        assert!(decode(&first[..first.len() / 2], "fixture-x86_64", zip).is_err());
        if !zip {
            assert!(decode(&first[..first.len() - 1], "fixture-x86_64", false).is_err());
            let mut corrupt = first.clone();
            let at = corrupt.len() - 8;
            corrupt[at] ^= 1;
            assert!(decode(&corrupt, "fixture-x86_64", false).is_err());
            let mut trailing = first.clone();
            trailing.extend_from_slice(b"unexpected");
            assert!(decode(&trailing, "fixture-x86_64", false).is_err());
        }
    }
}

#[test]
fn member_policy_rejects_private_escape_device_and_ambiguous_names() {
    for name in [
        "",
        "../outside",
        "/absolute",
        "docs/../file",
        "docs\\file",
        "docs/CON.txt",
        "docs/file ",
        "docs/file.",
        "docs/a:b",
        "docs//file",
        "docs/secret\0.txt",
        "secrets.md",
        "docs/secrets.md",
        "docs/state/receipt",
        ".env",
        "node_modules/file",
        "manifest.sig/extra",
    ] {
        assert!(validate_name(name).is_err(), "accepted {name:?}");
    }
    for name in [
        ".env.example",
        "README.md",
        "manifest.json",
        "manifest.sig",
        "docs/assets/logo.png",
        "licenses/pkg/LICENSE",
    ] {
        validate_name(name).expect(name);
    }
    let entries = BTreeMap::from([
        ("docs/Case.txt".into(), vec![1]),
        ("docs/case.txt".into(), vec![2]),
    ]);
    assert!(encode("fixture", &entries, true).is_err());
}
