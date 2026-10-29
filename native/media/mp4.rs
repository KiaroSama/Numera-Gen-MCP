use super::invalid;
use crate::error::Result;
use serde_json::{Value, json};

#[derive(Clone, Copy)]
struct Atom<'a> {
    kind: [u8; 4],
    body: &'a [u8],
    start: usize,
}

fn be32(bytes: &[u8], offset: usize) -> Result<u32> {
    let b = bytes.get(offset..offset + 4).ok_or_else(|| invalid("Truncated MP4 field."))?;
    Ok(u32::from_be_bytes(b.try_into().map_err(|_| invalid("Invalid MP4 field."))?))
}

fn be64(bytes: &[u8], offset: usize) -> Result<u64> {
    let b = bytes.get(offset..offset + 8).ok_or_else(|| invalid("Truncated MP4 field."))?;
    Ok(u64::from_be_bytes(b.try_into().map_err(|_| invalid("Invalid MP4 field."))?))
}

fn atoms(bytes: &[u8], base: usize) -> Result<Vec<Atom<'_>>> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        if out.len() >= 4096 { return Err(invalid("Too many MP4 boxes.")); }
        let size = be32(bytes, offset)?;
        let kind: [u8; 4] = bytes.get(offset + 4..offset + 8).ok_or_else(|| invalid("Truncated MP4 box."))?.try_into().map_err(|_| invalid("Invalid MP4 box type."))?;
        let (size, header) = match size {
            0 => (bytes.len() - offset, 8usize),
            1 => (usize::try_from(be64(bytes, offset + 8)?).map_err(|_| invalid("MP4 extended size overflow."))?, 16),
            n => (n as usize, 8),
        };
        let end = offset.checked_add(size).filter(|end| size >= header && *end <= bytes.len()).ok_or_else(|| invalid("MP4 box exceeds original bytes."))?;
        out.push(Atom { kind, body: &bytes[offset + header..end], start: base + offset + header });
        offset = end;
    }
    Ok(out)
}

fn one<'a>(boxes: &[Atom<'a>], kind: &[u8; 4]) -> Result<Atom<'a>> {
    let mut items = boxes.iter().filter(|b| &b.kind == kind);
    let item = *items.next().ok_or_else(|| invalid("Required MP4 box is missing."))?;
    if items.next().is_some() { return Err(invalid("Duplicate MP4 structural box.")); }
    Ok(item)
}

fn full(atom: Atom<'_>, minimum: usize) -> Result<()> {
    if atom.body.len() < minimum || atom.body[0] != 0 {
        return Err(invalid("Unsupported or truncated MP4 full box version."));
    }
    Ok(())
}

fn table(atom: Atom<'_>, entry_size: usize, prefix: usize) -> Result<usize> {
    full(atom, prefix)?;
    let count = be32(atom.body, prefix - 4)? as usize;
    if count > 1000000 || count.checked_mul(entry_size).and_then(|n| n.checked_add(prefix)) != Some(atom.body.len()) {
        return Err(invalid("MP4 sample table count/length is inconsistent."));
    }
    Ok(count)
}

struct Codec {
    kind: [u8; 4],
    width: u32,
    height: u32,
    nal_length: usize,
}

fn descriptions(stsd: Atom<'_>, max_pixels: u64, references: usize) -> Result<Vec<Codec>> {
    full(stsd, 8)?;
    let entries = atoms(&stsd.body[8..], stsd.start + 8)?;
    if be32(stsd.body, 4)? as usize != entries.len() || entries.is_empty() || entries.len() > 32 {
        return Err(invalid("MP4 sample descriptions are missing or inconsistent."));
    }
    let mut result = Vec::new();
    for entry in entries {
        if entry.body.len() < 78 { return Err(invalid("Truncated MP4 visual sample description.")); }
        let reference = u16::from_be_bytes([entry.body[6], entry.body[7]]) as usize;
        if reference == 0 || reference > references { return Err(invalid("MP4 sample selects a missing data reference.")); }
        let width = u32::from(u16::from_be_bytes([entry.body[24], entry.body[25]]));
        let height = u32::from(u16::from_be_bytes([entry.body[26], entry.body[27]]));
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > max_pixels {
            return Err(invalid("MP4 visual dimensions exceed the configured pixel limit."));
        }
        let extra = atoms(&entry.body[78..], entry.start + 78)?;
        let nal_length = match &entry.kind {
            b"jpeg" | b"mjpg" => 0,
            b"avc1" | b"avc3" => avcc(one(&extra, b"avcC")?.body)?,
            b"hvc1" | b"hev1" => hvcc(one(&extra, b"hvcC")?.body)?,
            _ => return Err(invalid("Unsupported MP4 visual codec; no decode/conversion fallback is available.")),
        };
        result.push(Codec { kind: entry.kind, width, height, nal_length });
    }
    Ok(result)
}

