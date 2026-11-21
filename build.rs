//! Embeds the approved logo as the Windows executable icon. The `.ico` is compiled into a
//! binary `.res` resource here, without a resource compiler or extra build dependency, and the
//! MSVC linker accepts that file directly. Other targets have no executable icon slot.
use std::{env, fs, io, path::PathBuf};

const ICON: &str = "docs/assets/logo.ico";
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const LANGUAGE_NEUTRAL: u16 = 0;

struct Image {
    width: u8,
    height: u8,
    colors: u8,
    planes: u16,
    bits: u16,
    bytes: Vec<u8>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn word(bytes: &[u8], at: usize) -> io::Result<u16> {
    bytes
        .get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| invalid("Truncated icon directory."))
}

fn dword(bytes: &[u8], at: usize) -> io::Result<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| invalid("Truncated icon directory."))
}

fn images(ico: &[u8]) -> io::Result<Vec<Image>> {
    if word(ico, 0)? != 0 || word(ico, 2)? != 1 {
        return Err(invalid("Logo icon is not an ICO directory."));
    }
    let count = usize::from(word(ico, 4)?);
    if count == 0 || count > 32 {
        return Err(invalid("Logo icon image count is outside its bound."));
    }
    (0..count)
        .map(|index| {
            let entry = 6 + index * 16;
            let size = dword(ico, entry + 8)? as usize;
            let offset = dword(ico, entry + 12)? as usize;
            let bytes = ico
                .get(
                    offset
                        ..offset
                            .checked_add(size)
                            .ok_or_else(|| invalid("Icon overflow."))?,
                )
                .ok_or_else(|| invalid("Logo icon image escapes the file."))?
                .to_vec();
            Ok(Image {
                width: ico[entry],
                height: ico[entry + 1],
                colors: ico[entry + 2],
                planes: word(ico, entry + 4)?,
                bits: word(ico, entry + 6)?,
                bytes,
            })
        })
        .collect()
}

/// One RESOURCEHEADER with ordinal type and name, followed by DWORD-aligned data.
fn resource(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    let fields: [&[u8]; 4] = [
        &(data.len() as u32).to_le_bytes(),
        &32u32.to_le_bytes(),
        &[0xff, 0xff],
        &kind.to_le_bytes(),
    ];
    for field in fields {
        out.extend_from_slice(field);
    }
    out.extend_from_slice(&[0xff, 0xff]);
    out.extend_from_slice(&name.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&LANGUAGE_NEUTRAL.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn compile(ico: &[u8]) -> io::Result<Vec<u8>> {
    let images = images(ico)?;
    // The leading empty entry marks a 32-bit resource file.
    let mut out = Vec::new();
    resource(&mut out, 0, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (index, image) in images.iter().enumerate() {
        let id = index as u16 + 1;
        resource(&mut out, RT_ICON, id, 0x1010, &image.bytes);
        group.extend_from_slice(&[image.width, image.height, image.colors, 0]);
        group.extend_from_slice(&image.planes.to_le_bytes());
        group.extend_from_slice(&image.bits.to_le_bytes());
        group.extend_from_slice(&(image.bytes.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    resource(&mut out, RT_GROUP_ICON, 1, 0x1030, &group);
    Ok(out)
}

fn rustc_version() -> String {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|stdout| stdout.split_whitespace().nth(1).map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-env=NUMERA_RUSTC_VERSION={}", rustc_version());
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let environment = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || environment != "msvc" {
        return Ok(());
    }
    let resource = compile(&fs::read(ICON)?)?;
    let path = PathBuf::from(env::var_os("OUT_DIR").ok_or_else(|| invalid("OUT_DIR missing."))?)
        .join("numera-logo.res");
    fs::write(&path, resource)?;
    println!("cargo:rustc-link-arg-bins={}", path.display());
    Ok(())
}
