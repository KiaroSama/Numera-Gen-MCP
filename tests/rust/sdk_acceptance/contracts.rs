use serde_json::{Value, json};

pub fn probe_stages(before: &str, after: &str) -> Value {
    let Some(delta) = after.strip_prefix(before) else {
        return json!({"capture":"unobserved","stages":[]});
    };
    let mut stages = Vec::new();
    let mut first_timestamp = None;
    // split_terminator alone would admit an unfinished last row; the source only reads newline rows.
    let complete = delta.rsplit_once('\n').map_or("", |(complete, _)| complete);
    for line in complete.split('\n') {
        let Ok(event) = serde_json::from_str::<Value>(line) else { continue; };
        let stage = if event["component"] == "tool" && event["tool"] == "generate_image" {
            match event["message"].as_str() {
                Some("Tool started.") => "started",
                Some("Tool completed.") => "completed",
                Some("Tool failed.") => "failed",
                _ => continue,
            }
        } else if event["component"] == "generation" && event["message"] == "Submission intent persisted." {
            "intent"
        } else if event["component"] == "http" && event["message"] == "Submission transport prepared." {
            "transport_prepared"
        } else { continue; };
        let mut entry = json!({"stage":stage});
        let fields: &[&str] = match stage {
            "transport_prepared" => &["client_build_ms", "pre_send_ms", "timeout_ms"],
            "intent" => &["before_submit_ms", "prepare_ms", "admission_ms", "reserve_ms", "update_ms"],
            _ => &["duration_ms"],
        };
        for field in fields {
            if event[*field].as_f64().is_some_and(|n| n.is_finite() && n >= 0.0) {
                entry[*field] = event[*field].clone();
            }
        }
        if let Some(timestamp) = event["timestamp"].as_str().filter(|t| exact_milliseconds(t))
            && let Ok(timestamp) = chrono::DateTime::parse_from_rfc3339(timestamp) {
            let timestamp = timestamp.timestamp_millis();
            let first = *first_timestamp.get_or_insert(timestamp);
            if timestamp >= first { entry["elapsed_from_first_ms"] = json!(timestamp - first); }
        }
        stages.push(entry);
    }
    if stages.len() > 8 { stages.drain(..stages.len() - 8); }
    json!({"capture":"available_rows_only","stages":stages})
}
fn exact_milliseconds(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 24 && bytes.iter().enumerate().all(|(n, b)| {
        match n { 4 | 7 => *b == b'-', 10 => *b == b'T', 13 | 16 => *b == b':', 19 => *b == b'.', 23 => *b == b'Z', _ => b.is_ascii_digit() }
    })
}

#[test]
fn probe_diagnostics_expose_only_fixed_stages_and_nonnegative_numeric_durations() {
    let before = "Earlier capture\n";
    let secret = "synthetic-private";
    let rows = [
        json!({"component":"tool","tool":"generate_image","message":"Tool started.","call_id":secret,"timestamp":"2026-10-17T00:00:00.000Z"}),
        json!({"component":"generation","message":"Submission intent persisted.","requestId":secret,"timestamp":"2026-10-17T00:00:00.600Z","before_submit_ms":1000,"prepare_ms":900,"admission_ms":80,"reserve_ms":20,"update_ms":1}),
        json!({"component":"http","message":"Submission transport prepared.","client_build_ms":900,"pre_send_ms":901,"timeout_ms":700,"timestamp":"2026-10-17T00:00:00.635Z","url":secret,"authorization":secret}),
        json!({"component":"http","message":"Submission transport prepared.","client_build_ms":-1,"pre_send_ms":null,"timeout_ms":secret}),
        json!({"component":"tool","tool":"generate_image","message":"Tool completed.","duration_ms":701,"timestamp":"2026-10-17T00:00:01.572Z","prompt":secret}),
        json!({"component":"tool","tool":"generate_image","message":"Tool failed.","duration_ms":secret}),
        json!({"component":"tool","tool":"generate_image","message":"constructor"}),
        json!({"component":"tool","tool":"get_job","message":"Tool started."}),
        Value::Null,
    ];
    let after = before.to_owned() + &rows.iter().map(|v| v.to_string() + "\n").collect::<String>();
    let projected = probe_stages(before, &(after.clone() + "{\"unfinished\":"));
    assert_eq!(projected, json!({"capture":"available_rows_only","stages":[
        {"stage":"started","elapsed_from_first_ms":0},
        {"stage":"intent","before_submit_ms":1000,"prepare_ms":900,"admission_ms":80,"reserve_ms":20,"update_ms":1,"elapsed_from_first_ms":600},
        {"stage":"transport_prepared","client_build_ms":900,"pre_send_ms":901,"timeout_ms":700,"elapsed_from_first_ms":635},
        {"stage":"transport_prepared"},
        {"stage":"completed","duration_ms":701,"elapsed_from_first_ms":1572},
        {"stage":"failed"}
    ]}));
    assert!(!projected.to_string().contains(secret));
    assert!(!projected.to_string().contains("2026-10-17"));
    assert_eq!(probe_stages("", &(json!({"component":"generation","message":"Submission intent persisted.","before_submit_ms":secret,"prepare_ms":-1,"admission_ms":null,"reserve_ms":{},"update_ms":null}).to_string()+"\n"))["stages"],json!([{"stage":"intent"}]));
    for timestamp in [Value::Null,json!(0),json!({}),json!(secret),json!("2026-99-17T00:00:00.000Z"),json!("2026-10-17T00:00:00Z")] {
        assert_eq!(probe_stages("",&(json!({"component":"generation","message":"Submission intent persisted.","timestamp":timestamp}).to_string()+"\n"))["stages"],json!([{"stage":"intent"}]));
    }
    let reversed = ["2026-10-17T00:00:00.600Z","2026-10-17T00:00:00.000Z","2026-10-17T00:00:00.635Z"].iter().map(|timestamp|json!({"component":"generation","message":"Submission intent persisted.","timestamp":timestamp}).to_string()+"\n").collect::<String>();
    assert_eq!(probe_stages("",&reversed)["stages"],json!([{"stage":"intent","elapsed_from_first_ms":0},{"stage":"intent"},{"stage":"intent","elapsed_from_first_ms":35}]));
    assert_eq!(probe_stages("",&after.repeat(3))["stages"].as_array().unwrap().len(),8);
    assert_eq!(probe_stages(before,&after[before.len()..]),json!({"capture":"unobserved","stages":[]}));
}

