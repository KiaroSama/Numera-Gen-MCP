//! Exact Apple ABI table and memory-only signing choreography. The Darwin adapter supplies
//! the resolver; deterministic tests supply the same function signatures without native execution.
use crate::context::{fail, sha};
use std::{ffi::c_void, io, path::Path, ptr::null};

pub(crate) type Ref = *const c_void;
type DataCreate = unsafe extern "C" fn(Ref, *const u8, isize) -> Ref;
type StringCreate = unsafe extern "C" fn(Ref, *const u8, isize, u32, u8) -> Ref;
type DictionaryCreate = unsafe extern "C" fn(Ref, *const Ref, *const Ref, isize, Ref, Ref) -> Ref;
type ArrayCount = unsafe extern "C" fn(Ref) -> isize;
type ArrayItem = unsafe extern "C" fn(Ref, isize) -> Ref;
type DictionaryValue = unsafe extern "C" fn(Ref, Ref) -> Ref;
type DataLength = unsafe extern "C" fn(Ref) -> isize;
type DataPointer = unsafe extern "C" fn(Ref) -> *const u8;
type UrlCreate = unsafe extern "C" fn(Ref, *const u8, isize, u8) -> Ref;
type Release = unsafe extern "C" fn(Ref);
type Import = unsafe extern "C" fn(Ref, Ref, *mut Ref) -> i32;
type CopyCertificate = unsafe extern "C" fn(Ref, *mut Ref) -> i32;
type CertificateData = unsafe extern "C" fn(Ref) -> Ref;
type StaticCode = unsafe extern "C" fn(Ref, u32, *mut Ref) -> i32;
type CreateSigner = unsafe extern "C" fn(Ref, u32, *mut Ref) -> i32;
type AddSignature = unsafe extern "C" fn(Ref, Ref, u32, *mut Ref) -> i32;

pub(crate) struct Api {
    data_create: DataCreate,
    string_create: StringCreate,
    dictionary_create: DictionaryCreate,
    array_count: ArrayCount,
    array_item: ArrayItem,
    dictionary_value: DictionaryValue,
    data_length: DataLength,
    data_pointer: DataPointer,
    url_create: UrlCreate,
    release: Release,
    import: Import,
    copy_certificate: CopyCertificate,
    certificate_data: CertificateData,
    static_code: StaticCode,
    create_signer: CreateSigner,
    add_signature: AddSignature,
    memory: Ref,
    passphrase: Ref,
    item_identity: Ref,
    signer_identity: Ref,
    timestamp: Ref,
    yes: Ref,
    no: Ref,
}

