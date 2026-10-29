use super::{Inspected, Kind, glb, invalid, mp4, wav};
use crate::error::Result;
use serde_json::json;

pub fn inspect(kind: Kind, bytes: &[u8], max_bytes: usize, max_pixels: u64) -> Result<Inspected> {
    if bytes.is_empty() || bytes.len() > max_bytes {
        return Err(invalid("Media is empty or exceeds the configured byte limit."));
    }
    let (mime, format, metadata) = match kind {
        Kind::Image => {
            let config = crate::compat::value::json!({"files":{"maxPixels":max_pixels}});
            let image = crate::files::images::inspect(bytes.to_vec(), &config, Some(max_bytes))?;
            return Ok(Inspected {
                kind,
                mime: image.mime,
                format: image.extension,
                metadata: json!({"width":image.width,"height":image.height,"alpha":image.alpha}),
            });
        }
        Kind::Audio => ("audio/wav", "wav", wav::inspect(bytes)?),
        Kind::Video => ("video/mp4", "mp4", mp4::inspect(bytes, max_pixels)?),
        Kind::Model3d => ("model/gltf-binary", "glb", glb::inspect(bytes, max_pixels)?),
    };
    Ok(Inspected {
        kind,
        mime: mime.into(),
        format: format.into(),
        metadata,
    })
}
