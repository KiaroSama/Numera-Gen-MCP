mod common;
use base64::{Engine, engine::general_purpose::STANDARD};
use common::Workspace;
use numera_gen_mcp::{
    compat::value::{Value, json},
    config::Config,
    files::images::{decode_base64, inspect},
    jobs::{NativePersistence, Persistence, Store},
    logging::Logger,
    runtime_paths::RuntimePaths,
    services::{generation::Generation, jobs},
};
use std::{
    cell::Cell,
    collections::HashMap,
    fs::File,
    io,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tokio_util::sync::CancellationToken;

struct PersistenceObservation {
    started: Instant,
    file_us: [AtomicU64; 8],
    setup_us: [AtomicU64; 5],
    receipt_updates: AtomicUsize,
    after_unlink_updates: AtomicUsize,
    last_update_us: AtomicU64,
}

impl PersistenceObservation {
    fn new(started: Instant) -> Self {
        Self {
            started,
            file_us: std::array::from_fn(|_| AtomicU64::new(u64::MAX)),
            setup_us: std::array::from_fn(|_| AtomicU64::new(u64::MAX)),
            receipt_updates: AtomicUsize::new(0),
            after_unlink_updates: AtomicUsize::new(0),
            last_update_us: AtomicU64::new(u64::MAX),
        }
    }

    fn stamp(&self, index: usize) {
        self.file_us[index].store(self.started.elapsed().as_micros() as u64, Ordering::Relaxed);
    }
}

struct ObservedPersistence(Arc<PersistenceObservation>);

impl Persistence for ObservedPersistence {
    fn before_receipt_update(&self) -> numera_gen_mcp::error::Result<()> {
        NativePersistence.before_receipt_update()?;
        self.0.receipt_updates.fetch_add(1, Ordering::Relaxed);
        if self.0.file_us[7].load(Ordering::Relaxed) != u64::MAX {
            self.0.after_unlink_updates.fetch_add(1, Ordering::Relaxed);
        }
        self.0.last_update_us.store(
            self.0.started.elapsed().as_micros() as u64,
            Ordering::Relaxed,
        );
        Ok(())
    }

    fn write(&self, file: &mut File, bytes: &[u8]) -> io::Result<()> {
        self.0.stamp(0);
        NativePersistence.write(file, bytes)?;
        self.0.stamp(1);
        Ok(())
    }

    fn sync(&self, file: &File) -> io::Result<()> {
        self.0.stamp(2);
        NativePersistence.sync(file)?;
        self.0.stamp(3);
        Ok(())
    }

    fn link(&self, original: &Path, target: &Path) -> io::Result<()> {
        self.0.stamp(4);
        NativePersistence.link(original, target)?;
        self.0.stamp(5);
        Ok(())
    }

    fn unlink(&self, path: &Path) -> io::Result<()> {
        self.0.stamp(6);
        NativePersistence.unlink(path)?;
        self.0.stamp(7);
        Ok(())
    }
}

fn generation(root: &Path, origin: &str, observation: Arc<PersistenceObservation>) -> Generation {
    let mark = |index: usize| {
        observation.setup_us[index].store(
            observation.started.elapsed().as_micros() as u64,
            Ordering::Relaxed,
        );
    };
    mark(0);
    let config = Config::parse(
        &json!({
            "schemaVersion":1,"defaultConnection":"local",
            "outputDir":root.join("outputs").to_str().unwrap(),
            "stateDir":root.join("state").to_str().unwrap(),
            "logging":{"directory":root.join("logs").to_str().unwrap()},
            "files":{"allowedInputRoots":[root.join("inputs").to_str().unwrap()]},
            "connections":{"local":{
                "adapter":"openai-responses","baseUrl":format!("{origin}/v1"),
                "auth":{"type":"none"},"orchestrationModel":"text","defaultModel":"image"
            }}
        })
        .to_string(),
        &root.join("config.json"),
        &[],
        &HashMap::new(),
    )
    .unwrap();
    let (_, connection) = config.select(None).unwrap();
    assert_eq!(connection["baseUrlMode"], "api-prefix");
    assert_eq!(connection["requestTimeoutMs"], 300000);
    assert_eq!(connection["discoveryTimeoutMs"], 15000);
    assert_eq!(connection["maxConcurrentRequests"], 2);
    mark(1);
    let store = Store::open_with_persistence(
        &root.join("state"),
        ObservedPersistence(observation.clone()),
    )
    .unwrap();
    mark(2);
    let logger = Logger::new(&root.join("logs"), "ERROR");
    mark(3);
    let runtime = RuntimePaths::new(root).unwrap();
    mark(4);
    Generation {
        config: Arc::new(config),
        store: Arc::new(Mutex::new(store)),
        logger: Arc::new(logger),
        runtime,
        env: HashMap::new().into(),
        active: Arc::new(Mutex::new(HashMap::new())),
    }
}

fn original() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".ci-work/parity/image-acceptance.json");
    assert!(std::fs::metadata(&path).unwrap().len() <= 1048576);
    let rows = Value::parse_with_limit(&std::fs::read_to_string(path).unwrap(), 1048576).unwrap();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "png")
        .unwrap();
    let bytes = decode_base64(row["base64"].as_str().unwrap(), 1048576).unwrap();
    let image = inspect(bytes.clone(), &json!({}), None).unwrap();
    assert_eq!((image.width, image.height), (8, 6));
    assert_eq!(image.mime, "image/png");
    assert!(image.alpha);
    assert_eq!(image.bytes, bytes);
    bytes
}

