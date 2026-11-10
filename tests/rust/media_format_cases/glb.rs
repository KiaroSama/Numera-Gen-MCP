use super::{invalid, valid};
use numera_gen_mcp::media::{self, Kind};
use serde_json::{Value, json};

fn parts() -> (Value, Vec<u8>) {
    let bytes = crate::bytes::glb();
    let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    (
        serde_json::from_slice(&bytes[20..20 + length]).unwrap(),
        bytes[28 + length..].to_vec(),
    )
}

fn encode(document: &Value, binary: &[u8]) -> Vec<u8> {
    let mut text = serde_json::to_vec(document).unwrap();
    text.resize(text.len().next_multiple_of(4), b' ');
    let mut binary = binary.to_vec();
    binary.resize(binary.len().next_multiple_of(4), 0);
    [
        b"glTF".as_slice(),
        &2u32.to_le_bytes(),
        &((28 + text.len() + binary.len()) as u32).to_le_bytes(),
        &(text.len() as u32).to_le_bytes(),
        b"JSON",
        &text,
        &(binary.len() as u32).to_le_bytes(),
        b"BIN\0",
        &binary,
    ]
    .concat()
}

fn assign(document: &mut Value, path: &str, value: Value) {
    let (parent, key) = path.rsplit_once('/').unwrap();
    document.pointer_mut(parent).unwrap()[key] = value;
}

fn textured(component: u32) -> (Value, Vec<u8>) {
    let (mut doc, mut binary) = parts();
    let width = match component {
        5121 => 1,
        5123 => 2,
        _ => 4,
    };
    let start = binary.len();
    for n in 0..3u32 {
        binary.extend_from_slice(&n.to_le_bytes()[..width]);
    }
    doc["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":start,"byteLength":3*width,"target":34963}));
    doc["accessors"]
        .as_array_mut()
        .unwrap()
        .push(json!({"bufferView":1,"componentType":component,"count":3,"type":"SCALAR"}));
    doc["meshes"][0]["primitives"][0]["indices"] = json!(1);
    binary.resize(binary.len().next_multiple_of(4), 0);
    let start = binary.len();
    let image = crate::bytes::png();
    binary.extend_from_slice(&image);
    doc["buffers"][0]["byteLength"] = json!(binary.len());
    doc["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":start,"byteLength":image.len()}));
    doc["images"] = json!([{"bufferView":2,"mimeType":"image/png"}]);
    doc["samplers"] = json!([{}]);
    doc["textures"] = json!([{"source":0,"sampler":0}]);
    doc["materials"] = json!([{"pbrMetallicRoughness":{"baseColorTexture":{"index":0},"metallicRoughnessTexture":{"index":0}},
        "normalTexture":{"index":0},"occlusionTexture":{"index":0},"emissiveTexture":{"index":0}}]);
    doc["meshes"][0]["primitives"][0]["material"] = json!(0);
    (doc, binary)
}

pub fn positive() {
    for component in [5121, 5123, 5125] {
        let (mut doc, binary) = textured(component);
        doc["nodes"] = json!([{"mesh":0,"translation":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}, {"children":[0]}]);
        doc["scenes"] = json!([{"nodes":[1]}]);
        let info = valid(Kind::Model3d, &encode(&doc, &binary));
        assert_eq!(info["texture_count"], 1);
        assert_eq!(info["node_count"], 2);
        assert_eq!(info["vertex_count"], 3);
    }
    for (component, size) in [
        (5120u32, 1usize),
        (5121, 1),
        (5122, 2),
        (5123, 2),
        (5125, 4),
        (5126, 4),
    ] {
        for (shape, rows, columns) in [
            ("SCALAR", 1usize, 1usize),
            ("VEC2", 2, 1),
            ("VEC3", 3, 1),
            ("VEC4", 4, 1),
            ("MAT2", 2, 2),
            ("MAT3", 3, 3),
            ("MAT4", 4, 4),
        ] {
            let (mut doc, mut binary) = parts();
            let width = if columns == 1 {
                rows * size
            } else {
                (rows * size).next_multiple_of(4) * columns
            };
            let start = binary.len();
            binary.resize(start + width, 0);
            doc["buffers"][0]["byteLength"] = json!(binary.len());
            doc["bufferViews"].as_array_mut().unwrap().push(json!({"buffer":0,"byteOffset":start,"byteLength":width,"byteStride":width.next_multiple_of(4)}));
            doc["accessors"].as_array_mut().unwrap().push(
                json!({"bufferView":1,"componentType":component,"count":1,"type":shape,
                "normalized":component<5125,"min":vec![0;rows*columns],"max":vec![1;rows*columns]}),
            );
            assert_eq!(
                valid(Kind::Model3d, &encode(&doc, &binary))["vertex_count"],
                3
            );
        }
    }
}

