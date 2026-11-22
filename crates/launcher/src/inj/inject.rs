//! Minimal DLL inj via the classic `LoadLibraryA` remote-thread route.
//! Raw Win32 declarations keep this dependency-free and small.

use std::ffi::{c_char, c_void, CString};
use std::path::Path;
use std::thread;
use std::time::Duration;

use coal_shared::ipc::Pipe;

#[allow(non_camel_case_types)]
pub(crate) mod win {
    use super::*;

    pub type HANDLE = *mut c_void;
    pub type HMODULE = *mut c_void;
    pub type LPVOID = *mut c_void;
    pub type DWORD = u32;
    pub type BOOL = i32;
    pub type LPCSTR = *const c_char;
    pub type FARPROC = Option<unsafe extern "system" fn() -> isize>;
    pub type LPTHREAD_START_ROUTINE = unsafe extern "system" fn(LPVOID) -> DWORD;

    #[repr(C)]
    pub struct MODULEENTRY32 {
        pub dw_size: DWORD,
        pub th32_module_id: DWORD,
        pub th32_process_id: DWORD,
        pub glbl_cnt_usage: DWORD,
        pub proc_cnt_usage: DWORD,
        pub mod_base_addr: *mut u8,
        pub mod_base_size: DWORD,
        pub h_module: HMODULE,
        pub sz_module: [c_char; 256],
        pub sz_exe_path: [c_char; 260],
    }

    #[repr(C)]
    pub struct THREADENTRY32 {
        pub dw_size: DWORD,
        pub cnt_usage: DWORD,
        pub th32_thread_id: DWORD,
        pub th32_owner_process_id: DWORD,
        pub tp_base_pri: i32,
        pub tp_delta_pri: i32,
        pub dw_flags: DWORD,
    }

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
        pub fn QueueUserAPC(
            pfnAPC: LPTHREAD_START_ROUTINE,
            hThread: HANDLE,
            dwData: usize,
        ) -> DWORD;
        pub fn OpenThread(dwDesiredAccess: DWORD, bInheritHandle: BOOL, dwThreadId: DWORD) -> HANDLE;
        pub fn SuspendThread(hThread: HANDLE) -> DWORD;
        pub fn ResumeThread(hThread: HANDLE) -> DWORD;
        pub fn GetThreadContext(hThread: HANDLE, lpContext: LPVOID) -> BOOL;
        pub fn SetThreadContext(hThread: HANDLE, lpContext: LPVOID) -> BOOL;
        pub fn ReadProcessMemory(
            hProcess: HANDLE,
            lpBaseAddress: LPVOID,
            lpBuffer: LPVOID,
            nSize: usize,
            lpNumberOfBytesRead: *mut usize,
        ) -> BOOL;
        pub fn CreateToolhelp32Snapshot(dwFlags: DWORD, th32ProcessID: DWORD) -> HANDLE;
        pub fn Thread32First(hSnapshot: HANDLE, lpte: *mut THREADENTRY32) -> BOOL;
        pub fn Thread32Next(hSnapshot: HANDLE, lpte: *mut THREADENTRY32) -> BOOL;
        pub fn Module32First(hSnapshot: HANDLE, lpme: *mut MODULEENTRY32) -> BOOL;
        pub fn Module32Next(hSnapshot: HANDLE, lpme: *mut MODULEENTRY32) -> BOOL;
    }

    #[link(name = "ntdll")]
    extern "system" {
        pub fn NtCreateThreadEx(
            thread_handle: *mut HANDLE,
            desired_access: DWORD,
            object_attributes: LPVOID,
            process_handle: HANDLE,
            start_routine: LPTHREAD_START_ROUTINE,
            argument: LPVOID,
            create_flags: DWORD,
            zero_bits: usize,
            stack_size: usize,
            maximum_stack_size: usize,
            attribute_list: LPVOID,
        ) -> i32;
    }
}

use win::*;

