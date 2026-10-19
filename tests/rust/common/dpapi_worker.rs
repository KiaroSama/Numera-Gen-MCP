use std::{
    io::{self, Write},
    os::windows::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let until = Instant::now() + Duration::from_secs(2);
        while !matches!(self.0.try_wait(), Ok(Some(_))) {
            if Instant::now() >= until {
                if std::thread::panicking() {
                    eprintln!("Native DPAPI descendant cleanup failed.");
                    return;
                }
                panic!("Native DPAPI descendant cleanup failed.");
            }
            pause(until);
        }
    }
}

fn pause(until: Instant) {
    std::thread::park_timeout(
        Duration::from_millis(10).min(until.saturating_duration_since(Instant::now())),
    );
}

fn descendant(mode: &str) -> io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", "windows::dpapi_script_worker", "--nocapture"])
        .env("NUMERA_FIXTURE_DPAPI_MODE", mode)
        .stdin(Stdio::null())
        .creation_flags(0x08000000);
    Ok(command)
}

pub fn run(mode: &str) -> io::Result<i32> {
    if mode == "leaf" {
        std::fs::write("native-descendant-ready", b"ready")?;
        let until = Instant::now() + Duration::from_secs(6);
        while Instant::now() < until {
            pause(until);
        }
        return Ok(0);
    }
    if mode == "branch" {
        let mut child = Guard(
            descendant("leaf")?
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        let until = Instant::now() + Duration::from_secs(2);
        while !Path::new("native-descendant-ready").exists() {
            if child.0.try_wait()?.is_some() || Instant::now() >= until {
                return Err(io::Error::other("Native descendant readiness failed."));
            }
            pause(until);
        }
        println!("fixture-descendant-ready");
        io::stdout().flush()?;
        let until = Instant::now() + Duration::from_secs(6);
        while child.0.try_wait()?.is_none() && Instant::now() < until {
            pause(until);
        }
        return Ok(0);
    }
    let mut command = if mode == "hang" {
        descendant("branch")?
    } else {
        assert_eq!(mode, "script");
        let mut command = Command::new(env!("CARGO_BIN_EXE_numera-dev"));
        command
            .args([
                "bounded", "15000", "8000", "--exec", "pwsh", "-NoProfile", "-NonInteractive", "-File",
            ])
            .arg(std::env::var_os("NUMERA_FIXTURE_DPAPI_SCRIPT").unwrap());
        if std::env::var("NUMERA_FIXTURE_DPAPI_WRAPPER").unwrap() == "0" {
            command
                .arg("-Path")
                .arg(std::env::var_os("NUMERA_FIXTURE_DPAPI").unwrap());
        }
        command
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .creation_flags(0x08000000);
    let mut child = Guard(command.spawn()?);
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.0.try_wait()? {
            return Ok(status.code().unwrap_or(1));
        }
        if Instant::now() >= until {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Native DPAPI worker exceeded its bound.",
            ));
        }
        pause(until);
    }
}
