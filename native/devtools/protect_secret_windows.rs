use super::{Secret, private_files::Attributes};
use std::{
    ffi::c_void,
    io::{self, Write},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};
type Handle = *mut c_void;
#[repr(C)]
struct Blob {
    length: u32,
    data: *mut u8,
}
#[link(name = "crypt32")]
unsafe extern "system" {
    fn CryptProtectData(
        input: *const Blob,
        description: *const u16,
        entropy: *const Blob,
        reserved: Handle,
        prompt: Handle,
        flags: u32,
        output: *mut Blob,
    ) -> i32;
    fn CryptUnprotectData(
        input: *const Blob,
        description: *mut *mut u16,
        entropy: *const Blob,
        reserved: Handle,
        prompt: *const c_void,
        flags: u32,
        output: *mut Blob,
    ) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LocalFree(memory: Handle) -> Handle;
    fn CreateFileW(
        path: *const u16,
        access: u32,
        share: u32,
        attributes: *const Attributes,
        disposition: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: Handle, mode: u32) -> i32;
    fn ReadConsoleW(
        handle: Handle,
        buffer: Handle,
        count: u32,
        read: *mut u32,
        control: Handle,
    ) -> i32;
}
struct Output(Blob);
impl Drop for Output {
    fn drop(&mut self) {
        if !self.0.data.is_null() {
            for index in 0..self.0.length as usize {
                unsafe {
                    ptr::write_volatile(self.0.data.add(index), 0);
                }
            }
            unsafe {
                LocalFree(self.0.data.cast());
            }
        }
    }
}
fn fail() -> io::Error {
    io::Error::other("Current-user secret protection failed; private input withheld.")
}

pub(super) fn encrypt(text: &str) -> io::Result<Vec<u8>> {
    let mut plain = Secret(Vec::with_capacity(text.len() * 2));
    for unit in text.encode_utf16() {
        plain.0.extend_from_slice(&unit.to_le_bytes());
    }
    let input = Blob {
        length: u32::try_from(plain.0.len()).map_err(|_| fail())?,
        data: plain.0.as_mut_ptr(),
    };
    let mut encrypted = Output(Blob {
        length: 0,
        data: ptr::null_mut(),
    });
    // SAFETY: live bounded UTF16LE bytes, no extra entropy/UI/machine-wide protection, scoped output.
    if unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            1,
            &mut encrypted.0,
        )
    } == 0
        || encrypted.0.data.is_null()
        || encrypted.0.length > 512 * 1024
    {
        return Err(fail());
    }
    // Verify the native round trip before any persistent destination is created.
    let mut recovered = Output(Blob {
        length: 0,
        data: ptr::null_mut(),
    });
    if unsafe {
        CryptUnprotectData(
            &encrypted.0,
            ptr::null_mut(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null(),
            1,
            &mut recovered.0,
        )
    } == 0
        || recovered.0.data.is_null()
        || recovered.0.length as usize != plain.0.len()
    {
        return Err(fail());
    }
    let actual =
        unsafe { std::slice::from_raw_parts(recovered.0.data, recovered.0.length as usize) };
    if actual != plain.0 {
        return Err(fail());
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(encrypted.0.data, encrypted.0.length as usize) };
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.extend_from_slice(&[HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]]);
    }
    Ok(encoded)
}

struct Console {
    handle: OwnedHandle,
    original: u32,
    restored: bool,
}
impl Console {
    fn restore(&mut self) -> io::Result<()> {
        if !self.restored {
            if unsafe { SetConsoleMode(self.handle.as_raw_handle(), self.original) } == 0 {
                return Err(fail());
            }
            self.restored = true;
        }
        Ok(())
    }
}
impl Drop for Console {
    fn drop(&mut self) {
        if self.restore().is_err() {
            eprintln!("ERROR: Console input mode restoration failed.");
        }
    }
}

pub(super) fn read_hidden() -> io::Result<Secret> {
    let name = "CONIN$".encode_utf16().chain([0]).collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            0xC0000000,
            3,
            ptr::null(),
            3,
            0,
            ptr::null_mut(),
        )
    };
    if handle == -1isize as Handle {
        return Err(fail());
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut original = 0;
    if unsafe { GetConsoleMode(handle.as_raw_handle(), &mut original) } == 0 {
        return Err(fail());
    }
    // Raw bounded input handles Ctrl-C as a character so restoration precedes cancellation.
    if unsafe { SetConsoleMode(handle.as_raw_handle(), original & !(1 | 2 | 4)) } == 0 {
        return Err(fail());
    }
    let mut console = Console {
        handle,
        original,
        restored: false,
    };
    io::stderr().write_all(b"Secret (hidden; never logged): ")?;
    io::stderr().flush()?;
    let mut bytes = Secret(Vec::new());
    let operation = (|| {
        loop {
            let mut unit = 0u16;
            let mut read = 0;
            if unsafe {
                ReadConsoleW(
                    console.handle.as_raw_handle(),
                    (&mut unit as *mut u16).cast(),
                    1,
                    &mut read,
                    ptr::null_mut(),
                )
            } == 0
                || read != 1
            {
                return Err(fail());
            }
            match unit {
                3 | 26 => {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Secret protection cancelled.",
                    ));
                }
                10 | 13 => break,
                8 => {
                    if bytes.0.len() >= 2 {
                        let end = bytes.0.len();
                        let removed = u16::from_le_bytes([bytes.0[end - 2], bytes.0[end - 1]]);
                        for byte in &mut bytes.0[end - 2..] {
                            unsafe {
                                ptr::write_volatile(byte, 0);
                            }
                        }
                        bytes.0.truncate(end - 2);
                        if (0xDC00..=0xDFFF).contains(&removed) && bytes.0.len() >= 2 {
                            let end = bytes.0.len();
                            let preceding =
                                u16::from_le_bytes([bytes.0[end - 2], bytes.0[end - 1]]);
                            if (0xD800..=0xDBFF).contains(&preceding) {
                                for byte in &mut bytes.0[end - 2..] {
                                    unsafe {
                                        ptr::write_volatile(byte, 0);
                                    }
                                }
                                bytes.0.truncate(end - 2);
                            }
                        }
                    }
                }
                0 => return Err(fail()),
                _ if bytes.0.len() < 131072 => bytes.0.extend_from_slice(&unit.to_le_bytes()),
                _ => return Err(fail()),
            }
        }
        let mut text = Secret(Vec::new());
        let units = bytes
            .0
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
        for character in std::char::decode_utf16(units) {
            let mut buffer = [0u8; 4];
            text.0.extend_from_slice(
                character
                    .map_err(|_| fail())?
                    .encode_utf8(&mut buffer)
                    .as_bytes(),
            );
        }
        Ok(text)
    })();
    console.restore()?;
    io::stderr().write_all(b"\n")?;
    operation
}
