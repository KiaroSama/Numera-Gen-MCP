mod policy;
mod preparation;
mod runtime;

use policy::{Config, fail};
use std::{
    fs::OpenOptions,
    io::{self, Write},
};

fn execute() -> io::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 1 {
        return Err(fail("Exactly one bootstrap operation is required."));
    }
    let config = Config::load()?;
    match args[0].as_str() {
        "validate" => config.checkout(false),
        "coverage-toolchain" => config.toolchain(),
        "version" => config.version(),
        "build-task" => config.build_task(),
        "prepare-checkout" => config.prepare_checkout(),
        "prepare-lock" => config.prepare_lock(),
        "prepare-format" => config.prepare_format(false),
        "prepare-format-check" => config.prepare_format(true),
        "prepare-package" => config.prepare_package(),
        "export" => {
            let file = std::path::PathBuf::from(policy::required("GITHUB_ENV")?);
            if !file.is_absolute()
                || !file.is_file()
                || std::fs::symlink_metadata(&file)?.is_symlink()
            {
                return Err(fail(
                    "Runner environment transport must be an existing regular file.",
                ));
            }
            let root = runtime::command_path(&config.root);
            let prefix = format!("numera-image-gen-mcp-0.2.0-{}", config.target);
            let owner = root
                .join(".ci-work/bootstrap")
                .join(prefix)
                .join("bin")
                .join(if cfg!(windows) {
                    "numera-dev.exe"
                } else {
                    "numera-dev"
                });
            let driver = root.join(".ci-work/bootstrap").join(if cfg!(windows) {
                "numera-bootstrap.exe"
            } else {
                "numera-bootstrap"
            });
            if !owner.is_file() || !driver.is_file() {
                return Err(fail("Authenticated bootstrap executables are missing."));
            }
            let mut output = OpenOptions::new().append(true).open(file)?;
            writeln!(
                output,
                "NUMERA_BOOTSTRAP_OWNER={}",
                owner
                    .to_str()
                    .ok_or_else(|| fail("Bootstrap owner path must be Unicode."))?
            )?;
            writeln!(
                output,
                "NUMERA_BOOTSTRAP_DRIVER={}",
                driver
                    .to_str()
                    .ok_or_else(|| fail("Bootstrap driver path must be Unicode."))?
            )?;
            output.flush()
        }
        _ => Err(fail(
            "Unsupported bootstrap operation; no fallback was executed.",
        )),
    }
}

fn main() {
    if let Err(error) = execute() {
        eprintln!(
            "Native bootstrap failed: {:?}. Child or private path details are withheld.",
            error.kind()
        );
        std::process::exit(1);
    }
}
