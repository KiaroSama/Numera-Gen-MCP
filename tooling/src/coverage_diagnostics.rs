use crate::context::{Context, fail};
use numera_gen_mcp::devtools::linux_snapshot::{self, Receipt};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

pub fn read_receipt(path: &Path, nonce: &str) -> io::Result<Receipt> {
    let bytes = crate::distribution::read_regular(path, linux_snapshot::RECEIPT_LIMIT)?;
    let receipt: Receipt = serde_json::from_slice(&bytes)
        .map_err(|_| fail("Coverage observation is incomplete."))?;
    receipt.validate(nonce)?;
    Ok(receipt)
}

/// Only ci::required's coverage-report gate calls this, never suites, builds or normal children.
pub fn run(
    context: &Context,
    report: &mut Command,
    wall: u64,
    idle: u64,
) -> io::Result<Vec<u8>> {
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let receipt = context.relative(&format!(".ci-work/native/coverage-observation-{nonce}.json"))?;
    let mut wrapper = context.command(std::env::current_exe()?);
    wrapper
        .arg("--coverage-observer")
        .arg(&receipt)
        .arg(&nonce)
        .arg("--");
    wrapper.args(report.get_args());
    for (key, value) in report.get_envs() {
        if let Some(value) = value {
            wrapper.env(key, value);
        } else {
            wrapper.env_remove(key);
        }
    }
    let started = std::time::Instant::now();
    let outcome = context.observed_outcome(&mut wrapper, wall, idle)?;
    io::stdout().write_all(&outcome.stdout)?;
    io::stderr().write_all(&outcome.stderr)?;
    if outcome
        .reason
        .as_deref()
        .is_some_and(|r| matches!(r, "idle" | "wall"))
    {
        let observation = match read_receipt(&receipt, &nonce) {
            Ok(receipt)
                if outcome
                    .observation
                    .as_ref()
                    .is_some_and(|owner| owner.pid == receipt.owner_pid && owner.signal_status == 1) =>
            {
                let mut value = serde_json::to_value(&receipt)
                    .map_err(|_| fail("Cannot encode coverage observation."))?;
                value["age_upper_ms"] = receipt
                    .sample_ms
                    .map(|ms| started.elapsed().as_millis().saturating_sub(u128::from(ms)) as u64)
                    .into();
                value["owner_signal_ms"] = outcome
                    .observation
                    .as_ref()
                    .and_then(|owner| owner.signal_ms)
                    .into();
                // A nonce binds the private receipt but is not useful in numeric diagnostics.
                value.as_object_mut().unwrap().remove("nonce");
                numera_gen_mcp::compat::value::Value::from_serde(&value)
                    .unwrap_or_else(|_| numera_gen_mcp::compat::value::json!({"status":6}))
            }
            _ => numera_gen_mcp::compat::value::json!({"status":6,
                "owner_signal_status":outcome.observation.as_ref().map(|owner| owner.signal_status),
                "owner_signal_ms":outcome.observation.as_ref().and_then(|owner| owner.signal_ms)}),
        };
        context.logger.log(
            "ERROR",
            "coverage-observation",
            "Bounded coverage observation; numeric samples are not a stack or phase trace.",
            observation,
        );
    }
    if outcome.code != 0 || outcome.reason.is_some() || !outcome.cleanup_verified {
        return Err(fail(
            "Required owned coverage report failed; inspect bounded diagnostics.",
        ));
    }
    Ok(outcome.stdout)
}

pub fn entry(args: &[String]) -> io::Result<Option<i32>> {
    match args.first().map(String::as_str) {
        Some("--coverage-sampler") if args.len() == 1 => {
            linux_snapshot::sample_from_stdio()?;
            Ok(Some(0))
        }
        Some("--coverage-observer") => {
            if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
                return Err(fail("Compilation and verification are hosted CI only."));
            }
            if args.len() != 15 || args[3] != "--" || !linux_snapshot::valid_nonce(&args[2]) {
                return Err(fail("Invalid private coverage observation invocation."));
            }
            let expected = [
                "+nightly-2026-10-10",
                "llvm-cov",
                "report",
                "--verbose",
                "--branch",
                "-p",
                "numera-gen-mcp",
                "--json",
                "--summary-only",
                "--output-path",
                ".ci-work/native/coverage.json",
            ];
            if args[4..].iter().map(String::as_str).ne(expected) {
                return Err(fail(
                    "Coverage observation accepts only the original report command.",
                ));
            }
            let receipt = PathBuf::from(&args[1]);
            let cwd = std::env::current_dir()?;
            if receipt.parent() != Some(cwd.join(".ci-work/native").as_path())
                || receipt.file_name().and_then(|s| s.to_str())
                    != Some(format!("coverage-observation-{}.json", args[2]).as_str())
            {
                return Err(fail("Coverage observation destination differs."));
            }
            let mut report = Command::new("cargo");
            report.args(&args[4..]);
            let mut helper = Command::new(std::env::current_exe()?);
            helper.arg("--coverage-sampler");
            let mut discovery = Command::new("rustc");
            discovery.args(["+nightly-2026-10-10", "--print", "target-libdir"]);
            Ok(Some(linux_snapshot::observe_report(
                &mut report,
                &mut helper,
                Some(&mut discovery),
                &receipt,
                &args[2],
            )?))
        }
        _ => Ok(None),
    }
}
