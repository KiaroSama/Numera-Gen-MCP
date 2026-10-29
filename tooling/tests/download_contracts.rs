use numera_task::fetch::{Limits, bounded_get};
use std::{sync::{Arc, atomic::{AtomicUsize, Ordering}}, time::Duration};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};

async fn server(responses: Vec<Vec<u8>>) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/tool", listener.local_addr().unwrap());
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let task = tokio::spawn(async move {
        for response in responses {
            let accepted = tokio::time::timeout(Duration::from_secs(2), listener.accept()).await;
            let Ok(Ok((mut socket, _))) = accepted else { break; };
            count.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0u8; 4096];
            tokio::time::timeout(Duration::from_secs(1), socket.read(&mut buffer)).await.unwrap().unwrap();
            // Hold an incomplete response until the downloader closes it, without a blind sleep.
            if response == b"stall" {
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\npartial").await.unwrap();
                let _ = tokio::time::timeout(Duration::from_secs(1), socket.read(&mut buffer)).await;
            } else {
                socket.write_all(&response).await.unwrap();
                socket.shutdown().await.unwrap();
            }
        }
    });
    (url, requests, task)
}
fn limits() -> Limits { Limits { bytes: 32, wall: Duration::from_secs(2), idle: Duration::from_millis(50) } }

#[tokio::test]
async fn timeout_retry_discards_partial_bytes_and_preserves_attempt_and_idle_bounds() {
    let (url, count, task) = server(vec![b"stall".to_vec(), b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nverified".to_vec()]).await;
    let result = bounded_get(&url, limits()).await;
    task.await.unwrap();
    assert_eq!(result.unwrap(), b"verified");
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let (url, count, task) = server(vec![b"stall".to_vec(); 3]).await;
    let result = bounded_get(&url, limits()).await;
    task.await.unwrap();
    assert!(result.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn oversized_incomplete_permanent_error_and_unsafe_redirect_are_never_retried() {
    for response in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 33\r\n\r\n".to_vec(),
        [b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".as_slice(), &[b'x';33]].concat(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nshort".to_vec(),
        b"HTTP/1.1 404 Missing\r\nContent-Length: 0\r\n\r\n".to_vec(),
        b"HTTP/1.1 302 Found\r\nLocation: http://example.invalid/leak\r\nContent-Length: 0\r\n\r\n".to_vec(),
    ] {
        let (url, count, task) = server(vec![response]).await;
        let result = bounded_get(&url, limits()).await;
        task.await.unwrap();
        assert!(result.is_err());
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    for url in ["file:///private", "http://localhost/tool", "https://user:password@github.com/tool", "https://example.invalid/tool"] {
        assert!(bounded_get(url, limits()).await.is_err());
    }
    let mut empty = limits(); empty.bytes = 0;
    assert!(bounded_get("https://static.rust-lang.org/tool", empty).await.is_err());
    let mut expired = limits(); expired.wall = Duration::ZERO;
    assert!(bounded_get("https://static.rust-lang.org/tool", expired).await.is_err());
}
