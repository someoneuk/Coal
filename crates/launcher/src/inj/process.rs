use sysinfo::{ProcessesToUpdate, System};

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

    let Ok(mut system) = System::new() else {
        return None;
    };
    system.refresh_processes(ProcessesToUpdate::All, true);

    let mut best: Option<ProcessInfo> = None;
    for (pid, process) in system.processes() {
        let name = process.name().to_string_lossy().into_owned();
        if normalize(&name) == needle {
            let info = ProcessInfo {
                pid: pid.as_u32(),
                name,
            };
            // Prefer the lowest PID for deterministic behaviour.
            if best.as_ref().map(|b| info.pid < b.pid).unwrap_or(true) {
                best = Some(info);
            }
        }
    }
    best
}

fn normalize(name: &str) -> String {
    name.trim().trim_end_matches(".exe").to_ascii_lowercase()
}
