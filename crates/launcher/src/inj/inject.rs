//! Minimal DLL inj via the classic `LoadLibraryA` remote-thread route.
//! Raw Win32 declarations keep this dependency-free and small.

use std::ffi::{c_char, c_void, CString};
use std::path::Path;

#[allow(non_camel_case_types)]
mod win {
    use super::*;

    pub type HANDLE = *mut c_void;
    pub type HMODULE = *mut c_void;
    pub type LPVOID = *mut c_void;
    pub type DWORD = u32;
    pub type BOOL = i32;
    pub type LPCSTR = *const c_char;
    pub type FARPROC = Option<unsafe extern "system" fn() -> isize>;
    pub type LPTHREAD_START_ROUTINE = unsafe extern "system" fn(LPVOID) -> DWORD;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetLastError() -> DWORD;
        pub fn OpenProcess(
            dwDesiredAccess: DWORD,
            bInheritHandle: BOOL,
            dwProcessId: DWORD,
        ) -> HANDLE;
        pub fn CloseHandle(hObject: HANDLE) -> BOOL;
        pub fn GetModuleHandleA(lpModuleName: LPCSTR) -> HMODULE;
        pub fn GetProcAddress(hModule: HMODULE, lpProcName: LPCSTR) -> FARPROC;
        pub fn VirtualAllocEx(
            hProcess: HANDLE,
            lpAddress: LPVOID,
            dwSize: usize,
            flAllocationType: DWORD,
            flProtect: DWORD,
        ) -> LPVOID;
        pub fn VirtualFreeEx(
            hProcess: HANDLE,
            lpAddress: LPVOID,
            dwSize: usize,
            dwFreeType: DWORD,
        ) -> BOOL;
        pub fn WriteProcessMemory(
            hProcess: HANDLE,
            lpBaseAddress: LPVOID,
            lpBuffer: *const c_void,
            nSize: usize,
            lpNumberOfBytesWritten: *mut usize,
        ) -> BOOL;
        pub fn CreateRemoteThread(
            hProcess: HANDLE,
            lpThreadAttributes: LPVOID,
            dwStackSize: usize,
            lpStartAddress: LPTHREAD_START_ROUTINE,
            lpParameter: LPVOID,
            dwCreationFlags: DWORD,
            lpThreadId: *mut DWORD,
        ) -> HANDLE;
        pub fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: DWORD) -> DWORD;
        pub fn GetExitCodeThread(hThread: HANDLE, lpExitCode: *mut DWORD) -> BOOL;
    }
}

use win::*;

const PROCESS_CREATE_THREAD: DWORD = 0x0002;
const PROCESS_QUERY_INFORMATION: DWORD = 0x0400;
const PROCESS_VM_OPERATION: DWORD = 0x0008;
const PROCESS_VM_WRITE: DWORD = 0x0020;
const PROCESS_VM_READ: DWORD = 0x0010;
const MEM_COMMIT: DWORD = 0x1000;
const MEM_RESERVE: DWORD = 0x2000;
const MEM_RELEASE: DWORD = 0x8000;
const PAGE_READWRITE: DWORD = 0x04;
const INFINITE: DWORD = 0xFFFF_FFFF;
const WAIT_OBJECT_0: DWORD = 0;

#[derive(Debug, Clone)]
pub struct InjectResult {
    pub pid: u32,
    pub module_base: u32,
}

struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

struct AllocGuard {
    process: HANDLE,
    address: LPVOID,
}
impl Drop for AllocGuard {
    fn drop(&mut self) {
        unsafe {
            VirtualFreeEx(self.process, self.address, 0, MEM_RELEASE);
        }
    }
}

pub fn inject_dll(pid: u32, dll_path: &Path) -> Result<InjectResult, String> {
    let full =
        std::fs::canonicalize(dll_path).map_err(|e| format!("cannot resolve DLL path: {e}"))?;
    let path = CString::new(full.to_string_lossy().as_bytes())
        .map_err(|e| format!("path contains a null byte: {e}"))?;

    unsafe {
        let process = OpenProcess(
            PROCESS_CREATE_THREAD
                | PROCESS_QUERY_INFORMATION
                | PROCESS_VM_OPERATION
                | PROCESS_VM_WRITE
                | PROCESS_VM_READ,
            0,
            pid,
        );
        if process.is_null() {
            return Err(format!(
                "OpenProcess failed (last error {})",
                GetLastError()
            ));
        }
        let _process = HandleGuard(process);

        let kernel32 = GetModuleHandleA(c"kernel32.dll".as_ptr());
        if kernel32.is_null() {
            return Err(format!(
                "GetModuleHandleA failed (last error {})",
                GetLastError()
            ));
        }
        let load_library = GetProcAddress(kernel32, c"LoadLibraryA".as_ptr())
            .ok_or_else(|| format!("GetProcAddress failed (last error {})", GetLastError()))?;
        let start: LPTHREAD_START_ROUTINE = std::mem::transmute(load_library);

        let bytes = path.as_bytes_with_nul();
        let remote = VirtualAllocEx(
            process,
            std::ptr::null_mut(),
            bytes.len(),
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if remote.is_null() {
            return Err(format!(
                "VirtualAllocEx failed (last error {})",
                GetLastError()
            ));
        }
        let _alloc = AllocGuard {
            process,
            address: remote,
        };

        let mut written = 0usize;
        if WriteProcessMemory(
            process,
            remote,
            bytes.as_ptr() as *const c_void,
            bytes.len(),
            &mut written,
        ) == 0
        {
            return Err(format!(
                "WriteProcessMemory failed (last error {})",
                GetLastError()
            ));
        }

        let thread = CreateRemoteThread(
            process,
            std::ptr::null_mut(),
            0,
            start,
            remote,
            0,
            std::ptr::null_mut(),
        );
        if thread.is_null() {
            return Err(format!(
                "CreateRemoteThread failed (last error {})",
                GetLastError()
            ));
        }
        let _thread = HandleGuard(thread);

        if WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0 {
            return Err(format!(
                "WaitForSingleObject failed (last error {})",
                GetLastError()
            ));
        }

        let mut module_base = 0u32;
        if GetExitCodeThread(thread, &mut module_base) == 0 {
            return Err(format!(
                "GetExitCodeThread failed (last error {})",
                GetLastError()
            ));
        }
        if module_base == 0 {
            return Err(
                "LoadLibraryA returned NULL in target (bad path or missing dependency)".into(),
            );
        }

        Ok(InjectResult { pid, module_base })
    }
}
