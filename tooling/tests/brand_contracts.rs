use numera_task::{context::sha, distribution};
use std::{fs, path::Path};

/// Cargo resolves this package's fields, so unrelated TOML tables cannot satisfy the checks.
#[test]
fn first_party_license_and_rust_minimum_match_the_distribution_notice() {
    assert_eq!(env!("CARGO_PKG_NAME"), "numera-task");
    assert_eq!(env!("CARGO_PKG_LICENSE"), "GPL-3.0-or-later");
    assert_eq!(env!("CARGO_PKG_RUST_VERSION"), "1.100.0");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let toolchain = fs::read_to_string(root.join("rust-toolchain.toml")).unwrap();
    assert!(
        toolchain
            .lines()
            .any(|line| line == "channel = \"1.100.0\"")
    );
    let readme = fs::read_to_string(root.join("README.md")).unwrap();
    for notice in [
        "License GPL-3.0-or-later",
        "Rust 1.100.0",
        "either version 3 of the License, or (at your option) any later version.",
    ] {
        assert!(readme.contains(notice));
    }
    assert_eq!(
        sha(&fs::read(root.join("LICENSE")).unwrap()),
        "3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986"
    );
}

#[test]
fn current_distribution_uses_new_identity_without_replacing_publisher_or_legacy_provenance() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let cargo = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("name = \"numera-gen-mcp\""));
    assert!(!cargo.contains("name = \"numera-image-gen-mcp\""));
    assert!(cargo.contains("https://github.com/KiaroSama/Numera-Gen-MCP"));
    assert_eq!(numera_gen_mcp::build_identity::PUBLISHER, "KiaroSama");
    assert_eq!(numera_gen_mcp::build_identity::PRODUCT, "Numera Gen MCP");
    let identity = numera_task::signing::public_identity().unwrap();
    assert_eq!(identity["product"], "Numera Gen MCP");
    let skill = fs::read_to_string(root.join("skills/numera-gen/SKILL.md")).unwrap();
    assert!(skill.starts_with("---\nname: numera-gen\n"));
    assert!(!root.join("skills/numera-image-gen").try_exists().unwrap());
    for contract in [
        "generate_image",
        "edit_image",
        "generate_media",
        "get_media_output_info",
        "outcome_unknown",
        "request_id",
        "outputTypes",
        "modes",
        "authorization",
    ] {
        assert!(
            skill.contains(contract),
            "missing shipped workflow contract: {contract}"
        );
    }
    assert_eq!(
        sha(include_bytes!("../../docs/signing/publisher-key.der")),
        "31ccb588ae7e529535dcccd94f06d8d39ae1b8847b7003e884ad8f44cd340c80"
    );
    assert_eq!(
        sha(include_bytes!("../../docs/signing/publisher-cert.der")),
        "657dd7869e90e2bf9a49942d5098e1e3a82e22cf84add51aa958e3dfd0af937f"
    );
    for target in [
        "x86_64-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
    ] {
        assert_eq!(
            distribution::prefix(target, false).unwrap(),
            format!("numera-gen-mcp-1.0.0-{target}")
        );
    }
    assert_eq!(
        numera_task::fixtures::BASELINE,
        "fc7da25c3b470199949b1b4c67127d77afa0c34a"
    );
    assert_eq!(
        numera_task::fixtures::SOURCE,
        "f2535577c2e3145da03e2d5c123d7de061656860"
    );
    assert!(distribution::ASSETS.contains(&"schemas/media-request.schema.json"));
    assert!(distribution::ASSETS.contains(&"examples/fal.json"));
}

/// The Windows executable icon is the approved logo: every ICO image is a PNG with the logo's
/// pixels at that size, and the actual built server carries those bytes as RT_ICON resources.
#[test]
fn approved_logo_is_the_windows_executable_icon() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let ico = fs::read(root.join("docs/assets/logo.ico")).unwrap();
    let word = |at: usize| u16::from_le_bytes([ico[at], ico[at + 1]]);
    let dword = |at: usize| u32::from_le_bytes(ico[at..at + 4].try_into().unwrap()) as usize;
    assert_eq!((word(0), word(2)), (0, 1));
    let count = usize::from(word(4));
    assert!(count >= 5, "icon needs the standard Explorer sizes");
    let images: Vec<&[u8]> = (0..count)
        .map(|index| {
            let entry = 6 + index * 16;
            &ico[dword(entry + 12)..dword(entry + 12) + dword(entry + 8)]
        })
        .collect();
    assert!(
        images
            .iter()
            .all(|image| image.starts_with(b"\x89PNG\r\n\x1a\n"))
    );
    let build = fs::read_to_string(root.join("build.rs")).unwrap();
    assert!(build.contains("docs/assets/logo.ico"));
    assert!(build.contains("cargo:rustc-link-arg-bins="));
    let executable = root.join("target/debug/numera-gen-mcp.exe");
    if cfg!(all(windows, target_env = "msvc")) && executable.is_file() {
        let binary = fs::read(executable).unwrap();
        for image in &images {
            assert!(
                binary.windows(image.len()).any(|window| window == *image),
                "built executable lacks an embedded logo icon image"
            );
        }
    }
}