pub(crate) const PROCESS_CREATE_THREAD: DWORD = 0x0002;
pub(crate) const PROCESS_QUERY_INFORMATION: DWORD = 0x0400;
pub(crate) const PROCESS_VM_OPERATION: DWORD = 0x0008;
pub(crate) const PROCESS_VM_WRITE: DWORD = 0x0020;
pub(crate) const PROCESS_VM_READ: DWORD = 0x0010;
pub(crate) const MEM_COMMIT: DWORD = 0x1000;
pub(crate) const MEM_RESERVE: DWORD = 0x2000;
pub(crate) const MEM_RELEASE: DWORD = 0x8000;
pub(crate) const PAGE_READWRITE: DWORD = 0x04;
const INFINITE: DWORD = 0xFFFF_FFFF;
const WAIT_OBJECT_0: DWORD = 0;
const TH32CS_SNAPTHREAD: DWORD = 0x0000_0004;
const TH32CS_SNAPMODULE: DWORD = 0x0000_0008;
const THREAD_SET_CONTEXT: DWORD = 0x0010;
const THREAD_GET_CONTEXT: DWORD = 0x0008;
const THREAD_SUSPEND_RESUME: DWORD = 0x0002;
pub(crate) const PAGE_EXECUTE_READWRITE: DWORD = 0x40;
const CONTEXT_FULL: u32 = 0x0010_000B;
const CONTEXT_FLAGS_OFFSET: usize = 0x30;
const CONTEXT_RIP_OFFSET: usize = 0xF8;
const CONTEXT_RCX_OFFSET: usize = 0x80;

#[repr(C, align(16))]
struct ThreadContext([u8; 1232]);

#[derive(Debug, Clone)]
pub struct InjectResult {
    pub pid: u32,
    pub module_base: u32,
}

pub(crate) struct HandleGuard(pub HANDLE);
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

/// Confirm a module is really present at `base` in the target. A remote-thread
/// exit code alone is not proof: protected targets can return an NTSTATUS
/// while silently refusing the load.
fn module_loaded(pid: u32, base: u32) -> bool {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid);
        if snapshot.is_null() || snapshot == usize::MAX as HANDLE {
            return false;
        }
        let _snapshot = HandleGuard(snapshot);
        let mut entry: MODULEENTRY32 = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<MODULEENTRY32>() as DWORD;
        let mut ok = Module32First(snapshot, &mut entry);
        while ok != 0 {
            if entry.th32_process_id == pid && entry.mod_base_addr as u32 == base {
                return true;
            }
            ok = Module32Next(snapshot, &mut entry);
        }
        false
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
        if module_base == 0 || module_base & 0xFFF != 0 || !module_loaded(pid, module_base) {
            return Err(format!(
                "target refused the load (thread status 0x{module_base:X}) — verify with the runtime log"
            ));
        }

        Ok(InjectResult { pid, module_base })
    }
}

