use numera_image_gen_mcp::files::images::{
    data_url, decode_base64, inspect, preview, validate_mask,
};
use serde_json::json;

#[test]
fn generated_original_image_fixtures_are_preserved_and_validated() {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".ci-work/parity/images.json");
    let bytes = std::fs::read(path).expect("source image fixture generation required");
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&bytes).expect("source fixtures");
    assert!(!cases.is_empty());
    for case in cases {
        let raw = decode_base64(case["base64"].as_str().expect("fixture bytes"), 1024 * 1024)
            .expect("base64");
        let image = inspect(
            raw.clone(),
            &json!({"files":{"maxOutputBytes":1048576,"maxPixels":64000000}}),
            None,
        )
        .expect("real image decode");
        assert_eq!(image.bytes, raw);
        assert_eq!(image.metadata(), case["metadata"], "{}", case["name"]);
        let config = json!({"files":{"maxPixels":64000000,"previewMaxDimension":3,"previewMaxBytes":262144}});
        let derivative = preview(&image, &config).unwrap();
        let derived = inspect(
            decode_base64(derivative["data"].as_str().unwrap(), 262144).unwrap(),
            &config,
            None,
        )
        .unwrap();
        assert!(derived.width <= 3 && derived.height <= 3);
        assert_eq!(derived.mime, "image/png");
        assert_eq!(image.bytes, raw);
        assert_eq!(
            data_url(
                &format!(
                    "data:{};base64,{}",
                    image.mime,
                    case["base64"].as_str().unwrap()
                ),
                1048576
            )
            .unwrap(),
            raw
        );
        if image.mime == "image/png" && image.alpha {
            validate_mask(&image, &image).unwrap();
            let mut wrong = image.clone();
            wrong.width += 1;
            assert_eq!(
                validate_mask(&wrong, &image).unwrap_err().code,
                "invalid_input"
            );
            wrong.width = image.width;
            wrong.alpha = false;
            assert_eq!(
                validate_mask(&wrong, &image).unwrap_err().code,
                "invalid_input"
            );
        } else {
            assert_eq!(
                validate_mask(&image, &image).unwrap_err().code,
                "invalid_input"
            );
        }
        assert_eq!(
            inspect(raw.clone(), &config, Some(raw.len() - 1))
                .err()
                .unwrap()
                .code,
            "invalid_input"
        );
        assert_eq!(
            inspect(raw.clone(), &json!({"files":{"maxPixels":1}}), None)
                .err()
                .unwrap()
                .code,
            "invalid_input"
        );
        assert_eq!(
            preview(
                &image,
                &json!({"files":{"previewMaxDimension":1,"previewMaxBytes":1}})
            )
            .unwrap_err()
            .code,
            "output_file_error"
        );
    }
    assert!(decode_base64("AAAA\n", 1024).is_err());
    assert!(decode_base64("", 1024).is_err());
    for invalid in ["AAA", "AAA\n", "AA=A", "A===", "AAAA====", "%%%%", "Y Q=="] {
        assert_eq!(
            decode_base64(invalid, 1024).unwrap_err().code,
            "invalid_response"
        );
    }
    for invalid in [
        "data:image/svg+xml;base64,YQ==",
        "data:image/png,YQ==",
        "data:text/plain;base64,YQ==",
        "https://example.com/image.png",
    ] {
        assert_eq!(data_url(invalid, 1024).unwrap_err().code, "invalid_input");
    }
    for invalid in [
        vec![],
        b"<svg/>".to_vec(),
        b"RIFFbrokenWEBP".to_vec(),
        b"\x89PNG\r\n\x1a\n".to_vec(),
    ] {
        assert_eq!(
            inspect(invalid, &json!({}), None).err().unwrap().code,
            "invalid_input"
        );
    }
}
