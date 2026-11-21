use super::invalid;
use super::mp4::{Atom, atoms, be32, be64, full, one, table};
use crate::error::Result;

pub(super) struct AudioTrack {
    pub codec: &'static str,
    pub sample_count: usize,
    pub duration_ms: u64,
}

pub(super) fn handler_type(track: Atom<'_>) -> Result<[u8; 4]> {
    let boxes = atoms(track.body, track.start)?;
    let mdia = one(&boxes, b"mdia")?;
    let mdia_boxes = atoms(mdia.body, mdia.start)?;
    let hdlr = one(&mdia_boxes, b"hdlr")?;
    full(hdlr, 24)?;
    let raw = hdlr
        .body
        .get(8..12)
        .ok_or_else(|| invalid("Truncated MP4 handler box."))?;
    raw.try_into()
        .map_err(|_| invalid("Invalid MP4 handler type."))
}

pub(super) fn inspect_audio(
    track: Atom<'_>,
    mdat: &[(usize, usize)],
    all: &[u8],
) -> Result<AudioTrack> {
    let boxes = atoms(track.body, track.start)?;
    let mdia = one(&boxes, b"mdia")?;
    let mdia_boxes = atoms(mdia.body, mdia.start)?;
    let mdhd = one(&mdia_boxes, b"mdhd")?;
    let (timescale, duration) = if mdhd.body.first() == Some(&1) {
        if mdhd.body.len() < 36 {
            return Err(invalid("Truncated MP4 audio mdhd v1 box."));
        }
        (
            be32(mdhd.body, 20)?,
            u32::try_from(be64(mdhd.body, 24)?).unwrap_or(u32::MAX),
        )
    } else {
        full(mdhd, 24)?;
        (be32(mdhd.body, 12)?, be32(mdhd.body, 16)?)
    };
    if timescale == 0 || duration == 0 {
        return Err(invalid("MP4 audio track timescale or duration is zero."));
    }
    let minf = one(&mdia_boxes, b"minf")?;
    let minf_boxes = atoms(minf.body, minf.start)?;
    let stbl = one(&minf_boxes, b"stbl")?;
    let stbl_boxes = atoms(stbl.body, stbl.start)?;
    let stsd = one(&stbl_boxes, b"stsd")?;
    full(stsd, 16)?;
    let entry_count = be32(stsd.body, 4)?;
    if entry_count == 0 || entry_count > 8 {
        return Err(invalid("Invalid MP4 audio sample description count."));
    }
    let entries = atoms(&stsd.body[8..], stsd.start + 8)?;
    if entries.len() != entry_count as usize {
        return Err(invalid(
            "MP4 audio sample description entries are inconsistent.",
        ));
    }
    let mut codec_name: Option<&'static str> = None;
    for entry in &entries {
        if entry.body.len() < 28 {
            return Err(invalid("Truncated MP4 audio sample entry."));
        }
        let child_boxes = atoms(&entry.body[28..], entry.start + 28)?;
        let current = match &entry.kind {
            b"mp4a" => {
                let esds = one(&child_boxes, b"esds")?;
                if esds.body.len() < 4 {
                    return Err(invalid("Invalid MP4 AAC esds configuration."));
                }
                "mp4a"
            }
            b"Opus" => {
                let dops = one(&child_boxes, b"dOps")?;
                if dops.body.len() < 11 {
                    return Err(invalid("Invalid MP4 Opus dOps configuration."));
                }
                "Opus"
            }
            _ => return Err(invalid("Unsupported MP4 audio codec.")),
        };
        if let Some(prev) = codec_name
            && prev != current
        {
            return Err(invalid(
                "Mixed MP4 audio codecs in one track are unsupported.",
            ));
        }
        codec_name = Some(current);
    }
    let stsz = one(&stbl_boxes, b"stsz")?;
    full(stsz, 12)?;
    let uniform = be32(stsz.body, 4)? as usize;
    let count = be32(stsz.body, 8)? as usize;
    if count == 0 || count > 2_000_000 {
        return Err(invalid("Invalid MP4 audio sample count."));
    }
    if uniform == 0 && count.checked_mul(4).and_then(|n| n.checked_add(12)) != Some(stsz.body.len())
    {
        return Err(invalid("MP4 audio sample size table is inconsistent."));
    }
    let stco = stbl_boxes
        .iter()
        .find(|b| matches!(&b.kind, b"stco" | b"co64"))
        .copied()
        .ok_or_else(|| invalid("MP4 audio chunk offsets are missing."))?;
    if stbl_boxes
        .iter()
        .filter(|b| matches!(&b.kind, b"stco" | b"co64"))
        .count()
        != 1
    {
        return Err(invalid("Duplicate MP4 audio chunk offset tables."));
    }
    let offset_width = if &stco.kind == b"co64" { 8 } else { 4 };
    let chunks = table(stco, offset_width, 8)?;
    if chunks == 0 {
        return Err(invalid("MP4 audio track has no chunks."));
    }
    let stsc = one(&stbl_boxes, b"stsc")?;
    let stsc_entries = table(stsc, 12, 8)?;
    if stsc_entries == 0 {
        return Err(invalid("MP4 audio sample-to-chunk table is empty."));
    }
    let mut mappings = Vec::with_capacity(stsc_entries);
    let mut previous_first = 0usize;
    for i in 0..stsc_entries {
        let first = be32(stsc.body, 8 + i * 12)? as usize;
        let samples = be32(stsc.body, 12 + i * 12)? as usize;
        let desc = be32(stsc.body, 16 + i * 12)? as usize;
        if first <= previous_first
            || first > chunks
            || samples == 0
            || desc == 0
            || desc > entries.len()
        {
            return Err(invalid("Invalid MP4 audio sample-to-chunk entry."));
        }
        previous_first = first;
        mappings.push((first, samples));
    }
    let mut sample_index = 0usize;
    let mut run = 0usize;
    for chunk in 1..=chunks {
        while run + 1 < mappings.len() && mappings[run + 1].0 <= chunk {
            run += 1;
        }
        let (_, samples) = mappings[run];
        let offset = if offset_width == 8 {
            be64(stco.body, 8 + (chunk - 1) * 8)?
        } else {
            u64::from(be32(stco.body, 8 + (chunk - 1) * 4)?)
        };
        let mut position =
            usize::try_from(offset).map_err(|_| invalid("MP4 audio offset overflow."))?;
        for _ in 0..samples {
            if sample_index >= count {
                return Err(invalid("MP4 audio chunk mapping exceeds sample count."));
            }
            let size = if uniform == 0 {
                be32(stsz.body, 12 + sample_index * 4)? as usize
            } else {
                uniform
            };
            let end = position
                .checked_add(size)
                .filter(|end| size > 0 && *end <= all.len())
                .ok_or_else(|| invalid("MP4 audio sample exceeds original bytes."))?;
            if !mdat
                .iter()
                .any(|(start, stop)| position >= *start && end <= *stop)
            {
                return Err(invalid(
                    "MP4 audio sample points outside media payload boxes.",
                ));
            }
            position = end;
            sample_index += 1;
        }
    }
    if sample_index != count {
        return Err(invalid(
            "MP4 audio chunk mapping leaves samples unresolved.",
        ));
    }
    Ok(AudioTrack {
        codec: codec_name.ok_or_else(|| invalid("Missing MP4 audio codec."))?,
        sample_count: count,
        duration_ms: u64::from(duration) * 1000 / u64::from(timescale),
    })
}
