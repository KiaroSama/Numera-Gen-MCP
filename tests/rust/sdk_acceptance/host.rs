use numera_gen_mcp::devtools::Owned;
use rmcp::{
    RoleClient,
    model::{ClientCapabilities, ClientConfig, Implementation, ProtocolVersion},
    service::{RunningService, ClientLifecycleMode, serve_client_with_lifecycle_and_ct},
};
use serde_json::{Value, json};
use std::{collections::HashMap, future::Future, path::{Path, PathBuf}, process::{Command, Stdio}, sync::{Arc, Mutex}, time::{Duration, Instant}};
use tokio::{io::AsyncReadExt, process::{ChildStdin, ChildStderr, ChildStdout}, task::JoinHandle, time::timeout};
use tokio_util::sync::CancellationToken;

pub type Client = RunningService<RoleClient, ClientConfig>;
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy)]
pub enum Era { Legacy, Auto, Pin }
impl Era {
    pub fn lifecycle(self) -> ClientLifecycleMode {
        match self {
            Self::Legacy => ClientLifecycleMode::Initialize,
            Self::Auto => ClientLifecycleMode::Auto {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                legacy_version: Some(ProtocolVersion::V_2025_11_25),
            },
            Self::Pin => ClientLifecycleMode::Discover { preferred_versions: vec![ProtocolVersion::V_2026_07_28] },
        }
    }
}

pub async fn bounded<T>(ms: u64, operation: impl Future<Output = T>) -> T {
    timeout(Duration::from_millis(ms), operation).await.expect("Native SDK fixture event deadline exceeded")
}

pub async fn scenario<T>(ms: u64, operation: impl Future<Output = T>) -> T {
    bounded(ms, operation).await
}

#[derive(Clone)]
pub struct Launch {
    pub command: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub name: String,
    pub version: String,
    pub env: HashMap<String, String>,
}
impl Launch {
    pub fn bridge(root: &Path, config: &Path, name: &str) -> Self {
        Self {
            command: binary(), args: vec!["bridge".into(), "--config".into(), config.to_str().unwrap().into()],
            cwd: root.to_path_buf(), name: name.into(), version: "1".into(), env: HashMap::new(),
        }
    }
}

pub fn binary() -> PathBuf {
    std::env::var_os("NUMERA_NATIVE_BINARY").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_numera-gen-mcp")))
}

