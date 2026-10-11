use numera_image_gen_mcp::files::images::{inspect, decode_base64};
use serde_json::json;

#[test]
fn generated_original_image_fixtures_are_preserved_and_validated() {
    let path=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".ci-work/parity/images.json");
    let bytes=std::fs::read(path).expect("source image fixture generation required");
    let cases:Vec<serde_json::Value>=serde_json::from_slice(&bytes).expect("source fixtures");
    assert!(!cases.is_empty());
    for case in cases {
        let raw=decode_base64(case["base64"].as_str().expect("fixture bytes"),1024*1024).expect("base64");
        let image=inspect(raw.clone(),&json!({"files":{"maxOutputBytes":1048576,"maxPixels":64000000}}),None).expect("real image decode");
        assert_eq!(image.bytes,raw);
        assert_eq!(image.metadata(),case["metadata"],"{}",case["name"]);
    }
    assert!(decode_base64("AAAA\n",1024).is_err());
    assert!(decode_base64("",1024).is_err());
}