#[tokio::test]
async fn original_malformed_response_can_omit_content_type_without_changing_its_bytes() {
    use super::{host, provider::{Provider, Reply}};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    host::scenario(10000, async {
        let provider = Provider::new(|_, _| async { Ok(Reply::bytes(200,"",b"not-json".to_vec())) }).await;
        let mut socket = host::bounded(1000, tokio::net::TcpStream::connect(provider.origin.strip_prefix("http://").unwrap())).await.unwrap();
        host::bounded(1000, socket.write_all(b"GET / HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n")).await.unwrap();
        let mut bytes = Vec::new();
        host::bounded(1000, (&mut socket).take(4096).read_to_end(&mut bytes)).await.unwrap();
        let response = String::from_utf8(bytes).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        assert!(!headers.to_ascii_lowercase().contains("content-type:"));
        assert!(headers.contains("Content-Length: 8"));
        assert_eq!(body,"not-json");
        provider.close().await;
    }).await;
}

#[tokio::test]
async fn cancelling_provider_finish_aborts_its_owner_and_drops_pending_handlers() {
    use super::{host, provider::{Gate, Provider, Reply}};
    use std::{future::Future, task::Poll};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_util::sync::CancellationToken;
    host::scenario(10000, async {
        let entered = Gate::new(); let enter = entered.clone();
        let dropped = CancellationToken::new(); let observed = dropped.clone();
        let provider = Provider::new(move |_, _| {
            let enter = enter.clone(); let observed = observed.clone();
            async move {
                let _drop_observation = observed.drop_guard();
                enter.release();
                std::future::pending::<host::Result<Reply>>().await
            }
        }).await;
        let mut socket = host::bounded(1000, tokio::net::TcpStream::connect(provider.origin.strip_prefix("http://").unwrap())).await.unwrap();
        host::bounded(1000, socket.write_all(b"GET / HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n")).await.unwrap();
        host::bounded(1000, entered.wait()).await;
        let mut finishing = Box::pin(provider.finish());
        std::future::poll_fn(|context| {
            assert!(finishing.as_mut().poll(context).is_pending());
            Poll::Ready(())
        }).await;
        drop(finishing);
        host::bounded(1000, dropped.cancelled()).await;
        let mut bytes = Vec::new();
        host::bounded(1000, (&mut socket).take(4096).read_to_end(&mut bytes)).await.unwrap();
        assert!(bytes.is_empty(), "cancelled provider cleanup must close its owned socket");
    }).await;
}

#[tokio::test]
async fn late_provider_handler_error_fails_even_when_scenario_body_already_passed() {
    use super::{host, provider::{Gate, Provider}};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for run_fails in [false, true] {
        let entered = Gate::new(); let enter = entered.clone();
        let provider = Provider::new(move |_, stop| {
            let enter = enter.clone();
            async move { enter.release(); stop.cancelled().await; Err("Controlled late handler failure".into()) }
        }).await;
        let address = provider.origin.strip_prefix("http://").unwrap();
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        socket.write_all(b"GET / HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n").await.unwrap();
        host::bounded(1000,entered.wait()).await;
        let mut errors = if run_fails { vec!["Controlled original scenario failure".to_owned()] } else { Vec::new() };
        errors.extend(provider.finish().await);
        assert!(errors.iter().any(|e|e=="Controlled late handler failure"));
        if run_fails { assert_eq!(errors[0],"Controlled original scenario failure"); }
        let mut bytes=Vec::new();host::bounded(1000,socket.read_to_end(&mut bytes)).await.unwrap();
        assert!(bytes.is_empty(),"late failed handler must close its owned response socket");
    }
}