pub fn command(launch: &Launch, workspace: &Path) -> Command {
    std::fs::create_dir_all(workspace.join("temp")).unwrap();
    let mut command = Command::new(&launch.command);
    command.args(&launch.args).current_dir(&launch.cwd).env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("TEMP", workspace.join("temp")).env("TMP", workspace.join("temp"))
        .env("TMPDIR", workspace.join("temp"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for key in ["SystemRoot", "WINDIR", "LLVM_PROFILE_FILE"] {
        if let Some(value) = std::env::var_os(key) { command.env(key, value); }
    }
    for (key, value) in &launch.env {
        assert!(["NUMERA_CONFIG", "NUMERA_LOG_DIR", "NUMERA_HTTP_TOKEN"].contains(&key.as_str()), "fixture overrides are allowlisted");
        command.env(key, value);
    }
    command
}

pub struct Host {
    pub client: Option<Client>,
    pub child: Option<Owned>,
    pub pid: u32,
    pub launch: Launch,
    pub stderr: Arc<Mutex<Vec<u8>>>,
    pub cancellation: CancellationToken,
    diagnostics: Option<JoinHandle<Result<()>>>,
    lifetime: Option<JoinHandle<()>>,
}

pub async fn capture(mut stderr: ChildStderr, output: Arc<Mutex<Vec<u8>>>) -> Result<()> {
    let mut chunk = [0u8; 4096];
    loop {
        let count = stderr.read(&mut chunk).await.map_err(|_| "Owned stderr read failed")?;
        if count == 0 { return Ok(()); }
        let mut bytes = output.lock().unwrap();
        if bytes.len() + count > 1048576 { return Err("Full native session stderr exceeded one MiB".into()); }
        bytes.extend_from_slice(&chunk[..count]);
    }
}

impl Host {
    pub async fn connect(workspace: &Path, launch: Launch, era: Era, pre_cancel: bool) -> Result<Self> {
        let mut child = Owned::spawn(&mut command(&launch, workspace)).map_err(|_| "Cannot start isolated native SDK child")?;
        let pid = child.id();
        assert!(pid > 0 && pid != std::process::id());
        let pipes = child.pipes().map_err(|_| "Native SDK pipes missing")?;
        let input = ChildStdout::from_std(pipes.stdout).map_err(|_| "Cannot convert SDK stdout")?;
        let output = ChildStdin::from_std(pipes.stdin.ok_or("Native SDK stdin missing")?).map_err(|_| "Cannot convert SDK stdin")?;
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let diagnostics = tokio::spawn(capture(ChildStderr::from_std(pipes.stderr).map_err(|_| "Cannot convert SDK stderr")?, stderr.clone()));
        let cancellation = CancellationToken::new();
        if pre_cancel { cancellation.cancel(); }
        let config = ClientConfig::new(ClientCapabilities::default(), Implementation::new(launch.name.clone(), launch.version.clone()))
            .with_protocol_version(ProtocolVersion::V_2025_11_25);
        // Establish the complete owner before the handshake await; cancellation must not detach
        // the stderr reader or leave cleanup to an untracked child handle.
        let mut host = Self { client: None, child: Some(child), pid, launch, stderr, cancellation, diagnostics: Some(diagnostics), lifetime: None };
        let connected = timeout(Duration::from_secs(8), serve_client_with_lifecycle_and_ct(config, (input, output), era.lifecycle(), host.cancellation.clone())).await;
        match connected {
            Ok(Ok(client)) => {
                host.client = Some(client);
                Ok(host)
            }
            failed => {
                // Pinned-era rejection must be the server's actual legacy-method rejection, not
                // an arbitrary timeout, spawn issue or unrelated handshake failure relabelled green.
                let expected_pin = matches!(&failed, Ok(Err(rmcp::service::ClientInitializeError::JsonRpcError(error))) if error.code == rmcp::model::ErrorCode::METHOD_NOT_FOUND)
                    || matches!(&failed, Ok(Err(rmcp::service::ClientInitializeError::NoCompatibleProtocolVersion { .. })));
                let cancelled = matches!(&failed, Ok(Err(rmcp::service::ClientInitializeError::Cancelled)));
                host.close().await?;
                if matches!(era, Era::Pin) && expected_pin { Err("ERA_NEGOTIATION_FAILED".into()) }
                else if pre_cancel && cancelled { Err("HANDSHAKE_CANCELLED_AND_CHILD_REAPED".into()) }
                else { Err("Native SDK handshake failed unexpectedly".into()) }
            }
        }
    }
    fn enforce_pair_lifetime(&mut self) {
        let signal = self.cancellation.clone();
        self.lifetime = Some(tokio::spawn(async move { tokio::time::sleep(Duration::from_secs(45)).await; signal.cancel(); }));
    }
    pub fn client(&self) -> &Client { self.client.as_ref().expect("connected real SDK client") }
    pub fn text(&self) -> String { String::from_utf8(self.stderr.lock().unwrap().clone()).expect("UTF-8 diagnostics") }
    pub async fn call_timeout(&self, name: &str, args: Value, ms: u64) -> Value {
        assert!(!self.cancellation.is_cancelled(), "Native host exceeded its source lifetime");
        let request: rmcp::model::ClientRequest = serde_json::from_value(json!({"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap();
        let handle = bounded(1000, self.client().send_request_with_option(
            request, rmcp::service::PeerRequestOptions::with_timeout(Duration::from_millis(ms)),
        )).await.expect("SDK request dispatch");
        let peer = handle.peer.clone();
        let request_id = handle.id.clone();
        let result = tokio::select! {
            result = timeout(Duration::from_millis(ms + 1000), handle.await_response()) => result.expect("SDK outer request deadline").expect("SDK call transport error"),
            _ = self.cancellation.cancelled() => {
                let notification: rmcp::model::ClientNotification = serde_json::from_value(json!({"method":"notifications/cancelled","params":{"requestId":request_id,"reason":"Fixture lifetime cancelled"}})).unwrap();
                let _ = bounded(1000, peer.send_notification(notification)).await;
                panic!("Native SDK call cancelled by fixture lifetime");
            },
        };
        let rmcp::model::ServerResult::CallToolResult(result) = result else { panic!("SDK tools/call returned a different protocol result"); };
        serde_json::to_value(result).unwrap()
    }
    pub async fn call(&self, name: &str, args: Value) -> Value { self.call_timeout(name, args, 5000).await }
    pub async fn job(&self, id: &str, refresh: bool) -> Value {
        let result = self.call("get_job", json!({"request_id":id,"refresh":refresh})).await;
        assert_ne!(result["isError"], true, "{result}");
        structured(&result)
    }
    pub async fn close(&mut self) -> Result<()> {
        let mut failures = Vec::new();
        if self.client.is_some() && self.cancellation.is_cancelled() { failures.push("Native host exceeded source lifetime before close"); }
        if let Some(mut client) = self.client.take() {
            if !matches!(timeout(Duration::from_secs(5), client.close()).await, Ok(Ok(_))) { failures.push("SDK client close failed"); }
            drop(client);
        }
        if let Some(mut child) = self.child.take() {
            let graceful = timeout(Duration::from_secs(2), async {
                while !child.exited().map_err(|_| "Cannot observe owned child")? { tokio::time::sleep(Duration::from_millis(10)).await; }
                Ok::<(), &str>(())
            }).await;
            if !matches!(graceful, Ok(Ok(()))) { failures.push("Native child did not close within two seconds"); }
            if child.stop(Instant::now() + Duration::from_secs(2)).is_err() { failures.push("Owned native child cleanup unverified"); }
        }
        if let Some(mut task) = self.diagnostics.take() {
            match timeout(Duration::from_secs(2), &mut task).await {
                Ok(Ok(Ok(()))) => {},
                Err(_) => { task.abort(); let _ = task.await; failures.push("Native diagnostic drain exceeded two seconds"); },
                _ => failures.push("Native diagnostic drain failed"),
            }
        }
        if let Some(task) = self.lifetime.take() { task.abort(); let _ = task.await; }
        #[cfg(unix)]
        if unsafe { libc::kill(self.pid as i32, 0) } != -1 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) { failures.push("Native PID still exists after SDK close"); }
        if failures.is_empty() { Ok(()) } else { Err(failures.join("; ")) }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Some(task) = self.lifetime.take() { task.abort(); }
        // Owned closes the anchored process group/private Job even if an assertion unwinds.
        drop(self.client.take());
        if let Some(mut child) = self.child.take() {
            if child.stop(Instant::now() + Duration::from_secs(2)).is_err() { eprintln!("ERROR: SDK child cleanup remains unverified"); }
        }
        if let Some(task) = self.diagnostics.take() { task.abort(); }
    }
}

pub fn structured(result: &Value) -> Value {
    result.get("structuredContent").filter(|v| v.is_object()).expect("actual SDK structuredContent required").clone()
}

pub async fn pair(root: &Path, config: &Value, instances: &mut std::collections::HashSet<String>) -> [Host; 2] {
    let file = root.join("config.json");
    std::fs::write(&file, serde_json::to_vec(config).unwrap()).unwrap();
    let launch = |name: &str| {
        let mut launch = Launch::bridge(root, &file, name);
        launch.env.insert("NUMERA_LOG_DIR".into(), config["logging"]["directory"].as_str().unwrap().into());
        launch
    };
    let mut first = Host::connect(root, launch("terminal-host-0"), Era::Auto, false).await.unwrap();
    first.enforce_pair_lifetime();
    observe_run(&first, instances).await;
    let mut second = Host::connect(root, launch("terminal-host-1"), Era::Auto, false).await.unwrap();
    second.enforce_pair_lifetime();
    observe_run(&second, instances).await;
    assert_ne!(first.pid, second.pid);
    [first, second]
}
pub async fn observe_run(host: &Host, instances: &mut std::collections::HashSet<String>) {
    let run = bounded(5000, async {
        loop {
            for line in host.text().lines() {
                if let Ok(event) = serde_json::from_str::<Value>(line)
                    && event["component"] == "MCP" && event["message"] == "Stdio server ready." {
                    return event["run_id"].as_str().unwrap().to_owned();
                }
            }
            tokio::task::yield_now().await;
        }
    }).await;
    assert_eq!(run.len(), 36);
    assert!(run.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-'));
    assert!(instances.insert(run), "actual process startup run_id must be unique");
}
pub async fn close_pair(hosts: &mut [Host; 2]) {
    let (first, second) = hosts.split_at_mut(1);
    let (a, b) = tokio::join!(first[0].close(), second[0].close());
    assert!(a.is_ok() && b.is_ok(), "both native SDK children must close: {a:?} {b:?}");
}
