use numera_gen_mcp::media::{self, Kind};
use serde_json::json;
#[path = "../fixtures/media/bytes.rs"]
mod bytes;
use bytes::{glb, mp4, wav};

#[test]
fn pcm_original_is_verified_without_invented_image_dimensions() {
    let bytes = wav();
    let original = bytes.clone();
    let info = media::inspect(Kind::Audio, &bytes, 1024, 16).unwrap();
    assert_eq!((info.kind, info.mime.as_str(), info.format.as_str()), (Kind::Audio, "audio/wav", "wav"));
    assert_eq!(info.metadata["sample_rate"], 8000);
    assert_eq!(info.metadata["channels"], 1);
    assert_eq!(info.metadata["sample_count"], 2);
    assert!(info.metadata.get("width").is_none());
    assert_eq!(bytes, original);
}

#[test]
fn wav_rejects_truncation_inconsistent_rates_and_wrong_kind() {
    let valid = wav();
    for len in [0, 4, 12, valid.len() - 1] {
        assert!(media::inspect(Kind::Audio, &valid[..len], 1024, 16).is_err());
    }
    let mut bad = valid.clone();
    bad[28..32].copy_from_slice(&1u32.to_le_bytes());
    assert!(media::inspect(Kind::Audio, &bad, 1024, 16).is_err());
    assert!(media::inspect(Kind::Video, &valid, 1024, 16).is_err());
    assert!(media::inspect(Kind::Audio, &valid, valid.len() - 1, 16).is_err());
    let mut fake = b"RIFF\x04\0\0\0WAVE".to_vec();
    fake.extend_from_slice(b"not audio");
    assert!(media::inspect(Kind::Audio, &fake, 1024, 16).is_err());
}

#[test]
fn self_contained_triangle_has_verified_mesh_metadata() {
    let bytes = glb();
    let info = media::inspect(Kind::Model3d, &bytes, 4096, 64).unwrap();
    assert_eq!(info.mime, "model/gltf-binary");
    assert_eq!(info.format, "glb");
    assert_eq!(info.metadata["mesh_count"], 1);
    assert_eq!(info.metadata["primitive_count"], 1);
    assert_eq!(info.metadata["vertex_count"], 3);
    assert!(info.metadata.get("width").is_none());
}

#[test]
fn glb_rejects_header_only_truncated_buffer_and_external_resources() {
    let good = glb();
    assert!(media::inspect(Kind::Model3d, &good[..12], 4096, 64).is_err());
    assert!(media::inspect(Kind::Model3d, &good[..good.len() - 1], 4096, 64).is_err());
    let mut bad = good.clone();
    bad[4..8].copy_from_slice(&1u32.to_le_bytes());
    assert!(media::inspect(Kind::Model3d, &bad, 4096, 64).is_err());
    let text_len = u32::from_le_bytes(good[12..16].try_into().unwrap()) as usize;
    let mut doc: serde_json::Value = serde_json::from_slice(&good[20..20 + text_len]).unwrap();
    doc["buffers"][0]["uri"] = json!("https://example.invalid/private.bin");
    let mut text = serde_json::to_vec(&doc).unwrap();
    while !text.len().is_multiple_of(4) { text.push(b' '); }
    let bin_chunk = &good[20 + text_len..];
    let mut external = b"glTF\x02\0\0\0".to_vec();
    external.extend_from_slice(&((20 + text.len() + bin_chunk.len()) as u32).to_le_bytes());
    external.extend_from_slice(&(text.len() as u32).to_le_bytes());
    external.extend_from_slice(b"JSON");
    external.extend_from_slice(&text);
    external.extend_from_slice(bin_chunk);
    assert!(media::inspect(Kind::Model3d, &external, 4096, 64).is_err());
}

#[test]
fn visual_mp4_validates_real_sample_tables_and_payload() {
    let bytes = mp4();
    let info = media::inspect(Kind::Video, &bytes, 8192, 64).unwrap();
    assert_eq!(info.mime, "video/mp4");
    assert_eq!(info.metadata["width"], 2);
    assert_eq!(info.metadata["height"], 2);
    assert_eq!(info.metadata["duration_ms"], 1000);
    assert_eq!(info.metadata["sample_count"], 1);
    assert_eq!(info.metadata["validation"], "structural");
    assert!(media::inspect(Kind::Video, &bytes, 8192, 3).is_err());
}

#[test]
fn mp4_rejects_magic_only_bad_offsets_and_truncated_samples() {
    let bytes = mp4();
    assert!(media::inspect(Kind::Video, &bytes[..24], 8192, 64).is_err());
    for len in [bytes.len() - 1, 32] {
        assert!(media::inspect(Kind::Video, &bytes[..len], 8192, 64).is_err());
    }
    let mut bad = bytes.clone();
    let stco = bad.windows(4).position(|v| v == b"stco").unwrap();
    bad[stco + 12..stco + 16].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(media::inspect(Kind::Video, &bad, 8192, 64).is_err());
    let mut bad = bytes.clone();
    let mdat = bad.windows(4).position(|v| v == b"mdat").unwrap();
    bad[mdat + 4] = 0;
    assert!(media::inspect(Kind::Video, &bad, 8192, 64).is_err());
}

#[test]
fn malformed_inputs_never_escape_as_panics_or_image_shaped_success() {
    let valid = [wav(), glb(), mp4()];
    for (kind, original) in [Kind::Audio, Kind::Model3d, Kind::Video].into_iter().zip(valid) {
        for length in [0, 1, 7, 11, original.len() / 2, original.len() - 1] {
            let result = std::panic::catch_unwind(|| media::inspect(kind, &original[..length], 8192, 64));
            assert!(result.is_ok(), "truncated media panicked");
            assert!(result.unwrap().is_err());
        }
    }
    let mut bad = glb();
    let json_length = u32::from_le_bytes(bad[12..16].try_into().unwrap()) as usize;
    let start = 28 + json_length;
    bad[start..start + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(media::inspect(Kind::Model3d, &bad, 4096, 64).is_err());
}

#[test]
fn images_reuse_real_decoder_and_kind_tokens_are_exact() {
    let bytes = bytes::png();
    let info = media::inspect(Kind::Image, &bytes, 2048, 1).unwrap();
    assert_eq!(info.metadata["width"], 1);
    assert_eq!(info.mime, "image/png");
    assert!(media::inspect(Kind::Image, &bytes[..16], 2048, 1).is_err());
    assert_eq!(serde_json::to_value(Kind::Model3d).unwrap(), "3d");
    assert!(serde_json::from_value::<Kind>(json!("model_3d")).is_err());
}
