use numera_task::context::{Context, fail};
use std::{io, path::PathBuf};

fn execute() -> io::Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|s| s == "--help") || args.is_empty() {
        println!("Numera hosted Rust tooling\nUsage: numera-task --root ABSOLUTE_ROOT COMMAND [ARGS]\nCommands are hosted-only; no implicit installation, provider call or host edit.");
        return Ok(());
    }
    if args.len() < 3 || args[0] != "--root" { return Err(fail("Explicit owning --root is required.")); }
    let root = PathBuf::from(&args[1]);
    if !root.is_absolute() { return Err(fail("Task root must be absolute.")); }
    args.drain(..2);
    let context = Context::new(&root)?;
    context.hosted()?;
    let target = || -> io::Result<&str> {
        if args.len() != 2 { return Err(fail("Command requires exactly one target.")); }
        numera_task::distribution::target(&args[1])?;
        Ok(args[1].as_str())
    };
    match args[0].as_str() {
        "fixtures" if args.len() == 1 => numera_task::fixtures::materialize(&context),
        "stable" if args.len() == 1 => numera_task::ci::stable(&context),
        "acceptance-prepare" if args.len() == 1 => numera_task::ci::acceptance_prepare(&context),
        "coverage" if args.len() == 1 => numera_task::ci::coverage(&context),
        "coverage-gate" if args.len() == 1 => numera_task::inventory::coverage(&context),
        "inventory" if args.len() == 1 => numera_task::inventory::check(&context, false),
        "retirement-gate" if args.len() == 1 => numera_task::inventory::check(&context, true),
        "evidence-capture" if args.len() == 1 => numera_task::evidence::capture(&context),
        "evidence-verify" if args.len() == 1 => numera_task::evidence::verify(&context),
        "ledger" if args.len() == 1 => numera_task::ledger::check(&context),
        "case-execution" if args.len() == 1 => numera_task::qualification::check(&context),
        "execution-matrix" if args.len() == 1 => numera_task::matrix::check(&context),
        "licenses" => numera_task::licenses::preserve(&context, target()?),
        "prepare" => numera_task::distribution::prepare(&context, target()?, true),
        "stage" => numera_task::distribution::stage(&context, target()?),
        "signing-tools" => tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(numera_task::signing_tools::tools(&context, target()?)),
        "signature-acceptance" => numera_task::signature_acceptance::verify(&context, target()?),
        "sign" => numera_task::signing::sign(&context, target()?),
        "package" => numera_task::signing::verify(&context, target()?),
        "extract-intermediate" => numera_task::acceptance::extract(&context, target()?, true),
        "extract-signed" => numera_task::acceptance::extract(&context, target()?, false),
        "activation" => {
            let target = target()?;
            numera_task::acceptance::activation(&context, &context.relative(&format!(".ci-work/prepared/{target}"))?)
        }
        _ => Err(fail("Unsupported task command; no fallback was executed.")),
    }
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("ERROR: {}", numera_task::context::diagnostic(&error));
        std::process::exit(1);
    }
}
