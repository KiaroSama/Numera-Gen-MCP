use numera_task::{
    signature_acceptance::{binary_members, require_rejection},
    signing_tools::{TOOL_MEMBERS, archive_entries, openssl3},
};
use serde_json::json;
use std::io::{Cursor, Write};

#[test]
fn only_completed_negative_commands_count_as_signature_rejections() {
    require_rejection(1, None, true).unwrap();
    for (code, reason, cleanup) in [
        (0, None, true),
        (124, Some("wall_timeout"), true),
        (124, Some("idle_timeout"), true),
        (1, None, false),
        (-1, Some("spawn_error"), true),
    ] {
        assert!(require_rejection(code, reason, cleanup).is_err());
    }
}

#[test]
fn acceptance_requires_both_distinct_native_executables_on_every_supported_target() {
    for target in [
        "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin", "aarch64-apple-darwin",
    ] {
        let suffix = if target.contains("windows") { ".exe" } else { "" };
        let names = [format!("bin/numera-gen-mcp{suffix}"), format!("bin/numera-dev{suffix}")];
        let manifest = json!({"files":[{"path":names[0]},{"path":names[1]},{"path":"README.md"}]});
        assert_eq!(binary_members(&manifest, target).unwrap(), names);
        for files in [json!([]), json!([{"path":names[0]}]),
            json!([{"path":names[0]},{"path":names[0]}]),
            json!([{"path":names[0]},{"path":names[1]},{"path":"bin/unlisted"}]),
            json!([{"path":names[0]},{"path":names[1]},{}])] {
            assert!(binary_members(&json!({"files":files}), target).is_err());
        }
    }
    assert!(binary_members(&json!({"files":[]}), "unrelated-target").is_err());
}

#[test]
fn preinstalled_openssl_requires_a_real_version_three_banner() {
    for version in ["OpenSSL 3.0.0 7 Sep 2021\n", "OpenSSL 3.5.4 30 Sep 2025\n"] {
        assert!(openssl3(version.as_bytes()));
    }
    for version in ["LibreSSL 3.3.6", "OpenSSL 1.1.1", "OpenSSL 30.1.0", "OpenSSL 3.1", "OpenSSL 3.a.1", "OpenSSL 3.1.0evil", "prefix OpenSSL 3.1.0"] {
        assert!(!openssl3(version.as_bytes()));
    }
    assert!(!openssl3(&[0xff]));
}

fn archive(names: &[&str]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in names {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated).unix_permissions(0o644)).unwrap();
        writer.write_all(b"tiny synthetic tool fixture; never executed").unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn signing_tool_archive_accepts_only_exact_regular_bounded_allowlisted_members() {
    let bytes = archive(TOOL_MEMBERS);
    let entries = archive_entries(&bytes).unwrap();
    assert_eq!(entries.len(), 7);
    assert_eq!(entries["bin/osslsigncode.exe"], b"tiny synthetic tool fixture; never executed");
    for names in [
        vec!["bin/osslsigncode.exe"],
        vec!["../escape"],
        vec!["bin/osslsigncode.exe", "bin/OSSLSIGNCODE.exe"],
    ] {
        assert!(archive_entries(&archive(&names)).is_err());
    }
    let mut wrong_case = TOOL_MEMBERS.to_vec();
    wrong_case[0] = "bin/OSSLSIGNCODE.exe";
    assert!(archive_entries(&archive(&wrong_case)).is_err());
    assert!(archive_entries(b"not a ZIP").is_err());
    let mut oversized = bytes;
    oversized.resize(4 * 1024 * 1024 + 1, 0);
    assert!(archive_entries(&oversized).is_err());
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in TOOL_MEMBERS {
        if *name == "bin/osslsigncode.exe" {
            writer.add_symlink(*name, "elsewhere", zip::write::SimpleFileOptions::default()).unwrap();
        } else {
            writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(b"fixture").unwrap();
        }
    }
    assert!(archive_entries(&writer.finish().unwrap().into_inner()).is_err());
}