/// Queue `LoadLibraryA` as an APC on every thread of the target. No new
/// remote thread is created, which gets past targets that reject them.
/// The DLL loads as soon as any queued thread enters an alertable wait.
pub fn inject_dll_apc(pid: u32, dll_path: &Path) -> Result<InjectResult, String> {
    let full =
        std::fs::canonicalize(dll_path).map_err(|e| format!("cannot resolve DLL path: {e}"))?;
    let path = CString::new(full.to_string_lossy().as_bytes())
        .map_err(|e| format!("path contains a null byte: {e}"))?;

    unsafe {
        let process = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
            0,
            pid,
        );
        if process.is_null() {
            return Err(format!("OpenProcess failed ({})", GetLastError()));
        }
        let _process = HandleGuard(process);

        let kernel32 = GetModuleHandleA(c"kernel32.dll".as_ptr());
        let load_library = GetProcAddress(kernel32, c"LoadLibraryA".as_ptr())
            .ok_or_else(|| format!("GetProcAddress failed ({})", GetLastError()))?;
        let start: LPTHREAD_START_ROUTINE = std::mem::transmute(load_library);

        // Intentionally leaked: the APC may run long after this function returns.
        let bytes = path.as_bytes_with_nul();
        let remote = VirtualAllocEx(
            process,
            std::ptr::null_mut(),
            bytes.len(),
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if remote.is_null() {
            return Err(format!("VirtualAllocEx failed ({})", GetLastError()));
        }
        let mut written = 0usize;
        if WriteProcessMemory(
            process,
            remote,
            bytes.as_ptr() as *const c_void,
            bytes.len(),
            &mut written,
        ) == 0
        {
            return Err(format!("WriteProcessMemory failed ({})", GetLastError()));
        }

        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot.is_null() || snapshot == usize::MAX as HANDLE {
            return Err(format!("CreateToolhelp32Snapshot failed ({})", GetLastError()));
        }
        let _snapshot = HandleGuard(snapshot);

        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<THREADENTRY32>() as DWORD;
        let mut ok = Thread32First(snapshot, &mut entry);
        let mut queued = 0usize;

        while ok != 0 {
            if entry.th32_owner_process_id == pid {
                let thread = OpenThread(THREAD_SET_CONTEXT, 0, entry.th32_thread_id);
                if !thread.is_null() {
                    if QueueUserAPC(start, thread, remote as usize) != 0 {
                        queued += 1;
                    }
                    CloseHandle(thread);
                }
            }
            ok = Thread32Next(snapshot, &mut entry);
        }

        if queued == 0 {
            return Err("no target threads accepted an APC".into());
        }

        for _ in 0..20 {
            if Pipe::connect(pid).is_ok() {
                return Ok(InjectResult {
                    pid,
                    module_base: 0,
                });
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err("runtime did not start in the target — the load was blocked".into())
    }
}

/// Same route as `inject_dll`, but creates the remote thread through the
/// native `NtCreateThreadEx` entry point instead of `CreateRemoteThread`.
/// Some protected targets reject the kernel32 wrapper but accept this.
pub fn inject_dll_nt(pid: u32, dll_path: &Path) -> Result<InjectResult, String> {
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
            return Err(format!("OpenProcess failed ({})", GetLastError()));
        }
        let _process = HandleGuard(process);

        let kernel32 = GetModuleHandleA(c"kernel32.dll".as_ptr());
        if kernel32.is_null() {
            return Err(format!("GetModuleHandleA failed ({})", GetLastError()));
        }
        let load_library = GetProcAddress(kernel32, c"LoadLibraryA".as_ptr())
            .ok_or_else(|| format!("GetProcAddress failed ({})", GetLastError()))?;
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
            return Err(format!("VirtualAllocEx failed ({})", GetLastError()));
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
            return Err(format!("WriteProcessMemory failed ({})", GetLastError()));
        }

        let mut thread: HANDLE = std::ptr::null_mut();
        let status = NtCreateThreadEx(
            &mut thread,
            0x1F_FFFF, // THREAD_ALL_ACCESS
            std::ptr::null_mut(),
            process,
            start,
            remote,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
        );
        if status < 0 {
            return Err(format!("NtCreateThreadEx failed (status 0x{status:08X})"));
        }
        if thread.is_null() {
            return Err("NtCreateThreadEx returned a null handle".into());
        }
        let _thread = HandleGuard(thread);

        if WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0 {
            return Err(format!("WaitForSingleObject failed ({})", GetLastError()));
        }

        let mut module_base = 0u32;
        if GetExitCodeThread(thread, &mut module_base) == 0 {
            return Err(format!("GetExitCodeThread failed ({})", GetLastError()));
        }
        if module_base == 0 || module_base & 0xFFF != 0 || !module_loaded(pid, module_base) {
            return Err(format!(
                "target refused the load (thread status 0x{module_base:X}) — verify with the runtime log"
            ));
        }

        Ok(InjectResult { pid, module_base })
    }
}

/// Read the first bytes of `LoadLibraryA` inside the target, for diagnostics.
pub fn load_library_bytes(pid: u32) -> Result<String, String> {
    unsafe {
        let process = OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, 0, pid);
        if process.is_null() {
            return Err(format!("OpenProcess failed ({})", GetLastError()));
        }
        let _process = HandleGuard(process);
        let kernel32 = GetModuleHandleA(c"kernel32.dll".as_ptr());
        let address = GetProcAddress(kernel32, c"LoadLibraryA".as_ptr())
            .ok_or_else(|| "GetProcAddress failed".to_string())? as usize;
        let mut buffer = [0u8; 24];
        let mut read = 0usize;
        if ReadProcessMemory(
            process,
            address as LPVOID,
            buffer.as_mut_ptr() as LPVOID,
            buffer.len(),
            &mut read,
        ) == 0
        {
            return Err(format!("ReadProcessMemory failed ({})", GetLastError()));
        }
        Ok(buffer.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "))
    }
}

