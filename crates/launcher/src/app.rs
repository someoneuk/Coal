//! Win32 launcher UI. Standard controls only, so there is no GUI framework
//! dependency and the whole launcher builds in seconds.

use std::path::Path;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;

use coal_shared::ipc::{self, Response};

use crate::inj::{find_process, inject_dll};
use crate::log::LogMsg;
use crate::win32::*;

static LOG_TX: OnceLock<Sender<LogMsg>> = OnceLock::new();
static MAIN_WND: AtomicIsize = AtomicIsize::new(0);

const ID_PROCESS: i32 = 100;
const ID_REFRESH: i32 = 101;
const ID_STATUS: i32 = 102;
const ID_DLL: i32 = 103;
const ID_BROWSE: i32 = 104;
const ID_INJECT: i32 = 105;
const ID_SCRIPT: i32 = 106;
const ID_RUN: i32 = 107;
const ID_LOG: i32 = 108;

pub fn run() {
    let (tx, rx) = mpsc::channel::<LogMsg>();
    let _ = LOG_TX.set(tx);
    std::thread::spawn(move || log_worker(rx));

    let hwnd = create_window();
    if hwnd.is_null() {
        return;
    }
    MAIN_WND.store(hwnd as isize, Ordering::SeqCst);

    emit("INFO", "Launcher ready.");
    refresh_status(hwnd);

    unsafe {
        ShowWindow(hwnd, 5); // SW_SHOW
        UpdateWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn emit(tag: &'static str, text: impl Into<String>) {
    if let Some(tx) = LOG_TX.get() {
        let _ = tx.send(LogMsg::new(tag, text));
    }
}

fn log_worker(rx: Receiver<LogMsg>) {
    while let Ok(msg) = rx.recv() {
        let raw = MAIN_WND.load(Ordering::SeqCst);
        if raw == 0 {
            continue;
        }
        let boxed = Box::new(msg);
        let ptr = Box::into_raw(boxed) as isize;
        unsafe {
            PostMessageW(raw as HWND, WM_APP_LOG, 0, ptr);
        }
    }
}

fn create_window() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let mut wcx: WNDCLASSEXW = std::mem::zeroed();
        wcx.cb_size = std::mem::size_of::<WNDCLASSEXW>() as UINT;
        wcx.style = CS_HREDRAW | CS_VREDRAW;
        wcx.lpfn_wnd_proc = Some(wnd_proc);
        wcx.h_instance = instance;
        wcx.h_cursor = LoadCursorW(std::ptr::null_mut(), idc_arrow());
        wcx.hbr_background = (COLOR_WINDOW + 1) as usize as HBRUSH;
        let class = wide("CoalExecutor");
        wcx.lpsz_class_name = class.as_ptr();
        RegisterClassExW(&wcx);

        let title = wide("Coal Executor");
        CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            100,
            100,
            920,
            830,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null_mut(),
        )
    }
}

fn add_control(
    parent: HWND,
    class: &str,
    text: &str,
    style: DWORD,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    let class_w = wide(class);
    let text_w = wide(text);
    unsafe {
        CreateWindowExW(
            0,
            class_w.as_ptr(),
            text_w.as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            x,
            y,
            width,
            height,
            parent,
            id as usize as HMENU,
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null_mut(),
        );
    }
}

fn create_controls(hwnd: HWND) {
    add_control(hwnd, "STATIC", "Target process:", SS_LEFT, 0, 20, 18, 120, 22);
    add_control(
        hwnd,
        "EDIT",
        "RobloxPlayerBeta",
        WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL,
        ID_PROCESS,
        150,
        16,
        300,
        24,
    );
    add_control(
        hwnd,
        "BUTTON",
        "Refresh",
        WS_TABSTOP,
        ID_REFRESH,
        460,
        14,
        110,
        28,
    );
    add_control(hwnd, "STATIC", "NOT RUNNING", SS_LEFT, ID_STATUS, 20, 52, 800, 24);

    add_control(hwnd, "STATIC", "DLL:", SS_LEFT, 0, 20, 90, 120, 22);
    add_control(
        hwnd,
        "EDIT",
        "",
        WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL,
        ID_DLL,
        150,
        88,
        520,
        24,
    );
    add_control(
        hwnd,
        "BUTTON",
        "Browse...",
        WS_TABSTOP,
        ID_BROWSE,
        680,
        86,
        110,
        28,
    );
    add_control(
        hwnd,
        "BUTTON",
        "Inject",
        WS_TABSTOP,
        ID_INJECT,
        20,
        128,
        140,
        32,
    );

    add_control(hwnd, "STATIC", "Script", SS_LEFT, 0, 20, 174, 100, 22);
    add_control(
        hwnd,
        "BUTTON",
        "Run",
        WS_TABSTOP,
        ID_RUN,
        770,
        171,
        110,
        28,
    );
    add_control(
        hwnd,
        "EDIT",
        "print(\"hello from coal\")\r\nreturn 1 + 1",
        WS_BORDER | WS_VSCROLL | WS_TABSTOP | ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN,
        ID_SCRIPT,
        20,
        202,
        860,
        250,
    );

    add_control(hwnd, "STATIC", "Log", SS_LEFT, 0, 20, 466, 100, 22);
    add_control(
        hwnd,
        "EDIT",
        "",
        WS_BORDER | WS_VSCROLL | ES_MULTILINE | ES_READONLY,
        ID_LOG,
        20,
        492,
        860,
        230,
    );
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: UINT,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            create_controls(hwnd);
            0
        }
        WM_APP_LOG => {
            let entry = Box::from_raw(l_param as *mut LogMsg);
            append_log(hwnd, &entry);
            if entry.text.starts_with("Injected") || entry.text.starts_with("Injection failed") {
                EnableWindow(GetDlgItem(hwnd, ID_INJECT), 1);
            }
            if entry.text.starts_with('>')
                || entry.text.starts_with("Lua error:")
                || entry.text.starts_with("IPC error:")
            {
                EnableWindow(GetDlgItem(hwnd, ID_RUN), 1);
            }
            0
        }
        WM_COMMAND => {
            let id = (w_param as u32 & 0xffff) as i32;
            match id {
                ID_REFRESH => refresh_status(hwnd),
                ID_BROWSE => browse(hwnd),
                ID_INJECT => request_inject(hwnd),
                ID_RUN => request_run(hwnd),
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w_param, l_param),
    }
}

