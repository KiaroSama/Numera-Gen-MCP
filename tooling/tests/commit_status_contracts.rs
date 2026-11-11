use numera_task::commit_status::{label, run_url, state};

#[test]
fn only_explicit_success_can_publish_a_successful_commit_status() {
    assert_eq!(state("success").unwrap(), "success");
    assert_eq!(state("failure").unwrap(), "failure");
    assert_eq!(state("cancelled").unwrap(), "error");
    for unknown in ["", "skipped", "pending", "SUCCESS", "in_progress"] {
        assert!(state(unknown).is_err());
    }
}

#[test]
fn status_contexts_and_links_bind_only_supported_jobs_and_original_attempts() {
    for job in ["native", "signing"] {
        let category = if job == "native" { "Tests" } else { "Signing" };
        assert_eq!(
            label(job, "x86_64-pc-windows-msvc").unwrap(),
            format!("{category} / {job}-x86_64-pc-windows-msvc")
        );
        assert!(label(job, "none").is_err());
        assert!(label(job, "unsupported").is_err());
    }
    for job in [
        "native-coverage",
        "execution-matrix",
        "signed-execution-matrix",
    ] {
        assert!(label(job, "none").unwrap().ends_with(job));
        assert!(label(job, "x86_64-pc-windows-msvc").is_err());
    }
    assert!(label("source-preparation", "none").is_err());
    assert!(label("retirement-verification", "none").is_err());
    assert_eq!(
        run_url("42", "2").unwrap(),
        "https://github.com/KiaroSama/Numera-Gen-MCP/actions/runs/42/attempts/2"
    );
    for invalid in ["", "0", "+1", "-1", "1/2", "18446744073709551616"] {
        assert!(run_url(invalid, "1").is_err());
        assert!(run_url("42", invalid).is_err());
    }
}
