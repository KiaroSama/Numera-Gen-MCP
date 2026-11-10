use numera_task::{context::sha, identity_tools};
use std::io::{Cursor, Write};

const MEMBER: &str = "x64/bin/libcrypto-3-x64.dll";

fn image(machine: u16) -> Vec<u8> {
    let mut bytes = vec![0u8; 128];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&machine.to_le_bytes());
    bytes[86..88].copy_from_slice(&0x2000u16.to_le_bytes());
    bytes[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
    bytes
}

fn archive(member: &str, bytes: &[u8], linked: bool, duplicate: bool) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    if linked {
        writer.add_symlink(member, "elsewhere", options).unwrap();
    } else {
        writer.start_file(member, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    if duplicate {
        let other = member.replace("libcrypto", "duplicate");
        assert_eq!(other.len(), member.len());
        writer.start_file(&other, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer
        .start_file("unused/never-extracted.txt", options)
        .unwrap();
    writer.write_all(b"unrelated archive member").unwrap();
    let mut result = writer.finish().unwrap().into_inner();
    if duplicate {
        let other = member.replace("libcrypto", "duplicate");
        let mut replaced = 0;
        for offset in 0..=result.len() - other.len() {
            if &result[offset..offset + other.len()] == other.as_bytes() {
                result[offset..offset + other.len()].copy_from_slice(member.as_bytes());
                replaced += 1;
            }
        }
        assert_eq!(
            replaced, 2,
            "both ZIP headers must contain the duplicate name"
        );
    }
    result
}

#[test]
fn identity_library_selection_returns_only_exact_native_regular_bytes() {
    for (member, machine) in [
        (MEMBER, 0x8664),
        ("arm64/bin/libcrypto-3-arm64.dll", 0xaa64),
    ] {
        let bytes = image(machine);
        let zip = archive(member, &bytes, false, false);
        assert_eq!(
            identity_tools::select_member(&zip, member, bytes.len(), &sha(&bytes), machine)
                .unwrap(),
            bytes
        );
    }
}

#[test]
fn identity_library_selection_rejects_size_digest_architecture_duplicate_and_link_changes() {
    let bytes = image(0x8664);
    let zip = archive(MEMBER, &bytes, false, false);
    for (case, size, digest, machine) in [
        ("smaller-size", bytes.len() - 1, sha(&bytes), 0x8664),
        ("larger-size", bytes.len() + 1, sha(&bytes), 0x8664),
        ("digest", bytes.len(), "0".repeat(64), 0x8664),
        ("architecture", bytes.len(), sha(&bytes), 0xaa64),
    ] {
        assert!(
            identity_tools::select_member(&zip, MEMBER, size, &digest, machine).is_err(),
            "{case}"
        );
    }
    let duplicate = archive(MEMBER, &bytes, false, true);
    let metadata = zip::ZipArchive::new(Cursor::new(&duplicate)).unwrap();
    assert!(metadata.has_duplicate_names());
    assert_eq!(
        metadata.len(),
        2,
        "indexed names collapse three raw entries"
    );
    drop(metadata);
    for (case, zip) in [
        ("linked", archive(MEMBER, &bytes, true, false)),
        ("duplicate", duplicate),
        (
            "missing",
            archive("wrong/bin/libcrypto-3-x64.dll", &bytes, false, false),
        ),
        ("truncated", zip[..zip.len() / 2].to_vec()),
    ] {
        assert!(
            identity_tools::select_member(&zip, MEMBER, bytes.len(), &sha(&bytes), 0x8664).is_err(),
            "{case}"
        );
    }
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in [MEMBER.to_owned(), MEMBER.to_ascii_uppercase()] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    let alias = writer.finish().unwrap().into_inner();
    assert!(
        identity_tools::select_member(&alias, MEMBER, bytes.len(), &sha(&bytes), 0x8664).is_err()
    );
    let mut wrong = bytes.clone();
    wrong[86..88].fill(0);
    assert!(
        identity_tools::select_member(
            &archive(MEMBER, &wrong, false, false),
            MEMBER,
            wrong.len(),
            &sha(&wrong),
            0x8664
        )
        .is_err()
    );
}

#[tokio::test]
async fn identity_download_rejects_other_paths_origins_ports_and_credentials_before_network() {
    use numera_task::fetch::{Limits, bounded_get};
    let limits = Limits {
        bytes: 1024,
        wall: std::time::Duration::from_millis(10),
        idle: std::time::Duration::from_millis(10),
    };
    for url in [
        "https://download.firedaemon.com/other.zip",
        "https://download.firedaemon.com:444/FireDaemon-OpenSSL/openssl-3.5.9.zip",
        "http://download.firedaemon.com/FireDaemon-OpenSSL/openssl-3.5.9.zip",
        "https://download.firedaemon.com/FireDaemon-OpenSSL/openssl-3.5.9.zip?download=1",
        "https://user:password@download.firedaemon.com/FireDaemon-OpenSSL/openssl-3.5.9.zip",
    ] {
        let error = bounded_get(url, limits).await.unwrap_err();
        assert_eq!(
            error.to_string(),
            "Official download destination is not allowlisted."
        );
    }
}
