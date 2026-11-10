use super::failure;
use crate::devtools::private_files::Secret;
use chrono::{Datelike, TimeZone, Utc};
use ring::{
    rand::{SecureRandom, SystemRandom},
    signature,
};
use serde_json::{Value, json};
use std::{
    ffi::{CString, c_char, c_int, c_void},
    io,
    path::Path,
};

#[path = "ffi.rs"]
mod ffi;
use ffi::{Api, Owned, Ref, Tm};

pub(super) fn digest(bytes: &[u8]) -> String {
    crate::compat::hash::sha256(bytes)
}
fn yes(value: c_int) -> io::Result<()> {
    if value > 0 { Ok(()) } else { Err(failure()) }
}
fn text(value: &str) -> io::Result<CString> {
    CString::new(value).map_err(|_| failure())
}
fn password(bytes: &[u8]) -> io::Result<Secret> {
    if bytes.len() != 64
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+/=".contains(byte))
    {
        return Err(failure());
    }
    let mut value = Secret(bytes.to_vec());
    value.0.push(0);
    Ok(value)
}
fn encoded(
    api: &Api,
    object: Ref,
    encode: unsafe extern "C" fn(Ref, *mut *mut u8) -> c_int,
    limit: usize,
) -> io::Result<Vec<u8>> {
    let _ = api;
    // SAFETY: live typed OpenSSL object; first call measures, second writes the owned sized buffer.
    let size = unsafe { encode(object, std::ptr::null_mut()) };
    if size <= 0 || size as usize > limit {
        return Err(failure());
    }
    let mut bytes = vec![0; size as usize];
    let mut cursor = bytes.as_mut_ptr();
    if unsafe { encode(object, &mut cursor) } != size {
        return Err(failure());
    }
    Ok(bytes)
}
fn key(api: &Api) -> io::Result<Owned<'_>> {
    let context = Owned::new(
        api,
        unsafe {
            (api.EVP_PKEY_CTX_new_from_name)(
                std::ptr::null_mut(),
                c"RSA".as_ptr(),
                std::ptr::null(),
            )
        },
        api.EVP_PKEY_CTX_free,
    )?;
    yes(unsafe { (api.EVP_PKEY_keygen_init)(context.value) })?;
    yes(unsafe { (api.EVP_PKEY_CTX_set_rsa_keygen_bits)(context.value, 3072) })?;
    let mut value = std::ptr::null_mut();
    let result = unsafe { (api.EVP_PKEY_keygen)(context.value, &mut value) };
    let owned = Owned::new(api, value, api.EVP_PKEY_free)?;
    yes(result)?;
    if unsafe { (api.EVP_PKEY_get_bits)(owned.value) } != 3072 {
        return Err(failure());
    }
    Ok(owned)
}
fn date(api: &Api, value: Ref) -> io::Result<chrono::DateTime<Utc>> {
    let mut output = Tm::default();
    yes(unsafe { (api.ASN1_TIME_to_tm)(value, &mut output) })?;
    Utc.with_ymd_and_hms(
        output.year + 1900,
        (output.month + 1) as u32,
        output.day as u32,
        output.hour as u32,
        output.min as u32,
        output.sec as u32,
    )
    .single()
    .ok_or_else(failure)
}
fn public(api: &Api, key: Ref) -> io::Result<Vec<u8>> {
    // i2d_PublicKey for an RSA EVP key is PKCS#1 RSAPublicKey, not SubjectPublicKeyInfo.
    encoded(api, key, api.i2d_PublicKey, 2048)
}
fn sign(api: &Api, key: Ref, bytes: &[u8]) -> io::Result<Vec<u8>> {
    let context = Owned::new(api, unsafe { (api.EVP_MD_CTX_new)() }, api.EVP_MD_CTX_free)?;
    yes(unsafe {
        (api.EVP_DigestSignInit)(
            context.value,
            std::ptr::null_mut(),
            (api.EVP_sha256)(),
            std::ptr::null_mut(),
            key,
        )
    })?;
    yes(unsafe {
        (api.EVP_DigestSignUpdate)(
            context.value,
            bytes.as_ptr().cast::<c_void>() as Ref,
            bytes.len(),
        )
    })?;
    let mut size = 0;
    yes(unsafe { (api.EVP_DigestSignFinal)(context.value, std::ptr::null_mut(), &mut size) })?;
    if size != 384 {
        return Err(failure());
    }
    let mut signature = vec![0; size];
    yes(unsafe { (api.EVP_DigestSignFinal)(context.value, signature.as_mut_ptr(), &mut size) })?;
    if size != signature.len() {
        return Err(failure());
    }
    Ok(signature)
}
fn self_signed<'a>(api: &'a Api, key: &Owned<'a>) -> io::Result<Owned<'a>> {
    let certificate = Owned::new(api, unsafe { (api.X509_new)() }, api.X509_free)?;
    yes(unsafe { (api.X509_set_version)(certificate.value, 2) })?;
    let mut serial = [0u8; 8];
    SystemRandom::new()
        .fill(&mut serial)
        .map_err(|_| failure())?;
    let serial = (u64::from_be_bytes(serial) & i64::MAX as u64).max(1);
    yes(unsafe {
        (api.ASN1_INTEGER_set_uint64)((api.X509_get_serialNumber)(certificate.value), serial)
    })?;
    let now = Utc::now();
    let start = now - chrono::Duration::minutes(10);
    let end = now
        .with_year(now.year() + 1)
        .or_else(|| {
            now.with_day(28)
                .and_then(|date| date.with_year(now.year() + 1))
        })
        .ok_or_else(failure)?;
    for (value, date) in [
        (
            unsafe { (api.X509_getm_notBefore)(certificate.value) },
            start,
        ),
        (unsafe { (api.X509_getm_notAfter)(certificate.value) }, end),
    ] {
        let encoded = text(&date.format("%Y%m%d%H%M%SZ").to_string())?;
        yes(unsafe { (api.ASN1_TIME_set_string_X509)(value, encoded.as_ptr()) })?;
    }
    yes(unsafe { (api.X509_set_pubkey)(certificate.value, key.value) })?;
    let name = unsafe { (api.X509_get_subject_name)(certificate.value) };
    if name.is_null() {
        return Err(failure());
    }
    yes(unsafe {
        (api.X509_NAME_add_entry_by_txt)(
            name,
            c"CN".as_ptr(),
            0x1000,
            b"KiaroSama".as_ptr(),
            9,
            -1,
            0,
        )
    })?;
    yes(unsafe { (api.X509_set_issuer_name)(certificate.value, name) })?;
    for (name, value) in [
        ("keyUsage", "critical,digitalSignature"),
        ("extendedKeyUsage", "critical,codeSigning"),
        ("basicConstraints", "critical,CA:FALSE"),
    ] {
        let name = text(name)?;
        let value = text(value)?;
        let extension = Owned::new(
            api,
            unsafe {
                (api.X509V3_EXT_conf)(
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    name.as_ptr(),
                    value.as_ptr(),
                )
            },
            api.X509_EXTENSION_free,
        )?;
        yes(unsafe { (api.X509_add_ext)(certificate.value, extension.value, -1) })?;
    }
    yes(unsafe { (api.X509_sign)(certificate.value, key.value, (api.EVP_sha256)()) })?;
    Ok(certificate)
}
fn verify(
    api: &Api,
    key: Ref,
    certificate: Ref,
    metadata: &Value,
    key_bytes: &[u8],
    cert_bytes: &[u8],
) -> io::Result<()> {
    yes(unsafe { (api.X509_check_private_key)(certificate, key) })?;
    let name = unsafe { (api.X509_get_subject_name)(certificate) };
    let mut common_name = [0u8; 32];
    let nid = unsafe { (api.OBJ_txt2nid)(c"commonName".as_ptr()) };
    if name.is_null()
        || nid <= 0
        || unsafe { (api.X509_NAME_entry_count)(name) } != 1
        || unsafe {
            (api.X509_NAME_get_text_by_NID)(
                name,
                nid,
                common_name.as_mut_ptr().cast::<c_char>(),
                common_name.len() as c_int,
            )
        } != 9
        || &common_name[..9] != b"KiaroSama"
        || unsafe { (api.EVP_PKEY_get_bits)(key) } != 3072
        || metadata["publisher"] != "KiaroSama"
        || metadata["algorithm"] != "rsa-pkcs1-sha256"
        || metadata["public_key_sha256"] != digest(key_bytes)
        || metadata["certificate_sha256"] != digest(cert_bytes)
        || public(api, key)? != key_bytes
        || encoded(api, certificate, api.i2d_X509, 16384)? != cert_bytes
    {
        return Err(failure());
    }
    let before = date(api, unsafe { (api.X509_getm_notBefore)(certificate) })?;
    let after = date(api, unsafe { (api.X509_getm_notAfter)(certificate) })?;
    let metadata_date = |name: &str| {
        metadata[name]
            .as_str()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|date| date.with_timezone(&Utc))
            .ok_or_else(failure)
    };
    if metadata_date("not_before")? != before
        || metadata_date("not_after")? != after
        || Utc::now() < before
        || Utc::now() > after
    {
        return Err(failure());
    }
    Ok(())
}