fn avcc(b: &[u8]) -> Result<usize> {
    if b.len() < 7 || b[0] != 1 || b[4] & 0xfc != 0xfc || b[5] & 0xe0 != 0xe0 {
        return Err(invalid("Invalid MP4 AVC decoder configuration."));
    }
    let length = usize::from(b[4] & 3) + 1;
    if length == 3 { return Err(invalid("Reserved MP4 AVC NAL length size.")); }
    let mut offset = 6usize;
    let sps = usize::from(b[5] & 31);
    if sps == 0 { return Err(invalid("MP4 AVC configuration has no SPS.")); }
    for _ in 0..sps { parameter_set(b, &mut offset, 7)?; }
    let pps = usize::from(*b.get(offset).ok_or_else(|| invalid("Missing MP4 AVC PPS count."))?);
    offset += 1;
    if pps == 0 { return Err(invalid("MP4 AVC configuration has no PPS.")); }
    for _ in 0..pps { parameter_set(b, &mut offset, 8)?; }
    // High-profile extension fields, when present, are structurally parsed as configuration data.
    if offset < b.len() {
        if b.len() - offset < 4 || b[offset] & 0xfc != 0xfc || b[offset + 1] & 0xf8 != 0xf8 || b[offset + 2] & 0xf8 != 0xf8 {
            return Err(invalid("Invalid MP4 AVC configuration extension."));
        }
        let count = b[offset + 3];
        offset += 4;
        for _ in 0..count { parameter_set(b, &mut offset, 13)?; }
    }
    if offset != b.len() { return Err(invalid("Unexpected MP4 AVC configuration trailing bytes.")); }
    Ok(length)
}

fn parameter_set(b: &[u8], offset: &mut usize, expected: u8) -> Result<()> {
    let field = b.get(*offset..*offset + 2).ok_or_else(|| invalid("Truncated MP4 AVC parameter length."))?;
    let size = usize::from(u16::from_be_bytes([field[0], field[1]]));
    *offset += 2;
    let minimum = if expected == 7 { 4 } else { 2 };
    let end = offset.checked_add(size).filter(|n| size >= minimum && *n <= b.len()).ok_or_else(|| invalid("MP4 AVC parameter exceeds decoder configuration."))?;
    if b[*offset] & 0x80 != 0 || b[*offset] & 31 != expected {
        return Err(invalid("MP4 AVC parameter set has an invalid NAL type."));
    }
    *offset = end;
    Ok(())
}

fn hvcc(b: &[u8]) -> Result<usize> {
    if b.len() < 23 || b[0] != 1 { return Err(invalid("Invalid MP4 HEVC decoder configuration.")); }
    let length = usize::from(b[21] & 3) + 1;
    let mut offset = 23usize;
    let mut parameter_types = Vec::new();
    for _ in 0..b[22] {
        let header = b.get(offset..offset + 3).ok_or_else(|| invalid("Truncated MP4 HEVC array."))?;
        let kind = header[0] & 63;
        let count = u16::from_be_bytes([header[1], header[2]]);
        offset += 3;
        for _ in 0..count {
            let size = b.get(offset..offset + 2).ok_or_else(|| invalid("Truncated MP4 HEVC parameter length."))?;
            let size = usize::from(u16::from_be_bytes([size[0], size[1]]));
            offset += 2;
            let minimum = match kind { 32 => 6usize, 33 => 15, 34 => 3, _ => 2 };
            let end = offset.checked_add(size).filter(|n| size >= minimum && *n <= b.len()).ok_or_else(|| invalid("MP4 HEVC parameter exceeds configuration."))?;
            if b[offset] & 0x80 != 0 || (b[offset] >> 1) & 63 != kind || b[offset + 1] & 7 == 0 {
                return Err(invalid("Invalid MP4 HEVC parameter NAL header."));
            }
            parameter_types.push(kind);
            offset = end;
        }
    }
    if offset != b.len() || ![32, 33, 34].iter().all(|t| parameter_types.contains(t)) {
        return Err(invalid("MP4 HEVC decoder configuration is incomplete."));
    }
    Ok(length)
}

