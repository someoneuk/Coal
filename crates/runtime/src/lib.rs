//! Injected library. Loaded into the target via `LoadLibraryA` by the launcher.
//! DllMain stays tiny: an IPC server thread hosts the Luau VM.

#![allow(non_snake_case)]
#![allow(non_camel_case_types)]

mod vm;

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write;

use coal_shared::ipc::{Pipe, ERR_PREFIX, OK_PREFIX};
use vm::Vm;

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

fn log_line(line: &str) {
    let mut path = std::env::temp_dir();
    path.push("coal-runtime.log");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

unsafe extern "system" fn worker(_param: *mut c_void) -> u32 {
    let pid = std::process::id();
    log_line(&format!("[runtime] attached pid={pid}"));

    let vm = match Vm::new() {
        Ok(vm) => vm,
        Err(err) => {
            log_line(&format!("[runtime] VM init failed: {err}"));
            return 0;
        }
    };

    let server = match Pipe::server(pid) {
        Ok(server) => server,
        Err(err) => {
            log_line(&format!("[runtime] IPC init failed: {err}"));
            return 0;
        }
    };
    log_line(&format!("[runtime] listening for scripts"));

    loop {
        if let Err(err) = server.accept() {
            log_line(&format!("[runtime] accept failed: {err}"));
            break;
        }

        match server.read_frame() {
            Ok(frame) => {
                let source = String::from_utf8_lossy(&frame);
                log_line(&format!("[runtime] executing {} bytes", frame.len()));
                let response = match vm.run(&source) {
                    Ok(value) => format!("{OK_PREFIX}{value}"),
                    Err(err) => format!("{ERR_PREFIX}{err}"),
                };
                if let Err(err) = server.write_frame(response.as_bytes()) {
                    log_line(&format!("[runtime] response write failed: {err}"));
                }
            }
            Err(err) => {
                log_line(&format!("[runtime] frame read failed: {err}"));
            }
        }

        server.disconnect();
    }
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
            log_line("[runtime] detached");
        }
        _ => {}
    }
    1
}