/// Hijack an existing thread instead of creating a new one. A small x64
/// trampoline saves the interrupted state, calls `LoadLibraryA`, then restores
/// every register and jumps back to the original instruction pointer.
pub fn inject_dll_hijack(pid: u32, dll_path: &Path) -> Result<InjectResult, String> {
    let full =
        std::fs::canonicalize(dll_path).map_err(|e| format!("cannot resolve DLL path: {e}"))?;
    let path = CString::new(full.to_string_lossy().as_bytes())
        .map_err(|e| format!("path contains a null byte: {e}"))?;

    unsafe {
        let process = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
            0,
            pid,
        );
        if process.is_null() {
            return Err(format!("OpenProcess failed ({})", GetLastError()));
        }
        let _process = HandleGuard(process);

        let kernel32 = GetModuleHandleA(c"kernel32.dll".as_ptr());
        let load_library = GetProcAddress(kernel32, c"LoadLibraryA".as_ptr())
            .ok_or_else(|| format!("GetProcAddress failed ({})", GetLastError()))?;
        let load_library_addr = load_library as usize;
        if load_library_addr == 0 {
            return Err("could not resolve LoadLibraryA".into());
        }

        let path_bytes = path.as_bytes_with_nul();
        let total = path_bytes.len() + 128 + 8;
        let region = VirtualAllocEx(
            process,
            std::ptr::null_mut(),
            total,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_EXECUTE_READWRITE,
        );
        if region.is_null() {
            return Err(format!("VirtualAllocEx failed ({})", GetLastError()));
        }

        let mut written = 0usize;
        if WriteProcessMemory(
            process,
            region,
            path_bytes.as_ptr() as *const c_void,
            path_bytes.len(),
            &mut written,
        ) == 0
        {
            return Err(format!("WriteProcessMemory failed ({})", GetLastError()));
        }

        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot.is_null() || snapshot == usize::MAX as HANDLE {
            return Err(format!("CreateToolhelp32Snapshot failed ({})", GetLastError()));
        }
        let _snapshot = HandleGuard(snapshot);

        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<THREADENTRY32>() as DWORD;
        let mut ok = Thread32First(snapshot, &mut entry);
        let mut chosen = 0u32;
        while ok != 0 {
            if entry.th32_owner_process_id == pid {
                chosen = entry.th32_thread_id;
                break;
            }
            ok = Thread32Next(snapshot, &mut entry);
        }
        if chosen == 0 {
            return Err("no threads found in target process".into());
        }

        let thread = OpenThread(
            THREAD_SET_CONTEXT | THREAD_GET_CONTEXT | THREAD_SUSPEND_RESUME,
            0,
            chosen,
        );
        if thread.is_null() {
            return Err(format!("OpenThread failed ({})", GetLastError()));
        }
        let _thread = HandleGuard(thread);

        if SuspendThread(thread) == u32::MAX {
            return Err(format!("SuspendThread failed ({})", GetLastError()));
        }

        let mut context: ThreadContext = std::mem::zeroed();
        *(context.0.as_mut_ptr().add(CONTEXT_FLAGS_OFFSET) as *mut u32) = CONTEXT_FULL;
        if GetThreadContext(thread, context.0.as_mut_ptr() as LPVOID) == 0 {
            ResumeThread(thread);
            return Err(format!("GetThreadContext failed ({})", GetLastError()));
        }
        let original_rip = *(context.0.as_ptr().add(CONTEXT_RIP_OFFSET) as *const u64);

        let code_addr = region as usize + path_bytes.len();
        let stub: Vec<u8> = vec![
            0x50, // push rax
            0x51, // push rcx
            0x52, // push rdx
            0x41, 0x50, // push r8
            0x41, 0x51, // push r9
            0x41, 0x52, // push r10
            0x41, 0x53, // push r11
            0x9C, // pushfq
            0x55, // push rbp
            0x48, 0x89, 0xE5, // mov rbp, rsp  (callee-saved stack pointer)
            0x48, 0x83, 0xE4, 0xF0, // and rsp, -16
            0x48, 0x83, 0xEC, 0x20, // sub rsp, 0x20
            0x48, 0xB9, // mov rcx, path
        ]
        .into_iter()
        .chain((region as usize).to_le_bytes())
        .chain([0x49, 0xBB]) // mov r11, LoadLibraryA
        .chain(load_library_addr.to_le_bytes())
        .chain([
            0x41, 0xFF, 0xD3, // call r11
            0x48, 0x83, 0xC4, 0x20, // add rsp, 0x20
            0x48, 0x89, 0xEC, // mov rsp, rbp
            0x5D, // pop rbp
            0x9D, // popfq
            0x41, 0x5B, // pop r11
            0x41, 0x5A, // pop r10
            0x41, 0x59, // pop r9
            0x41, 0x58, // pop r8
            0x5A, // pop rdx
            0x59, // pop rcx
            0x58, // pop rax
            0xFF, 0x25, 0x00, 0x00, 0x00, 0x00, // jmp [rip]
        ])
        .chain(original_rip.to_le_bytes())
        .collect();

        let code_region = code_addr as *mut c_void;
        if WriteProcessMemory(
            process,
            code_region,
            stub.as_ptr() as *const c_void,
            stub.len(),
            &mut written,
        ) == 0
        {
            ResumeThread(thread);
            return Err(format!("shellcode write failed ({})", GetLastError()));
        }

        *(context.0.as_mut_ptr().add(CONTEXT_RIP_OFFSET) as *mut u64) = code_addr as u64;
        if SetThreadContext(thread, context.0.as_mut_ptr() as LPVOID) == 0 {
            ResumeThread(thread);
            return Err(format!("SetThreadContext failed ({})", GetLastError()));
        }

        // The region stays allocated: the trampoline and path must remain valid.
        std::mem::forget(AllocGuard {
            process,
            address: region,
        });

        ResumeThread(thread);

        for _ in 0..20 {
            if Pipe::connect(pid).is_ok() {
                return Ok(InjectResult {
                    pid,
                    module_base: 0,
                });
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err("runtime did not start in the target — the load was blocked".into())
    }
}

/// Execute a table of 3-argument calls inside the target by hijacking one
/// thread. Each table entry is 32 bytes: [fn_ptr, arg1, arg2, arg3], ending
/// with a zero fn_ptr. `table` must already be mapped in the target.
pub(crate) fn hijack_execute_table(process: HANDLE, pid: u32, table: LPVOID) -> Result<(), String> {
    use win::*;

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot.is_null() || snapshot == usize::MAX as HANDLE {
            return Err(format!("CreateToolhelp32Snapshot failed ({})", GetLastError()));
        }
        let _snapshot = HandleGuard(snapshot);

        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<THREADENTRY32>() as DWORD;
        let mut ok = Thread32First(snapshot, &mut entry);
        let mut chosen = 0u32;
        while ok != 0 {
            if entry.th32_owner_process_id == pid {
                chosen = entry.th32_thread_id;
                break;
            }
            ok = Thread32Next(snapshot, &mut entry);
        }
        if chosen == 0 {
            return Err("no threads found in target".into());
        }

        let thread = OpenThread(
            THREAD_SET_CONTEXT | THREAD_GET_CONTEXT | THREAD_SUSPEND_RESUME,
            0,
            chosen,
        );
        if thread.is_null() {
            return Err(format!("OpenThread failed ({})", GetLastError()));
        }
        let _thread = HandleGuard(thread);

        if SuspendThread(thread) == u32::MAX {
            return Err(format!("SuspendThread failed ({})", GetLastError()));
        }

        let mut context: ThreadContext = std::mem::zeroed();
        *(context.0.as_mut_ptr().add(CONTEXT_FLAGS_OFFSET) as *mut u32) = CONTEXT_FULL;
        if GetThreadContext(thread, context.0.as_mut_ptr() as LPVOID) == 0 {
            ResumeThread(thread);
            return Err(format!("GetThreadContext failed ({})", GetLastError()));
        }
        let original_rip = *(context.0.as_ptr().add(CONTEXT_RIP_OFFSET) as *const u64);

        let region = VirtualAllocEx(
            process,
            std::ptr::null_mut(),
            256,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_EXECUTE_READWRITE,
        );
        if region.is_null() {
            ResumeThread(thread);
            return Err(format!("VirtualAllocEx failed ({})", GetLastError()));
        }

        let stub: Vec<u8> = vec![
            0x50, 0x51, 0x52, // push rax, rcx, rdx
            0x41, 0x50, 0x41, 0x51, 0x41, 0x52, 0x41, 0x53, // push r8..r11
            0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, // push r12..r15
            0x55, 0x9C, // push rbp, pushfq
            0x48, 0x89, 0xE5, // mov rbp, rsp
            0x49, 0x89, 0xCF, // mov r15, rcx  (call table)
            0x48, 0x83, 0xE4, 0xF0, // and rsp, -16
            0x48, 0x83, 0xEC, 0x20, // sub rsp, 0x20
            // loop:
            0x49, 0x83, 0x3F, 0x00, // cmp qword [r15], 0
            0x74, 0x18, // je +0x18 -> done
            0x49, 0x8B, 0x4F, 0x08, // mov rcx, [r15+8]
            0x49, 0x8B, 0x57, 0x10, // mov rdx, [r15+0x10]
            0x49, 0x8B, 0x47, 0x18, // mov r8, [r15+0x18]
            0x49, 0x8B, 0x17, // mov r10, [r15]
            0x41, 0xFF, 0xD2, // call r10
            0x49, 0x83, 0xC7, 0x20, // add r15, 0x20
            0xEB, 0xE6, // jmp loop
            // done:
            0x48, 0x83, 0xC4, 0x20, // add rsp, 0x20
            0x48, 0x89, 0xEC, // mov rsp, rbp
            0x9D, // popfq
            0x5D, // pop rbp
            0x41, 0x5F, 0x41, 0x5E, 0x41, 0x5D, 0x41, 0x5C, // pop r15..r12
            0x41, 0x5B, 0x41, 0x5A, 0x41, 0x59, 0x41, 0x58, // pop r11..r8
            0x5A, 0x59, 0x58, // pop rdx, rcx, rax
            0xFF, 0x25, 0x00, 0x00, 0x00, 0x00, // jmp [rip]
        ]
        .into_iter()
        .chain(original_rip.to_le_bytes())
        .collect();

        let mut written = 0usize;
        let wrote = WriteProcessMemory(
            process,
            region,
            stub.as_ptr() as *const c_void,
            stub.len(),
            &mut written,
        ) != 0;

        *(context.0.as_mut_ptr().add(CONTEXT_RIP_OFFSET) as *mut u64) = region as u64;
        *(context.0.as_mut_ptr().add(CONTEXT_RCX_OFFSET) as *mut u64) = table as u64;
        let set = wrote && SetThreadContext(thread, context.0.as_mut_ptr() as LPVOID) != 0;

        if !set {
            VirtualFreeEx(process, region, 0, MEM_RELEASE);
            ResumeThread(thread);
            return Err("could not install the remote call stub".into());
        }
        // Code and table stay allocated: the resumed thread still needs them.
        ResumeThread(thread);
        Ok(())
    }
}

