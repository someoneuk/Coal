use std::ffi::CStr;

use crate::win32::{
    CloseHandle, CreateToolhelp32Snapshot, Process32First, Process32Next, HANDLE,
    PROCESSENTRY32A, TH32CS_SNAPPROCESS,
};

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
}

/// First live process whose name matches `target` (extension-insensitive).
pub fn find_process(target: &str) -> Option<ProcessInfo> {
    let needle = normalize(target);
    if needle.is_empty() {
        return None;
    }

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot == usize::MAX as HANDLE {
            return None;
        }

        let mut entry: PROCESSENTRY32A = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<PROCESSENTRY32A>() as u32;

        let mut ok = Process32First(snapshot, &mut entry);
        let mut best: Option<ProcessInfo> = None;
        while ok != 0 {
            let name = CStr::from_ptr(entry.sz_exe_file.as_ptr())
                .to_string_lossy()
                .into_owned();
            if normalize(&name) == needle {
                let info = ProcessInfo {
                    pid: entry.th32_process_id,
                    name,
                };
                // Prefer the lowest PID for deterministic behaviour.
                if best.as_ref().map(|b| info.pid < b.pid).unwrap_or(true) {
                    best = Some(info);
                }
            }
            ok = Process32Next(snapshot, &mut entry);
        }

        CloseHandle(snapshot);
        best
    }
}

fn normalize(name: &str) -> String {
    name.trim()
        .trim_end_matches(".exe")
        .to_ascii_lowercase()
}
