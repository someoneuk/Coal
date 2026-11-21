//! End-to-end smoke test: inject the runtime into Notepad, run Lua in it,
//! then clean up. Run from the workspace root:
//!   cargo run --release -p launcher --example e2e

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use coal_launcher::inj::inject_dll;
use coal_shared::ipc::{self, Response};

struct Target(Child);

impl Drop for Target {
    fn drop(&mut self) {
        let _ = Command::new("taskkill")
            .args(["/PID", &self.0.id().to_string(), "/F"])
            .status();
    }
}

fn main() {
    let dll = Path::new("target/release/runtime.dll")
        .canonicalize()
        .expect("build the workspace first");

    let child = Command::new("notepad.exe")
        .stdin(Stdio::null())
        .spawn()
        .expect("spawn notepad");
    let target = Target(child);
    let pid = target.0.id();
    println!("target pid: {pid}");
    thread::sleep(Duration::from_millis(800));

    let result = inject_dll(pid, &dll);
    println!("inject: {result:?}");
    assert!(result.is_ok(), "injection failed");

    let mut response = None;
    for _ in 0..40 {
        match ipc::execute(pid, "return 40 + 2") {
            Ok(Response::Ok(value)) => {
                response = Some(value);
                break;
            }
            Ok(Response::Err(err)) => panic!("lua error: {err}"),
            Err(_) => thread::sleep(Duration::from_millis(250)),
        }
    }

    match response {
        Some(value) => {
            println!("lua result: {value}");
            assert_eq!(value, "42");
            println!("E2E PASSED");
        }
        None => panic!("runtime never became reachable over IPC"),
    }
}
