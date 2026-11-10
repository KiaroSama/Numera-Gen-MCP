mod common;
use common::Workspace;
use numera_gen_mcp::{devtools::supervisor::{Bounds, run}, logging::Logger};
use std::{ffi::OsString, path::Path, process::Command, sync::atomic::AtomicBool};

fn invoke(root: &Path, args: &[OsString]) -> numera_gen_mcp::devtools::supervisor::Outcome {
    let mut command = Command::new(env!("CARGO_BIN_EXE_numera-dev"));
    command.current_dir(root).env_clear().args(args);
    for key in ["PATH", "SystemRoot", "WINDIR", "USERPROFILE", "LLVM_PROFILE_FILE"] {
        if let Some(value) = std::env::var_os(key) { command.env(key, value); }
    }
    let logger = Logger::new(&root.join("captures"), "ERROR");
    let result = run(&mut command, root, Bounds::new(5000, 2000).unwrap(), &AtomicBool::new(false), &logger).unwrap();
    assert!(result.cleanup_verified && result.reason.is_none());
    result
}

#[test]
fn native_private_commands_reject_invalid_arguments_before_input_or_custody_mutation() {
    let workspace = Workspace::new();
    let root = workspace.0.canonicalize().unwrap();
    for args in [
        vec!["protect-secret", "--path", "relative.txt", "--dry-run"],
        vec!["protect-secret", "--path", "relative.txt", "--path", "other.txt"],
        vec!["protect-secret", "--unknown"],
        vec!["identity", "inspect", "--root", "relative"],
    ] {
        let args = args.into_iter().map(OsString::from).collect::<Vec<_>>();
        assert_ne!(invoke(&root, &args).code, 0);
        assert!(!root.join(".runtime").exists());
        assert!(!root.join("logs").exists());
    }
    let args = vec![OsString::from("identity"), OsString::from("inspect"), OsString::from("--root"), root.clone().into_os_string(), OsString::from("--root"), root.clone().into_os_string()];
    assert_ne!(invoke(&root, &args).code, 0);
    assert!(!root.join(".runtime").exists());
    #[cfg(windows)]
    {
        let registry = root.join("secrets.md");
        let original = b"Synthetic disposable registry fixture; no credential values.\n";
        numera_gen_mcp::devtools::private_files::write_new(&registry, original).unwrap();
        let args = vec![OsString::from("identity"), OsString::from("create"), OsString::from("--root"), root.clone().into_os_string(), OsString::from("--dry-run")];
        assert_eq!(invoke(&root, &args).code, 0);
        assert_eq!(std::fs::read(registry).unwrap(), original);
        assert!(!root.join(".runtime").exists());
        assert!(!root.join("logs").exists());
    }
}

#[cfg(windows)]
#[test]
fn native_protection_cli_whatif_preserves_outside_destination_and_existing_file_without_input() {
    use numera_gen_mcp::devtools::private_files;
    let workspace = Workspace::new();
    let root = workspace.0.canonicalize().unwrap();
    let outside = Workspace::new();
    let private = outside.0.canonicalize().unwrap().join("private");
    private_files::create_directory(&private).unwrap();
    let file = private.join("protected 日本語.txt");
    for option in ["--dry-run", "--what-if", "-WhatIf"] {
        let args = vec![OsString::from("protect-secret"), OsString::from("--path"), file.clone().into_os_string(), OsString::from(option)];
        assert_eq!(invoke(&root, &args).code, 0);
        assert!(!file.exists());
        assert!(!root.join("logs").exists());
    }
    private_files::write_new(&file, b"existing bytes").unwrap();
    let args = vec![OsString::from("protect-secret"), OsString::from("--path"), file.clone().into_os_string()];
    assert_ne!(invoke(&root, &args).code, 0);
    assert_eq!(std::fs::read(&file).unwrap(), b"existing bytes");
    assert!(!root.join("logs").exists());
}
