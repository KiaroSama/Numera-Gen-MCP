use crate::{
    compat::value::{Value, json},
    error::{NumeraError, Result},
    jobs::Receipt,
    media::{Inspected, Kind},
};

pub fn snapshot(endpoint: &str, parameters: &serde_json::Value) -> Result<Value> {
    let mut expected = json!({});
    // Only explicit effective parameters of these exact contracts define image expectations.
    // Other media and mesh sidecars must not inherit image dimensions or an inferred format.
    if matches!(endpoint, "fal-ai/flux/dev" | "fal-ai/flux/dev/image-to-image") {
        expected["kind"] = json!("image");
        expected["role"] = json!("image");
        for key in ["num_images", "output_format", "image_size"] {
            if let Some(value) = parameters.get(key) {
                expected[key] = Value::from_serde(value)?;
            }
        }
    }
    Ok(expected)
}

pub fn deviations(receipt: &Receipt, inspected: &Inspected, role: &str, index: u32) -> Vec<String> {
    let expected = &receipt.value["output_requirements"]["media"];
    if expected["kind"] != inspected.kind.as_str()
        || expected["role"] != role
        || inspected.kind != Kind::Image
    {
        return Vec::new();
    }
    let mut deviations = Vec::new();
    if let Some(format) = expected["output_format"].as_str()
        && inspected.mime != format!("image/{format}")
    {
        deviations.push(format!(
            "Output {index} format is {}, not requested {format}.",
            inspected.format
        ));
    }
    if let (Some(width), Some(height)) = (
        expected["image_size"]["width"].as_u64(),
        expected["image_size"]["height"].as_u64(),
    ) && (inspected.metadata["width"].as_u64() != Some(width)
        || inspected.metadata["height"].as_u64() != Some(height))
    {
        deviations.push(format!("Output {index} dimensions differ from requested size."));
    }
    deviations
}

pub fn reconcile_count(receipt: &mut Receipt) -> Result<()> {
    let expected = &receipt.value["output_requirements"]["media"];
    let Some(count) = expected["num_images"].as_u64() else {
        return Ok(());
    };
    let actual = crate::error::array(&receipt.value["outputs"])
        .iter()
        .filter(|output| output["kind"] == "image" && output["role"] == "image")
        .count();
    let deviations = receipt.value["deviations"]
        .as_array_mut()
        .ok_or_else(NumeraError::storage)?;
    deviations.retain(|value| {
        !value.as_str().is_some_and(|text| {
            text.starts_with("Requested ") && text.contains(" image original(s), saved ")
        })
    });
    if count != actual as u64 {
        deviations.push(json!(format!("Requested {count} image original(s), saved {actual}.")));
    }
    Ok(())
}
