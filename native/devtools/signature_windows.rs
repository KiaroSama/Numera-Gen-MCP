use crate::{
    compat::hash::sha256,
    error::{NumeraError, Result},
};
use std::{ffi::c_void, mem::size_of, os::windows::ffi::OsStrExt, path::Path, ptr::null_mut};

#[repr(C)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}
#[repr(C)]
struct FileInfo {
    size: u32,
    path: *const u16,
    file: *mut c_void,
    subject: *mut Guid,
}
#[repr(C)]
struct TrustData {
    size: u32,
    policy: *mut c_void,
    sip: *mut c_void,
    ui: u32,
    revocation: u32,
    choice: u32,
    file: *mut FileInfo,
    action: u32,
    state: *mut c_void,
    url: *mut u16,
    flags: u32,
    context: u32,
    signatures: *mut c_void,
}
#[repr(C)]
struct Usage {
    count: u32,
    identifiers: *mut *mut u8,
}
#[repr(C)]
struct UsageMatch {
    kind: u32,
    usage: Usage,
}
#[repr(C)]
struct ChainPara {
    size: u32,
    usage: UsageMatch,
}
#[repr(C)]
struct CreateInfo {
    size: u32,
    engine: *mut c_void,
    para: *mut ChainPara,
    flags: u32,
    reserved: *mut c_void,
}
#[repr(C)]
struct PolicyData {
    size: u32,
    signer: *mut CreateInfo,
    counter: *mut CreateInfo,
    callback: PolicyCallback,
    argument: *mut c_void,
}
type PolicyCallback = unsafe extern "system" fn(
    *const c_void,
    u32,
    u32,
    u32,
    *const *const SignerInfo,
    *mut c_void,
) -> i32;
#[repr(C)]
struct SignerInfo {
    size: u32,
    chain: *const Chain,
    kind: u32,
    message: *const c_void,
    error: u32,
    counters: u32,
    counter: *const c_void,
}
#[repr(C)]
struct Status {
    error: u32,
    info: u32,
}
#[repr(C)]
struct Chain {
    size: u32,
    status: Status,
    count: u32,
    chains: *const *const SimpleChain,
}
#[repr(C)]
struct SimpleChain {
    size: u32,
    status: Status,
    count: u32,
    elements: *const *const Element,
}
#[repr(C)]
struct Element {
    size: u32,
    certificate: *const Certificate,
}
#[repr(C)]
struct Certificate {
    encoding: u32,
    bytes: *const u8,
    length: u32,
    info: *const c_void,
    store: *mut c_void,
}
#[repr(C)]
struct PolicyPara {
    size: u32,
    flags: u32,
    extra: *mut c_void,
}
#[repr(C)]
struct PolicyStatus {
    size: u32,
    error: u32,
    chain: i32,
    element: i32,
    extra: *mut c_void,
}
#[link(name = "wintrust")]
unsafe extern "system" {
    fn WinVerifyTrust(window: *mut c_void, action: *mut Guid, data: *mut TrustData) -> i32;
}
#[link(name = "crypt32")]
unsafe extern "system" {
    fn CertVerifyCertificateChainPolicy(
        oid: *const u8,
        chain: *const Chain,
        para: *const PolicyPara,
        status: *mut PolicyStatus,
    ) -> i32;
}

