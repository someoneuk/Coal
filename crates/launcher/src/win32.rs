//! Raw Win32 declarations used by the launcher. No GUI framework deps.

#![allow(non_camel_case_types, non_snake_case, dead_code)]

use std::ffi::{c_char, c_void};

pub type HANDLE = *mut c_void;
pub type HWND = *mut c_void;
pub type HINSTANCE = *mut c_void;
pub type HBRUSH = *mut c_void;
pub type HCURSOR = *mut c_void;
pub type HICON = *mut c_void;
pub type HMENU = *mut c_void;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type UINT = u32;
pub type DWORD = u32;
pub type BOOL = i32;
pub type WORD = u16;
pub type ATOM = u16;
pub type LPCSTR = *const c_char;
pub type LPSTR = *mut c_char;
pub type LPCWSTR = *const u16;
pub type LPWSTR = *mut u16;

pub type WndProc = unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT;

pub const WM_CREATE: UINT = 0x0001;
pub const WM_DESTROY: UINT = 0x0002;
pub const WM_CLOSE: UINT = 0x0010;
pub const WM_COMMAND: UINT = 0x0111;
pub const WM_SETTEXT: UINT = 0x000C;
pub const WM_APP_LOG: UINT = 0x8001;

pub const EM_SETSEL: UINT = 0x00B1;
pub const EM_SCROLLCARET: UINT = 0x00B7;
pub const EM_REPLACESEL: UINT = 0x00C2;

pub const WS_BORDER: DWORD = 0x0080_0000;
pub const WS_CHILD: DWORD = 0x4000_0000;
pub const WS_VISIBLE: DWORD = 0x1000_0000;
pub const WS_VSCROLL: DWORD = 0x0020_0000;
pub const WS_TABSTOP: DWORD = 0x0001_0000;
pub const WS_OVERLAPPEDWINDOW: DWORD = 0x00CF_0000;

pub const ES_MULTILINE: DWORD = 0x0004;
pub const ES_AUTOVSCROLL: DWORD = 0x0040;
pub const ES_AUTOHSCROLL: DWORD = 0x0080;
pub const ES_WANTRETURN: DWORD = 0x1000;
pub const ES_READONLY: DWORD = 0x0800;

pub const SS_LEFT: DWORD = 0;

pub const CS_HREDRAW: UINT = 0x0002;
pub const CS_VREDRAW: UINT = 0x0001;
pub fn idc_arrow() -> LPCWSTR {
    32512usize as LPCWSTR
}
pub const COLOR_WINDOW: i32 = 5;

pub const TH32CS_SNAPPROCESS: DWORD = 0x0000_0002;
pub const MAX_PATH: usize = 260;

pub const OFN_PATHMUSTEXIST: DWORD = 0x0000_0800;
pub const OFN_FILEMUSTEXIST: DWORD = 0x0000_1000;

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[repr(C)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: UINT,
    pub w_param: WPARAM,
    pub l_param: LPARAM,
    pub time: DWORD,
    pub pt: POINT,
}

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cb_size: UINT,
    pub style: UINT,
    pub lpfn_wnd_proc: Option<WndProc>,
    pub cb_cls_extra: i32,
    pub cb_wnd_extra: i32,
    pub h_instance: HINSTANCE,
    pub h_icon: HICON,
    pub h_cursor: HCURSOR,
    pub hbr_background: HBRUSH,
    pub lpsz_menu_name: LPCWSTR,
    pub lpsz_class_name: LPCWSTR,
    pub h_icon_sm: HICON,
}

#[repr(C)]
pub struct PROCESSENTRY32A {
    pub dw_size: DWORD,
    pub cnt_usage: DWORD,
    pub th32_process_id: DWORD,
    pub th32_default_heap_id: usize,
    pub th32_module_id: DWORD,
    pub cnt_threads: DWORD,
    pub th32_parent_process_id: DWORD,
    pub pc_pri_class_base: i32,
    pub dw_flags: DWORD,
    pub sz_exe_file: [c_char; MAX_PATH],
}