fn available(pointer: *mut c_void) -> io::Result<*mut c_void> {
    if pointer.is_null() { Err(fail("macos_signing_capability_unavailable")) } else { Ok(pointer) }
}
impl Api {
    /// # Safety
    /// The resolver must return live addresses with the exact declared Apple function ABIs,
    /// or addresses of live CF reference variables for constants. Their code/data stays loaded
    /// through this table's last call and every associated object's release.
    pub(crate) unsafe fn resolve(mut address: impl FnMut(&'static [u8]) -> io::Result<*mut c_void>) -> io::Result<Self> {
        macro_rules! function {
            ($name:literal, $kind:ty) => {{
                let pointer = available(address(concat!($name, "\0").as_bytes())?)?;
                // SAFETY: the resolver contract requires this exact named ABI, checked non-null.
                unsafe { std::mem::transmute::<*mut c_void, $kind>(pointer) }
            }};
        }
        macro_rules! constant {
            ($name:literal) => {{
                let pointer = available(address(concat!($name, "\0").as_bytes())?)?;
                // SAFETY: exported CF constants are variables containing a reference, not the
                // object itself. Read exactly one pointer indirection; ownership is not transferred.
                let reference = unsafe { *pointer.cast::<Ref>() };
                if reference.is_null() { return Err(fail("macos_signing_capability_unavailable")); }
                reference
            }};
        }
        // All capabilities resolve before private bytes enter any CF allocation or import.
        Ok(Self {
            memory: constant!("kSecImportToMemoryOnly"),
            passphrase: constant!("kSecImportExportPassphrase"),
            item_identity: constant!("kSecImportItemIdentity"),
            signer_identity: constant!("kSecCodeSignerIdentity"),
            timestamp: constant!("kSecCodeSignerRequireTimestamp"),
            yes: constant!("kCFBooleanTrue"), no: constant!("kCFBooleanFalse"),
            data_create: function!("CFDataCreate", DataCreate),
            string_create: function!("CFStringCreateWithBytes", StringCreate),
            dictionary_create: function!("CFDictionaryCreate", DictionaryCreate),
            array_count: function!("CFArrayGetCount", ArrayCount),
            array_item: function!("CFArrayGetValueAtIndex", ArrayItem),
            dictionary_value: function!("CFDictionaryGetValue", DictionaryValue),
            data_length: function!("CFDataGetLength", DataLength),
            data_pointer: function!("CFDataGetBytePtr", DataPointer),
            url_create: function!("CFURLCreateFromFileSystemRepresentation", UrlCreate),
            release: function!("CFRelease", Release),
            import: function!("SecPKCS12Import", Import),
            copy_certificate: function!("SecIdentityCopyCertificate", CopyCertificate),
            certificate_data: function!("SecCertificateCopyData", CertificateData),
            static_code: function!("SecStaticCodeCreateWithPath", StaticCode),
            create_signer: function!("SecCodeSignerCreate", CreateSigner),
            add_signature: function!("SecCodeSignerAddSignatureWithErrors", AddSignature),
        })
    }
    fn own(&self, value: Ref) -> io::Result<Owned<'_>> {
        if value.is_null() { Err(fail("macos_signing_allocation_failed")) } else { Ok(Owned { value, api: self }) }
    }
    fn output(&self, status: i32, value: Ref) -> io::Result<Owned<'_>> {
        // Own a partial out-reference before checking its status: Apple can return an object
        // alongside failure. It must be released even when the caller never receives a result.
        let owned = Owned { value, api: self };
        if status != 0 || value.is_null() { Err(fail("macos_signing_native_operation_failed")) } else { Ok(owned) }
    }
    fn dictionary(&self, pairs: &[(Ref, Ref)]) -> io::Result<Owned<'_>> {
        let keys = pairs.iter().map(|pair| pair.0).collect::<Vec<_>>();
        let values = pairs.iter().map(|pair| pair.1).collect::<Vec<_>>();
        // SAFETY: bounded equal-length live slices; null callbacks deliberately retain nothing.
        // Every referenced input remains owned on the enclosing stack through dictionary release.
        self.own(unsafe { (self.dictionary_create)(null(), keys.as_ptr(), values.as_ptr(), pairs.len() as isize, null(), null()) })
    }
    fn sign(&self, pfx: &[u8], password: &[u8], pin: &str, files: &[&Path]) -> io::Result<()> {
        // SAFETY: validated bounded slices and platform-native path bytes remain live; exact ABI
        // function pointers were qualified before this entry. Create/copy/out results acquire one
        // owner immediately; borrowed identities remain anchored by the imported-items owner.
        unsafe {
            let data = self.own((self.data_create)(null(), pfx.as_ptr(), pfx.len() as isize))?;
            let secret = self.own((self.string_create)(null(), password.as_ptr(), password.len() as isize, 0x08000100, 0))?;
            let options = self.dictionary(&[(self.memory, self.yes), (self.passphrase, secret.value)])?;
            let mut items = null();
            let status = (self.import)(data.value, options.value, &mut items);
            let items = self.output(status, items)?;
            if (self.array_count)(items.value) != 1 { return Err(fail("macos_signing_single_identity_required")); }
            let item = (self.array_item)(items.value, 0);
            let identity = if item.is_null() { null() } else { (self.dictionary_value)(item, self.item_identity) };
            if identity.is_null() { return Err(fail("macos_signing_identity_not_found")); }
            let mut certificate = null();
            let status = (self.copy_certificate)(identity, &mut certificate);
            let certificate = self.output(status, certificate)?;
            let der = self.own((self.certificate_data)(certificate.value))?;
            let length = (self.data_length)(der.value);
            let bytes = (self.data_pointer)(der.value);
            if !(1..=16384).contains(&length) { return Err(fail("macos_signing_certificate_bound")); }
            if bytes.is_null() || sha(std::slice::from_raw_parts(bytes, length as usize)) != pin { return Err(fail("macos_signing_certificate_mismatch")); }
            let parameters = self.dictionary(&[(self.signer_identity, identity), (self.timestamp, self.no)])?;
            let mut signer = null();
            let status = (self.create_signer)(parameters.value, 0, &mut signer);
            let signer = self.output(status, signer)?;
            for file in files {
                let path = file.as_os_str().as_encoded_bytes();
                let url = self.own((self.url_create)(null(), path.as_ptr(), path.len() as isize, 0))?;
                let mut code = null();
                let status = (self.static_code)(url.value, 0, &mut code);
                let code = self.output(status, code)?;
                let mut error = null();
                let status = (self.add_signature)(signer.value, code.value, 0, &mut error);
                let _error = Owned { value: error, api: self };
                if status != 0 { return Err(fail("macos_signing_native_signature_failed")); }
            }
        }
        Ok(())
    }
}

struct Owned<'a> { value: Ref, api: &'a Api }
impl Drop for Owned<'_> {
    fn drop(&mut self) {
        if !self.value.is_null() {
            // SAFETY: exactly one live create/copy/out reference belongs to this scoped owner.
            unsafe { (self.api.release)(self.value) };
        }
    }
}

pub(crate) fn validate(pfx: &[u8], password: &[u8], pin: &str, files: &[&Path]) -> io::Result<()> {
    if pfx.is_empty() || pfx.len() > 1024 * 1024 || password.is_empty() || password.len() > 4095
        || !password.iter().all(|byte| byte.is_ascii_alphanumeric() || b"+/=_-".contains(byte))
        || pin.len() != 64 || !pin.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || !(1..=2).contains(&files.len()) {
        Err(fail("invalid_memory_signing_input"))
    } else { Ok(()) }
}

/// # Safety
/// The resolver must meet `Api::resolve`'s exact ABI/lifetime contract.
pub(crate) unsafe fn sign_with(
    resolve: impl FnMut(&'static [u8]) -> io::Result<*mut c_void>,
    pfx: &[u8], password: &[u8], pin: &str, files: &[&Path],
) -> io::Result<()> {
    validate(pfx, password, pin, files)?;
    // SAFETY: the caller supplies a resolver satisfying the exact named Apple ABI contract.
    unsafe { Api::resolve(resolve) }?.sign(pfx, password, pin, files)
}
