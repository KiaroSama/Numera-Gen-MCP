use numera_image_gen_mcp::http::{url::operation_url, assets::private_address, sse::events};
use serde_json::json;

#[test]
fn operation_prefix_and_sensitive_query_are_preserved() {
    let url=operation_url("https://example.org/v1","api-prefix","images/generations","/v1",&json!({})).expect("valid prefix");
    assert_eq!(url.as_str(),"https://example.org/v1/images/generations");
    assert!(operation_url("https://example.org/v1","api-prefix","images/generations","/v1",&json!({"api_key":"secret"})).is_err());
    assert!(operation_url("https://example.org/v1","origin","models","/v1",&json!({})).is_err());
}

#[test]
fn private_asset_destinations_remain_blocked() {
    for address in ["127.0.0.1","10.0.0.1","169.254.169.254","::1","::ffff:127.0.0.1","::ffff:7f00:1","2001:db8::1"] {
        assert!(private_address(address),"{address}");
    }
    assert!(!private_address("8.8.8.8"));
}

#[test]
fn terminal_sse_is_not_discarded_as_generic_error() {
    let value=events(b"event: response.failed\ndata: {\"response\":{\"id\":\"same-job\",\"status\":\"failed\"}}\n\n",1024).expect("terminal frame");
    assert_eq!(value[0].kind,"response.failed");
    assert!(events(b"data: {}",1024).is_err());
    let mut reader=numera_image_gen_mcp::http::sse::Reader::new(1024);
    assert!(reader.push(b"event: error\ndata: {}\n\n").is_err());
    assert_eq!(events(b"event: error\ndata: {}\n\n",1024).unwrap()[0].kind,"error");
    for chunks in [1,2,7,64] {
        let mut reader=numera_image_gen_mcp::http::sse::Reader::new(4096);
        let frames="event: progress\r\ndata: {\"text\":\"日本語 🌍\"}\r\n\r\nevent: response.completed\ndata: {\"response\":{\"id\":\"same\",\"status\":\"completed\"}}\n\n";
        for chunk in frames.as_bytes().chunks(chunks){reader.push(chunk).unwrap();}
        let retained=reader.finish().unwrap();let parsed=events(&retained,4096).unwrap();assert_eq!(parsed.len(),1);assert_eq!(parsed[0].kind,"response.completed");
    }
}
