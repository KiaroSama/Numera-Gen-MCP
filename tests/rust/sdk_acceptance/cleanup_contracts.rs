use super::{
    cleanup, fixtures as f, host,
    provider::{Gate, Provider},
};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

fn panic_text(error: tokio::task::JoinError) -> String {
    let payload = error.into_panic();
    if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        payload.downcast_ref::<&str>().unwrap().to_string()
    }
}

#[tokio::test]
async fn failed_scenario_awaits_late_provider_errors_and_preserves_the_original_failure() {
    for run_fails in [false, true] {
        let entered = Gate::new();
        let enter = entered.clone();
        let finished = CancellationToken::new();
        let observed = finished.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let task: tokio::task::JoinHandle<()> = tokio::spawn(cleanup::run(async move {
            let provider = Provider::new(move |_, stop| {
                let enter = enter.clone();
                let observed = observed.clone();
                async move {
                    enter.release();
                    stop.cancelled().await;
                    observed.cancel();
                    Err("Controlled late provider failure".into())
                }
            })
            .await;
            let mut socket = host::bounded(
                1000,
                tokio::net::TcpStream::connect(provider.origin.strip_prefix("http://").unwrap()),
            )
            .await
            .unwrap();
            host::bounded(
                1000,
                socket.write_all(b"GET / HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n"),
            )
            .await
            .unwrap();
            host::bounded(1000, entered.wait()).await;
            sender.send(socket).unwrap();
            if run_fails {
                panic!("Controlled original scenario failure");
            }
        }));
        let task = tokio_util::task::AbortOnDropHandle::new(task);
        let mut socket = host::bounded(1000, receiver).await.unwrap();
        let failure = panic_text(host::bounded(8000, task).await.unwrap_err());
        assert_eq!(
            failure.contains("Controlled original scenario failure"),
            run_fails
        );
        assert!(failure.contains("Controlled late provider failure"));
        assert!(
            finished.is_cancelled(),
            "handler must finish before the scenario returns"
        );
        let mut bytes = Vec::new();
        host::bounded(1000, socket.read_to_end(&mut bytes))
            .await
            .unwrap();
        assert!(bytes.is_empty());
    }
}

#[tokio::test]
async fn timed_out_scenario_releases_held_handlers_before_removing_its_workspace() {
    let path = Arc::new(Mutex::new(None));
    let captured = path.clone();
    let drained = CancellationToken::new();
    let observed = drained.clone();
    let task: tokio::task::JoinHandle<()> = tokio::spawn(cleanup::run(async move {
        let work = f::workspace();
        *captured.lock().unwrap() = Some(work.0.clone());
        let marker = work.0.join("still-owned.txt");
        std::fs::write(&marker, b"owned fixture").unwrap();
        let entered = Gate::new();
        let enter = entered.clone();
        let release = Gate::new();
        let held = release.clone();
        let _release_on_failure = release.release_on_drop();
        let provider = Provider::new(move |_, _| {
            let enter = enter.clone();
            let held = held.clone();
            let marker = marker.clone();
            let observed = observed.clone();
            async move {
                enter.release();
                held.wait().await;
                assert!(
                    marker.is_file(),
                    "workspace must survive awaited handler cleanup"
                );
                observed.cancel();
                Ok(super::provider::Reply::json(json!({})))
            }
        })
        .await;
        let mut socket = host::bounded(
            1000,
            tokio::net::TcpStream::connect(provider.origin.strip_prefix("http://").unwrap()),
        )
        .await
        .unwrap();
        host::bounded(
            1000,
            socket.write_all(b"GET / HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n"),
        )
        .await
        .unwrap();
        host::bounded(1000, entered.wait()).await;
        host::scenario(1, std::future::pending::<()>()).await;
    }));
    let task = tokio_util::task::AbortOnDropHandle::new(task);
    let failure = panic_text(host::bounded(8000, task).await.unwrap_err());
    assert!(failure.contains("Native SDK fixture event deadline exceeded"));
    assert!(
        drained.is_cancelled(),
        "timeout must await the released provider handler"
    );
    assert!(!path.lock().unwrap().as_ref().unwrap().exists());
}

#[tokio::test]
async fn failed_actual_sdk_scenario_reaps_the_child_and_diagnostics_before_workspace_cleanup() {
    let state = Arc::new(Mutex::new(None));
    let captured = state.clone();
    let task: tokio::task::JoinHandle<()> = tokio::spawn(cleanup::run(async move {
        let work = f::workspace();
        let directory = f::directory(&work, "failed-owner");
        let config = f::config(&directory, json!({}));
        let child = f::start(&directory, &config, "failure-cleanup-fixture").await;
        #[cfg(windows)]
        let observer = Some(windows_child_handle(child.pid));
        #[cfg(not(windows))]
        let observer = None::<()>;
        *captured.lock().unwrap() = Some((work.0.clone(), child.pid, observer));
        let result = child.call("health_check", json!({"probe":false})).await;
        assert_ne!(result["isError"], true);
        assert_eq!(host::structured(&result)["generation_verified"], false);
        panic!("Controlled actual SDK scenario failure");
    }));
    let task = tokio_util::task::AbortOnDropHandle::new(task);
    let failure = panic_text(host::bounded(15000, task).await.unwrap_err());
    assert!(failure.contains("Controlled actual SDK scenario failure"));
    assert!(
        !failure.contains("cleanup:"),
        "actual owner cleanup must be verified: {failure}"
    );
    let state = state.lock().unwrap();
    let (path, pid, observer) = state.as_ref().unwrap();
    assert!(!path.exists());
    #[cfg(unix)]
    {
        let _ = observer;
        assert_eq!(unsafe { libc::kill(*pid as i32, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
    #[cfg(windows)]
    {
        let _ = pid;
        assert_windows_stopped(observer.as_ref().unwrap());
    }
}

#[cfg(windows)]
fn windows_child_handle(pid: u32) -> std::os::windows::io::OwnedHandle {
    use std::os::windows::io::FromRawHandle;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
    }
    // SAFETY: query/synchronize only; retain the original live process before failure cleanup.
    let handle = unsafe { OpenProcess(0x00101000, 0, pid) };
    assert!(
        !handle.is_null(),
        "fixture child must be live before failure"
    );
    unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(handle) }
}

#[cfg(windows)]
fn assert_windows_stopped(handle: &std::os::windows::io::OwnedHandle) {
    use std::os::windows::io::AsRawHandle;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
    }
    // SAFETY: immutable original handle, observation only and zero wait.
    assert_eq!(unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) }, 0);
}