pub fn negative() {
    let (doc, binary) = parts();
    for (path, value) in [
        ("/asset/version", json!("1.0")),
        ("/buffers/0/byteLength", json!(0)),
        ("/buffers/0/uri", json!("private.bin")),
        ("/bufferViews/0/buffer", json!(1)),
        ("/bufferViews/0/byteOffset", json!(10000)),
        ("/bufferViews/0/byteLength", json!(0)),
        ("/bufferViews/0/byteStride", json!(3)),
        ("/bufferViews/0/target", json!(1)),
        ("/accessors/0/sparse", json!({})),
        ("/accessors/0/bufferView", json!(99)),
        ("/accessors/0/componentType", json!(999)),
        ("/accessors/0/type", json!("OTHER")),
        ("/accessors/0/count", json!(0)),
        ("/accessors/0/byteOffset", json!(1)),
        ("/accessors/0/normalized", json!(true)),
        ("/accessors/0/normalized", json!("yes")),
        ("/accessors/0/min", json!([0])),
        ("/accessors/0/max", json!([null, null, null])),
        ("/meshes/0/primitives/0/attributes/POSITION", json!(99)),
        ("/meshes/0/primitives/0/attributes", json!({})),
        ("/meshes/0/primitives/0/mode", json!(7)),
        ("/meshes/0/primitives/0/targets", json!([])),
        ("/meshes/0/primitives/0/extensions", json!({})),
        ("/meshes/0/primitives/0/material", json!(99)),
        ("/nodes/0/mesh", json!(99)),
        ("/nodes/0/translation", json!([0])),
        ("/nodes/0/matrix", json!(vec![0; 16])),
        ("/nodes/0/skin", json!(0)),
        ("/nodes/0/children", json!([0])),
        ("/nodes/0/children", json!([99])),
        ("/scenes/0/nodes", json!([99])),
        ("/scene", json!(99)),
        ("/skins", json!([{}])),
        ("/animations", json!([{}])),
        ("/extensionsRequired", json!(["unsupported"])),
        ("/meshes", json!([])),
        ("/bufferViews", json!("invalid")),
    ] {
        let mut changed = doc.clone();
        assign(&mut changed, path, value);
        if path == "/nodes/0/matrix" {
            changed["nodes"][0]["translation"] = json!([0, 0, 0]);
        }
        invalid(Kind::Model3d, &encode(&changed, &binary));
    }
    let (doc, binary) = textured(5125);
    for (path, value) in [
        ("/images/0/uri", json!("outside.png")),
        ("/images/0/mimeType", json!("image/jpeg")),
        ("/images/0/mimeType", json!("image/gif")),
        ("/images/0/bufferView", json!(99)),
        ("/bufferViews/2/byteStride", json!(4)),
        ("/textures/0/source", json!(99)),
        ("/textures/0/sampler", json!(99)),
        ("/materials/0/normalTexture/index", json!(99)),
        ("/accessors/1/componentType", json!(5122)),
        ("/accessors/1/componentType", json!(5126)),
        ("/accessors/1/type", json!("VEC2")),
        ("/accessors/1/normalized", json!(true)),
        ("/meshes/0/primitives/0/indices", json!(99)),
    ] {
        let mut changed = doc.clone();
        assign(&mut changed, path, value);
        invalid(Kind::Model3d, &encode(&changed, &binary));
    }
    let mut bad = binary.clone();
    bad[36..40].copy_from_slice(&3u32.to_le_bytes());
    invalid(Kind::Model3d, &encode(&doc, &bad));
    let mut bad = doc.clone();
    bad["images"] = json!([doc["images"][0].clone(), doc["images"][0].clone()]);
    assert!(media::inspect(Kind::Model3d, &encode(&bad, &binary), 4096, 1).is_err());
    bad["images"] = json!(vec![doc["images"][0].clone(); 4]);
    invalid(Kind::Model3d, &encode(&bad, &binary));
    bad = doc.clone();
    bad["nodes"] = json!([{"children":[2]},{"children":[2]},{"mesh":0}]);
    invalid(Kind::Model3d, &encode(&bad, &binary));
    bad["nodes"] = json!([{"children":[1]},{"mesh":0}]);
    bad["scenes"] = json!([{"nodes":[1]}]);
    invalid(Kind::Model3d, &encode(&bad, &binary));
    bad = doc.clone();
    bad["meshes"][0]["primitives"][0]["mode"] = json!(1);
    invalid(Kind::Model3d, &encode(&bad, &binary));
    bad["accessors"][1]["count"] = json!(1);
    for mode in [2, 3, 4, 5, 6] {
        bad["meshes"][0]["primitives"][0]["mode"] = json!(mode);
        invalid(Kind::Model3d, &encode(&bad, &binary));
    }
}