unsafe extern "system" fn policy(
    _: *const c_void,
    _: u32,
    _: u32,
    count: u32,
    signers: *const *const SignerInfo,
    argument: *mut c_void,
) -> i32 {
    const REJECT: i32 = 0x800b0004u32 as i32;
    if count != 1 || signers.is_null() || argument.is_null() {
        return REJECT;
    }
    // SAFETY: WinVerifyTrust supplies callback-scoped structures and our live bool context.
    unsafe {
        let signer = *signers;
        if signer.is_null()
            || (*signer).chain.is_null()
            || (*signer).error != 0
            || (*signer).kind != 0
            || (*signer).counters != 0
        {
            return REJECT;
        }
        let chain = &*(*signer).chain;
        // Only the pinned self-signed root is exempt from public-root trust. Other errors fail.
        if chain.count != 1 || chain.chains.is_null() || chain.status.error & !0x20 != 0 {
            return REJECT;
        }
        let simple = *chain.chains;
        if simple.is_null() || (*simple).count != 1 || (*simple).elements.is_null() {
            return REJECT;
        }
        let element = *(*simple).elements;
        if element.is_null() || (*element).certificate.is_null() {
            return REJECT;
        }
        let cert = &*(*element).certificate;
        if cert.bytes.is_null() || cert.length == 0 || cert.length > 8192 {
            return REJECT;
        }
        let expected = include_bytes!("../../docs/signing/publisher-cert.der");
        if std::slice::from_raw_parts(cert.bytes, cert.length as usize) != expected {
            return REJECT;
        }
        let para = PolicyPara {
            size: size_of::<PolicyPara>() as u32,
            flags: 0x10,
            extra: null_mut(),
        };
        let mut status = PolicyStatus {
            size: size_of::<PolicyStatus>() as u32,
            error: 1,
            chain: -1,
            element: -1,
            extra: null_mut(),
        };
        // Authenticode still enforces usage/time/signature; only the pinned unknown CA is allowed.
        if CertVerifyCertificateChainPolicy(
            2usize as *const u8,
            (*signer).chain,
            &para,
            &mut status,
        ) == 0
            || status.error != 0
        {
            return REJECT;
        }
        *(argument as *mut bool) = true;
    }
    0
}

pub(super) fn verify(path: &Path) -> Result<()> {
    let reject = || {
        NumeraError::new(
            "invalid_artifact",
            "Native executable signature verification failed; activation refused.",
        )
    };
    if sha256(include_bytes!("../../docs/signing/publisher-cert.der"))
        != "657dd7869e90e2bf9a49942d5098e1e3a82e22cf84add51aa958e3dfd0af937f"
    {
        return Err(reject());
    }
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    let mut file = FileInfo {
        size: size_of::<FileInfo>() as u32,
        path: wide.as_ptr(),
        file: null_mut(),
        subject: null_mut(),
    };
    let mut oid = c"1.3.6.1.5.5.7.3.3".as_ptr() as *mut u8;
    let mut para = ChainPara {
        size: size_of::<ChainPara>() as u32,
        usage: UsageMatch {
            kind: 0,
            usage: Usage {
                count: 1,
                identifiers: &mut oid,
            },
        },
    };
    let mut create = CreateInfo {
        size: size_of::<CreateInfo>() as u32,
        engine: null_mut(),
        para: &mut para,
        flags: 4,
        reserved: null_mut(),
    };
    let mut accepted = false;
    let mut policy_data = PolicyData {
        size: size_of::<PolicyData>() as u32,
        signer: &mut create,
        counter: null_mut(),
        callback: policy,
        argument: &mut accepted as *mut bool as *mut c_void,
    };
    let mut data = TrustData {
        size: size_of::<TrustData>() as u32,
        policy: &mut policy_data as *mut PolicyData as *mut c_void,
        sip: null_mut(),
        ui: 2,
        revocation: 0,
        choice: 1,
        file: &mut file,
        action: 1,
        state: null_mut(),
        url: null_mut(),
        flags: 0x1010,
        context: 0,
        signatures: null_mut(),
    };
    let mut action = Guid {
        a: 0xfc451c16,
        b: 0xac75,
        c: 0x11d1,
        d: [0xb4, 0xb8, 0, 0xc0, 0x4f, 0xb6, 0x6e, 0xa0],
    };
    // SAFETY: all ABI structures/pointers remain live through verification and state close.
    let status = unsafe { WinVerifyTrust(-1isize as *mut c_void, &mut action, &mut data) };
    data.action = 2;
    // SAFETY: close uses the same live action/data and releases only this operation's state.
    let closed = unsafe { WinVerifyTrust(-1isize as *mut c_void, &mut action, &mut data) };
    if status != 0 || closed != 0 || !accepted {
        return Err(reject());
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
