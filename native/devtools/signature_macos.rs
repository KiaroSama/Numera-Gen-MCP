use crate::{
    compat::hash::sha256,
    error::{NumeraError, Result},
};
use std::{ffi::c_void, os::unix::ffi::OsStrExt, path::Path, ptr::null};

type Ref = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(
        allocator: Ref,
        bytes: *const u8,
        length: isize,
        directory: u8,
    ) -> Ref;
    fn CFArrayGetCount(array: Ref) -> isize;
    fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
    fn CFDictionaryGetValue(dictionary: Ref, key: Ref) -> Ref;
    fn CFDataGetLength(data: Ref) -> isize;
    fn CFDataGetBytePtr(data: Ref) -> *const u8;
    fn CFRelease(value: Ref);
}
#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecStaticCodeCreateWithPath(path: Ref, flags: u32, code: *mut Ref) -> i32;
    fn SecStaticCodeCheckValidity(code: Ref, flags: u32, requirement: Ref) -> i32;
    fn SecCodeCopySigningInformation(code: Ref, flags: u32, information: *mut Ref) -> i32;
    fn SecCertificateCopyData(certificate: Ref) -> Ref;
    static kSecCodeInfoCertificates: Ref;
}
struct Owned(Ref);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this wrapper owns one create/copy reference and releases it exactly once.
            unsafe { CFRelease(self.0) };
        }
    }
}

pub(super) fn verify(path: &Path) -> Result<()> {
    let reject = || {
        NumeraError::new(
            "invalid_artifact",
            "Native executable signature verification failed; activation refused.",
        )
    };
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: path bytes remain live; create/copy results have scoped ownership and borrowed
    // certificate/array pointers remain anchored by the retained signing-information dictionary.
    unsafe {
        let url = Owned(CFURLCreateFromFileSystemRepresentation(
            null(),
            bytes.as_ptr(),
            bytes.len() as isize,
            0,
        ));
        if url.0.is_null() {
            return Err(reject());
        }
        let mut code = null();
        let status = SecStaticCodeCreateWithPath(url.0, 0, &mut code);
        let code = Owned(code);
        if status != 0 || code.0.is_null() {
            return Err(reject());
        }
        // Check all architectures and strict structure without public trust/notarization policy.
        // Network access is not enabled; the project pin, not a public root, authenticates identity.
        if SecStaticCodeCheckValidity(code.0, (1 << 0) | (1 << 4), null()) != 0 {
            return Err(reject());
        }
        let mut info = null();
        let status = SecCodeCopySigningInformation(code.0, 1 << 1, &mut info);
        let info = Owned(info);
        if status != 0 || info.0.is_null() {
            return Err(reject());
        }
        let certificates = CFDictionaryGetValue(info.0, kSecCodeInfoCertificates);
        if certificates.is_null() || CFArrayGetCount(certificates) != 1 {
            return Err(reject());
        }
        let certificate = CFArrayGetValueAtIndex(certificates, 0);
        if certificate.is_null() {
            return Err(reject());
        }
        let der = Owned(SecCertificateCopyData(certificate));
        if der.0.is_null() {
            return Err(reject());
        }
        let length = CFDataGetLength(der.0);
        let data = CFDataGetBytePtr(der.0);
        if !(1..=8192).contains(&length) || data.is_null() {
            return Err(reject());
        }
        if sha256(std::slice::from_raw_parts(data, length as usize))
            != "657dd7869e90e2bf9a49942d5098e1e3a82e22cf84add51aa958e3dfd0af937f"
        {
            return Err(reject());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn unsigned_test_executable_is_not_accepted_as_native_publisher_code() {
        let path = std::env::current_exe().unwrap();
        assert!(super::verify(&path).is_err());
    }
}