fn sample(bytes: &[u8], codec: &Codec, max_pixels: u64) -> Result<()> {
    if matches!(&codec.kind, b"jpeg" | b"mjpg") {
        let config = crate::compat::value::json!({"files":{"maxPixels":max_pixels}});
        let image = crate::files::images::inspect(bytes.to_vec(), &config, Some(bytes.len()))?;
        if image.mime != "image/jpeg" || image.width != codec.width || image.height != codec.height {
            return Err(invalid("MP4 JPEG sample differs from its declared visual format."));
        }
        return Ok(());
    }
    let hevc = matches!(&codec.kind, b"hvc1" | b"hev1");
    let mut offset = 0usize;
    let mut coded_picture = false;
    while offset < bytes.len() {
        let field = bytes.get(offset..offset + codec.nal_length).ok_or_else(|| invalid("Truncated MP4 NAL length."))?;
        let size = field.iter().fold(0usize, |value, b| (value << 8) | usize::from(*b));
        offset += codec.nal_length;
        let end = offset.checked_add(size).filter(|n| size >= (if hevc { 3 } else { 2 }) && *n <= bytes.len()).ok_or_else(|| invalid("MP4 NAL payload exceeds its sample."))?;
        let header = bytes[offset];
        if header & 0x80 != 0 { return Err(invalid("MP4 NAL forbidden bit is set.")); }
        if hevc {
            if bytes[offset + 1] & 7 == 0 { return Err(invalid("MP4 HEVC temporal identifier is invalid.")); }
            coded_picture |= ((header >> 1) & 63) <= 31;
        } else {
            let kind = header & 31;
            if kind == 0 || kind >= 24 { return Err(invalid("Invalid MP4 AVC NAL unit type.")); }
            coded_picture |= (1..=5).contains(&kind);
        }
        offset = end;
    }
    if !coded_picture { return Err(invalid("MP4 sample has no coded picture payload.")); }
    Ok(())
}