async fn request(socket: &mut TcpStream) -> (String, String) {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = socket.read(&mut buffer).await.unwrap();
            assert!(
                read > 0,
                "race fixture request ended before its declared body"
            );
            bytes.extend_from_slice(&buffer[..read]);
            assert!(
                bytes.len() <= 1048576 + 65536,
                "bounded race fixture request"
            );
            let Some(boundary) = bytes.windows(4).position(|chunk| chunk == b"\r\n\r\n") else {
                assert!(bytes.len() <= 65536, "bounded race fixture headers");
                continue;
            };
            assert!(boundary <= 65536);
            let headers = std::str::from_utf8(&bytes[..boundary]).unwrap();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            assert!(length <= 1048576, "bounded race fixture body");
            if bytes.len() < boundary + 4 + length {
                continue;
            }
            assert_eq!(bytes.len(), boundary + 4 + length);
            let mut first = headers.lines().next().unwrap().split_whitespace();
            return (
                first.next().unwrap().to_owned(),
                first.next().unwrap().to_owned(),
            );
        }
    })
    .await
    .expect("race fixture socket read exceeded three seconds")
}

async fn reply(socket: &mut TcpStream, value: &Value) {
    let body = value.to_string();
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        socket.write_all(headers.as_bytes()).await.unwrap();
        socket.write_all(body.as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
    })
    .await
    .expect("race fixture socket write exceeded three seconds");
}

#[test]
fn exact_source_refresher_fences_original_responses_waiter_without_duplicate_output() {
    let workspace = Workspace::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(waiter_case(&workspace));
}

