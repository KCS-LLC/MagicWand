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

    let data = read_memory(pid, base, size)?;

    let pattern_bytes: Vec<Option<u8>> = pattern
        .split_whitespace()
        .map(|b| if b == "??" || b == "?" { None } else { Some(u8::from_str_radix(b, 16).unwrap_or(0)) })
        .collect();

    data.windows(pattern_bytes.len())
        .position(|window| {
            window.iter().zip(&pattern_bytes).all(|(b, p)| p.map_or(true, |pb| *b == pb))
        })
        .map(|i| base + i)
        .ok_or_else(|| "Pattern not found".to_string())
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
        { current = usize::from_le_bytes(data.try_into().unwrap()) + offset; }
        #[cfg(target_pointer_width = "32")]
        { current = u32::from_le_bytes(data.try_into().unwrap()) as usize + offset; }
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