pub(super) struct Material {
    pub pfx: Secret,
    pub password: Secret,
    pub key: Vec<u8>,
    pub certificate: Vec<u8>,
    pub metadata: Value,
}
pub(super) struct Crypto {
    api: Api,
}
impl Crypto {
    pub fn open(path: &Path, hash: &str) -> io::Result<Self> {
        Ok(Self {
            api: Api::open(path, hash)?,
        })
    }
    pub fn create(&self) -> io::Result<Material> {
        let api = &self.api;
        let private = key(api)?;
        let certificate = self_signed(api, &private)?;
        let mut random = Secret(vec![0; 48]);
        SystemRandom::new()
            .fill(&mut random.0)
            .map_err(|_| failure())?;
        use base64::Engine;
        let pass = Secret(
            base64::engine::general_purpose::STANDARD
                .encode(&random.0)
                .into_bytes(),
        );
        let terminated = password(&pass.0)?;
        let cipher = unsafe { (api.OBJ_txt2nid)(c"AES-256-CBC".as_ptr()) };
        if cipher <= 0 {
            return Err(failure());
        }
        let pfx = Owned::new(
            api,
            unsafe {
                (api.PKCS12_create)(
                    terminated.0.as_ptr().cast(),
                    c"KiaroSama".as_ptr(),
                    private.value,
                    certificate.value,
                    std::ptr::null_mut(),
                    cipher,
                    cipher,
                    200000,
                    200000,
                    0,
                )
            },
            api.PKCS12_free,
        )?;
        yes(unsafe {
            (api.PKCS12_set_mac)(
                pfx.value,
                terminated.0.as_ptr().cast(),
                pass.0.len() as c_int,
                std::ptr::null_mut(),
                16,
                200000,
                (api.EVP_sha256)(),
            )
        })?;
        let key = public(api, private.value)?;
        let cert = encoded(api, certificate.value, api.i2d_X509, 16384)?;
        let mut thumbprint = [0u8; 20];
        let mut thumbprint_size = 0;
        yes(unsafe {
            (api.X509_digest)(
                certificate.value,
                (api.EVP_sha1)(),
                thumbprint.as_mut_ptr(),
                &mut thumbprint_size,
            )
        })?;
        if thumbprint_size != 20 {
            return Err(failure());
        }
        let thumbprint = thumbprint
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>();
        let metadata = json!({"publisher":"KiaroSama","product":"Numera Gen MCP","algorithm":"rsa-pkcs1-sha256",
            "certificate_thumbprint":thumbprint,
            "public_key_sha256":digest(&key),"certificate_sha256":digest(&cert),
            "not_before":date(api, unsafe { (api.X509_getm_notBefore)(certificate.value) })?.to_rfc3339(),
            "not_after":date(api, unsafe { (api.X509_getm_notAfter)(certificate.value) })?.to_rfc3339(),
            "trust":"self-signed; not public OS trust, Apple Developer ID or notarization",
            "timestamp":"none; verification requires current certificate validity"});
        verify(
            api,
            private.value,
            certificate.value,
            &metadata,
            &key,
            &cert,
        )?;
        Ok(Material {
            pfx: Secret(encoded(api, pfx.value, api.i2d_PKCS12, 32768)?),
            password: pass,
            key,
            certificate: cert,
            metadata,
        })
    }
    pub fn check(
        &self,
        bytes: &[u8],
        pass: &[u8],
        key_bytes: &[u8],
        cert_bytes: &[u8],
        metadata: &Value,
    ) -> io::Result<()> {
        if bytes.is_empty()
            || bytes.len() > 32768
            || key_bytes.len() > 2048
            || cert_bytes.len() > 16384
        {
            return Err(failure());
        }
        let api = &self.api;
        let mut cursor = bytes.as_ptr();
        let pfx = Owned::new(
            api,
            unsafe {
                (api.d2i_PKCS12)(
                    std::ptr::null_mut(),
                    &mut cursor,
                    bytes.len() as std::ffi::c_long,
                )
            },
            api.PKCS12_free,
        )?;
        if cursor != unsafe { bytes.as_ptr().add(bytes.len()) } {
            return Err(failure());
        }
        let terminated = password(pass)?;
        let mut private = std::ptr::null_mut();
        let mut certificate = std::ptr::null_mut();
        let parsed = unsafe {
            (api.PKCS12_parse)(
                pfx.value,
                terminated.0.as_ptr().cast(),
                &mut private,
                &mut certificate,
                std::ptr::null_mut(),
            )
        };
        let private = Owned::take(api, private, api.EVP_PKEY_free);
        let certificate = Owned::take(api, certificate, api.X509_free);
        yes(parsed)?;
        if private.value.is_null() || certificate.value.is_null() {
            return Err(failure());
        }
        verify(
            api,
            private.value,
            certificate.value,
            metadata,
            key_bytes,
            cert_bytes,
        )?;
        let message = "Numera approved identity 日本語 fixture".as_bytes();
        let signed = sign(api, private.value, message)?;
        let verifier =
            signature::UnparsedPublicKey::new(&signature::RSA_PKCS1_2048_8192_SHA256, key_bytes);
        verifier.verify(message, &signed).map_err(|_| failure())?;
        let mut changed = message.to_vec();
        changed[0] ^= 1;
        if verifier.verify(&changed, &signed).is_ok() {
            return Err(failure());
        }
        let wrong = key(api)?;
        let wrong_key = public(api, wrong.value)?;
        if signature::UnparsedPublicKey::new(&signature::RSA_PKCS1_2048_8192_SHA256, wrong_key)
            .verify(message, &signed)
            .is_ok()
        {
            return Err(failure());
        }
        Ok(())
    }
    pub fn fixture_signer<'a>(
        &'a self,
        bytes: &[u8],
        pass: &[u8],
    ) -> io::Result<FixtureSigner<'a>> {
        let api = &self.api;
        let terminated = password(pass)?;
        let mut cursor = bytes.as_ptr();
        let pfx = Owned::new(
            api,
            unsafe {
                (api.d2i_PKCS12)(
                    std::ptr::null_mut(),
                    &mut cursor,
                    bytes.len() as std::ffi::c_long,
                )
            },
            api.PKCS12_free,
        )?;
        let mut private = std::ptr::null_mut();
        let result = unsafe {
            (api.PKCS12_parse)(
                pfx.value,
                terminated.0.as_ptr().cast(),
                &mut private,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        let private = Owned::take(api, private, api.EVP_PKEY_free);
        yes(result)?;
        if private.value.is_null() {
            return Err(failure());
        }
        let wrong = key(api)?;
        Ok(FixtureSigner {
            api,
            private,
            wrong,
        })
    }
}

pub(super) struct FixtureSigner<'a> {
    api: &'a Api,
    private: Owned<'a>,
    wrong: Owned<'a>,
}
impl FixtureSigner<'_> {
    pub fn signatures(&self, payload: &[u8]) -> io::Result<(Vec<u8>, Vec<u8>)> {
        Ok((
            sign(self.api, self.private.value, payload)?,
            sign(self.api, self.wrong.value, payload)?,
        ))
    }
}
