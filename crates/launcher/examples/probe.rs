//! Probe a live process: inject runtime.dll and try IPC.
//! Usage: cargo run --release -p launcher --example probe -- <pid>

use std::path::Path;
use std::thread;
use std::time::Duration;

use coal_launcher::inj::{
    inject_dll, inject_dll_apc, inject_dll_hijack, inject_dll_hijack_all, inject_dll_nt,
    load_library_bytes,
};
use coal_shared::ipc::{self, Response};

fn main() {
    let pid: u32 = std::env::args()
        .nth(1)
        .expect("pass a target PID")
        .parse()
        .expect("PID must be numeric");

    if std::env::args().any(|arg| arg == "--ll") {
        match load_library_bytes(pid) {
            Ok(bytes) => println!("target LoadLibraryA: {bytes}"),
            Err(err) => println!("LoadLibraryA read failed: {err}"),
        }
        return;
    }

    let dll = Path::new("target/release/runtime.dll")
        .canonicalize()
        .expect("build the workspace first");

    let use_nt = std::env::args().any(|arg| arg == "--nt");
    let use_apc = std::env::args().any(|arg| arg == "--apc");
    let use_hijack = std::env::args().any(|arg| arg == "--hijack");
    let use_hijack_all = std::env::args().any(|arg| arg == "--hijack-all");
    println!(
        "dll: {} (nt={use_nt}, apc={use_apc}, hijack={use_hijack}, all={use_hijack_all})",
        dll.display()
    );
    let injected = if use_hijack_all {
        inject_dll_hijack_all(pid, &dll)
    } else if use_hijack {
        inject_dll_hijack(pid, &dll)
    } else if use_apc {
        inject_dll_apc(pid, &dll)
    } else if use_nt {
        inject_dll_nt(pid, &dll)
    } else {
        inject_dll(pid, &dll)
    };
    match injected {
        Ok(result) => println!("inject OK: base=0x{:X}", result.module_base),
        Err(err) => {
            println!("inject FAILED: {err}");
            return;
        }
    }

    for attempt in 0..20 {
        match ipc::execute(pid, "return 40 + 2") {
            Ok(Response::Ok(value)) => {
                println!("IPC OK on attempt {attempt}: {value}");
                return;
            }
            Ok(Response::Err(err)) => {
                println!("Lua error: {err}");
                return;
            }
            Err(err) => {
                if attempt == 0 || attempt == 19 {
                    println!("IPC attempt {attempt}: {err}");
                }
                thread::sleep(Duration::from_millis(250));
            }
        }
    }
    println!("IPC unreachable");
}
