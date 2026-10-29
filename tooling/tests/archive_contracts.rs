use numera_task::archives::{decode, encode, validate_name};
use std::collections::BTreeMap;

#[test]
fn sorted_archive_members_round_trip_exact_bytes_in_both_formats() {
    let entries = BTreeMap::from([
        ("bin/numera-gen-mcp".to_owned(), b"synthetic; never executed".to_vec()),
        ("docs/example.txt".to_owned(), "تصویر 日本語\n".as_bytes().to_vec()),
    ]);
    for zip in [true, false] {
        let first = encode("fixture", &entries, zip).expect("bounded archive");
        assert_eq!(first, encode("fixture", &entries, zip).expect("deterministic archive"));
        assert_eq!(decode(&first, "fixture", zip).expect("regular files"), entries);
        assert!(decode(&first, "another", zip).is_err());
        assert!(decode(&first[..first.len() / 2], "fixture", zip).is_err());
        if !zip {
            assert!(decode(&first[..first.len()-1], "fixture", false).is_err());
            let mut corrupt = first.clone(); let at = corrupt.len()-8; corrupt[at] ^= 1;
            assert!(decode(&corrupt, "fixture", false).is_err());
            let mut trailing = first.clone(); trailing.extend_from_slice(b"unexpected");
            assert!(decode(&trailing, "fixture", false).is_err());
        }
    }
}

#[test]
fn member_policy_rejects_private_escape_device_and_ambiguous_names() {
    for name in ["", "../outside", "/absolute", "docs/../file", "docs\\file", "docs/CON.txt",
        "docs/file ", "docs/file.", "docs/a:b", "docs//file", "docs/secret\0.txt", "secrets.md",
        "docs/secrets.md", "docs/state/receipt", ".env", "node_modules/file", "manifest.sig/extra"] {
        assert!(validate_name(name).is_err(), "accepted {name:?}");
    }
    for name in [".env.example", "README.md", "manifest.json", "manifest.sig", "docs/assets/logo.png", "licenses/pkg/LICENSE"] {
        validate_name(name).expect(name);
    }
    let entries = BTreeMap::from([("docs/Case.txt".into(), vec![1]), ("docs/case.txt".into(), vec![2])]);
    assert!(encode("fixture", &entries, true).is_err());
}
