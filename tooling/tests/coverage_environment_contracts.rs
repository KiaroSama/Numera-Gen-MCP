use numera_task::coverage_environment::parse;
use std::path::Path;

#[test]
fn instrumented_environment_is_data_not_executable_shell_and_keeps_paths_contained() {
    let root = if cfg!(windows) { Path::new("C:/owned root") } else { Path::new("/owned root") };
    let target = root.join("target/llvm-cov-target").to_string_lossy().replace('\\', "/");
    let build = root.join("target/llvm-cov-build").to_string_lossy().replace('\\', "/");
    let text = format!("CARGO_LLVM_COV_TARGET_DIR='{target}'\nCARGO_LLVM_COV_BUILD_DIR='{build}'\nLLVM_PROFILE_FILE='{target}/numera-%p-%m.profraw'\nRUSTFLAGS='-C instrument-coverage -Z coverage-options=branch'\nCARGO_LLVM_COV=1\n");
    let parsed = parse(root, text.as_bytes()).unwrap();
    assert_eq!(parsed["CARGO_TARGET_DIR"], target);
    assert_eq!(parsed["RUSTFLAGS"], "-C instrument-coverage -Z coverage-options=branch");
    for invalid in [text.replace(&target, "/outside"), text.replace("CARGO_LLVM_COV=1", "PATH=/attacker"), text.replace("CARGO_LLVM_COV=1", "CARGO_LLVM_COV=$(execute)"), format!("{text}CARGO_LLVM_COV=2\n"), "CARGO_LLVM_COV=1\n".into()] {
        assert!(parse(root, invalid.as_bytes()).is_err());
    }
}
