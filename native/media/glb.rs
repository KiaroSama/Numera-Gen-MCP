use super::invalid;
use crate::error::Result;
use serde_json::{Value, json};

fn uint(value: &Value) -> Result<usize> {
    value.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| invalid("GLB expects a bounded unsigned integer."))
}

fn array<'a>(value: &'a Value, name: &str) -> Result<&'a [Value]> {
    match value.get(name) {
        None => Ok(&[]),
        Some(Value::Array(items)) if items.len() <= 100000 => Ok(items),
        _ => Err(invalid("GLB array is invalid or excessive.")),
    }
}

fn field(value: &Value, name: &str, default: usize) -> Result<usize> {
    value.get(name).map(uint).unwrap_or(Ok(default))
}

fn le32(bytes: &[u8], offset: usize) -> Result<u32> {
    let b = bytes.get(offset..offset + 4).ok_or_else(|| invalid("Truncated GLB header/chunk."))?;
    Ok(u32::from_le_bytes(b.try_into().map_err(|_| invalid("Invalid GLB field."))?))
}

struct View {
    start: usize,
    length: usize,
    stride: Option<usize>,
}

struct Accessor {
    start: usize,
    stride: usize,
    count: usize,
    component: usize,
    components: usize,
    size: usize,
}

fn checked_end(start: usize, count: usize, stride: usize, width: usize) -> Result<usize> {
    count.checked_sub(1).and_then(|n| n.checked_mul(stride)).and_then(|n| n.checked_add(start)).and_then(|n| n.checked_add(width))
        .ok_or_else(|| invalid("GLB accessor offset/count overflow."))
}

