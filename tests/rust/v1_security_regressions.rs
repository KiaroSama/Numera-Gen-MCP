//! Regression tests for v1.0.0 security fixes (Plans 001-007, DD-01, DD-05).
use numera_gen_mcp::{
    adapters::{self, types::Input},
    compat::{
        js_json::{self, JsValue, MAX_JSON_DEPTH},
        value::{JsString, json},
    },
    config::{PinnedConnection, snapshot_credentials},
    http::{assets, url::operation_url},
    logging,
    mcp::js_envelope::OriginalArguments,
    runtime_paths::RuntimePaths,
};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

#[test]
fn dd01_js_json_rejects_excessive_nesting_depth_without_stack_overflow() {
    let deep = format!(
        "{}0{}",
        "[".repeat(MAX_JSON_DEPTH + 4),
        "]".repeat(MAX_JSON_DEPTH + 4)
    );
    let err = js_json::parse(&deep).unwrap_err();
    assert_eq!(err.code, "invalid_json");

    let allowed = format!(
        "{}0{}",
        "[".repeat(MAX_JSON_DEPTH - 1),
        "]".repeat(MAX_JSON_DEPTH - 1)
    );
    assert!(js_json::parse(&allowed).is_ok());
}

#[test]
fn plan001_ssrf_blocks_ipv6_transition_and_documentation_ranges() {
    for blocked in [
        "::ffff:127.0.0.1",
        "::127.0.0.1",
        "64:ff9b::127.0.0.1",
        "64:ff9b::169.254.169.254",
        "64:ff9b:1::1",
        "2002:7f00:0001::1",
        "2002:0a00:0001::1",
        "2001::1",
        "2001:db8::1",
        "3fff::1",
        "100::1",
        "fec0::1",
        "192.0.2.1",
        "198.51.100.1",
        "203.0.113.1",
        "192.88.99.1",
    ] {
        assert!(
            assets::private_address(blocked),
            "expected {blocked} to be classified private/non-public"
        );
    }
    for public in [
        "8.8.8.8",
        "1.1.1.1",
        "2606:4700:4700::1111",
        "2001:4860:4860::8888",
    ] {
        assert!(
            !assets::private_address(public),
            "expected {public} to remain public"
        );
    }
}

#[test]
fn plan002_operation_url_blocks_encoded_traversal_but_preserves_gemini_component() {
    for bad in [
        "a%2f..%2fb",
        "a%2F..%2Fb",
        "a%5c..%5cb",
        "%2e%2e/secret",
        "foo/%2e/bar",
        "foo/%00/bar",
    ] {
        let err = operation_url(
            "https://api.example/v1",
            "api-prefix",
            bad,
            "/v1",
            &json!({}),
        )
        .unwrap_err();
        assert_eq!(err.code, "invalid_configuration", "case {bad}");
    }
    // Positive controls: Gemini percent-encoded model names with %2F and spaces remain valid.
    let ok = operation_url(
        "https://generativelanguage.googleapis.com",
        "origin",
        "models/publishers%2Fgoogle%2Fmodels%2Fimagen:generateContent",
        "/v1beta",
        &json!({}),
    )
    .unwrap();
    assert!(ok.path().contains("publishers%2Fgoogle%2Fmodels%2Fimagen"));
}

#[tokio::test]
async fn plan002_asset_allowances_reject_encoded_traversal_and_pin_multiple_ips() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nOKAY")
                .await;
        }
    });
    let config = json!({
        "files": {
            "assetAllowances": [{"origin": format!("http://allowed.test:{port}"), "pathPrefix": "/allowed"}]
        }
    });
    let cancel = CancellationToken::new();
    for bad in [
        format!("http://allowed.test:{port}/allowed/%2e%2e/secret"),
        format!("http://allowed.test:{port}/allowed/%2fsecret"),
        format!("http://allowed.test:{port}/allowed/%5csecret"),
    ] {
        let err = assets::fetch_with_resolver(
            &JsString::from(bad.as_str()),
            &config,
            1024,
            &cancel,
            |_, _| async { Ok(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]) },
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "unsafe_destination");
    }

    // Valid allowed path still succeeds.
    let bytes = assets::fetch_with_resolver(
        &JsString::from(format!("http://allowed.test:{port}/allowed/item.png").as_str()),
        &config,
        1024,
        &cancel,
        |_, _| async { Ok(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]) },
    )
    .await
    .unwrap();
    assert_eq!(bytes, b"OKAY");
    let _ = server.await;
}

