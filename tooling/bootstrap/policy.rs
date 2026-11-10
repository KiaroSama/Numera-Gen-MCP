use std::{env, io, path::PathBuf};

#[path = "events.rs"]
mod events;

pub fn fail(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

pub fn host_target(os: &str, architecture: &str) -> io::Result<&'static str> {
    match (os, architecture) {
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        ("windows", "aarch64") => Ok("aarch64-pc-windows-msvc"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        _ => Err(fail("Unsupported native bootstrap host.")),
    }
}

pub fn flags(coverage: &str, build: &str, job: &str, operation: &str) -> io::Result<(bool, bool)> {
    let boolean = |value| match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(fail("Bootstrap flags require exact booleans.")),
    };
    let coverage = boolean(coverage)?;
    let build = boolean(build)?;
    if !matches!(
        operation,
        "qualify" | "source-preparation" | "sign-qualified"
    ) || (!build
        && (job != "source-preparation" || operation != "source-preparation" || coverage))
        || (operation == "source-preparation" && (build || coverage || job != "source-preparation"))
    {
        return Err(fail(
            "Bootstrap operation does not authorize the selected work.",
        ));
    }
    Ok((coverage, build))
}

pub fn allowed_changes(bytes: &[u8]) -> io::Result<Vec<String>> {
    if bytes.len() > 1024 * 1024 || (!bytes.is_empty() && bytes.last() != Some(&0)) {
        return Err(fail(
            "Preparation path inventory is truncated or oversized.",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| fail("Preparation paths must be UTF-8."))?;
    let mut paths = std::collections::BTreeSet::new();
    for name in text.split('\0').filter(|name| !name.is_empty()) {
        if name.len() > 1024
            || (name != "Cargo.lock" && !name.ends_with(".rs"))
            || name
                .bytes()
                .any(|byte| byte == b'\\' || byte == b':' || byte.is_ascii_control())
            || name.split('/').any(|part| matches!(part, "" | "." | ".."))
            || !paths.insert(name.to_owned())
        {
            return Err(fail("Unexpected or duplicate preparation path."));
        }
    }
    Ok(paths.into_iter().collect())
}

pub fn checksum_output(bytes: &[u8]) -> io::Result<String> {
    let text = std::str::from_utf8(bytes).map_err(|_| fail("Hash tool output is not UTF-8."))?;
    let digest = text.split_whitespace().next().unwrap_or("");
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(fail("Hash tool did not return an exact SHA-256 digest."));
    }
    Ok(digest.to_owned())
}

pub fn json_string(value: &str) -> String {
    let mut result = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ch if ch.is_control() => result.push_str(&format!("\\u{:04x}", ch as u32)),
            other => result.push(other),
        }
    }
    result.push('"');
    result
}

pub fn required(key: &str) -> io::Result<String> {
    env::var(key)
        .ok()
        .filter(|value| {
            !value.is_empty() && !value.chars().any(|ch| matches!(ch, '\0' | '\r' | '\n'))
        })
        .ok_or_else(|| fail("Required bootstrap environment is missing or invalid."))
}

pub struct Config {
    pub root: PathBuf,
    pub target: String,
    pub coverage: bool,
    pub build: bool,
    pub source: String,
    pub reference: String,
    pub run: String,
    pub attempt: String,
}
impl Config {
    pub fn load() -> io::Result<Self> {
        if required("GITHUB_ACTIONS")? != "true"
            || required("GITHUB_REPOSITORY")? != "KiaroSama/Numera-Gen-MCP"
            || !matches!(required("GITHUB_EVENT_NAME")?.as_str(), "workflow_dispatch" | "push" | "pull_request")
        {
            return Err(fail(
                "Bootstrap is restricted to the selected hosted repository and CI events.",
            ));
        }
        let target = required("BOOTSTRAP_TARGET")?;
        if target != host_target(env::consts::OS, env::consts::ARCH)? {
            return Err(fail("Bootstrap target differs from its actual host."));
        }
        let (coverage, build) = flags(
            &required("BOOTSTRAP_COVERAGE")?,
            &required("BOOTSTRAP_BUILD_TASK")?,
            &required("GITHUB_JOB")?,
            &required("NUMERA_CI_OPERATION")?,
        )?;
        if required("GITHUB_EVENT_NAME")? != "workflow_dispatch"
            && required("NUMERA_CI_OPERATION")? != "qualify"
        {
            return Err(fail("Automatic CI cannot prepare or sign a release."));
        }
        let raw = PathBuf::from(required("GITHUB_WORKSPACE")?);
        if !raw.is_absolute()
            || raw
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(fail("Bootstrap requires an absolute owning workspace."));
        }
        let root = raw.canonicalize()?;
        if !root.join("Cargo.toml").is_file() || !root.join("native/lib.rs").is_file() {
            return Err(fail("Bootstrap workspace is not the owning repository."));
        }
        let source = required("GITHUB_SHA")?;
        if source.len() != 40
            || !source
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(fail("Bootstrap requires an exact source revision."));
        }
        let reference = required("GITHUB_REF")?;
        if !events::valid_reference(&required("GITHUB_EVENT_NAME")?, &reference) {
            return Err(fail("Bootstrap requires a matching branch or pull-request reference."));
        }
        let number = |key| {
            let value = required(key)?;
            if !value.bytes().all(|b| b.is_ascii_digit())
                || value.parse::<u64>().ok().is_none_or(|n| n == 0)
            {
                return Err(fail(
                    "Bootstrap requires numeric run and attempt identities.",
                ));
            }
            Ok(value)
        };
        Ok(Self {
            root,
            target,
            coverage,
            build,
            source,
            reference,
            run: number("GITHUB_RUN_ID")?,
            attempt: number("GITHUB_RUN_ATTEMPT")?,
        })
    }
    pub fn preparation(&self) -> io::Result<()> {
        if self.build
            || self.coverage
            || required("GITHUB_JOB")? != "source-preparation"
            || required("NUMERA_CI_OPERATION")? != "source-preparation"
        {
            return Err(fail("Source preparation cannot build, qualify or sign."));
        }
        Ok(())
    }
}
