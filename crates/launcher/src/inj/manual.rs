//! Generic x64 PE manual mapper. Maps a DLL into a target process without
//! `LoadLibrary`, processes relocations and imports, then runs TLS callbacks
//! and the entry point through a hijacked thread.
//!
//! Note: protected targets with page-integrity checks (e.g. modern Roblox)
//! additionally require whitelisting the mapped pages; that step is not here.

use std::ffi::{c_char, c_void, CString};
use std::path::Path;
use std::thread;
use std::time::Duration;

use super::inject::{
    hijack_execute_table, win::*, HandleGuard, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE,
    PAGE_EXECUTE_READWRITE, PAGE_READWRITE, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION,
    PROCESS_VM_READ, PROCESS_VM_WRITE,
};
use crate::inj::inject::InjectResult;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: LPCSTR) -> HMODULE;
    fn GetProcAddress(module: HMODULE, name: LPCSTR) -> FARPROC;
}

const IMAGE_DIRECTORY_ENTRY_IMPORT: usize = 1;
const IMAGE_DIRECTORY_ENTRY_BASERELOC: usize = 5;
const IMAGE_DIRECTORY_ENTRY_TLS: usize = 9;
const DLL_PROCESS_ATTACH: u64 = 1;

fn u16_at(data: &[u8], offset: usize) -> Result<u16, String> {
    data.get(offset..offset + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| "truncated PE".into())
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32, String> {
    data.get(offset..offset + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| "truncated PE".into())
}

fn u64_at(data: &[u8], offset: usize) -> Result<u64, String> {
    data.get(offset..offset + 8)
        .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
        .ok_or_else(|| "truncated PE".into())
}

fn set_u64(data: &mut [u8], offset: usize, value: u64) -> Result<(), String> {
    let bytes = value.to_le_bytes();
    let slot = data
        .get_mut(offset..offset + 8)
        .ok_or_else(|| "truncated PE".to_string())?;
    slot.copy_from_slice(&bytes);
    Ok(())
}

struct PeInfo {
    image_base: u64,
    size_of_image: usize,
    size_of_headers: usize,
    entry_rva: u32,
    sections: Vec<(u32, u32, u32)>, // RVA, raw size, raw pointer
    import_rva: u32,
    import_size: u32,
    reloc_rva: u32,
    reloc_size: u32,
    tls_rva: u32,
    tls_size: u32,
}

fn parse_pe(data: &[u8]) -> Result<PeInfo, String> {
    if u16_at(data, 0)? != 0x5A4D {
        return Err("not a PE file".into());
    }
    let nt = u32_at(data, 0x3C)? as usize;
    if u32_at(data, nt)? != 0x0000_4550 {
        return Err("bad PE signature".into());
    }
    let num_sections = u16_at(data, nt + 6)? as usize;
    let size_of_optional = u16_at(data, nt + 20)? as usize;
    let opt = nt + 24;
    if u16_at(data, opt)? != 0x020B {
        return Err("not a PE32+ image".into());
    }

    let mut info = PeInfo {
        image_base: u64_at(data, opt + 24)?,
        size_of_image: u32_at(data, opt + 56)? as usize,
        size_of_headers: u32_at(data, opt + 60)? as usize,
        entry_rva: u32_at(data, opt + 16)?,
        sections: Vec::new(),
        import_rva: 0,
        import_size: 0,
        reloc_rva: 0,
        reloc_size: 0,
        tls_rva: 0,
        tls_size: 0,
    };

    let dirs = opt + 112;
    info.import_rva = u32_at(data, dirs + IMAGE_DIRECTORY_ENTRY_IMPORT * 8)?;
    info.import_size = u32_at(data, dirs + IMAGE_DIRECTORY_ENTRY_IMPORT * 8 + 4)?;
    info.reloc_rva = u32_at(data, dirs + IMAGE_DIRECTORY_ENTRY_BASERELOC * 8)?;
    info.reloc_size = u32_at(data, dirs + IMAGE_DIRECTORY_ENTRY_BASERELOC * 8 + 4)?;
    info.tls_rva = u32_at(data, dirs + IMAGE_DIRECTORY_ENTRY_TLS * 8)?;
    info.tls_size = u32_at(data, dirs + IMAGE_DIRECTORY_ENTRY_TLS * 8 + 4)?;

    let section_base = opt + size_of_optional;
    for index in 0..num_sections {
        let header = section_base + index * 40;
        info.sections.push((
            u32_at(data, header + 12)?,
            u32_at(data, header + 16)?,
            u32_at(data, header + 20)?,
        ));
    }
    Ok(info)
}

fn cstr_at(image: &[u8], rva: u32) -> Result<String, String> {
    let start = rva as usize;
    let end = image[start..]
        .iter()
        .position(|byte| *byte == 0)
        .map(|offset| start + offset)
        .ok_or_else(|| "unterminated string in image".to_string())?;
    Ok(String::from_utf8_lossy(&image[start..end]).into_owned())
}

fn relocate(image: &mut [u8], info: &PeInfo, delta: i64) -> Result<(), String> {
    if delta == 0 || info.reloc_size == 0 {
        return Ok(());
    }
    let mut cursor = info.reloc_rva as usize;
    let end = cursor + info.reloc_size as usize;
    while cursor < end {
        let page_rva = u32_at(image, cursor)? as usize;
        let block_size = u32_at(image, cursor + 4)? as usize;
        if block_size == 0 || cursor + block_size > image.len() {
            break;
        }
        let mut entry = cursor + 8;
        let block_end = cursor + block_size;
        while entry + 2 <= block_end {
            let value = u16_at(image, entry)?;
            let kind = value >> 12;
            let offset = (value & 0x0FFF) as usize;
            if kind == 10 {
                // IMAGE_REL_BASED_DIR64
                let address = page_rva + offset;
                let current = u64_at(image, address)?;
                set_u64(image, address, current.wrapping_add(delta as u64))?;
            }
            entry += 2;
        }
        cursor += block_size;
    }
    Ok(())
}

unsafe fn resolve_imports(image: &mut [u8], info: &PeInfo) -> Result<(), String> {
    if info.import_size == 0 {
        return Ok(());
    }
    let mut descriptor = info.import_rva as usize;
    loop {
        let original_thunk = u32_at(image, descriptor)?;
        let first_thunk = u32_at(image, descriptor + 16)?;
        let name_rva = u32_at(image, descriptor + 12)?;
        if original_thunk == 0 && first_thunk == 0 && name_rva == 0 {
            break;
        }

        let dll_name = CString::new(cstr_at(image, name_rva)?).map_err(|e| e.to_string())?;
        let module = LoadLibraryA(dll_name.as_ptr() as *const c_char);
        if module.is_null() {
            return Err(format!("could not load dependency {}", dll_name.to_string_lossy()));
        }

        let lookup = if original_thunk != 0 {
            original_thunk as usize
        } else {
            first_thunk as usize
        };
        let mut index = 0usize;
        loop {
            let thunk = u64_at(image, lookup + index * 8)?;
            if thunk == 0 {
                break;
            }
            let address = if thunk & (1u64 << 63) != 0 {
                let ordinal = (thunk & 0xFFFF) as usize;
                GetProcAddress(module, ordinal as LPCSTR)
            } else {
                let import = cstr_at(image, (thunk & 0x7FFF_FFFF) as u32 + 2)?;
                let name = CString::new(import).map_err(|e| e.to_string())?;
                GetProcAddress(module, name.as_ptr() as *const c_char)
            };
            set_u64(
                image,
                first_thunk as usize + index * 8,
                address.map(|f| f as usize).unwrap_or(0) as u64,
            )?;
            index += 1;
        }
        descriptor += 20;
    }
    Ok(())
}

fn tls_callbacks(image: &[u8], info: &PeInfo, target_base: u64) -> Result<Vec<u64>, String> {
    if info.tls_size == 0 || info.tls_rva == 0 {
        return Ok(Vec::new());
    }
    // IMAGE_TLS_DIRECTORY64: AddressOfCallBacks is the third u64 field.
    let callbacks_va = u64_at(image, info.tls_rva as usize + 16)?;
    if callbacks_va == 0 {
        return Ok(Vec::new());
    }
    // This field is a VA (absolute), already relocated to the target base.
    let mut callbacks = Vec::new();
    let mut offset = callbacks_va.wrapping_sub(target_base) as usize;
    loop {
        let callback = u64_at(image, offset)?;
        if callback == 0 {
            break;
        }
        callbacks.push(callback);
        offset += 8;
    }
    Ok(callbacks)
}

pub fn manual_map(pid: u32, dll_path: &Path) -> Result<InjectResult, String> {
    let file = std::fs::read(dll_path).map_err(|e| format!("cannot read DLL: {e}"))?;
    let info = parse_pe(&file)?;

    unsafe {
        let process = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
            0,
            pid,
        );
        if process.is_null() {
            return Err(format!("OpenProcess failed ({})", GetLastError()));
        }
        let _process = HandleGuard(process);

        let target_base = VirtualAllocEx(
            process,
            std::ptr::null_mut(),
            info.size_of_image,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_EXECUTE_READWRITE,
        );
        if target_base.is_null() {
            return Err(format!("VirtualAllocEx failed ({})", GetLastError()));
        }

        let mut image = vec![0u8; info.size_of_image];
        let header_len = info.size_of_headers.min(file.len()).min(image.len());
        image[..header_len].copy_from_slice(&file[..header_len]);

        for (rva, raw_size, raw_pointer) in &info.sections {
            let size = *raw_size as usize;
            let start = *rva as usize;
            let source = *raw_pointer as usize;
            if start + size <= image.len() && source + size <= file.len() {
                image[start..start + size].copy_from_slice(&file[source..source + size]);
            }
        }

        let delta = target_base as i64 - info.image_base as i64;
        relocate(&mut image, &info, delta)?;
        resolve_imports(&mut image, &info)?;

        let entry_point = target_base as u64 + info.entry_rva as u64;
        let mut calls: Vec<[u64; 4]> = tls_callbacks(&image, &info, target_base as u64)?
            .into_iter()
            .map(|callback| [callback, target_base as u64, DLL_PROCESS_ATTACH, 0])
            .collect();
        calls.push([entry_point, target_base as u64, DLL_PROCESS_ATTACH, 0]);
        calls.push([0, 0, 0, 0]);

        let table_bytes: Vec<u8> = calls
            .iter()
            .flat_map(|entry| entry.iter().flat_map(|value| value.to_le_bytes()))
            .collect();
        let table = VirtualAllocEx(
            process,
            std::ptr::null_mut(),
            table_bytes.len(),
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if table.is_null() {
            VirtualFreeEx(process, target_base, 0, MEM_RELEASE);
            return Err(format!("table allocation failed ({})", GetLastError()));
        }

        let mut written = 0usize;
        let wrote_image = WriteProcessMemory(
            process,
            target_base,
            image.as_ptr() as *const c_void,
            image.len(),
            &mut written,
        ) != 0;
        let wrote_table = WriteProcessMemory(
            process,
            table,
            table_bytes.as_ptr() as *const c_void,
            table_bytes.len(),
            &mut written,
        ) != 0;

        if !wrote_image || !wrote_table {
            VirtualFreeEx(process, table, 0, MEM_RELEASE);
            VirtualFreeEx(process, target_base, 0, MEM_RELEASE);
            return Err(format!("WriteProcessMemory failed ({})", GetLastError()));
        }

        hijack_execute_table(process, pid, table)?;

        // The mapped image and call table are intentionally left allocated.
        for _ in 0..20 {
            if coal_shared::ipc::Pipe::connect(pid).is_ok() {
                return Ok(InjectResult {
                    pid,
                    module_base: target_base as u32,
                });
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err("mapped DLL did not start its runtime".into())
    }
}
