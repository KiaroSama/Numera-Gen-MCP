use crate::context::{fail, sha};
use std::io::{self, Cursor, Read};

const ARCHIVE_LIMIT: usize = 64 * 1024 * 1024;
const MEMBER_LIMIT: usize = 8 * 1024 * 1024;
#[cfg(windows)]
const URL: &str = "https://download.firedaemon.com/FireDaemon-OpenSSL/openssl-3.5.9.zip";
#[cfg(windows)]
const ARCHIVE_SHA256: &str = "76da391395be029b44794857b9ff380255e02a0960a98314cd3f6a6816d82696";

/// The caller authenticates the whole archive; only exact native library bytes are returned.
pub fn select_member(
    bytes: &[u8],
    member: &str,
    expected_size: usize,
    expected_sha256: &str,
    machine: u16,
) -> io::Result<Vec<u8>> {
    if bytes.is_empty()
        || bytes.len() > ARCHIVE_LIMIT
        || expected_size == 0
        || expected_size > MEMBER_LIMIT
    {
        return Err(fail(
            "Identity dependency archive or member exceeds its bound.",
        ));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| fail("Invalid identity dependency ZIP."))?;
    if archive.is_empty() || archive.len() > 1024 || archive.has_duplicate_names() {
        return Err(fail("Identity dependency inventory exceeds its bound."));
    }
    let mut selected = None;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|_| fail("Unreadable identity dependency member."))?;
        let name = file
            .name()
            .map_err(|_| fail("Invalid identity dependency member name."))?;
        if !name.eq_ignore_ascii_case(member) {
            continue;
        }
        let mode = file.unix_mode().unwrap_or(0) & 0o170000;
        if name != member
            || selected.is_some()
            || !file.is_file()
            || !matches!(mode, 0 | 0o100000)
            || file.size() != expected_size as u64
        {
            return Err(fail(
                "Identity dependency member is ambiguous, linked or changed.",
            ));
        }
        let mut data = Vec::with_capacity(expected_size);
        file.take(expected_size as u64 + 1).read_to_end(&mut data)?;
        if data.len() != expected_size || sha(&data) != expected_sha256 {
            return Err(fail("Identity dependency member bytes differ."));
        }
        numera_gen_mcp::devtools::identity::validate_library_image(&data, machine)
            .map_err(|_| fail("Identity dependency PE architecture or format differs."))?;
        selected = Some(data);
    }
    selected.ok_or_else(|| fail("Required identity dependency member is missing."))
}

/// Qualifies a test-only native library, without loading it or touching signing authority.
pub fn prepare(context: &crate::context::Context) -> io::Result<(std::path::PathBuf, String)> {
    context.hosted()?;
    #[cfg(not(windows))]
    {
        Err(fail(
            "Identity test dependency preparation requires a native Windows runner.",
        ))
    }
    #[cfg(windows)]
    {
        use std::time::Duration;
        let (member, size, digest, machine) = match std::env::consts::ARCH {
            "aarch64" => (
                "arm64/bin/libcrypto-3-arm64.dll",
                6_909_112,
                "154847053fb0477e4f721a473eaeef96ce55afa538bfbe5ee86e64f5661e5661",
                0xaa64,
            ),
            "x86_64" => (
                "x64/bin/libcrypto-3-x64.dll",
                7_225_016,
                "5268a29c9a5e353770b894f3dbd148ee8c60971aa1620713c54d8e56563d0aca",
                0x8664,
            ),
            _ => {
                return Err(fail(
                    "Unsupported native Windows identity dependency target.",
                ));
            }
        };
        let directory = context.relative(".ci-work/identity-tools")?;
        std::fs::create_dir(&directory)?;
        let bytes = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(crate::fetch::bounded_get(
                URL,
                crate::fetch::Limits {
                    bytes: ARCHIVE_LIMIT,
                    wall: Duration::from_secs(90),
                    idle: Duration::from_secs(30),
                },
            ))?;
        if sha(&bytes) != ARCHIVE_SHA256 {
            return Err(fail("Identity dependency archive checksum differs."));
        }
        let data = select_member(&bytes, member, size, digest, machine)?;
        let name = member
            .rsplit('/')
            .next()
            .ok_or_else(|| fail("Identity library filename missing."))?;
        let relative = format!(".ci-work/identity-tools/{name}");
        context.write_new(&relative, &data)?;
        let path = context.relative(&relative)?.canonicalize()?;
        if sha(&crate::distribution::read_regular(&path, MEMBER_LIMIT)?) != digest {
            return Err(fail("Written identity dependency bytes differ."));
        }
        Ok((
            numera_gen_mcp::runtime_paths::command_path(&path),
            digest.to_owned(),
        ))
    }
}
