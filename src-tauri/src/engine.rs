use sysinfo::System;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};

use windows::Win32::System::ProcessStatus::{EnumProcessModules, GetModuleBaseNameW};

pub fn get_module_base_address(pid: u32, module_name: &str) -> Option<usize> {
    unsafe {
        let handle = OpenProcess(PROCESS_ALL_ACCESS, false, pid).ok()?;
        let mut modules = [Default::default(); 1024];
        let mut cb_needed = 0;
        
        if EnumProcessModules(
            handle,
            modules.as_mut_ptr(),
            std::mem::size_of_val(&modules) as u32,
            &mut cb_needed,
        ).is_err() {
            let _ = CloseHandle(handle);
            return None;
        }
        
        let count = cb_needed as usize / std::mem::size_of::<HANDLE>();
        for i in 0..count {
            let mut name = [0u16; 256];
            let len = GetModuleBaseNameW(handle, Some(modules[i]), &mut name);
            if len > 0 {
                let current_name = String::from_utf16_lossy(&name[..len as usize]);
                if current_name.to_lowercase() == module_name.to_lowercase() {
                    let _ = CloseHandle(handle);
                    return Some(modules[i].0 as usize);
                }
            }
        }
        
        let _ = CloseHandle(handle);
        None
    }
}

pub fn find_process_by_name(name: &str) -> Option<u32> {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    for (pid, process) in sys.processes() {
        let process_name = process.name().to_string_lossy();
        if process_name.to_lowercase() == name.to_lowercase() {
            return Some(pid.as_u32());
        }
    }
    None
}

pub fn read_memory(pid: u32, address: usize, size: usize) -> Result<Vec<u8>, String> {
    unsafe {
        let handle = OpenProcess(PROCESS_ALL_ACCESS, false, pid)
            .map_err(|e| format!("Failed to open process: {}", e))?;
        
        let mut buffer = vec![0u8; size];
        let mut bytes_read = 0;
        
        let success = ReadProcessMemory(
            handle,
            address as *const _,
            buffer.as_mut_ptr() as *mut _,
            size,
            Some(&mut bytes_read),
        );
        
        let _ = CloseHandle(handle);
        
        if success.is_ok() {
            Ok(buffer)
        } else {
            Err("Failed to read memory".to_string())
        }
    }
}

pub fn resolve_pointer_path(pid: u32, base_address: usize, offsets: &[usize]) -> Result<usize, String> {
    let mut current_address = base_address;

    for &offset in offsets {
        let data = read_memory(pid, current_address, std::mem::size_of::<usize>())?;
        if data.len() != std::mem::size_of::<usize>() {
            return Err("Failed to read pointer address".to_string());
        }
        
        // Convert bytes to usize (assuming 64-bit for modern games, adjust if 32-bit support is needed)
        #[cfg(target_pointer_width = "64")]
        {
            current_address = usize::from_le_bytes(data.try_into().unwrap()) + offset;
        }
        #[cfg(target_pointer_width = "32")]
        {
            current_address = u32::from_le_bytes(data.try_into().unwrap()) as usize + offset;
        }
    }

    Ok(current_address)
}

pub fn write_memory(pid: u32, address: usize, data: &[u8]) -> Result<(), String> {
    unsafe {
        let handle = OpenProcess(PROCESS_ALL_ACCESS, false, pid)
            .map_err(|e| format!("Failed to open process: {}", e))?;
        
        let mut bytes_written = 0;
        
        let success = WriteProcessMemory(
            handle,
            address as *const _,
            data.as_ptr() as *const _,
            data.len(),
            Some(&mut bytes_written),
        );
        
        let _ = CloseHandle(handle);
        
        if success.is_ok() {
            Ok(())
        } else {
            Err("Failed to write memory".to_string())
        }
    }
}
