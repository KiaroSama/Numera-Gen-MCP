use super::super::failure;
use std::{ffi::{c_char, c_int, c_long, c_void}, fs::{File, OpenOptions}, io::{self, Read}, os::windows::{ffi::OsStrExt, fs::OpenOptionsExt}, path::Path};

pub type Ref = *mut c_void;
type U8 = *const u8;
macro_rules! functions {
    ($($name:ident: unsafe extern "C" fn($($arg:ty),*) $(-> $ret:ty)?;)+) => {
        pub struct Api {
            module: Ref,
            _file: File,
            $(pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)+
        }
        impl Api {
            pub fn open(path: &Path, expected: &str) -> io::Result<Self> {
                if !path.is_absolute() { return Err(failure()); }
                let _parents = crate::devtools::private_files::lock_parents(path)?;
                let mut file = OpenOptions::new().read(true).share_mode(1).custom_flags(0x00200000).open(path)?;
                let info = file.metadata()?;
                use std::os::windows::fs::MetadataExt;
                if !info.is_file() || info.file_attributes() & 0x400 != 0 || info.len() > 64 * 1024 * 1024
                    || crate::files::windows_identity::links(&file)? != 1 { return Err(failure()); }
                let mut bytes = Vec::new();
                (&mut file).take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
                if bytes.len() as u64 != info.len() || crate::compat::hash::sha256(&bytes) != expected { return Err(failure()); }
                let machine = if cfg!(target_arch = "aarch64") { 0xaa64 } else if cfg!(target_arch = "x86_64") { 0x8664 } else { return Err(failure()); };
                crate::devtools::identity::validate_library_image(&bytes, machine)?;
                let canonical = path.canonicalize()?;
                let wide = canonical.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
                // SAFETY: absolute locked verified DLL; only System32 can supply dependencies.
                let module = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), 0x00000800) };
                if module.is_null() { return Err(failure()); }
                let loaded = (|| {
                    $(let address = unsafe { GetProcAddress(module, concat!(stringify!($name), "\0").as_ptr().cast()) };
                    if address.is_null() { return Err(failure()); }
                    // SAFETY: exact OpenSSL 3 public function declaration and verified symbol name.
                    let $name = unsafe { std::mem::transmute::<Ref, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(address) };)+
                    initialize(module)?;
                    Ok(Self { module, _file: file, $($name,)+ })
                })();
                if loaded.is_err() { unsafe { FreeLibrary(module); } }
                loaded
            }
        }
        impl Drop for Api {
            fn drop(&mut self) { unsafe { FreeLibrary(self.module); } }
        }
    };
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: Ref, flags: u32) -> Ref;
    fn GetProcAddress(module: Ref, name: *const c_char) -> Ref;
    fn FreeLibrary(module: Ref) -> i32;
    fn GetModuleHandleExW(flags: u32, address: *const u16, module: *mut Ref) -> i32;
}

fn initialize(module: Ref) -> io::Result<()> {
    let init = unsafe { GetProcAddress(module, c"OPENSSL_init_crypto".as_ptr()) };
    let version = unsafe { GetProcAddress(module, c"OPENSSL_version_major".as_ptr()) };
    if init.is_null() || version.is_null() { return Err(failure()); }
    // SAFETY: exact stable OpenSSL 3 ABI; these loader-only functions are not stored in Api.
    let init = unsafe { std::mem::transmute::<Ref, unsafe extern "C" fn(u64, Ref) -> c_int>(init) };
    let version = unsafe { std::mem::transmute::<Ref, unsafe extern "C" fn() -> u32>(version) };
    // Pin for process lifetime: cleanup forbids reinitialization and exit callbacks need live code.
    // Windows cannot remove this qualified DLL until process exit; no private key object is pinned.
    let mut pinned = std::ptr::null_mut();
    if unsafe { GetModuleHandleExW(5, init as *const () as *const u16, &mut pinned) } == 0 { return Err(failure()); }
    // Must precede every cryptographic call: ignore environment-selected config/providers.
    if unsafe { init(0x80, std::ptr::null_mut()) } != 1 || unsafe { version() } != 3 { return Err(failure()); }
    Ok(())
}

