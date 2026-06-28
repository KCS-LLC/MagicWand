use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, UpdateKind};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Memory::{
    VirtualProtectEx, VirtualQueryEx,
    PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS,
    MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_NOACCESS, PAGE_GUARD,
};
use windows::Win32::System::ProcessStatus::{EnumProcessModules, GetModuleBaseNameW, GetModuleInformation, MODULEINFO};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};

struct ProcessHandle(HANDLE);

impl ProcessHandle {
    fn open(pid: u32) -> Result<Self, String> {
        unsafe {
            OpenProcess(PROCESS_ALL_ACCESS, false, pid)
                .map(ProcessHandle)
                .map_err(|e| format!("Failed to open process: {}", e))
        }
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe { let _ = CloseHandle(self.0); }
    }
}

pub fn get_module_info(pid: u32, module_name: &str) -> Option<(usize, usize)> {
    unsafe {
        let handle = ProcessHandle::open(pid).ok()?;
        let mut modules = [Default::default(); 1024];
        let mut cb_needed = 0;

        if EnumProcessModules(handle.0, modules.as_mut_ptr(), std::mem::size_of_val(&modules) as u32, &mut cb_needed).is_err() {
            return None;
        }

        let count = cb_needed as usize / std::mem::size_of::<HANDLE>();
        let target = module_name.to_lowercase();
        for i in 0..count {
            let mut name = [0u16; 256];
            let len = GetModuleBaseNameW(handle.0, Some(modules[i]), &mut name);
            if len > 0 {
                let current_name = String::from_utf16_lossy(&name[..len as usize]).to_lowercase();
                if current_name == target {
                    let mut info = MODULEINFO::default();
                    if GetModuleInformation(handle.0, modules[i], &mut info, std::mem::size_of::<MODULEINFO>() as u32).is_ok() {
                        return Some((modules[i].0 as usize, info.SizeOfImage as usize));
                    }
                }
            }
        }
        None
    }
}

pub fn aob_scan(pid: u32, module_name: &str, pattern: &str) -> Result<usize, String> {
    let (base, size) = get_module_info(pid, module_name)
        .ok_or_else(|| format!("Could not find module {}", module_name))?;
    crate::mwlog!("[aob_scan] '{}' base=0x{:X} size=0x{:X} pattern={}", module_name, base, size, pattern);
    let result = aob_scan_range(pid, base, size, pattern)
        .ok_or_else(|| "Pattern not found".to_string());
    match &result {
        Ok(addr) => crate::mwlog!("[aob_scan] found at 0x{:X} (RVA=0x{:X})", addr, addr.wrapping_sub(base)),
        Err(e) => crate::mwlog!("[aob_scan] NOT found: {}", e),
    }
    result
}

pub fn aob_scan_range(pid: u32, base: usize, size: usize, pattern: &str) -> Option<usize> {
    let pattern_bytes: Vec<Option<u8>> = pattern
        .split_whitespace()
        .map(|b| if b == "??" || b == "?" { None } else { u8::from_str_radix(b, 16).ok() })
        .collect();
    if pattern_bytes.is_empty() { return None; }
    let pat_len = pattern_bytes.len();
    const CHUNK: usize = 16 * 1024 * 1024;

    // Use VirtualQueryEx to walk only committed, readable pages within the range.
    // This avoids allocating buffers for the large unmapped gaps that inflate SizeOfImage
    // on big modules (BL4 reports 883 MB virtual but most of that is reserved/unmapped).
    unsafe {
        let handle = ProcessHandle::open(pid).ok()?;
        let range_end = base + size;
        let unreadable = PAGE_NOACCESS.0 | PAGE_GUARD.0;
        let mut region_addr = base;

        while region_addr < range_end {
            let mut mbi = MEMORY_BASIC_INFORMATION::default();
            if VirtualQueryEx(
                handle.0,
                Some(region_addr as *const _),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            ) == 0 {
                break;
            }

            let region_base = mbi.BaseAddress as usize;
            let region_end  = region_base.saturating_add(mbi.RegionSize).min(range_end);
            // Advance past this region regardless of whether we scan it.
            region_addr = region_base.saturating_add(mbi.RegionSize);
            if region_addr == 0 { break; }

            if mbi.State != MEM_COMMIT || (mbi.Protect.0 & unreadable) != 0 {
                continue; // skip reserved/free/guarded pages — no allocation needed
            }

            // Scan this committed region in 16 MB chunks to bound per-allocation size.
            let mut pos = region_base.max(base);
            while pos + pat_len <= region_end {
                let chunk_size = CHUNK.min(region_end - pos);
                if let Ok(data) = read_memory_raw(handle.0, pos, chunk_size) {
                    if let Some(i) = data.windows(pat_len)
                        .position(|w| w.iter().zip(&pattern_bytes).all(|(b, p)| p.map_or(true, |pb| *b == pb)))
                    {
                        return Some(pos + i);
                    }
                }
                // Overlap by pat_len-1 to catch matches split across chunk boundaries.
                // chunk_size >= pat_len here, so the step is always >= 1.
                pos += chunk_size - (pat_len - 1);
            }
        }
    }
    None
}

