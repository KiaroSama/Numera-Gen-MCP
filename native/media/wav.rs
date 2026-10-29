use super::invalid;
use crate::error::Result;
use serde_json::{Value, json};

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16> {
    let field = bytes.get(offset..offset + 2).ok_or_else(|| invalid("Truncated WAV field."))?;
    Ok(u16::from_le_bytes(field.try_into().map_err(|_| invalid("Invalid WAV field."))?))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    let field = bytes.get(offset..offset + 4).ok_or_else(|| invalid("Truncated WAV field."))?;
    Ok(u32::from_le_bytes(field.try_into().map_err(|_| invalid("Invalid WAV field."))?))
}

pub(super) fn inspect(bytes: &[u8]) -> Result<Value> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(invalid("Only structurally valid PCM or IEEE-float RIFF/WAVE audio is supported."));
    }
    let end = usize::try_from(u32_at(bytes, 4)?).ok().and_then(|n| n.checked_add(8));
    if end != Some(bytes.len()) {
        return Err(invalid("WAV RIFF length does not match the original bytes."));
    }
    let mut offset = 12usize;
    let mut format = None;
    let mut data = None;
    let mut chunks = 0usize;
    while offset < bytes.len() {
        chunks += 1;
        if chunks > 4096 {
            return Err(invalid("WAV has too many chunks."));
        }
        let tag = bytes.get(offset..offset + 4).ok_or_else(|| invalid("Truncated WAV chunk."))?;
        let size = u32_at(bytes, offset + 4)? as usize;
        let start = offset.checked_add(8).ok_or_else(|| invalid("WAV offset overflow."))?;
        let stop = start.checked_add(size).filter(|n| *n <= bytes.len()).ok_or_else(|| invalid("WAV chunk exceeds the original bytes."))?;
        let body = &bytes[start..stop];
        if tag == b"fmt " {
            if format.is_some() || size < 16 {
                return Err(invalid("WAV needs one complete format chunk."));
            }
            let code = u16_at(body, 0)?;
            let channels = u16_at(body, 2)?;
            let rate = u32_at(body, 4)?;
            let byte_rate = u32_at(body, 8)?;
            let align = u16_at(body, 12)?;
            let bits = u16_at(body, 14)?;
            if channels == 0 || channels > 64 || rate == 0 || rate > 768000
                || !matches!((code, bits), (1, 8 | 16 | 24 | 32) | (3, 32 | 64))
            {
                return Err(invalid("Unsupported or invalid WAV encoding, channels or sample rate."));
            }
            if size != 16 && (size < 18 || usize::from(u16_at(body, 16)?) + 18 != size) {
                return Err(invalid("WAV format extension length is inconsistent."));
            }
            let expected_align = channels.checked_mul(bits / 8).ok_or_else(|| invalid("WAV block alignment overflow."))?;
            if align != expected_align || rate.checked_mul(u32::from(align)) != Some(byte_rate) {
                return Err(invalid("WAV byte rate or block alignment is inconsistent."));
            }
            format = Some((code, channels, rate, align, bits));
        } else if tag == b"data" {
            if format.is_none() || data.is_some() || size == 0 {
                return Err(invalid("WAV needs one nonempty data chunk."));
            }
            data = Some(body);
        }
        offset = stop.checked_add(size % 2).filter(|n| *n <= bytes.len()).ok_or_else(|| invalid("Missing WAV chunk padding."))?;
    }
    let (code, channels, rate, align, bits) = format.ok_or_else(|| invalid("WAV format chunk is missing."))?;
    let data = data.ok_or_else(|| invalid("WAV sample data is missing."))?;
    if !data.len().is_multiple_of(usize::from(align)) {
        return Err(invalid("WAV samples do not match the declared block alignment."));
    }
    if code == 3 {
        let finite = if bits == 32 {
            data.chunks_exact(4).all(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]]).is_finite())
        } else {
            data.chunks_exact(8).all(|v| f64::from_le_bytes([v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]]).is_finite())
        };
        if !finite {
            return Err(invalid("WAV floating-point samples must be finite."));
        }
    }
    let frames = data.len() as u64 / u64::from(align);
    Ok(json!({"sample_rate":rate,"channels":channels,"bits_per_sample":bits,
        "sample_count":frames,"duration_ms":frames.saturating_mul(1000) / u64::from(rate),
        "encoding":if code == 1 { "pcm" } else { "ieee_float" },"validation":"structural"}))
}
