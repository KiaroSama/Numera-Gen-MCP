use super::verify_ancestors;
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    io,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr,
};
type Handle = *mut c_void;
#[repr(C)]
struct Attributes {
    length: u32,
    descriptor: *mut c_void,
    inherit: i32,
}
#[repr(C)]
struct FileInformation {
    attributes: u32,
    created: [u32; 2],
    accessed: [u32; 2],
    written: [u32; 2],
    volume: u32,
    size: [u32; 2],
    links: u32,
    index: [u32; 2],
}
#[repr(C)]
struct AclSize {
    count: u32,
    used: u32,
    free: u32,
}
#[repr(C)]
struct AceHeader {
    kind: u8,
    flags: u8,
    size: u16,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LocalFree(memory: Handle) -> Handle;
    fn GetCurrentProcess() -> Handle;
    fn CreateDirectoryW(path: *const u16, attributes: *const Attributes) -> i32;
    fn GetFileInformationByHandle(file: Handle, info: *mut FileInformation) -> i32;
    fn CreateFileW(
        path: *const u16,
        access: u32,
        share: u32,
        attributes: *const Attributes,
        disposition: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: i32,
        data: Handle,
        length: u32,
        needed: *mut u32,
    ) -> i32;
    fn EqualSid(first: Handle, second: Handle) -> i32;
    fn ConvertSidToStringSidW(sid: Handle, text: *mut *mut u16) -> i32;
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        text: *const u16,
        revision: u32,
        descriptor: *mut Handle,
        size: *mut u32,
    ) -> i32;
    fn ConvertSecurityDescriptorToStringSecurityDescriptorW(
        descriptor: Handle,
        revision: u32,
        information: u32,
        text: *mut *mut u16,
        size: *mut u32,
    ) -> i32;
    fn GetSecurityInfo(
        file: Handle,
        kind: i32,
        information: u32,
        owner: *mut Handle,
        group: *mut Handle,
        dacl: *mut Handle,
        sacl: *mut Handle,
        descriptor: *mut Handle,
    ) -> u32;
    fn GetSecurityDescriptorControl(
        descriptor: Handle,
        control: *mut u16,
        revision: *mut u32,
    ) -> i32;
    fn GetAclInformation(acl: Handle, data: Handle, length: u32, class: i32) -> i32;
    fn GetAce(acl: Handle, index: u32, ace: *mut Handle) -> i32;
    fn GetSecurityDescriptorDacl(
        descriptor: Handle,
        present: *mut i32,
        acl: *mut Handle,
        defaulted: *mut i32,
    ) -> i32;
    fn SetSecurityInfo(
        file: Handle,
        kind: i32,
        information: u32,
        owner: Handle,
        group: Handle,
        dacl: Handle,
        sacl: Handle,
    ) -> u32;
}
struct Local(Handle);
impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let mut value = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if value.contains(&0) {
        return Err(io::Error::other("Private path contains NUL."));
    }
    value.push(0);
    Ok(value)
}
fn local_text(pointer: *mut u16) -> io::Result<String> {
    let _memory = Local(pointer.cast());
    if pointer.is_null() {
        return Err(io::Error::other("Private security text missing."));
    }
    let mut length = 0;
    while length < 65536 && unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    if length == 65536 {
        return Err(io::Error::other("Private security text exceeds bound."));
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, length) })
        .map_err(|_| io::Error::other("Invalid private security text."))
}
struct CurrentUser {
    _token: OwnedHandle,
    data: Vec<usize>,
}
impl CurrentUser {
    fn new() -> io::Result<Self> {
        let mut token = ptr::null_mut();
        // SAFETY: borrowed process pseudo handle and one initialized owned token output.
        if unsafe { OpenProcessToken(GetCurrentProcess(), 8, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = unsafe { OwnedHandle::from_raw_handle(token) };
        let mut needed = 0;
        unsafe {
            GetTokenInformation(token.as_raw_handle(), 1, ptr::null_mut(), 0, &mut needed);
        }
        if !(std::mem::size_of::<usize>() as u32..=65536).contains(&needed) {
            return Err(io::Error::other("Current user token unavailable."));
        }
        let mut data = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                1,
                data.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            _token: token,
            data,
        })
    }
    fn sid(&self) -> Handle {
        self.data[0] as Handle
    }
    fn descriptor(&self, directory: bool) -> io::Result<Local> {
        let mut text = ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(self.sid(), &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let sid = local_text(text)?;
        let flags = if directory { "OICI" } else { "" };
        let sddl = format!("O:{sid}D:P(A;{flags};FA;;;{sid})")
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
        let mut descriptor = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Local(descriptor))
    }
}

