pub mod glb;
pub mod mp4;
pub mod wav;

use numera_gen_mcp::media::{self, Kind};

#[track_caller]
pub fn valid(kind: Kind, bytes: &[u8]) -> serde_json::Value {
    media::inspect(kind, bytes, bytes.len(), 64).unwrap().metadata
}

#[track_caller]
pub fn invalid(kind: Kind, bytes: &[u8]) {
    assert!(media::inspect(kind, bytes, bytes.len(), 64).is_err());
}

pub fn robustness() {
    for (kind, bytes) in [
        (Kind::Audio, crate::bytes::wav()),
        (Kind::Model3d, crate::bytes::glb()),
        (Kind::Video, crate::bytes::mp4()),
    ] {
        // Every truncation must fail; byte mutations may remain valid but must never panic.
        for end in 0..bytes.len() {
            invalid(kind, &bytes[..end]);
        }
        for offset in 0..bytes.len() {
            for value in [0, 0xff] {
                let mut changed = bytes.clone();
                changed[offset] = value;
                let result = std::panic::catch_unwind(|| {
                    media::inspect(kind, &changed, changed.len(), 64)
                });
                assert!(result.is_ok(), "media mutation panicked at {offset}");
                if let Ok(info) = result.unwrap() {
                    assert_eq!(info.kind, kind);
                    assert_eq!(info.metadata["validation"], "structural");
                }
            }
        }
    }
}