fn track(track: Atom<'_>, mdat: &[(usize, usize)], all: &[u8], max_pixels: u64) -> Result<(u32, u32, u64, usize)> {
    let boxes = atoms(track.body, track.start)?;
    let tkhd = one(&boxes, b"tkhd")?;
    full(tkhd, 84)?;
    if be32(tkhd.body, 12)? == 0 { return Err(invalid("MP4 track identifier is zero.")); }
    let mdia = one(&boxes, b"mdia")?;
    let boxes = atoms(mdia.body, mdia.start)?;
    let hdlr = one(&boxes, b"hdlr")?;
    full(hdlr, 24)?;
    if hdlr.body.get(8..12) != Some(b"vide") { return Err(invalid("Only self-contained visual MP4 tracks are supported.")); }
    let mdhd = one(&boxes, b"mdhd")?;
    full(mdhd, 24)?;
    let timescale = be32(mdhd.body, 12)?;
    let duration = be32(mdhd.body, 16)?;
    if timescale == 0 || duration == 0 || duration == u32::MAX { return Err(invalid("MP4 media time scale/duration is invalid.")); }
    let minf = one(&boxes, b"minf")?;
    let boxes = atoms(minf.body, minf.start)?;
    let dinf = one(&boxes, b"dinf")?;
    let dinf_boxes = atoms(dinf.body, dinf.start)?;
    let dref = one(&dinf_boxes, b"dref")?;
    full(dref, 8)?;
    let references = atoms(&dref.body[8..], dref.start + 8)?;
    if references.is_empty() || references.len() != be32(dref.body, 4)? as usize {
        return Err(invalid("MP4 data references are missing or inconsistent."));
    }
    for reference in &references {
        if &reference.kind != b"url " || reference.body != [0, 0, 0, 1] {
            return Err(invalid("External MP4 data references are unsupported."));
        }
    }
    let vmhd = one(&boxes, b"vmhd")?;
    full(vmhd, 12)?;
    let stbl = one(&boxes, b"stbl")?;
    let boxes = atoms(stbl.body, stbl.start)?;
    let codecs = descriptions(one(&boxes, b"stsd")?, max_pixels, references.len())?;
    let stsz = one(&boxes, b"stsz")?;
    full(stsz, 12)?;
    let uniform = be32(stsz.body, 4)? as usize;
    let count = be32(stsz.body, 8)? as usize;
    if count == 0 || count > 1000000 || (uniform == 0 && count.checked_mul(4).and_then(|n| n.checked_add(12)) != Some(stsz.body.len())) || (uniform != 0 && stsz.body.len() != 12) {
        return Err(invalid("MP4 sample sizes/count are inconsistent."));
    }
    let stts = one(&boxes, b"stts")?;
    let entries = table(stts, 8, 8)?;
    let mut timed_count = 0u64;
    let mut timed_duration = 0u64;
    for i in 0..entries {
        let n = u64::from(be32(stts.body, 8 + i * 8)?);
        let delta = u64::from(be32(stts.body, 12 + i * 8)?);
        if n == 0 || delta == 0 { return Err(invalid("MP4 timing table has an empty entry.")); }
        timed_count = timed_count.checked_add(n).ok_or_else(|| invalid("MP4 timing count overflow."))?;
        timed_duration = timed_duration.checked_add(n.checked_mul(delta).ok_or_else(|| invalid("MP4 duration overflow."))?).ok_or_else(|| invalid("MP4 duration overflow."))?;
    }
    if timed_count != count as u64 || timed_duration != u64::from(duration) { return Err(invalid("MP4 timing does not match sample count/duration.")); }
    if let Some(ctts) = boxes.iter().find(|b| &b.kind == b"ctts").copied() {
        if ctts.body.first().is_none_or(|v| *v > 1) || ctts.body.len() < 8 {
            return Err(invalid("Invalid MP4 composition timing box."));
        }
        let entries = be32(ctts.body, 4)? as usize;
        if entries > 1000000 || entries.checked_mul(8).and_then(|n| n.checked_add(8)) != Some(ctts.body.len()) {
            return Err(invalid("MP4 composition timing count is inconsistent."));
        }
        let mut samples = 0u64;
        for i in 0..entries {
            samples = samples.checked_add(u64::from(be32(ctts.body, 8 + i * 8)?)).ok_or_else(|| invalid("MP4 composition count overflow."))?;
        }
        if samples != count as u64 { return Err(invalid("MP4 composition timing does not match the sample count.")); }
    }
    let stco = boxes.iter().find(|b| matches!(&b.kind, b"stco" | b"co64")).copied().ok_or_else(|| invalid("MP4 chunk offsets are missing."))?;
    if boxes.iter().filter(|b| matches!(&b.kind, b"stco" | b"co64")).count() != 1 { return Err(invalid("Duplicate MP4 chunk offset tables.")); }
    let offset_width = if &stco.kind == b"co64" { 8usize } else { 4 };
    let chunks = table(stco, offset_width, 8)?;
    let stsc = one(&boxes, b"stsc")?;
    let runs = table(stsc, 12, 8)?;
    if runs == 0 || chunks == 0 { return Err(invalid("MP4 chunk mapping is empty.")); }
    let mut mappings: Vec<(usize, usize, usize)> = Vec::new();
    for i in 0..runs {
        let first = be32(stsc.body, 8 + i * 12)? as usize;
        let samples = be32(stsc.body, 12 + i * 12)? as usize;
        let description = be32(stsc.body, 16 + i * 12)? as usize;
        if first == 0 || first > chunks || samples == 0 || description == 0 || description > codecs.len() || (i == 0 && first != 1) || mappings.last().is_some_and(|(previous, _, _)| *previous >= first) {
            return Err(invalid("MP4 sample-to-chunk mapping is invalid."));
        }
        mappings.push((first, samples, description - 1));
    }
    if let Some(stss) = boxes.iter().find(|b| &b.kind == b"stss").copied() {
        let entries = table(stss, 4, 8)?;
        let mut previous = 0;
        for i in 0..entries {
            let index = be32(stss.body, 8 + i * 4)? as usize;
            if index <= previous || index > count { return Err(invalid("MP4 sync sample index is invalid.")); }
            previous = index;
        }
    }
    if codecs.iter().any(|c| c.width != codecs[0].width || c.height != codecs[0].height) {
        return Err(invalid("Changing MP4 sample dimensions require a separate output metadata contract."));
    }
    let mut sample_index = 0usize;
    let mut run = 0usize;
    let mut previous_end = 0usize;
    let mut decoded_pixels = 0u64;
    for chunk in 1..=chunks {
        while run + 1 < mappings.len() && mappings[run + 1].0 <= chunk { run += 1; }
        let (_, samples, description) = mappings[run];
        let offset = if offset_width == 8 { be64(stco.body, 8 + (chunk - 1) * 8)? } else { u64::from(be32(stco.body, 8 + (chunk - 1) * 4)?) };
        let mut position = usize::try_from(offset).map_err(|_| invalid("MP4 sample offset overflow."))?;
        if position < previous_end { return Err(invalid("Overlapping or reordered MP4 sample chunks are unsupported.")); }
        for _ in 0..samples {
            if sample_index >= count { return Err(invalid("MP4 chunk mapping exceeds the sample count.")); }
            let size = if uniform == 0 { be32(stsz.body, 12 + sample_index * 4)? as usize } else { uniform };
            let end = position.checked_add(size).filter(|end| size > 0 && *end <= all.len()).ok_or_else(|| invalid("MP4 sample exceeds original bytes."))?;
            if !mdat.iter().any(|(start, stop)| position >= *start && end <= *stop) { return Err(invalid("MP4 sample points outside media payload boxes.")); }
            if matches!(&codecs[description].kind, b"jpeg" | b"mjpg") {
                decoded_pixels = decoded_pixels.checked_add(u64::from(codecs[description].width) * u64::from(codecs[description].height)).filter(|n| *n <= max_pixels).ok_or_else(|| invalid("MP4 JPEG frame decoding exceeds the aggregate pixel budget."))?;
            }
            sample(&all[position..end], &codecs[description], max_pixels)?;
            position = end;
            sample_index += 1;
        }
        previous_end = position;
    }
    if sample_index != count { return Err(invalid("MP4 chunk mapping leaves samples unresolved.")); }
    Ok((codecs[0].width, codecs[0].height, u64::from(duration) * 1000 / u64::from(timescale), count))
}

