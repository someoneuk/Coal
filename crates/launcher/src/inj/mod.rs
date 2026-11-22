pub mod inject;
pub mod manual;
pub mod process;

#[allow(unused_imports)]
pub use inject::{
    inject_dll, inject_dll_apc, inject_dll_hijack, inject_dll_hijack_all, inject_dll_nt,
    load_library_bytes, InjectResult,
};
pub use manual::manual_map;
#[allow(unused_imports)]
pub use process::{find_process, ProcessInfo};
