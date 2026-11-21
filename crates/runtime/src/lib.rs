//! Injected library. Loaded into the target via `LoadLibraryA` by the launcher.
//! Keep DllMain tiny: no meaningful work under the loader lock.

#![allow(non_snake_case)]
#![allow(non_camel_case_types)]

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write;

const DLL_PROCESS_ATTACH: u32 = 1;
const DLL_PROCESS_DETACH: u32 = 0;

#[link(name = "kernel32")]
extern "system" {
    fn DisableThreadLibraryCalls(hModule: *mut c_void) -> i32;
    fn CreateThread(
        lpThreadAttributes: *mut c_void,
        dwStackSize: usize,
        lpStartAddress: unsafe extern "system" fn(*mut c_void) -> u32,
        lpParameter: *mut c_void,
        dwCreationFlags: u32,
        lpThreadId: *mut u32,
    ) -> *mut c_void;
}

unsafe extern "system" fn worker(_param: *mut c_void) -> u32 {
    let mut path = std::env::temp_dir();
    path.push("executor-runtime.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            f,
            "[runtime] attached pid={} thread={:?}",
            std::process::id(),
            std::thread::current().id()
        );
    }
    // Future initialization (Lua state, hooks, IPC) goes here.
    0
}

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(module: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    match reason {
        DLL_PROCESS_ATTACH => unsafe {
            DisableThreadLibraryCalls(module);
            CreateThread(
                std::ptr::null_mut(),
                0,
                worker,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
            );
        },
        DLL_PROCESS_DETACH => {
            // Teardown hook goes here.
        }
        _ => {}
    }
    1
}