pub(super) fn inspect(bytes: &[u8], max_pixels: u64) -> Result<Value> {
    let top = atoms(bytes, 0)?;
    let ftyp = one(&top, b"ftyp")?;
    if ftyp.body.len() < 8 || !ftyp.body.len().is_multiple_of(4) {
        return Err(invalid("Invalid MP4 file type box."));
    }
    let mut brands = [ftyp.body.get(..4)].into_iter().flatten().chain(ftyp.body[8..].chunks_exact(4));
    if !brands.any(|b| matches!(b, b"isom" | b"iso2" | b"iso4" | b"iso5" | b"iso6" | b"mp41" | b"mp42" | b"avc1" | b"M4V ")) {
        return Err(invalid("Unsupported MP4 file brand."));
    }
    if top.iter().any(|b| matches!(&b.kind, b"moof" | b"sidx")) { return Err(invalid("Fragmented MP4 needs a separate validated implementation.")); }
    let moov = one(&top, b"moov")?;
    let boxes = atoms(moov.body, moov.start)?;
    if boxes.iter().any(|b| &b.kind == b"mvex") { return Err(invalid("Fragmented MP4 is unsupported.")); }
    let mvhd = one(&boxes, b"mvhd")?;
    full(mvhd, 100)?;
    if be32(mvhd.body, 12)? == 0 { return Err(invalid("MP4 movie time scale is zero.")); }
    let mdat: Vec<_> = top.iter().filter(|b| &b.kind == b"mdat" && !b.body.is_empty()).map(|b| (b.start, b.start + b.body.len())).collect();
    if mdat.is_empty() { return Err(invalid("MP4 contains no media payload.")); }
    let tracks: Vec<_> = boxes.iter().filter(|b| &b.kind == b"trak").copied().collect();
    if tracks.len() != 1 { return Err(invalid("The structural MP4 validator currently accepts one visual track only.")); }
    let (width, height, duration_ms, sample_count) = track(tracks[0], &mdat, bytes, max_pixels)?;
    Ok(json!({"width":width,"height":height,"duration_ms":duration_ms,"sample_count":sample_count,"track_count":1,"validation":"structural"}))
}
