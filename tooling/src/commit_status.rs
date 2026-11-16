use crate::context::{Context, fail};
use std::io;

pub fn state(outcome: &str) -> io::Result<&'static str> {
    match outcome {
        // Manual-dispatch check runs are not linked beside the commit, so each job announces
        // itself as pending when it starts and its final always() step replaces that state.
        "started" => Ok("pending"),
        "success" => Ok("success"),
        "failure" => Ok("failure"),
        "cancelled" => Ok("error"),
        _ => Err(fail("Unknown job outcome cannot publish a commit status.")),
    }
}

pub fn description(outcome: &str) -> String {
    if outcome == "started" {
        "Job started; its observed result replaces this status.".into()
    } else {
        format!("Observed job result: {outcome}")
    }
}

pub fn label(job: &str, target: &str) -> io::Result<String> {
    Ok(match job {
        "native" | "signing" => {
            crate::distribution::target(target)?;
            let category = if job == "native" { "Tests" } else { "Signing" };
            format!("{category} / {job}-{target}")
        }
        "native-coverage" | "execution-matrix" if target == "none" => {
            format!("Tests / {job}")
        }
        "signed-execution-matrix" if target == "none" => format!("Signing / {job}"),
        _ => return Err(fail("Unsupported commit-status job or target.")),
    })
}

pub fn run_url(run: &str, attempt: &str) -> io::Result<String> {
    for value in [run, attempt] {
        if value.parse::<u64>().ok().is_none_or(|n| n == 0)
            || !value.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(fail("Commit status requires an exact run and attempt."));
        }
    }
    Ok(format!(
        "https://github.com/KiaroSama/Numera-Gen-MCP/actions/runs/{run}/attempts/{attempt}"
    ))
}

/// Publish only the hosting job's observed result, never an ancestor's status.
pub fn publish(context: &Context, outcome: &str, target: &str) -> io::Result<()> {
    context.hosted()?;
    let sha = context.source_sha()?;
    if std::env::var("GITHUB_REPOSITORY").as_deref() != Ok("KiaroSama/Numera-Gen-MCP")
        || std::env::var("GITHUB_EVENT_NAME").as_deref() != Ok("workflow_dispatch")
    {
        return Err(fail(
            "Commit status belongs to this repository's manual CI.",
        ));
    }
    let label = label(&std::env::var("GITHUB_JOB").unwrap_or_default(), target)?;
    let url = run_url(
        &std::env::var("GITHUB_RUN_ID").unwrap_or_default(),
        &std::env::var("GITHUB_RUN_ATTEMPT").unwrap_or_default(),
    )?;
    let endpoint = format!("repos/KiaroSama/Numera-Gen-MCP/statuses/{sha}");
    // `gh api` prints nothing until the response arrives, so the idle bound equals the wall bound;
    // a cold start on the emulated Windows ARM runner exceeded a shorter idle bound.
    let posted = context.capture(
        context
            .command("gh")
            .args([
                "api",
                "--hostname",
                "github.com",
                "--method",
                "POST",
                &endpoint,
            ])
            .args(["-f", &format!("state={}", state(outcome)?)])
            .args(["-f", &format!("context={label}")])
            .args(["-f", &format!("target_url={url}")])
            .args(["-f", &format!("description={}", description(outcome))])
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_PAGER", "cat")
            .env_remove("GH_DEBUG"),
        45000,
        45000,
        65536,
    );
    match posted {
        Ok(_) => Ok(()),
        // The pending announcement is advisory: the job's final always() status still binds, so a
        // failed announcement must not fail qualification. Final results stay fatal.
        Err(_) if outcome == "started" => {
            context.logger.log(
                "WARNING",
                "task",
                "Pending commit status was not posted; the final status still binds.",
                numera_gen_mcp::compat::value::json!({"context": label}),
            );
            Ok(())
        }
        Err(error) => Err(error),
    }
}