pub fn find_process_by_name(name: &str) -> Option<u32> {
    let refresh = ProcessRefreshKind::nothing().with_exe(UpdateKind::Always);
    let mut sys = System::new_with_specifics(RefreshKind::nothing().with_processes(refresh));
    sys.refresh_processes(ProcessesToUpdate::All, false);

    let search = name.to_lowercase();
    let search_no_ext = search.strip_suffix(".exe").unwrap_or(&search);

    for (pid, process) in sys.processes() {
        let pname = process.name().to_string_lossy().to_lowercase();
        if pname == search || pname == search_no_ext {
            return Some(pid.as_u32());
        }
    }
    None
}

pub fn read_memory(pid: u32, address: usize, size: usize) -> Result<Vec<u8>, String> {
    let handle = ProcessHandle::open(pid)?;
    read_memory_raw(handle.0, address, size)
}

fn read_memory_raw(handle: HANDLE, address: usize, size: usize) -> Result<Vec<u8>, String> {
    unsafe {
        let mut buffer = vec![0u8; size];
        let mut bytes_read = 0;
        ReadProcessMemory(
            handle,
            address as *const _,
            buffer.as_mut_ptr() as *mut _,
            size,
            Some(&mut bytes_read),
        )
        .map(|_| buffer)
        .map_err(|_| "Failed to read memory".to_string())
    }
}

pub fn resolve_pointer_path(pid: u32, base_address: usize, offsets: &[usize]) -> Result<usize, String> {
    let mut current = base_address;
    for &offset in offsets {
        let data = read_memory(pid, current, std::mem::size_of::<usize>())?;
        #[cfg(target_pointer_width = "64")]
        let ptr = usize::from_le_bytes(data.try_into().unwrap());
        #[cfg(target_pointer_width = "32")]
        let ptr = u32::from_le_bytes(data.try_into().unwrap()) as usize;
        if ptr == 0 {
            return Err(format!("Null pointer at 0x{:X} in pointer chain", current));
        }
        current = ptr + offset;
    }
    Ok(current)
}

pub fn scan_memory_for_bytes(pid: u32, needle: &[u8]) -> Result<Vec<usize>, String> {
    unsafe {
        let handle = ProcessHandle::open(pid)?;
        let mut results = Vec::new();
        let mut address: usize = 0;

        loop {
            let mut mbi = MEMORY_BASIC_INFORMATION::default();
            let ret = VirtualQueryEx(
                handle.0,
                Some(address as *const _),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            );
            if ret == 0 { break; }

            let next = mbi.BaseAddress as usize + mbi.RegionSize;
            let unreadable = PAGE_NOACCESS.0 | PAGE_GUARD.0;

            if mbi.State == MEM_COMMIT
                && (mbi.Protect.0 & unreadable) == 0
                && mbi.RegionSize <= 512 * 1024 * 1024
            {
                if let Ok(data) = read_memory_raw(handle.0, mbi.BaseAddress as usize, mbi.RegionSize) {
                    for (i, window) in data.windows(needle.len()).enumerate() {
                        if window == needle {
                            results.push(mbi.BaseAddress as usize + i);
                        }
                    }
                }
            }

            address = next;
            if address == 0 { break; }
        }

        Ok(results)
    }
}

pub fn scan_for_int(pid: u32, value: i32) -> Result<Vec<usize>, String> {
    scan_memory_for_bytes(pid, &value.to_le_bytes())
}

pub fn scan_for_float(pid: u32, value: f32) -> Result<Vec<usize>, String> {
    scan_memory_for_bytes(pid, &value.to_le_bytes())
}

pub fn scan_for_double(pid: u32, value: f64) -> Result<Vec<usize>, String> {
    scan_memory_for_bytes(pid, &value.to_le_bytes())
}

pub fn read_double(pid: u32, address: usize) -> Result<f64, String> {
    let data = read_memory(pid, address, 8)?;
    data.try_into()
        .map(f64::from_le_bytes)
        .map_err(|_| "Failed to read 8 bytes for double".to_string())
}

pub fn write_double(pid: u32, address: usize, value: f64) -> Result<(), String> {
    write_memory(pid, address, &value.to_le_bytes())
}

pub fn write_memory(pid: u32, address: usize, data: &[u8]) -> Result<(), String> {
    unsafe {
        let handle = ProcessHandle::open(pid)?;
        let mut bytes_written = 0;
        WriteProcessMemory(
            handle.0,
            address as *const _,
            data.as_ptr() as *const _,
            data.len(),
            Some(&mut bytes_written),
        )
        .map_err(|_| "Failed to write memory".to_string())
    }
}

pub fn patch_memory(pid: u32, address: usize, data: &[u8]) -> Result<(), String> {
    unsafe {
        let handle = ProcessHandle::open(pid)?;
        let mut old_protect = PAGE_PROTECTION_FLAGS::default();

        VirtualProtectEx(
            handle.0,
            address as *const _,
            data.len(),
            PAGE_EXECUTE_READWRITE,
            &mut old_protect,
        )
        .map_err(|e| format!("Failed to change memory protection: {}", e))?;

        let mut bytes_written = 0;
        let result = WriteProcessMemory(
            handle.0,
            address as *const _,
            data.as_ptr() as *const _,
            data.len(),
            Some(&mut bytes_written),
        );

        let _ = VirtualProtectEx(handle.0, address as *const _, data.len(), old_protect, &mut old_protect);

        result.map_err(|_| "Failed to write patched bytes".to_string())
    }
}