pub(super) fn inspect(bytes: &[u8], max_pixels: u64) -> Result<Value> {
    if bytes.get(..4) != Some(b"glTF") || le32(bytes, 4)? != 2 || le32(bytes, 8)? as usize != bytes.len() {
        return Err(invalid("Only complete GLB version 2 files are supported."));
    }
    let mut offset = 12usize;
    let mut json_chunk = None;
    let mut bin = None;
    let mut chunks = 0usize;
    while offset < bytes.len() {
        chunks += 1;
        if chunks > 32 { return Err(invalid("Too many GLB chunks.")); }
        let size = le32(bytes, offset)? as usize;
        let tag = le32(bytes, offset + 4)?;
        let start = offset.checked_add(8).ok_or_else(|| invalid("GLB offset overflow."))?;
        let end = start.checked_add(size).filter(|n| *n <= bytes.len()).ok_or_else(|| invalid("GLB chunk exceeds original bytes."))?;
        if !size.is_multiple_of(4) || size == 0 { return Err(invalid("GLB chunk size/alignment is invalid.")); }
        match tag {
            0x4e4f534a if chunks == 1 => json_chunk = Some(&bytes[start..end]),
            0x004e4942 if chunks == 2 => bin = Some(&bytes[start..end]),
            0x4e4f534a | 0x004e4942 => return Err(invalid("Duplicate or out-of-order GLB JSON/BIN chunk.")),
            _ if chunks == 1 => return Err(invalid("GLB must begin with its JSON chunk.")),
            _ => {},
        }
        offset = end;
    }
    let document: Value = serde_json::from_slice(json_chunk.ok_or_else(|| invalid("GLB JSON chunk is missing."))?)
        .map_err(|_| invalid("GLB JSON is malformed or too deeply nested."))?;
    if document["asset"]["version"].as_str() != Some("2.0") || !document.is_object() {
        return Err(invalid("GLB needs a glTF 2.0 asset declaration."));
    }
    if !array(&document, "extensionsRequired")?.is_empty()
        || !array(&document, "skins")?.is_empty() || !array(&document, "animations")?.is_empty()
    {
        return Err(invalid("GLB required extensions, skins and animations are not supported by this structural validator."));
    }
    let buffers = array(&document, "buffers")?;
    if buffers.len() != 1 || buffers[0].get("uri").is_some() {
        return Err(invalid("GLB must contain one embedded buffer; external resources are not fetched."));
    }
    let bin = bin.ok_or_else(|| invalid("GLB embedded BIN data is missing."))?;
    let buffer_length = uint(&buffers[0]["byteLength"])?;
    if buffer_length == 0 || buffer_length > bin.len() || bin.len() - buffer_length > 3 || bin[buffer_length..].iter().any(|b| *b != 0) {
        return Err(invalid("GLB buffer length/padding does not match its BIN payload."));
    }
    let mut views = Vec::new();
    for value in array(&document, "bufferViews")? {
        if uint(&value["buffer"])? != 0 { return Err(invalid("GLB buffer view selects a missing buffer.")); }
        let start = field(value, "byteOffset", 0)?;
        let length = uint(&value["byteLength"])?;
        if length == 0 || start.checked_add(length).is_none_or(|end| end > buffer_length) {
            return Err(invalid("GLB buffer view exceeds its embedded buffer."));
        }
        let stride = value.get("byteStride").map(uint).transpose()?;
        if stride.is_some_and(|s| !(4..=252).contains(&s) || !s.is_multiple_of(4)) {
            return Err(invalid("GLB vertex stride is invalid."));
        }
        if let Some(target) = value.get("target") {
            if !matches!(uint(target)?, 34962 | 34963) { return Err(invalid("Invalid GLB buffer view target.")); }
        }
        views.push(View { start, length, stride });
    }
    let mut accessors = Vec::new();
    let mut inspected_elements = 0usize;
    for value in array(&document, "accessors")? {
        if value.get("sparse").is_some() { return Err(invalid("Sparse GLB accessors are not supported.")); }
        let view = views.get(uint(&value["bufferView"])?).ok_or_else(|| invalid("GLB accessor selects a missing view."))?;
        let component = uint(&value["componentType"])?;
        let size = match component { 5120 | 5121 => 1usize, 5122 | 5123 => 2, 5125 | 5126 => 4, _ => return Err(invalid("Invalid GLB accessor component type.")) };
        let (columns, rows) = match value["type"].as_str() {
            Some("SCALAR") => (1, 1), Some("VEC2") => (1, 2), Some("VEC3") => (1, 3), Some("VEC4") => (1, 4),
            Some("MAT2") => (2, 2), Some("MAT3") => (3, 3), Some("MAT4") => (4, 4), _ => return Err(invalid("Invalid GLB accessor shape.")),
        };
        let width = if columns == 1 { rows * size } else { (rows * size).div_ceil(4) * 4 * columns };
        let count = uint(&value["count"])?;
        let local = field(value, "byteOffset", 0)?;
        let start = view.start.checked_add(local).ok_or_else(|| invalid("GLB accessor offset overflow."))?;
        let stride = view.stride.unwrap_or(width);
        if count == 0 || stride < width || !start.is_multiple_of(size) || !local.is_multiple_of(size)
            || checked_end(local, count, stride, width)? > view.length
        {
            return Err(invalid("GLB accessor data range/alignment is invalid."));
        }
        inspected_elements = inspected_elements.checked_add(count.checked_mul(rows * columns).ok_or_else(|| invalid("GLB accessor work overflow."))?).ok_or_else(|| invalid("GLB accessor work overflow."))?;
        if inspected_elements > buffer_length.saturating_mul(8) {
            return Err(invalid("Overlapping GLB accessors exceed the bounded structural inspection budget."));
        }
        if let Some(normalized) = value.get("normalized") {
            if !normalized.is_boolean() || (normalized.as_bool() == Some(true) && matches!(component, 5125 | 5126)) {
                return Err(invalid("GLB accessor normalization is invalid."));
            }
        }
        for name in ["min", "max"] {
            if let Some(bounds) = value.get(name) {
                if bounds.as_array().is_none_or(|a| a.len() != rows * columns || a.iter().any(|v| v.as_f64().is_none_or(|n| !n.is_finite()))) {
                    return Err(invalid("GLB accessor bounds are invalid."));
                }
            }
        }
        if component == 5126 {
            for n in 0..count {
                let at = start + n * stride;
                for col in 0..columns {
                    let column = at + if columns == 1 { 0 } else { col * (rows * size).div_ceil(4) * 4 };
                    for row in 0..rows {
                        let p = column + row * 4;
                        if !f32::from_le_bytes([bin[p], bin[p + 1], bin[p + 2], bin[p + 3]]).is_finite() {
                            return Err(invalid("GLB floating-point geometry must be finite."));
                        }
                    }
                }
            }
        }
        accessors.push(Accessor { start, stride, count, component, components: rows * columns, size });
    }
    let images = array(&document, "images")?;
    let mut inspected_image_bytes = 0usize;
    let mut inspected_image_pixels = 0u64;
    for image in images {
        if image.get("uri").is_some() || !matches!(image["mimeType"].as_str(), Some("image/png" | "image/jpeg" | "image/webp")) {
            return Err(invalid("GLB texture images must be embedded supported images."));
        }
        let view = views.get(uint(&image["bufferView"])?).ok_or_else(|| invalid("GLB image selects a missing view."))?;
        if view.stride.is_some() { return Err(invalid("GLB image buffer view cannot have a byte stride.")); }
        inspected_image_bytes = inspected_image_bytes.checked_add(view.length).filter(|n| *n <= buffer_length.saturating_mul(2)).ok_or_else(|| invalid("GLB embedded image work exceeds the byte budget."))?;
        let remaining_pixels = max_pixels.checked_sub(inspected_image_pixels).filter(|n| *n > 0).ok_or_else(|| invalid("GLB texture pixel budget is exhausted."))?;
        let config = crate::compat::value::json!({"files":{"maxPixels":remaining_pixels}});
        let verified = crate::files::images::inspect(bin[view.start..view.start + view.length].to_vec(), &config, Some(buffer_length))?;
        inspected_image_pixels += u64::from(verified.width) * u64::from(verified.height);
        if image["mimeType"].as_str() != Some(verified.mime.as_str()) { return Err(invalid("GLB texture MIME does not match embedded bytes.")); }
    }
    let samplers = array(&document, "samplers")?;
    let textures = array(&document, "textures")?;
    for texture in textures {
        if uint(&texture["source"])? >= images.len() { return Err(invalid("GLB texture selects a missing embedded image.")); }
        if let Some(sampler) = texture.get("sampler") {
            if uint(sampler)? >= samplers.len() { return Err(invalid("GLB texture selects a missing sampler.")); }
        }
    }
    let materials = array(&document, "materials")?;
    for material in materials {
        for info in [material.pointer("/pbrMetallicRoughness/baseColorTexture"), material.pointer("/pbrMetallicRoughness/metallicRoughnessTexture"), material.get("normalTexture"), material.get("occlusionTexture"), material.get("emissiveTexture")].into_iter().flatten() {
            if uint(&info["index"])? >= textures.len() { return Err(invalid("GLB material selects a missing texture.")); }
        }
    }
    let meshes = array(&document, "meshes")?;
    let mut primitive_count = 0usize;
    let mut vertex_count = 0usize;
    for mesh in meshes {
        let primitives = array(mesh, "primitives")?;
        if primitives.is_empty() { return Err(invalid("GLB mesh has no primitives.")); }
        for primitive in primitives {
            primitive_count += 1;
            if primitive.get("targets").is_some() || primitive.get("extensions").is_some() {
                return Err(invalid("GLB morph/compressed primitives require a separate validated implementation."));
            }
            let attributes = primitive["attributes"].as_object().filter(|a| !a.is_empty()).ok_or_else(|| invalid("GLB primitive attributes are missing."))?;
            let positions = accessors.get(uint(&primitive["attributes"]["POSITION"])?).ok_or_else(|| invalid("GLB positions select a missing accessor."))?;
            if positions.component != 5126 || positions.components != 3 { return Err(invalid("GLB positions must be float VEC3 values.")); }
            vertex_count = vertex_count.checked_add(positions.count).ok_or_else(|| invalid("GLB vertex count overflow."))?;
            for index in attributes.values() {
                let a = accessors.get(uint(index)?).ok_or_else(|| invalid("GLB attribute selects a missing accessor."))?;
                if a.count != positions.count { return Err(invalid("GLB vertex attribute counts disagree.")); }
            }
            let mode = field(primitive, "mode", 4)?;
            if mode > 6 { return Err(invalid("Invalid GLB primitive mode.")); }
            let mut draw_count = positions.count;
            if let Some(indices) = primitive.get("indices") {
                let a = accessors.get(uint(indices)?).ok_or_else(|| invalid("GLB indices select a missing accessor."))?;
                if a.components != 1 || !matches!(a.component, 5121 | 5123 | 5125) { return Err(invalid("GLB indices need unsigned scalar values.")); }
                draw_count = a.count;
                for n in 0..a.count {
                    let p = a.start + n * a.stride;
                    let index = match a.size { 1 => usize::from(bin[p]), 2 => usize::from(u16::from_le_bytes([bin[p], bin[p + 1]])), _ => u32::from_le_bytes([bin[p], bin[p + 1], bin[p + 2], bin[p + 3]]) as usize };
                    if index >= positions.count { return Err(invalid("GLB index references a vertex outside the primitive.")); }
                }
            }
            if (mode == 1 && !draw_count.is_multiple_of(2)) || (mode == 4 && !draw_count.is_multiple_of(3)) || (matches!(mode, 2 | 3) && draw_count < 2) || (matches!(mode, 5 | 6) && draw_count < 3) {
                return Err(invalid("GLB primitive element count is inconsistent with its mode."));
            }
            if let Some(material) = primitive.get("material") {
                if uint(material)? >= materials.len() { return Err(invalid("GLB primitive selects a missing material.")); }
            }
        }
    }
    if meshes.is_empty() || vertex_count == 0 { return Err(invalid("GLB must contain actual mesh geometry.")); }
    let nodes = array(&document, "nodes")?;
    let mut parents = vec![0u8; nodes.len()];
    for node in nodes {
        for (name, size) in [("matrix", 16), ("translation", 3), ("rotation", 4), ("scale", 3)] {
            if let Some(transform) = node.get(name) {
                if transform.as_array().is_none_or(|values| values.len() != size || values.iter().any(|v| v.as_f64().is_none_or(|n| !n.is_finite()))) {
                    return Err(invalid("GLB node transform has an invalid shape or value."));
                }
            }
        }
        if node.get("matrix").is_some() && ["translation", "rotation", "scale"].iter().any(|key| node.get(*key).is_some()) {
            return Err(invalid("GLB nodes cannot combine matrix and TRS transforms."));
        }
        if let Some(mesh) = node.get("mesh") {
            if uint(mesh)? >= meshes.len() { return Err(invalid("GLB node selects a missing mesh.")); }
        }
        if node.get("skin").is_some() { return Err(invalid("Skinned GLB nodes are unsupported.")); }
        for child in array(node, "children")? {
            let index = uint(child)?;
            let parent = parents.get_mut(index).ok_or_else(|| invalid("GLB child node is missing."))?;
            *parent = parent.checked_add(1).filter(|n| *n <= 1).ok_or_else(|| invalid("GLB node has multiple parents."))?;
        }
    }
    let mut stack: Vec<usize> = parents.iter().enumerate().filter_map(|(i, n)| (*n == 0).then_some(i)).collect();
    let mut visited = 0usize;
    while let Some(i) = stack.pop() {
        visited += 1;
        for child in array(&nodes[i], "children")? { stack.push(uint(child)?); }
    }
    if visited != nodes.len() { return Err(invalid("GLB node graph contains a cycle.")); }
    let scenes = array(&document, "scenes")?;
    for scene in scenes {
        for node in array(scene, "nodes")? {
            let index = uint(node)?;
            if index >= nodes.len() || parents[index] != 0 { return Err(invalid("GLB scene selects a missing or non-root node.")); }
        }
    }
    if let Some(scene) = document.get("scene") {
        if uint(scene)? >= scenes.len() { return Err(invalid("GLB default scene is missing.")); }
    }
    Ok(json!({"mesh_count":meshes.len(),"primitive_count":primitive_count,"vertex_count":vertex_count,
        "node_count":nodes.len(),"texture_count":textures.len(),"validation":"structural"}))
}
