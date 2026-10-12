mod common;
#[path = "common/image.rs"]
mod image_fixture;
use common::Workspace;
use image::GenericImageView;
use numera_image_gen_mcp::files::{editing, images, paths};
use serde_json::json;

#[test]
fn request_seed_matches_source_safe_integer_domain_before_submission() {
    use numera_image_gen_mcp::services::request;
    for seed in [-9007199254740991.0, -0.0, 9007199254740991.0] {
        assert!(request::parse(json!({"prompt":"draw","seed":seed})).is_ok());
    }
    for seed in [-9007199254740992.0, 1.5, 9007199254740992.0] {
        assert_eq!(
            request::parse(json!({"prompt":"draw","seed":seed}))
                .unwrap_err()
                .code,
            "invalid_input"
        );
    }
}

#[test]
fn region_masks_preserve_polarity_pixels_and_reject_outside_bounds() {
    let config = json!({"files":{"maxPixels":64000000}});
    let source = images::inspect(image_fixture::original(), &config, None).unwrap();
    let request = json!({"edit_region":{"x":1,"y":1,"width":2,"height":2}});
    for (polarity, outside, inside) in [
        ("transparent-edit", [255, 255, 255, 255], [255, 255, 255, 0]),
        ("white-edit", [0, 0, 0, 255], [255, 255, 255, 255]),
        ("black-edit", [255, 255, 255, 255], [0, 0, 0, 255]),
    ] {
        let mask = editing::region_mask(
            &request,
            &source,
            &json!({"edit":{"maskPolarity":polarity}}),
            &config,
        )
        .unwrap();
        let decoded = image::load_from_memory(&mask.bytes).unwrap();
        assert_eq!(decoded.get_pixel(0, 0).0, outside);
        assert_eq!(decoded.get_pixel(1, 1).0, inside);
        images::validate_mask(&mask, &source).unwrap();
    }
    assert_eq!(
        editing::region_mask(&request, &source, &json!({}), &config)
            .err()
            .unwrap()
            .code,
        "unsupported_operation"
    );
    assert_eq!(
        editing::region_mask(
            &json!({"edit_region":{"x":source.width,"y":0,"width":1,"height":1}}),
            &source,
            &json!({"edit":{"maskPolarity":"transparent-edit"}}),
            &config
        )
        .err()
        .unwrap()
        .code,
        "invalid_input"
    );
    assert_eq!(
        editing::region_mask(
            &request,
            &source,
            &json!({"edit":{"maskPolarity":"transparent-edit"}}),
            &json!({"files":{"maxAggregateBytes":1}})
        )
        .err()
        .unwrap()
        .code,
        "invalid_input"
    );
}

#[test]
fn editing_contracts_and_contained_unicode_files_preserve_original_bytes() {
    let workspace = Workspace::new();
    let bytes = image_fixture::original();
    let root = workspace.0.join("inputs");
    std::fs::create_dir(&root).unwrap();
    let file = root.join("café-日本語.png");
    std::fs::write(&file, &bytes).unwrap();
    assert_eq!(
        paths::input(
            file.to_str().unwrap(),
            std::slice::from_ref(&root),
            bytes.len()
        )
        .unwrap(),
        bytes
    );
    let url = url::Url::from_file_path(&file).unwrap();
    assert_eq!(
        paths::input(url.as_str(), std::slice::from_ref(&root), bytes.len()).unwrap(),
        bytes
    );
    assert_eq!(
        paths::input(file.to_str().unwrap(), &[], bytes.len())
            .unwrap_err()
            .code,
        "permission_denied"
    );
    assert_eq!(
        paths::input(file.to_str().unwrap(), std::slice::from_ref(&root), 1)
            .unwrap_err()
            .code,
        "input_file_error"
    );
    for path in [
        "",
        "../escape",
        "a/../b",
        "./a",
        "/absolute",
        "a//b",
        "CON.png",
        "lpt1",
        "a.",
        "a ",
        "a:stream",
        "a?b",
    ] {
        assert_eq!(
            paths::safe_relative(path).unwrap_err().code,
            "invalid_input"
        );
    }
    assert_eq!(
        paths::safe_relative("batch/café-output").unwrap(),
        "batch/café-output"
    );
    let outputs = workspace.0.join("outputs");
    let directory = paths::output_directory(&outputs, Some("batch/café-output")).unwrap();
    assert!(directory.starts_with(std::fs::canonicalize(outputs).unwrap()));
    for size in ["0x10", "01x10", "10x0", "10x01", "10x", "10X10", "abc"] {
        assert!(editing::dimensions(size).is_none());
    }
    assert_eq!(editing::dimensions("16x32"), Some((16, 32)));
    let translated = editing::request(
        json!({"prompt":"Preserve drawing","reference_images":[{}],"target_language":"Japanese"}),
        "edit",
    )
    .unwrap();
    assert!(
        translated["prompt"]
            .as_str()
            .unwrap()
            .contains("Replace the original text inside the image")
    );
    let upscale=editing::request(json!({"prompt":"Preserve drawing","reference_images":[{}],"upscale":true,"count":1,"size":"32x32"}),"edit").unwrap();
    assert!(
        upscale["prompt"]
            .as_str()
            .unwrap()
            .contains("Do not add or remove objects")
    );
    for request in [
        json!({"target_language":"Japanese"}),
        json!({"reference_images":[{}],"mask":{},"edit_region":{}}),
        json!({"reference_images":[{},{}],"upscale":true,"count":1,"size":"32x32"}),
    ] {
        assert_eq!(
            editing::request(request, "edit").unwrap_err().code,
            "invalid_input"
        );
    }
}
