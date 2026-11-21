//! Local named-pipe transport between the launcher and the injected runtime.
//! Framing: 4-byte little-endian length followed by UTF-8 bytes.

use std::ffi::{c_char, c_void, CString};

pub const PIPE_PREFIX: &str = r"\\.\pipe\coal-executor-";
pub const MAX_FRAME: usize = 8 * 1024 * 1024;
pub const OK_PREFIX: &str = "OK\n";
pub const ERR_PREFIX: &str = "ERR\n";

#[allow(non_camel_case_types)]
mod win {
    use super::*;

    pub type HANDLE = *mut c_void;
    pub type LPVOID = *mut c_void;
    pub type DWORD = u32;
    pub type BOOL = i32;
    pub type LPCSTR = *const c_char;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateNamedPipeA(
            lpName: LPCSTR,
            dwOpenMode: DWORD,
            dwPipeMode: DWORD,
            nMaxInstances: DWORD,
            nOutBufferSize: DWORD,
            nInBufferSize: DWORD,
            nDefaultTimeOut: DWORD,
            lpSecurityAttributes: LPVOID,
        ) -> HANDLE;
        pub fn ConnectNamedPipe(hNamedPipe: HANDLE, lpOverlapped: LPVOID) -> BOOL;
        pub fn DisconnectNamedPipe(hNamedPipe: HANDLE) -> BOOL;
        pub fn CreateFileA(
            lpFileName: LPCSTR,
            dwDesiredAccess: DWORD,
            dwShareMode: DWORD,
            lpSecurityAttributes: LPVOID,
            dwCreationDisposition: DWORD,
            dwFlagsAndAttributes: DWORD,
            hTemplateFile: HANDLE,
        ) -> HANDLE;
        pub fn ReadFile(
            hFile: HANDLE,
            lpBuffer: LPVOID,
            nNumberOfBytesToRead: DWORD,
            lpNumberOfBytesRead: *mut DWORD,
            lpOverlapped: LPVOID,
        ) -> BOOL;
        pub fn WriteFile(
            hFile: HANDLE,
            lpBuffer: *const c_void,
            nNumberOfBytesToWrite: DWORD,
            lpNumberOfBytesWritten: *mut DWORD,
            lpOverlapped: LPVOID,
        ) -> BOOL;
        pub fn CloseHandle(hObject: HANDLE) -> BOOL;
        pub fn GetLastError() -> DWORD;
    }
}
use win::*;

const PIPE_ACCESS_DUPLEX: DWORD = 0x0000_0003;
const PIPE_TYPE_MESSAGE: DWORD = 0x0000_0004;
const PIPE_READMODE_MESSAGE: DWORD = 0x0000_0002;
const PIPE_WAIT: DWORD = 0;
const PIPE_UNLIMITED_INSTANCES: DWORD = 255;
const GENERIC_READ: DWORD = 0x8000_0000;
const GENERIC_WRITE: DWORD = 0x4000_0000;
const OPEN_EXISTING: DWORD = 3;
const FILE_ATTRIBUTE_NORMAL: DWORD = 0x0000_0080;
const ERROR_PIPE_CONNECTED: DWORD = 232;

pub fn pipe_path(pid: u32) -> String {
    format!("{PIPE_PREFIX}{pid}")
}

fn invalid_handle() -> HANDLE {
    usize::MAX as HANDLE
}

fn last_error() -> String {
    format!("last error {}", unsafe { GetLastError() })
}

pub struct Pipe(HANDLE);

impl Drop for Pipe {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

impl Pipe {
    /// Server endpoint created inside the injected runtime.
    pub fn server(pid: u32) -> Result<Self, String> {
        let name = CString::new(pipe_path(pid)).map_err(|e| e.to_string())?;
        let handle = unsafe {
            CreateNamedPipeA(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                MAX_FRAME as DWORD,
                MAX_FRAME as DWORD,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle.is_null() || handle == invalid_handle() {
            Err(format!("CreateNamedPipeA failed ({})", last_error()))
        } else {
            Ok(Self(handle))
        }
    }

    /// Client endpoint used by the launcher.
    pub fn connect(pid: u32) -> Result<Self, String> {
        let name = CString::new(pipe_path(pid)).map_err(|e| e.to_string())?;
        let handle = unsafe {
            CreateFileA(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle.is_null() || handle == invalid_handle() {
            Err(format!(
                "cannot reach runtime at PID {} ({}) — is it injected?",
                pid,
                last_error()
            ))
        } else {
            Ok(Self(handle))
        }
    }

    pub fn accept(&self) -> Result<(), String> {
        let result = unsafe { ConnectNamedPipe(self.0, std::ptr::null_mut()) };
        if result == 0 {
            let err = unsafe { GetLastError() };
            if err != ERROR_PIPE_CONNECTED {
                return Err(format!("ConnectNamedPipe failed ({err})"));
            }
        }
        Ok(())
    }

    pub fn disconnect(&self) {
        unsafe {
            DisconnectNamedPipe(self.0);
        }
    }

    pub fn write_frame(&self, data: &[u8]) -> Result<(), String> {
        if data.len() > MAX_FRAME {
            return Err("frame exceeds size limit".into());
        }
        write_all(self.0, &(data.len() as u32).to_le_bytes())?;
        write_all(self.0, data)
    }

    pub fn read_frame(&self) -> Result<Vec<u8>, String> {
        let mut header = [0u8; 4];
        read_all(self.0, &mut header)?;
        let len = u32::from_le_bytes(header) as usize;
        if len > MAX_FRAME {
            return Err(format!("peer announced oversized frame ({len} bytes)"));
        }
        let mut buffer = vec![0u8; len];
        read_all(self.0, &mut buffer)?;
        Ok(buffer)
    }
}

#[derive(Debug)]
pub enum Response {
    Ok(String),
    Err(String),
}

/// Convenience client: send one script and return the VM's response.
pub fn execute(pid: u32, source: &str) -> Result<Response, String> {
    let pipe = Pipe::connect(pid)?;
    pipe.write_frame(source.as_bytes())?;
    let raw = pipe.read_frame()?;
    let text = String::from_utf8_lossy(&raw);
    if let Some(body) = text.strip_prefix(OK_PREFIX) {
        Ok(Response::Ok(body.to_string()))
    } else if let Some(body) = text.strip_prefix(ERR_PREFIX) {
        Ok(Response::Err(body.to_string()))
    } else {
        Ok(Response::Ok(text.to_string()))
    }
}

fn write_all(handle: HANDLE, mut data: &[u8]) -> Result<(), String> {
    while !data.is_empty() {
        let mut written = 0u32;
        let ok = unsafe {
            WriteFile(
                handle,
                data.as_ptr() as *const c_void,
                data.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || written == 0 {
            return Err(format!("WriteFile failed ({})", last_error()));
        }
        data = &data[written as usize..];
    }
    Ok(())
}

fn read_all(handle: HANDLE, mut buffer: &mut [u8]) -> Result<(), String> {
    while !buffer.is_empty() {
        let mut read = 0u32;
        let ok = unsafe {
            ReadFile(
                handle,
                buffer.as_mut_ptr() as LPVOID,
                buffer.len() as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || read == 0 {
            return Err(format!("ReadFile failed ({})", last_error()));
        }
        buffer = &mut buffer[read as usize..];
    }
    Ok(())
}