fn set_text(hwnd: HWND, id: i32, text: &str) {
    unsafe {
        SetWindowTextW(GetDlgItem(hwnd, id), wide(text).as_ptr());
    }
}

fn get_text(hwnd: HWND, id: i32) -> String {
    unsafe {
        let control = GetDlgItem(hwnd, id);
        let len = GetWindowTextLengthW(control);
        let mut buffer = vec![0u16; len as usize + 1];
        GetWindowTextW(control, buffer.as_mut_ptr(), len + 1);
        String::from_utf16_lossy(&buffer[..len as usize])
    }
}

fn append_log(hwnd: HWND, entry: &LogMsg) {
    let control = unsafe { GetDlgItem(hwnd, ID_LOG) };
    let len = unsafe { GetWindowTextLengthW(control) };
    let line = format!("[{}] [{}] {}\r\n", entry.time, entry.tag, entry.text);
    let wide_line: Vec<u16> = line.encode_utf16().collect();
    unsafe {
        SendMessageW(control, EM_SETSEL, len as WPARAM, len as LPARAM);
        SendMessageW(control, EM_REPLACESEL, 0, wide_line.as_ptr() as LPARAM);
        SendMessageW(control, EM_SCROLLCARET, 0, 0);
    }
}

fn refresh_status(hwnd: HWND) {
    let name = get_text(hwnd, ID_PROCESS);
    match find_process(&name) {
        Some(info) => {
            set_text(
                hwnd,
                ID_STATUS,
                &format!("RUNNING   {}  ·  PID {}", info.name, info.pid),
            );
            emit(
                " OK ",
                format!("Target found: {} (PID {})", info.name, info.pid),
            );
        }
        None => {
            set_text(hwnd, ID_STATUS, "NOT RUNNING");
            emit("WARN", format!("No live process matching '{name}'"));
        }
    }
}

fn browse(hwnd: HWND) {
    let mut file = [0i8; MAX_PATH];
    let filter = b"DLL files\0*.dll\0All files\0*.*\0\0";
    let title = b"Select DLL\0";
    let mut dialog: OPENFILENAMEA = unsafe { std::mem::zeroed() };
    dialog.l_struct_size = std::mem::size_of::<OPENFILENAMEA>() as DWORD;
    dialog.hwnd_owner = hwnd;
    dialog.lpstr_filter = filter.as_ptr() as *const i8;
    dialog.lpstr_file = file.as_mut_ptr();
    dialog.n_max_file = MAX_PATH as DWORD;
    dialog.lpstr_title = title.as_ptr() as *const i8;
    dialog.flags = OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST;

    unsafe {
        if GetOpenFileNameA(&mut dialog) != 0 {
            let path = std::ffi::CStr::from_ptr(file.as_ptr())
                .to_string_lossy()
                .into_owned();
            set_text(hwnd, ID_DLL, &path);
            emit("INFO", format!("Selected: {path}"));
        }
    }
}

fn request_inject(hwnd: HWND) {
    let target = get_text(hwnd, ID_PROCESS);
    let dll = get_text(hwnd, ID_DLL);

    let info = match find_process(&target) {
        Some(info) => info,
        None => {
            emit("ERR ", "No target process detected. Hit Refresh.");
            return;
        }
    };
    if dll.trim().is_empty() || !Path::new(&dll).is_file() {
        emit("ERR ", "Choose a valid DLL path first.");
        return;
    }

    unsafe {
        EnableWindow(GetDlgItem(hwnd, ID_INJECT), 0);
    }
    std::thread::spawn(move || {
        emit(
            "INFO",
            format!("Injecting into {} (PID {})...", info.name, info.pid),
        );
        match inject_dll(info.pid, Path::new(&dll)) {
            Ok(result) => emit(
                " OK ",
                format!(
                    "Injected into PID {}. Remote module base 0x{:X}",
                    result.pid, result.module_base
                ),
            ),
            Err(err) => emit("ERR ", format!("Injection failed: {err}")),
        }
    });
}

fn request_run(hwnd: HWND) {
    let target = get_text(hwnd, ID_PROCESS);
    let script = get_text(hwnd, ID_SCRIPT);

    let info = match find_process(&target) {
        Some(info) => info,
        None => {
            emit("ERR ", "No target process detected. Hit Refresh.");
            return;
        }
    };
    if script.trim().is_empty() {
        emit("WARN", "Nothing to run — the script editor is empty.");
        return;
    }

    unsafe {
        EnableWindow(GetDlgItem(hwnd, ID_RUN), 0);
    }
    std::thread::spawn(move || match ipc::execute(info.pid, &script) {
        Ok(Response::Ok(value)) => emit(" OK ", format!("> {value}")),
        Ok(Response::Err(err)) => emit("ERR ", format!("Lua error: {err}")),
        Err(err) => emit("ERR ", format!("IPC error: {err}")),
    });
}