#[allow(non_snake_case)]
mod declarations {
    use super::*;
    functions! {
        EVP_PKEY_CTX_new_from_name: unsafe extern "C" fn(Ref, *const c_char, *const c_char) -> Ref;
        EVP_PKEY_CTX_free: unsafe extern "C" fn(Ref);
        EVP_PKEY_keygen_init: unsafe extern "C" fn(Ref) -> c_int;
        EVP_PKEY_CTX_set_rsa_keygen_bits: unsafe extern "C" fn(Ref, c_int) -> c_int;
        EVP_PKEY_keygen: unsafe extern "C" fn(Ref, *mut Ref) -> c_int;
        EVP_PKEY_free: unsafe extern "C" fn(Ref);
        EVP_PKEY_get_bits: unsafe extern "C" fn(Ref) -> c_int;
        i2d_PublicKey: unsafe extern "C" fn(Ref, *mut *mut u8) -> c_int;
        EVP_MD_CTX_new: unsafe extern "C" fn() -> Ref;
        EVP_MD_CTX_free: unsafe extern "C" fn(Ref);
        EVP_sha256: unsafe extern "C" fn() -> Ref;
        EVP_sha1: unsafe extern "C" fn() -> Ref;
        X509_digest: unsafe extern "C" fn(Ref, Ref, *mut u8, *mut u32) -> c_int;
        EVP_DigestSignInit: unsafe extern "C" fn(Ref, *mut Ref, Ref, Ref, Ref) -> c_int;
        EVP_DigestSignUpdate: unsafe extern "C" fn(Ref, Ref, usize) -> c_int;
        EVP_DigestSignFinal: unsafe extern "C" fn(Ref, *mut u8, *mut usize) -> c_int;
        X509_new: unsafe extern "C" fn() -> Ref;
        X509_free: unsafe extern "C" fn(Ref);
        X509_set_version: unsafe extern "C" fn(Ref, c_long) -> c_int;
        X509_get_serialNumber: unsafe extern "C" fn(Ref) -> Ref;
        ASN1_INTEGER_set_uint64: unsafe extern "C" fn(Ref, u64) -> c_int;
        X509_getm_notBefore: unsafe extern "C" fn(Ref) -> Ref;
        X509_getm_notAfter: unsafe extern "C" fn(Ref) -> Ref;
        ASN1_TIME_set_string_X509: unsafe extern "C" fn(Ref, *const c_char) -> c_int;
        ASN1_TIME_to_tm: unsafe extern "C" fn(Ref, *mut Tm) -> c_int;
        X509_set_pubkey: unsafe extern "C" fn(Ref, Ref) -> c_int;
        X509_get_subject_name: unsafe extern "C" fn(Ref) -> Ref;
        X509_NAME_add_entry_by_txt: unsafe extern "C" fn(Ref, *const c_char, c_int, U8, c_int, c_int, c_int) -> c_int;
        X509_NAME_entry_count: unsafe extern "C" fn(Ref) -> c_int;
        X509_NAME_get_text_by_NID: unsafe extern "C" fn(Ref, c_int, *mut c_char, c_int) -> c_int;
        X509_set_issuer_name: unsafe extern "C" fn(Ref, Ref) -> c_int;
        X509V3_EXT_conf: unsafe extern "C" fn(Ref, Ref, *const c_char, *const c_char) -> Ref;
        X509_add_ext: unsafe extern "C" fn(Ref, Ref, c_int) -> c_int;
        X509_EXTENSION_free: unsafe extern "C" fn(Ref);
        X509_sign: unsafe extern "C" fn(Ref, Ref, Ref) -> c_int;
        X509_check_private_key: unsafe extern "C" fn(Ref, Ref) -> c_int;
        i2d_X509: unsafe extern "C" fn(Ref, *mut *mut u8) -> c_int;
        OBJ_txt2nid: unsafe extern "C" fn(*const c_char) -> c_int;
        PKCS12_create: unsafe extern "C" fn(*const c_char, *const c_char, Ref, Ref, Ref, c_int, c_int, c_int, c_int, c_int) -> Ref;
        PKCS12_set_mac: unsafe extern "C" fn(Ref, *const c_char, c_int, *mut u8, c_int, c_int, Ref) -> c_int;
        PKCS12_free: unsafe extern "C" fn(Ref);
        i2d_PKCS12: unsafe extern "C" fn(Ref, *mut *mut u8) -> c_int;
        d2i_PKCS12: unsafe extern "C" fn(*mut Ref, *mut U8, c_long) -> Ref;
        PKCS12_parse: unsafe extern "C" fn(Ref, *const c_char, *mut Ref, *mut Ref, *mut Ref) -> c_int;
    }
}
pub use declarations::Api;

#[repr(C)]
#[derive(Default)]
pub struct Tm {
    pub sec: c_int, pub min: c_int, pub hour: c_int, pub day: c_int,
    pub month: c_int, pub year: c_int, pub weekday: c_int, pub yearday: c_int, pub dst: c_int,
}

pub struct Owned<'a> {
    pub value: Ref,
    free: unsafe extern "C" fn(Ref),
    _api: std::marker::PhantomData<&'a Api>,
}
impl<'a> Owned<'a> {
    pub fn take(_api: &'a Api, value: Ref, free: unsafe extern "C" fn(Ref)) -> Self {
        Self { value, free, _api: std::marker::PhantomData }
    }
    pub fn new(api: &'a Api, value: Ref, free: unsafe extern "C" fn(Ref)) -> io::Result<Self> {
        let owned = Self::take(api, value, free);
        if value.is_null() { return Err(failure()); }
        Ok(owned)
    }
}
impl Drop for Owned<'_> {
    fn drop(&mut self) { if !self.value.is_null() { unsafe { (self.free)(self.value); } } }
}