#[repr(C)]
pub struct OPENFILENAMEA {
    pub l_struct_size: DWORD,
    pub hwnd_owner: HWND,
    pub h_instance: HINSTANCE,
    pub lpstr_filter: LPCSTR,
    pub lpstr_custom_filter: LPSTR,
    pub n_max_cust_filter: DWORD,
    pub n_filter_index: DWORD,
    pub lpstr_file: LPSTR,
    pub n_max_file: DWORD,
    pub lpstr_file_title: LPSTR,
    pub n_max_file_title: DWORD,
    pub lpstr_initial_dir: LPCSTR,
    pub lpstr_title: LPCSTR,
    pub flags: DWORD,
    pub n_file_offset: WORD,
    pub n_file_extension: WORD,
    pub lpstr_def_ext: LPCSTR,
    pub l_cust_data: LPARAM,
    pub lpfn_hook: *mut c_void,
    pub lp_template_name: LPCSTR,
    pub pv_reserved: *mut c_void,
    pub dw_reserved: DWORD,
    pub flags_ex: DWORD,
}

#[link(name = "user32")]
extern "system" {
    pub fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> ATOM;
    pub fn CreateWindowExW(
        dw_ex_style: DWORD,
        lp_class_name: LPCWSTR,
        lp_window_name: LPCWSTR,
        dw_style: DWORD,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        hwnd_parent: HWND,
        h_menu: HMENU,
        h_instance: HINSTANCE,
        lp_param: *mut c_void,
    ) -> HWND;
    pub fn DefWindowProcW(hwnd: HWND, msg: UINT, w_param: WPARAM, l_param: LPARAM) -> LRESULT;
    pub fn GetMessageW(lp_msg: *mut MSG, hwnd: HWND, min: UINT, max: UINT) -> BOOL;
    pub fn TranslateMessage(lp_msg: *const MSG) -> BOOL;
    pub fn DispatchMessageW(lp_msg: *const MSG) -> LRESULT;
    pub fn PostMessageW(hwnd: HWND, msg: UINT, w_param: WPARAM, l_param: LPARAM) -> BOOL;
    pub fn SendMessageW(hwnd: HWND, msg: UINT, w_param: WPARAM, l_param: LPARAM) -> LRESULT;
    pub fn PostQuitMessage(exit_code: i32);
    pub fn ShowWindow(hwnd: HWND, cmd: i32) -> BOOL;
    pub fn UpdateWindow(hwnd: HWND) -> BOOL;
    pub fn GetDlgItem(hwnd: HWND, id: i32) -> HWND;
    pub fn SetWindowTextW(hwnd: HWND, text: LPCWSTR) -> BOOL;
    pub fn GetWindowTextLengthW(hwnd: HWND) -> i32;
    pub fn GetWindowTextW(hwnd: HWND, buffer: LPWSTR, max_count: i32) -> i32;
    pub fn EnableWindow(hwnd: HWND, enable: BOOL) -> BOOL;
    pub fn IsWindowEnabled(hwnd: HWND) -> BOOL;
    pub fn LoadCursorW(h_instance: HINSTANCE, lp_cursor_name: LPCWSTR) -> HCURSOR;
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetModuleHandleW(lp_module_name: LPCWSTR) -> HINSTANCE;
    pub fn CreateToolhelp32Snapshot(dw_flags: DWORD, th32_process_id: DWORD) -> HANDLE;
    pub fn Process32First(h_snapshot: HANDLE, lppe: *mut PROCESSENTRY32A) -> BOOL;
    pub fn Process32Next(h_snapshot: HANDLE, lppe: *mut PROCESSENTRY32A) -> BOOL;
    pub fn CloseHandle(h_object: HANDLE) -> BOOL;
}

#[link(name = "comdlg32")]
extern "system" {
    pub fn GetOpenFileNameA(lpofn: *mut OPENFILENAMEA) -> BOOL;
    pub fn CommDlgExtendedError() -> DWORD;
}
