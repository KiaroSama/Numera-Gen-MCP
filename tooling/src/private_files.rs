use crate::context::fail;
use std::{fs,io::{self,Write},path::{Path,PathBuf}};

pub struct Secret(pub Vec<u8>);
impl Drop for Secret {fn drop(&mut self){for byte in &mut self.0 {
    // SAFETY: each byte is exclusively borrowed from a live owned allocation.
    unsafe {std::ptr::write_volatile(byte,0)};
}}}

pub struct PrivateInputs {pub directory:PathBuf,pub pfx:PathBuf,pub password:PathBuf}
impl PrivateInputs {
    pub fn create(root:&Path,pfx:&[u8],password:&[u8])->io::Result<Self>{
        if pfx.is_empty()||pfx.len()>32768||password.is_empty()||password.len()>4095
            || !password.iter().all(|b|b.is_ascii_alphanumeric()||b"+/=".contains(b)){
            return Err(fail("Selected signing inputs are missing or malformed."));
        }
        let parent=root.join(".ci-work");fs::create_dir_all(&parent)?;
        if !parent.canonicalize()?.starts_with(root.canonicalize()?){return Err(fail("Private input path escapes workspace."));}
        let directory=parent.join("signing");
        create_directory(&directory)?;
        let inputs=Self{pfx:directory.join("publisher.pfx"),password:directory.join("password.txt"),directory};
        write(&inputs.pfx,pfx)?;write(&inputs.password,password)?;Ok(inputs)
    }
    pub fn cleanup(self)->io::Result<()>{
        for path in [&self.pfx,&self.password]{fs::remove_file(path)?;}
        fs::remove_dir(&self.directory)?;
        if self.directory.try_exists()?{return Err(fail("Private input cleanup failed."));}Ok(())
    }
}
impl Drop for PrivateInputs{fn drop(&mut self){
    for path in [&self.pfx,&self.password]{if path.exists()&&fs::remove_file(path).is_err(){eprintln!("ERROR: Private signing input cleanup remains unverified.");}}
    if self.directory.exists()&&fs::remove_dir(&self.directory).is_err(){eprintln!("ERROR: Private signing directory cleanup remains unverified.");}
}}
fn write(path:&Path,bytes:&[u8])->io::Result<()> {
    let mut options=fs::OpenOptions::new();options.create_new(true).write(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    #[cfg(windows)] {use std::os::windows::fs::OpenOptionsExt;options.share_mode(0);}
    let mut file=options.open(path)?;file.write_all(bytes)?;file.sync_all()
}
#[cfg(unix)]
pub(crate) fn create_directory(path:&Path)->io::Result<()>{
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(path)
}
#[cfg(windows)]
pub(crate) fn create_directory(path:&Path)->io::Result<()>{
    use std::{ffi::c_void,os::windows::ffi::OsStrExt};
    #[repr(C)]struct Attributes{length:u32,descriptor:*mut c_void,inherit:i32}
    #[link(name="advapi32")]unsafe extern "system"{
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(text:*const u16,revision:u32,descriptor:*mut *mut c_void,size:*mut u32)->i32;
    }
    #[link(name="kernel32")]unsafe extern "system"{
        fn CreateDirectoryW(path:*const u16,attributes:*const Attributes)->i32;
        fn LocalFree(memory:*mut c_void)->*mut c_void;
    }
    let sddl="D:P(A;OICI;FA;;;OW)".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let path=path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    let mut descriptor=std::ptr::null_mut();
    // SAFETY: initialized output pointer; scoped LocalAlloc-compatible descriptor and live UTF16 strings.
    unsafe{
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(),1,&mut descriptor,std::ptr::null_mut())==0{return Err(fail("Private signing ACL capability unavailable."));}
        let attributes=Attributes{length:std::mem::size_of::<Attributes>() as u32,descriptor,inherit:0};
        let result=CreateDirectoryW(path.as_ptr(),&attributes);let error=io::Error::last_os_error();LocalFree(descriptor);
        if result==0{return Err(error);}
    }Ok(())
}
