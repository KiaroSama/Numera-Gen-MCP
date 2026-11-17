use super::{
    fixtures as f, host,
    provider::{Provider, Reply},
    support as s,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hunyuan_usdz_sidecar_preserves_verified_primary_glb() {
    host::scenario(25000, async {
        let work = f::workspace();
        let directory = f::directory(&work, "audit-hunyuan-usdz");
        let (url, mime, bytes) = s::data("3d");
        let output = json!({
            "model_glb":{"url":url,"content_type":mime},
            "model_urls":{
                "glb":{"url":url,"content_type":mime},
                "obj":null,
                "fbx":null,
                "usdz":{"url":"https://example.invalid/unsupported.usdz"}
            }
        });
        let wire = s::normal_provider(s::MESH, output, Arc::new(AtomicUsize::new(0))).await;
        let config = s::config(
            &directory,
            &wire.origin,
            &[(s::MESH, "text-to-3d", &["3d", "image"])],
        );
        let mut app = s::start(&directory, &config, "audit-hunyuan-usdz").await;
        let done = s::generate(&app, s::request("usdz", s::MESH, "text-to-3d", true, &[])).await;
        s::verified(&app, &done, "3d", mime, &bytes).await;
        assert_eq!(wire.posts(), 1);
        assert!(done["errors"].as_array().unwrap().is_empty());
        app.close().await.unwrap();
        wire.close().await;
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_download_recovery_resolves_only_the_recovered_failure() {
    host::scenario(30000, download_recovery(false)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_download_recovery_keeps_unresolved_sibling_errors() {
    host::scenario(30000, download_recovery(true)).await;
}

async fn download_recovery(bad_sibling: bool) {
    let work = f::workspace();
    let directory = f::directory(&work, "audit-download-recovery");
    let ready = Arc::new(AtomicBool::new(false));
    let downloads = Arc::new(AtomicUsize::new(0));
    let kind = if bad_sibling { "3d" } else { "audio" };
    let (_, mime, original) = s::data(kind);
    let bytes = original.clone();
    let available = ready.clone();
    let count = downloads.clone();
    let asset = Provider::new(move |request, _| {
        let available = available.clone();
        let count = count.clone();
        let bytes = bytes.clone();
        async move {
            assert_eq!(request.method, "GET");
            assert_eq!(request.header("authorization"), None);
            count.fetch_add(1, Ordering::SeqCst);
            if available.load(Ordering::SeqCst) {
                Ok(Reply::bytes(200, mime, bytes))
            } else {
                Ok(Reply::status(
                    json!({"error":"temporary fixture outage"}),
                    503,
                ))
            }
        }
    })
    .await;
    let endpoint = if bad_sibling {
        "fal-ai/hyper3d/rodin"
    } else {
        s::AUDIO
    };
    let mode = if bad_sibling {
        "text-to-3d"
    } else {
        "text-to-audio"
    };
    let kinds: &[&str] = if bad_sibling {
        &["3d", "image"]
    } else {
        &["audio"]
    };
    let url = format!("{}/original", asset.origin);
    let output = if bad_sibling {
        json!({"model_mesh":{"url":url,"content_type":mime},"textures":[{"url":false}]})
    } else {
        s::output(endpoint, &url, mime)
    };
    let wire = Provider::new(move |request, _| {
        let output = output.clone();
        async move {
            s::authenticated(&request);
            if request.method == "POST" {
                return Ok(Reply::json(s::handle(endpoint, "download")));
            }
            if request.path.contains("/status") {
                return Ok(Reply::json(
                    json!({"request_id":"download","status":"COMPLETED"}),
                ));
            }
            Ok(Reply::json(output))
        }
    })
    .await;
    let mut config = s::config(&directory, &wire.origin, &[(endpoint, mode, kinds)]);
    config["files"]["assetAllowances"] = json!([{"origin":asset.origin,"pathPrefix":"/original"}]);
    let mut app = s::start(&directory, &config, "audit-download-first").await;
    let mut request = s::request("download", endpoint, mode, true, &[]);
    request["wait"] = json!(false);
    s::generate(&app, request.clone()).await;
    let failed = app.job("download", true).await;
    assert!(
        failed["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| { error["code"] == "input_file_error" && error["stage"] == "download" })
    );
    assert!(failed["outputs"].as_array().unwrap().is_empty());
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
    app.close().await.unwrap();
    ready.store(true, Ordering::SeqCst);
    let mut reopened = s::start(&directory, &config, "audit-download-reopened").await;
    let recovered = reopened.job("download", true).await;
    assert_eq!(
        recovered["status"],
        if bad_sibling { "partial" } else { "completed" }
    );
    let outputs = recovered["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(
        std::fs::read(outputs[0]["path"].as_str().unwrap()).unwrap(),
        original
    );
    assert_eq!(
        outputs[0]["sha256"],
        numera_gen_mcp::compat::hash::sha256(&original)
    );
    let errors = recovered["errors"].as_array().unwrap();
    assert!(
        !errors
            .iter()
            .any(|error| error["code"] == "input_file_error")
    );
    assert_eq!(errors.is_empty(), !bad_sibling);
    if bad_sibling {
        assert!(
            errors
                .iter()
                .any(|error| error["code"] == "invalid_response")
        );
    }
    assert!(
        recovered["recovered_errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| { error["code"] == "input_file_error" && error["stage"] == "download" })
    );
    let again = reopened.job("download", true).await;
    assert_eq!(again["outputs"], recovered["outputs"]);
    let replay = s::generate(&reopened, request).await;
    assert_eq!(replay["outputs"], recovered["outputs"]);
    assert_eq!(wire.posts(), 1);
    assert_eq!(downloads.load(Ordering::SeqCst), 2);
    reopened.close().await.unwrap();
    wire.close().await;
    asset.close().await;
}

fn write_key(path: &Path, value: &str) {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).unwrap();
    file.write_all(value.as_bytes()).unwrap();
    file.sync_all().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_waiter_pins_credentials_when_key_rotates_after_submission() {
    host::scenario(30000, rotation(false)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_waiter_pins_credentials_across_consecutive_status_polls() {
    host::scenario(30000, rotation(true)).await;
}

async fn rotation(after_poll: bool) {
    let work = f::workspace();
    let directory = f::directory(&work, "audit-media-key-rotation");
    let key = directory.join("private-fixture-key");
    write_key(&key, "synthetic-before");
    let source = key.clone();
    let polls = Arc::new(AtomicUsize::new(0));
    let seen = polls.clone();
    let (url, mime, bytes) = s::data("audio");
    let output = s::output(s::AUDIO, &url, mime);
    let wire = Provider::new(move |request, _| {
        let key = source.clone();
        let polls = seen.clone();
        let output = output.clone();
        async move {
            let next = request.path.contains("next");
            if request.method == "POST" {
                let first = request.json()["prompt"] == "first";
                assert_eq!(request.header("authorization"), Some(if first {
                    "Key synthetic-before"
                } else {
                    "Key synthetic-after"
                }));
                if first && !after_poll {
                    write_key(&key, "synthetic-after");
                }
                return Ok(Reply::json(s::handle(s::AUDIO, if first { "owned" } else { "next" })));
            }
            assert_eq!(request.header("authorization"), Some(if next {
                "Key synthetic-after"
            } else {
                "Key synthetic-before"
            }));
            if request.path.contains("/status") {
                let waiting = !next && after_poll && polls.fetch_add(1, Ordering::SeqCst) == 0;
                if waiting {
                    write_key(&key, "synthetic-after");
                }
                return Ok(Reply::json(json!({"request_id":if next {"next"} else {"owned"},"status":if waiting {"IN_PROGRESS"} else {"COMPLETED"}})));
            }
            Ok(Reply::json(output))
        }
    })
    .await;
    let mut config = s::config(
        &directory,
        &wire.origin,
        &[(s::AUDIO, "text-to-audio", &["audio"])],
    );
    config["connections"]["local"]["auth"] =
        json!({"type":"key","secretFile":key,"origin":wire.origin});
    let mut app = s::start(&directory, &config, "audit-media-rotation").await;
    for name in ["first", "next"] {
        let mut request = s::request(name, s::AUDIO, "text-to-audio", true, &[]);
        request["prompt"] = json!(name);
        let done = s::generate(&app, request).await;
        s::verified(&app, &done, "audio", mime, &bytes).await;
    }
    assert_eq!(wire.posts(), 2);
    let text = app.text();
    assert!(!text.contains("synthetic-before"));
    assert!(!text.contains("synthetic-after"));
    app.close().await.unwrap();
    wire.close().await;
}

#[test]
fn unknown_hunyuan_variant_and_excessive_artifacts_remain_rejected() {
    use numera_gen_mcp::media::fal;
    let good = json!({"url":"https://example.invalid/model.glb"});
    let output: Value = json!({"model_glb":good,"model_urls":{"unrecognized":good}});
    assert!(fal::outputs(s::MESH, &output).is_err());
    assert!(fal::outputs(s::IMAGE, &json!({"images":vec![good;65]})).is_err());
}