pub struct PrivateSecurity {
    pub owner_current_user: bool,
    pub owner_only: bool,
    pub protected: bool,
}
pub struct RegistryAccess {
    pub file: File,
    pub owner_current_user: bool,
    pub owner_only: bool,
    pub protected: bool,
    pub original_sddl: String,
}
struct Security {
    descriptor: Local,
    owner: Handle,
    dacl: Handle,
    protected: bool,
}
impl Security {
    fn new(file: &File) -> io::Result<Self> {
        let mut descriptor = ptr::null_mut();
        let mut owner = ptr::null_mut();
        let mut dacl = ptr::null_mut();
        let result = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                1,
                5,
                &mut owner,
                ptr::null_mut(),
                &mut dacl,
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        let descriptor = Local(descriptor);
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        let mut control = 0;
        let mut revision = 0;
        if unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            descriptor,
            owner,
            dacl,
            protected: control & 0x1000 != 0,
        })
    }
    fn inspect(&self, user: &CurrentUser) -> io::Result<PrivateSecurity> {
        let owner_current_user =
            !self.owner.is_null() && unsafe { EqualSid(self.owner, user.sid()) } != 0;
        let mut owner_only = !self.dacl.is_null();
        let mut allow = false;
        if !self.dacl.is_null() {
            let mut info = AclSize {
                count: 0,
                used: 0,
                free: 0,
            };
            if unsafe {
                GetAclInformation(
                    self.dacl,
                    (&mut info as *mut AclSize).cast(),
                    std::mem::size_of::<AclSize>() as u32,
                    2,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            if info.count > 4096 {
                return Err(io::Error::other("Private ACL exceeds bound."));
            }
            for index in 0..info.count {
                let mut ace = ptr::null_mut();
                if unsafe { GetAce(self.dacl, index, &mut ace) } == 0 || ace.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let header = unsafe { &*ace.cast::<AceHeader>() };
                if header.kind == 0 && header.size >= 16 {
                    let sid = unsafe { ace.cast::<u8>().add(8).cast() };
                    allow = true;
                    if unsafe { EqualSid(sid, user.sid()) } == 0 {
                        owner_only = false;
                    }
                } else if header.kind != 1 {
                    owner_only = false;
                }
            }
        }
        Ok(PrivateSecurity {
            owner_current_user,
            owner_only: owner_only && allow,
            protected: self.protected,
        })
    }
    fn sddl(&self) -> io::Result<String> {
        let mut text = ptr::null_mut();
        if unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                self.descriptor.0,
                1,
                5,
                &mut text,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        local_text(text)
    }
}
fn regular(file: &File, directory: bool) -> io::Result<()> {
    let metadata = file.metadata()?;
    if metadata.file_attributes() & 0x400 != 0
        || metadata.is_dir() != directory
        || (!directory && !metadata.is_file())
    {
        return Err(io::Error::other("Private regular object required."));
    }
    let mut info: FileInformation = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if !directory && info.links != 1 {
        return Err(io::Error::other("Hard-linked private file rejected."));
    }
    Ok(())
}
fn metadata_file(path: &Path, directory: bool, write: bool) -> io::Result<File> {
    verify_ancestors(path)?;
    let _parents = lock_parents(path)?;
    let mut options = OpenOptions::new();
    options
        .access_mode(if write { 0xC0060000 } else { 0x00020000 })
        .share_mode(if write { 0 } else { 7 })
        .custom_flags(0x00200000 | if directory { 0x02000000 } else { 0 });
    let file = options.open(path)?;
    regular(&file, directory)?;
    Ok(file)
}
pub fn lock_parents(path: &Path) -> io::Result<Vec<File>> {
    let mut handles = Vec::new();
    let mut parent = path.parent();
    while let Some(directory) = parent {
        if !directory.as_os_str().is_empty() {
            let file = OpenOptions::new()
                .access_mode(0x80)
                .share_mode(3)
                .custom_flags(0x00200000 | 0x02000000)
                .open(directory)?;
            regular(&file, true)?;
            handles.push(file);
        }
        parent = directory.parent();
    }
    Ok(handles)
}

pub fn inspect_private(path: &Path, directory: bool) -> io::Result<PrivateSecurity> {
    let file = metadata_file(path, directory, false)?;
    Security::new(&file)?.inspect(&CurrentUser::new()?)
}
pub fn verify_file(file: &File) -> io::Result<()> {
    regular(file, false)?;
    let security = Security::new(file)?.inspect(&CurrentUser::new()?)?;
    if !security.owner_current_user || !security.owner_only || !security.protected {
        return Err(io::Error::other(
            "Private owner-only protected ACL required.",
        ));
    }
    Ok(())
}
pub fn open_file(path: &Path, write: bool) -> io::Result<File> {
    verify_ancestors(path)?;
    let _parents = lock_parents(path)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(write)
        .share_mode(0)
        .custom_flags(0x00200000);
    let file = options.open(path)?;
    regular(&file, false)?;
    Ok(file)
}
pub fn open_registry(path: &Path, max_bytes: u64) -> io::Result<RegistryAccess> {
    let file = metadata_file(path, false, true)?;
    if file.metadata()?.len() > max_bytes {
        return Err(io::Error::other("Private registry exceeds bound."));
    }
    let security = Security::new(&file)?;
    let status = security.inspect(&CurrentUser::new()?)?;
    if !status.owner_current_user {
        return Err(io::Error::other(
            "Private registry must belong to the current user.",
        ));
    }
    Ok(RegistryAccess {
        file,
        owner_current_user: status.owner_current_user,
        owner_only: status.owner_only,
        protected: status.protected,
        original_sddl: security.sddl()?,
    })
}
pub fn restrict_registry(access: &RegistryAccess) -> io::Result<()> {
    let user = CurrentUser::new()?;
    if !Security::new(&access.file)?
        .inspect(&user)?
        .owner_current_user
    {
        return Err(io::Error::other(
            "Foreign registry ownership cannot be changed.",
        ));
    }
    let descriptor = user.descriptor(false)?;
    let mut present = 0;
    let mut dacl = ptr::null_mut();
    let mut defaulted = 0;
    if unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted) }
        == 0
        || present == 0
        || dacl.is_null()
    {
        return Err(io::Error::last_os_error());
    }
    let result = unsafe {
        SetSecurityInfo(
            access.file.as_raw_handle(),
            1,
            0x80000004,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null_mut(),
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    verify_file(&access.file)
}
pub fn create_directory(path: &Path) -> io::Result<()> {
    let _parents = lock_parents(path)?;
    let descriptor = CurrentUser::new()?.descriptor(true)?;
    let attributes = Attributes {
        length: std::mem::size_of::<Attributes>() as u32,
        descriptor: descriptor.0,
        inherit: 0,
    };
    if unsafe { CreateDirectoryW(wide(path)?.as_ptr(), &attributes) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let security = inspect_private(path, true)?;
    if !security.owner_current_user || !security.owner_only || !security.protected {
        return Err(io::Error::other("New private directory ACL differs."));
    }
    Ok(())
}
pub fn create_file(path: &Path) -> io::Result<File> {
    let descriptor = CurrentUser::new()?.descriptor(false)?;
    // SAFETY: CREATE_NEW receives a live complete descriptor and a noninheritable handle.
    let attributes = Attributes {
        length: std::mem::size_of::<Attributes>() as u32,
        descriptor: descriptor.0,
        inherit: 0,
    };
    let raw = unsafe {
        CreateFileW(
            wide(path)?.as_ptr(),
            0xC0020000,
            0,
            &attributes,
            1,
            0x00200080,
            ptr::null_mut(),
        )
    };
    if raw == -1isize as Handle {
        return Err(io::Error::last_os_error());
    }
    let file = unsafe { File::from_raw_handle(raw) };
    Ok(file)
}
