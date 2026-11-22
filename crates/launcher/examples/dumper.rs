//! Dump readable module images from a live process for analysis.
//! Usage: cargo run --release -p launcher --example dumper -- <pid> [out_dir]

use std::ffi::{c_char, c_void};
use std::fs;
use std::path::PathBuf;

#[allow(non_camel_case_types)]
mod win {
    use super::*;
    pub type HANDLE = *mut c_void;
    pub type LPVOID = *mut c_void;
    pub type DWORD = u32;
    pub type BOOL = i32;
    pub type SIZE_T = usize;

    #[repr(C)]
    pub struct MODULEENTRY32 {
        pub dw_size: DWORD,
        pub th32_module_id: DWORD,
        pub th32_process_id: DWORD,
        pub glbl_cnt_usage: DWORD,
        pub proc_cnt_usage: DWORD,
        pub mod_base_addr: *mut u8,
        pub mod_base_size: DWORD,
        pub h_module: HANDLE,
        pub sz_module: [c_char; 256],
        pub sz_exe_path: [c_char; 260],
    }

    #[repr(C)]
    pub struct MEMORY_BASIC_INFORMATION {
        pub base_address: LPVOID,
        pub allocation_base: LPVOID,
        pub allocation_protect: DWORD,
        pub region_size: SIZE_T,
        pub state: DWORD,
        pub protect: DWORD,
        pub region_type: DWORD,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn OpenProcess(desired: DWORD, inherit: BOOL, pid: DWORD) -> HANDLE;
        pub fn CloseHandle(handle: HANDLE) -> BOOL;
        pub fn ReadProcessMemory(
            process: HANDLE,
            base: LPVOID,
            buffer: LPVOID,
            size: SIZE_T,
            read: *mut SIZE_T,
        ) -> BOOL;
        pub fn VirtualQueryEx(
            process: HANDLE,
            address: LPVOID,
            info: *mut MEMORY_BASIC_INFORMATION,
            length: SIZE_T,
        ) -> SIZE_T;
        pub fn CreateToolhelp32Snapshot(flags: DWORD, pid: DWORD) -> HANDLE;
        pub fn Module32First(snapshot: HANDLE, entry: *mut MODULEENTRY32) -> BOOL;
        pub fn Module32Next(snapshot: HANDLE, entry: *mut MODULEENTRY32) -> BOOL;
        pub fn GetLastError() -> DWORD;
    }

}
use win::*;

const PROCESS_QUERY_INFORMATION: DWORD = 0x0400;
const PROCESS_VM_READ: DWORD = 0x0010;
const TH32CS_SNAPMODULE: DWORD = 0x0008;
const MEM_COMMIT: DWORD = 0x1000;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pid: u32 = args.get(1).expect("pass a PID").parse().expect("numeric PID");
    let out_dir = PathBuf::from(
        args.get(2)
            .cloned()
            .unwrap_or_else(|| r"C:\Users\obama\AppData\Local\Temp\coal_dump".into()),
    );
    fs::create_dir_all(&out_dir).expect("create output dir");

    unsafe {
        let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if process.is_null() || process == usize::MAX as HANDLE {
            panic!("OpenProcess failed ({})", GetLastError());
        }
        let _process = Guard(process);

        // 1. Dump every module image from memory.
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid);
        if !snapshot.is_null() && snapshot != usize::MAX as HANDLE {
            let _snapshot = Guard(snapshot);
            let mut entry: MODULEENTRY32 = std::mem::zeroed();
            entry.dw_size = std::mem::size_of::<MODULEENTRY32>() as DWORD;
            let mut ok = Module32First(snapshot, &mut entry);
            while ok != 0 {
                if entry.th32_process_id == pid {
                    let name = std::ffi::CStr::from_ptr(entry.sz_module.as_ptr())
                        .to_string_lossy()
                        .to_string();
                    let base = entry.mod_base_addr as usize;
                    let size = entry.mod_base_size as usize;
                    let mut buffer = vec![0u8; size];
                    let mut read = 0usize;
                    if ReadProcessMemory(
                        process,
                        base as LPVOID,
                        buffer.as_mut_ptr() as LPVOID,
                        size,
                        &mut read,
                    ) != 0
                    {
                        let safe = name.replace(['/', '\\', ':', '.'], "_");
                        let path = out_dir.join(format!("{safe}_{base:X}.bin"));
                        fs::write(&path, &buffer).expect("write module dump");
                        println!("MODULE {name} base=0x{base:X} size={size} -> {}", path.display());
                    }
                }
                ok = Module32Next(snapshot, &mut entry);
            }
        }

        // 2. Dump executable committed regions that are NOT inside a module.
        let mut address = 0usize;
        let mut dumped = 0usize;
        while address < 0x7FFF_FFFF_FFFF {
            let mut info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
            let result = VirtualQueryEx(
                process,
                address as LPVOID,
                &mut info,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            );
            if result == 0 {
                break;
            }
            let base = info.base_address as usize;
            let size = info.region_size;
            let executable = matches!(info.protect, 0x10 | 0x20 | 0x40 | 0x80); // EXECUTE variants
            if info.state == MEM_COMMIT && executable && size >= 0x1000 && size <= 64 * 1024 * 1024 {
                let mut buffer = vec![0u8; size];
                let mut read = 0usize;
                if ReadProcessMemory(
                    process,
                    base as LPVOID,
                    buffer.as_mut_ptr() as LPVOID,
                    size,
                    &mut read,
                ) != 0
                {
                    let path = out_dir.join(format!("exec_{base:X}_{size:X}.bin"));
                    fs::write(&path, &buffer).expect("write region dump");
                    println!("EXEC 0x{base:X} size={size} -> {}", path.display());
                    dumped += 1;
                    if dumped > 200 {
                        break;
                    }
                }
            }
            address = base + size;
        }
        println!("done: {dumped} executable regions");
    }
}

struct Guard(HANDLE);
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
