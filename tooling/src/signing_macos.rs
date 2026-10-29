use crate::context::fail;
use std::{ffi::{c_char,c_void},io,path::Path,ptr::NonNull};

#[path = "apple_signer.rs"]
mod apple_signer;

unsafe extern "C" {
    fn dlopen(path:*const c_char,flags:i32)->*mut c_void;
    fn dlsym(handle:*mut c_void,name:*const c_char)->*mut c_void;
    fn dlclose(handle:*mut c_void)->i32;
}

struct Framework(NonNull<c_void>);
impl Framework {
    fn open(path:&'static [u8])->io::Result<Self>{
        // SAFETY: fixed Apple system framework path, NUL-terminated; RTLD_NOW resolves the
        // framework before any private bytes are imported. No user-selected library is loaded.
        NonNull::new(unsafe{dlopen(path.as_ptr().cast(),2)}).map(Self)
            .ok_or_else(||fail("macos_signing_capability_unavailable"))
    }
    fn address(&self,name:&'static [u8])->io::Result<*mut c_void>{
        // SAFETY: retained dlopen handle and static NUL-terminated ABI symbol. No object ownership
        // transfers; the framework remains loaded until every associated CF owner is released.
        let value=unsafe{dlsym(self.0.as_ptr(),name.as_ptr().cast())};
        if value.is_null(){Err(fail("macos_signing_capability_unavailable"))}else{Ok(value)}
    }
}
impl Drop for Framework {
    fn drop(&mut self){
        // SAFETY: exactly one successful dlopen belongs to this owner; all API objects have
        // already been released before the enclosing resolver/framework scope can be dropped.
        if unsafe{dlclose(self.0.as_ptr())}!=0{eprintln!("ERROR: Apple signing framework handle cleanup failed.");}
    }
}
struct Frameworks{core:Framework,security:Framework}
impl Frameworks {
    fn open()->io::Result<Self>{
        Ok(Self{
            core:Framework::open(b"/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation\0")?,
            security:Framework::open(b"/System/Library/Frameworks/Security.framework/Security\0")?,
        })
    }
    fn address(&self,name:&'static [u8])->io::Result<*mut c_void>{
        if name.starts_with(b"CF")||name.starts_with(b"kCF"){self.core.address(name)}else{self.security.address(name)}
    }
}

pub fn qualify()->io::Result<()>{
    let frameworks=Frameworks::open()?;
    // SAFETY: the explicit system handles return addresses for the exact Apple function/constant
    // ABIs declared by the table. Resolving this table allocates/imports no private material.
    let _=unsafe{apple_signer::Api::resolve(|name|frameworks.address(name))}?;
    Ok(())
}

pub fn sign(pfx:&[u8],password:&[u8],pin:&str,files:&[&Path])->io::Result<()> {
    apple_signer::validate(pfx,password,pin,files)?;
    let frameworks=Frameworks::open()?;
    // SAFETY: only the explicit Apple system frameworks satisfy the production resolver; their
    // handles outlive the API table and every create/copy/error reference. No keychain fallback.
    unsafe{apple_signer::sign_with(|name|frameworks.address(name),pfx,password,pin,files)}
}
