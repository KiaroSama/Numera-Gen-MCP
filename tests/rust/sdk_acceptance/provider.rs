use serde_json::Value;
use std::{future::Future, pin::Pin, sync::{Arc, Mutex}, time::Duration};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}, task::{JoinHandle, JoinSet}, time::timeout};
use tokio_util::sync::CancellationToken;
use super::host::Result;

#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: String,
    pub body: Vec<u8>,
}
impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
    pub fn json(&self) -> Value { serde_json::from_slice(&self.body).expect("exact fixture JSON body") }
}
pub struct Reply { pub status: u16, pub mime: &'static str, pub chunks: Vec<Vec<u8>>, pub disconnect: bool }
impl Reply {
    pub fn json(value: Value) -> Self { Self::status(value, 200) }
    pub fn status(value: Value, status: u16) -> Self { Self::bytes(status, "application/json; charset=utf-8", serde_json::to_vec(&value).unwrap()) }
    pub fn bytes(status: u16, mime: &'static str, body: Vec<u8>) -> Self { Self { status, mime, chunks: vec![body], disconnect: false } }
    pub fn disconnect() -> Self { Self { status: 200, mime: "application/json", chunks: Vec::new(), disconnect: true } }
    pub fn sse(frame: String) -> Self {
        let bytes = frame.into_bytes();
        let boundary = 19.min(bytes.len());
        Self { status: 200, mime: "text/event-stream", chunks: vec![bytes[..boundary].to_vec(), bytes[boundary..].to_vec()], disconnect: false }
    }
}
type Handler = Arc<dyn Fn(Request, CancellationToken) -> Pin<Box<dyn Future<Output = Result<Reply>> + Send>> + Send + Sync>;

pub struct Provider {
    pub origin: String,
    pub requests: Arc<Mutex<Vec<Request>>>,
    stop: CancellationToken,
    task: Option<JoinHandle<Vec<String>>>,
}
impl Provider {
    pub async fn new<F, A>(handler: F) -> Self
    where F: Fn(Request, CancellationToken) -> A + Send + Sync + 'static,
          A: Future<Output = Result<Reply>> + Send + 'static {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let stop = CancellationToken::new();
        let cancel = stop.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let handler: Handler = Arc::new(move |request, token| Box::pin(handler(request, token)));
        let task = tokio::spawn(async move {
            let _cancel_on_drop = cancel.clone().drop_guard();
            let mut tasks = JoinSet::new();
            let mut errors = Vec::new();
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    result = tasks.join_next(), if !tasks.is_empty() => record(result, &mut errors),
                    accepted = listener.accept() => {
                        let (socket, _) = accepted.expect("owned provider accept");
                        tasks.spawn(connection(socket, handler.clone(), captured.clone(), cancel.child_token()));
                    }
                }
            }
            drop(listener);
            let drained = timeout(Duration::from_secs(6), async {
                while let Some(result) = tasks.join_next().await { record(Some(result), &mut errors); }
            }).await;
            if drained.is_err() {
                tasks.abort_all();
                while let Some(result) = tasks.join_next().await {
                    if !matches!(result, Err(ref error) if error.is_cancelled()) { record(Some(result), &mut errors); }
                }
                errors.push("Owned provider cleanup exceeded six seconds".into());
            }
            errors
        });
        Self { origin, requests, stop, task: Some(task) }
    }
    pub fn captured(&self) -> Vec<Request> { self.requests.lock().unwrap().clone() }
    pub fn posts(&self) -> usize { self.requests.lock().unwrap().iter().filter(|r| r.method == "POST").count() }
    pub async fn finish(mut self) -> Vec<String> {
        self.stop.cancel();
        // Keep ownership until the await completes: dropping a JoinHandle would detach cleanup.
        timeout(Duration::from_secs(7), self.task.as_mut().unwrap()).await.expect("provider owner cleanup deadline").expect("provider owner panicked")
    }
    pub async fn close(self) { let errors = self.finish().await; assert!(errors.is_empty(), "owned provider errors: {errors:?}"); }
}
impl Drop for Provider { fn drop(&mut self) { self.stop.cancel(); if let Some(task) = self.task.take() { task.abort(); } } }
fn record(result: Option<std::result::Result<Result<()>, tokio::task::JoinError>>, errors: &mut Vec<String>) {
    match result { Some(Ok(Ok(()))) | None => {}, Some(Ok(Err(error))) => errors.push(error), Some(Err(_)) => errors.push("Owned provider handler panicked".into()) }
}
async fn connection(mut socket: TcpStream, handler: Handler, requests: Arc<Mutex<Vec<Request>>>, stop: CancellationToken) -> Result<()> {
    let captured = tokio::select! {
        result = timeout(Duration::from_secs(5), capture(&mut socket)) => result.map_err(|_| "Fixture headers/body exceeded five seconds")??,
        _ = stop.cancelled() => return Ok(()),
    };
    let Some(request) = captured else { return Ok(()); };
    requests.lock().unwrap().push(request.clone());
    // Cancellation is observable by the callback, so a failure discovered while draining is not lost.
    let reply = timeout(Duration::from_secs(5), handler(request, stop.clone())).await.map_err(|_| "Provider response exceeded five seconds")??;
    if reply.disconnect { return Ok(()); }
    let size: usize = reply.chunks.iter().map(Vec::len).sum();
    let content_type = if reply.mime.is_empty() { String::new() } else { format!("Content-Type: {}\r\n", reply.mime) };
    let headers = format!("HTTP/1.1 {} Fixture\r\n{content_type}Content-Length: {size}\r\nConnection: close\r\n\r\n", reply.status);
    let operation = async {
        if socket.write_all(headers.as_bytes()).await.is_err() { return Ok::<(), String>(()); }
        for chunk in reply.chunks { if socket.write_all(&chunk).await.is_err() { return Ok(()); } }
        let _ = socket.shutdown().await;
        Ok(())
    };
    tokio::select! { result = timeout(Duration::from_secs(5), operation) => result.map_err(|_| "Provider write exceeded five seconds")?, _ = stop.cancelled() => Ok(()) }
}
async fn capture(socket: &mut TcpStream) -> Result<Option<Request>> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let count = socket.read(&mut buffer).await.map_err(|_| "Fixture request read failed")?;
        if count == 0 { return Ok(None); }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > 1048576 + 65536 { return Err("Fixture request exceeds one MiB".into()); }
        let Some(boundary) = bytes.windows(4).position(|b| b == b"\r\n\r\n") else {
            if bytes.len() > 65536 { return Err("Fixture headers exceed 64KiB".into()); }
            continue;
        };
        let headers = std::str::from_utf8(&bytes[..boundary]).map_err(|_| "Invalid fixture headers")?;
        let mut first = headers.lines().next().ok_or("Missing fixture request line")?.split_whitespace();
        let method = first.next().ok_or("Missing fixture method")?.to_owned();
        let path = first.next().ok_or("Missing fixture path")?.to_owned();
        let length = headers.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>())
        }).transpose().map_err(|_| "Invalid fixture content length")?.unwrap_or(0);
        if length > 1048576 { return Err("Fixture body exceeds one MiB".into()); }
        if headers.lines().any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:")) { return Err("Unexpected chunked fixture request".into()); }
        if bytes.len() < boundary + 4 + length { continue; }
        if bytes.len() != boundary + 4 + length { return Err("Unexpected pipelined fixture bytes".into()); }
        return Ok(Some(Request { method, path, headers: headers.to_owned(), body: bytes[boundary + 4..].to_vec() }));
    }
}

#[derive(Clone)]
pub struct Gate(Arc<tokio::sync::Semaphore>);
impl Gate {
    pub fn new() -> Self { Self(Arc::new(tokio::sync::Semaphore::new(0))) }
    pub fn release(&self) { self.0.add_permits(65536); }
    pub async fn wait(&self) { self.0.acquire().await.unwrap().forget(); }
}