#[test]
fn plan003_provider_options_cannot_override_security_invariants() {
    let model = JsString::from("gpt-image-1");
    let conn_openai = json!({
        "adapter": "openai-responses",
        "orchestrationModel": "gpt-5",
        "baseUrl": "https://api.openai.com/v1",
        "auth": {"type": "none"}
    });
    for forbidden in [
        json!({"store": true}),
        json!({"tools": []}),
        json!({"tool_choice": "none"}),
        json!({"model": "evil"}),
        json!({"input": []}),
    ] {
        let req = json!({"prompt": "hi", "count": 1, "provider_options": forbidden});
        let err = adapters::build_request(&Input {
            connection: &conn_openai,
            request: &req,
            model: &model,
            operation: "generate",
            references: &[],
            mask: None,
        })
        .err()
        .unwrap();
        assert_eq!(err.code, "unsupported_parameter");
    }

    let conn_openrouter = json!({
        "adapter": "openrouter-images",
        "baseUrl": "https://openrouter.ai/api/v1",
        "auth": {"type": "none"}
    });
    for forbidden in [
        json!({"provider": null}),
        json!({"provider": false}),
        json!({"provider": {"allow_fallbacks": true}}),
        json!({"model": "evil"}),
    ] {
        let req = json!({"prompt": "hi", "count": 1, "provider_options": forbidden});
        let err = adapters::build_request(&Input {
            connection: &conn_openrouter,
            request: &req,
            model: &model,
            operation: "generate",
            references: &[],
            mask: None,
        })
        .err()
        .unwrap();
        assert_eq!(err.code, "unsupported_parameter");
    }
    // Positive control: benign OpenRouter provider.order still works with allow_fallbacks=false.
    let ok_or = adapters::build_request(&Input {
        connection: &conn_openrouter,
        request: &json!({"prompt": "hi", "count": 1, "provider_options": {"provider": {"order": ["X"]}}}),
        model: &model,
        operation: "generate",
        references: &[],
        mask: None,
    })
    .unwrap();
    if let numera_gen_mcp::http::api::Body::Json(body) = ok_or.body {
        assert_eq!(body["provider"]["allow_fallbacks"], json!(false));
        assert_eq!(body["provider"]["order"][0], json!("X"));
    } else {
        panic!("expected json body");
    }

    let conn_gemini = json!({
        "adapter": "gemini",
        "baseUrl": "https://generativelanguage.googleapis.com",
        "auth": {"type": "none"}
    });
    let err = adapters::build_request(&Input {
        connection: &conn_gemini,
        request: &json!({"prompt": "hi", "count": 1, "provider_options": {"responseModalities": ["TEXT"]}}),
        model: &JsString::from("gemini-2.5-flash-image"),
        operation: "generate",
        references: &[],
        mask: None,
    })
    .err()
    .unwrap();
    assert_eq!(err.code, "unsupported_parameter");
}

#[tokio::test]
async fn plan004_proxy_secret_env_fails_closed_via_injected_env() {
    let dir = std::env::temp_dir().join(format!("numera-p004-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let runtime = RuntimePaths::new(&dir).unwrap();
    let conn = json!({
        "adapter": "openai-images",
        "baseUrl": "https://api.example/v1",
        "auth": {"type": "none"},
        "proxy": {"url": "http://proxy.example:8080", "secretEnv": "MY_PROXY_CRED"}
    });
    let empty: HashMap<String, String> = HashMap::new();
    let err = snapshot_credentials(&conn, &empty, &runtime)
        .await
        .err()
        .expect("missing proxy secretEnv must fail closed");
    assert_eq!(err.code, "missing_credentials");

    let env = HashMap::from([("MY_PROXY_CRED".to_owned(), "Basic dXNlcjpwYXNz".to_owned())]);
    let pinned: PinnedConnection = snapshot_credentials(&conn, &env, &runtime).await.unwrap();
    assert_eq!(pinned.proxy_auth(), Some("Basic dXNlcjpwYXNz"));
    drop(pinned);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plan005_logging_redacts_key_basic_api_headers_and_data_urls() {
    let sample = json!({
        "msg": "Key fal_secret_1234567890 Basic dXNlcjpwYXNz x-api-key: sk-live-9999999999 data:image/png;base64,AAAA1234====",
        "cookie": "sid=abc",
        "safe": "hello"
    });
    let redacted = logging::redact(&sample, 0);
    let msg = redacted["msg"].as_str().unwrap();
    assert!(!msg.contains("fal_secret_1234567890"));
    assert!(!msg.contains("dXNlcjpwYXNz"));
    assert!(!msg.contains("sk-live-9999999999"));
    assert!(!msg.contains("AAAA1234===="));
    assert!(msg.contains("Key [REDACTED]"));
    assert!(msg.contains("Basic [REDACTED]"));
    assert!(msg.contains("x-api-key: [REDACTED]"));
    assert!(msg.contains("data:[REDACTED]"));
    assert_eq!(redacted["cookie"], json!("[REDACTED]"));
    assert_eq!(redacted["safe"], json!("hello"));
}

#[test]
fn plan007_original_arguments_take_frees_payload_once() {
    let original = OriginalArguments::new(JsValue::Object(Vec::new()));
    assert!(original.take_arguments().is_some());
    assert!(original.take_arguments().is_none());
}

#[tokio::test]
async fn plan002_asset_fetch_rejects_non_http_and_oversized_response() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let _ = stream
                .write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1/secret\r\nContent-Length: 0\r\n\r\n")
                .await;
        }
    });
    let config = json!({
        "files": {
            "downloadTimeoutMs": 5000,
            "assetAllowances": [{"origin": format!("http://allowed.test:{port}"), "pathPrefix": "/allowed"}]
        }
    });
    let cancel = CancellationToken::new();
    let err = assets::fetch_with_resolver(
        &JsString::from(format!("http://allowed.test:{port}/allowed/redir.png").as_str()),
        &config,
        1024,
        &cancel,
        |_, _| async { Ok(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]) },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "unsafe_destination");
    let _ = server.await;
}