async fn waiter_case(workspace: &Workspace) {
    let bytes = original();
    let server_stage = Cell::new("setup");
    let client_stage = Cell::new("setup");
    let posts_seen = Cell::new(0);
    let polls_seen = Cell::new(0);
    let started = Instant::now();
    let server_us: [Cell<Option<u128>>; 8] = std::array::from_fn(|_| Cell::new(None));
    let client_us: [Cell<Option<u128>>; 6] = std::array::from_fn(|_| Cell::new(None));
    let mark_server = |index: usize, label: &'static str| {
        server_stage.set(label);
        server_us[index].set(Some(started.elapsed().as_micros()));
    };
    let mark_client = |index: usize, label: &'static str| {
        client_stage.set(label);
        client_us[index].set(Some(started.elapsed().as_micros()));
    };
    let observation = Arc::new(PersistenceObservation::new(started));
    let completed = json!({"id":"owned","status":"completed","output":[{
        "type":"image_generation_call","status":"completed","result":STANDARD.encode(&bytes)
    }]});
    let scenario = async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let generation = generation(&workspace.0, &origin, observation.clone());
        mark_server(0, "setup_done");
        let signal = CancellationToken::new();
        let (ready, mut waiting) = oneshot::channel::<()>();
        let (release, held) = oneshot::channel::<()>();
        // The first GET socket stays owned while the second GET is accepted and answered.
        let server = async {
            let mut posts = 0;
            let mut polls = 0;
            let (mut submission, _) = listener.accept().await.unwrap();
            let (method, path) = request(&mut submission).await;
            assert_eq!((method.as_str(), path.as_str()), ("POST", "/v1/responses"));
            posts += 1;
            posts_seen.set(posts);
            mark_server(1, "post_received");
            reply(
                &mut submission,
                &json!({"id":"owned","status":"in_progress"}),
            )
            .await;
            drop(submission);
            mark_server(2, "post_replied");
            let (mut first, _) = listener.accept().await.unwrap();
            let (method, path) = request(&mut first).await;
            assert_eq!(
                (method.as_str(), path.as_str()),
                ("GET", "/v1/responses/owned")
            );
            polls += 1;
            polls_seen.set(polls);
            mark_server(3, "first_get_held");
            ready
                .send(())
                .expect("original waiter ended before its first GET was held");
            let (mut second, _) = listener.accept().await.unwrap();
            let (method, path) = request(&mut second).await;
            assert_eq!(
                (method.as_str(), path.as_str()),
                ("GET", "/v1/responses/owned")
            );
            polls += 1;
            polls_seen.set(polls);
            mark_server(4, "second_get_received");
            reply(&mut second, &completed).await;
            drop(second);
            mark_server(5, "second_get_replied");
            held.await
                .expect("refresher must release the held original waiter");
            mark_server(6, "held_released");
            reply(&mut first, &completed).await;
            mark_server(7, "first_get_replied");
            (posts, polls)
        };
        let client = async {
            let original = generation.run(
                json!({"prompt":"draw","request_id":"responses-race"}),
                "generate",
                &signal,
            );
            tokio::pin!(original);
            mark_client(0, "waiting_ready");
            tokio::select! {
                ready = &mut waiting => ready.expect("fixture ended before its held first GET"),
                outcome = original.as_mut() => {
                    panic!("original generation ended before held-poll readiness: {:?}", outcome.map(|receipt| receipt.status().to_owned()));
                }
            }
            mark_client(1, "refresher_waiting");
            let recovered = jobs::get(&generation, "responses-race", true, &signal, None).await;
            mark_client(2, "refresher_returned");
            // Settle the original even when recovery returns an error, before asserting it.
            release
                .send(())
                .expect("held original socket must still be owned");
            mark_client(3, "release_sent");
            original.as_mut().await.unwrap();
            mark_client(4, "original_settled");
            let recovered = recovered.unwrap();
            assert_eq!(recovered.status(), "completed");
            let stored = generation
                .store
                .lock()
                .unwrap()
                .get("responses-race")
                .unwrap();
            assert_eq!(stored.value["outputs"].as_array().unwrap().len(), 1);
            assert_eq!(
                generation
                    .store
                    .lock()
                    .unwrap()
                    .list_outputs(0, 25)
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                std::fs::read(stored.value["outputs"][0]["path"].as_str().unwrap()).unwrap(),
                bytes
            );
            mark_client(5, "assertions_done");
        };
        let ((posts, polls), ()) = tokio::join!(server, client);
        assert_eq!(posts, 1);
        assert_eq!(polls, 2);
    };
    // Keep the source's three-second scenario bound, not the provider's default timeout.
    let (result, resolved_us) = {
        let deadline = tokio::time::timeout(Duration::from_secs(3), scenario);
        tokio::pin!(deadline);
        let result = deadline.as_mut().await;
        // Sample before the unchanged scenario drop can close its SQLite connection.
        (result, started.elapsed().as_micros())
    };
    let file_us: [Option<u64>; 8] = std::array::from_fn(|index| {
        let value = observation.file_us[index].load(Ordering::Relaxed);
        (value != u64::MAX).then_some(value)
    });
    if result.is_err() {
        eprintln!(
            "Responses waiter setup_us [begin,config_done,store_done,logger_done,runtime_done]={:?}",
            observation.setup_us.each_ref().map(|value| {
                let value = value.load(Ordering::Relaxed);
                (value != u64::MAX).then_some(value)
            })
        );
        eprintln!(
            "Responses waiter timeout: server_stage={} client_stage={} posts={} polls={} resolved_us={} elapsed_ms={}",
            server_stage.get(),
            client_stage.get(),
            posts_seen.get(),
            polls_seen.get(),
            resolved_us,
            started.elapsed().as_millis()
        );
        eprintln!(
            "Responses waiter server_us [setup_done,post_received,post_replied,first_get_held,second_get_received,second_get_replied,held_released,first_get_replied]={:?}",
            server_us.each_ref().map(|value| value.get())
        );
        eprintln!(
            "Responses waiter client_us [waiting_ready,refresher_waiting,refresher_returned,release_sent,original_settled,assertions_done]={:?}",
            client_us.each_ref().map(|value| value.get())
        );
        eprintln!(
            "Responses waiter file_us [write_begin,write_end,sync_begin,sync_end,link_begin,link_end,unlink_begin,unlink_end]={file_us:?}"
        );
        // Entries are not commit acknowledgements; directory sync and SQL still follow unlink.
        let last_update = observation.last_update_us.load(Ordering::Relaxed);
        eprintln!(
            "Responses waiter receipt_update_entries={} after_unlink_entries={} last_update_us={:?}",
            observation.receipt_updates.load(Ordering::Relaxed),
            observation.after_unlink_updates.load(Ordering::Relaxed),
            (last_update != u64::MAX).then_some(last_update)
        );
    }
    result.expect("exact source Responses waiter scenario exceeded three seconds");
    // Exercise the observation on this same real publication, without a timing threshold.
    assert!(file_us.iter().all(Option::is_some));
    assert!(file_us.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(observation.after_unlink_updates.load(Ordering::Relaxed) >= 2);
}