/// Hijack every thread in the target. Each thread gets its own trampoline with
/// its own original RIP, so the DLL load is attempted from whichever thread
/// the target actually lets us redirect.
pub fn inject_dll_hijack_all(pid: u32, dll_path: &Path) -> Result<InjectResult, String> {
    let full =
        std::fs::canonicalize(dll_path).map_err(|e| format!("cannot resolve DLL path: {e}"))?;
    let path = CString::new(full.to_string_lossy().as_bytes())
        .map_err(|e| format!("path contains a null byte: {e}"))?;

    unsafe {
        let process = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
            0,
            pid,
        );
        if process.is_null() {
            return Err(format!("OpenProcess failed ({})", GetLastError()));
        }
        let _process = HandleGuard(process);

        let kernel32 = GetModuleHandleA(c"kernel32.dll".as_ptr());
        let load_library = GetProcAddress(kernel32, c"LoadLibraryA".as_ptr())
            .ok_or_else(|| "GetProcAddress failed".to_string())? as usize;

        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot.is_null() || snapshot == usize::MAX as HANDLE {
            return Err(format!("CreateToolhelp32Snapshot failed ({})", GetLastError()));
        }
        let _snapshot = HandleGuard(snapshot);

        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<THREADENTRY32>() as DWORD;
        let mut ok = Thread32First(snapshot, &mut entry);
        let mut thread_ids = Vec::new();
        while ok != 0 {
            if entry.th32_owner_process_id == pid {
                thread_ids.push(entry.th32_thread_id);
            }
            ok = Thread32Next(snapshot, &mut entry);
        }

        let path_bytes = path.as_bytes_with_nul();
        let total = path_bytes.len() + 128 + 8;
        let mut hijacked = 0usize;

        for tid in thread_ids {
            let thread = OpenThread(
                THREAD_SET_CONTEXT | THREAD_GET_CONTEXT | THREAD_SUSPEND_RESUME,
                0,
                tid,
            );
            if thread.is_null() {
                continue;
            }

            if SuspendThread(thread) == u32::MAX {
                CloseHandle(thread);
                continue;
            }

            let mut context: ThreadContext = std::mem::zeroed();
            *(context.0.as_mut_ptr().add(CONTEXT_FLAGS_OFFSET) as *mut u32) = CONTEXT_FULL;
            if GetThreadContext(thread, context.0.as_mut_ptr() as LPVOID) == 0 {
                ResumeThread(thread);
                CloseHandle(thread);
                continue;
            }
            let original_rip = *(context.0.as_ptr().add(CONTEXT_RIP_OFFSET) as *const u64);

            let region = VirtualAllocEx(
                process,
                std::ptr::null_mut(),
                total,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_EXECUTE_READWRITE,
            );
            if region.is_null() {
                ResumeThread(thread);
                CloseHandle(thread);
                continue;
            }

            let mut written = 0usize;
            if WriteProcessMemory(
                process,
                region,
                path_bytes.as_ptr() as *const c_void,
                path_bytes.len(),
                &mut written,
            ) == 0
            {
                VirtualFreeEx(process, region, 0, MEM_RELEASE);
                ResumeThread(thread);
                CloseHandle(thread);
                continue;
            }

            let code_addr = region as usize + path_bytes.len();
            let stub: Vec<u8> = vec![
                0x50, 0x51, 0x52, 0x41, 0x50, 0x41, 0x51, 0x41, 0x52, 0x41, 0x53, 0x9C, 0x55,
                0x48, 0x89, 0xE5, 0x48, 0x83, 0xE4, 0xF0, 0x48, 0x83, 0xEC, 0x20, 0x48, 0xB9,
            ]
            .into_iter()
            .chain((region as usize).to_le_bytes())
            .chain([0x49, 0xBB])
            .chain(load_library.to_le_bytes())
            .chain([
                0x41, 0xFF, 0xD3, 0x48, 0x83, 0xC4, 0x20, 0x48, 0x89, 0xEC, 0x5D, 0x9D, 0x41,
                0x5B, 0x41, 0x5A, 0x41, 0x59, 0x41, 0x58, 0x5A, 0x59, 0x58, 0xFF, 0x25, 0x00,
                0x00, 0x00, 0x00,
            ])
            .chain(original_rip.to_le_bytes())
            .collect();

            let wrote_stub = WriteProcessMemory(
                process,
                code_addr as LPVOID,
                stub.as_ptr() as *const c_void,
                stub.len(),
                &mut written,
            ) != 0;

            *(context.0.as_mut_ptr().add(CONTEXT_RIP_OFFSET) as *mut u64) = code_addr as u64;
            let set = wrote_stub
                && SetThreadContext(thread, context.0.as_mut_ptr() as LPVOID) != 0;

            if set {
                hijacked += 1; // region intentionally leaked
            } else {
                VirtualFreeEx(process, region, 0, MEM_RELEASE);
            }
            ResumeThread(thread);
            CloseHandle(thread);
        }

        if hijacked == 0 {
            return Err("no threads could be hijacked".into());
        }
        println!("hijacked {hijacked} threads");
        for _ in 0..20 {
            if Pipe::connect(pid).is_ok() {
                return Ok(InjectResult {
                    pid,
                    module_base: 0,
                });
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err("runtime did not start in the target — the load was blocked".into())
    }
}
