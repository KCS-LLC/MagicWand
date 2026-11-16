// Minimal, standalone DLL injector: CreateRemoteThread + LoadLibraryW only — no debugging
// APIs (no DebugActiveProcess/SetThreadContext), since attaching an actual debugger to
// Borderlands4.exe is known to crash it (Denuvo). Plain remote-thread injection is the same
// class of technique WeMod itself already uses successfully against BL4.
//
// Usage: inject_dll <process_name_or_pid> <dll_path>
// Example: inject_dll Borderlands4.exe C:\Users\renga\Dumper\Dumper-7\x64\Debug\Dumper-7.dll

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::PCWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE};
use windows::Win32::System::Threading::{CreateRemoteThread, OpenProcess, WaitForSingleObject, PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: inject_dll <process_name_or_pid> <dll_path>");
        std::process::exit(1);
    }

    let target = &args[1];
    let dll_path = &args[2];

    if !std::path::Path::new(dll_path).exists() {
        eprintln!("DLL not found: {}", dll_path);
        std::process::exit(1);
    }

    let pid: u32 = match target.parse() {
        Ok(p) => p,
        Err(_) => magic_wand_lib::engine::find_process_by_name(target)
            .unwrap_or_else(|| {
                eprintln!("Could not find running process: {}", target);
                std::process::exit(1);
            }),
    };
    println!("Target PID: {}", pid);

    unsafe {
        let access = PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ;
        let process = match OpenProcess(access, false, pid) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("OpenProcess failed: {}", e);
                std::process::exit(1);
            }
        };

        // kernel32.dll is mapped at the same base address in every process in this Windows
        // session, so an address resolved locally is valid in the remote process too — the
        // classic basis for CreateRemoteThread-based injection.
        let kernel32 = to_wide("kernel32.dll");
        let kernel32_handle = match GetModuleHandleW(PCWSTR(kernel32.as_ptr())) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("GetModuleHandleW(kernel32.dll) failed: {}", e);
                let _ = CloseHandle(process);
                std::process::exit(1);
            }
        };
        let load_library_addr = GetProcAddress(kernel32_handle, windows::core::s!("LoadLibraryW"));
        let load_library_addr = match load_library_addr {
            Some(addr) => addr,
            None => {
                eprintln!("GetProcAddress(LoadLibraryW) failed");
                let _ = CloseHandle(process);
                std::process::exit(1);
            }
        };

        let wide_path = to_wide(dll_path);
        let path_bytes = wide_path.len() * 2;

        let remote_buf = VirtualAllocEx(process, None, path_bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if remote_buf.is_null() {
            eprintln!("VirtualAllocEx failed");
            let _ = CloseHandle(process);
            std::process::exit(1);
        }

        let write_ok = WriteProcessMemory(
            process,
            remote_buf,
            wide_path.as_ptr() as *const _,
            path_bytes,
            None,
        );
        if write_ok.is_err() {
            eprintln!("WriteProcessMemory failed");
            let _ = VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);
            let _ = CloseHandle(process);
            std::process::exit(1);
        }

        let thread_start: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32 =
            std::mem::transmute(load_library_addr);

        let thread = match CreateRemoteThread(process, None, 0, Some(thread_start), Some(remote_buf), 0, None) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("CreateRemoteThread failed: {}", e);
                let _ = VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);
                let _ = CloseHandle(process);
                std::process::exit(1);
            }
        };

        println!("Remote thread created, waiting for LoadLibraryW to return...");
        WaitForSingleObject(thread, 10_000);

        let mut exit_code: u32 = 0;
        let _ = windows::Win32::System::Threading::GetExitCodeThread(thread, &mut exit_code);
        println!("LoadLibraryW remote thread exit code (low 32 bits of HMODULE, 0 = likely failure): 0x{:X}", exit_code);

        let _ = VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);
        let _ = CloseHandle(thread);
        let _ = CloseHandle(process);

        let dll_file_name = std::path::Path::new(dll_path).file_name().and_then(|f| f.to_str()).unwrap_or(dll_path.as_str());
        match magic_wand_lib::engine::get_module_info(pid, dll_file_name) {
            Some((base, size)) => println!("Verified: '{}' is now loaded in target process at 0x{:X} (size 0x{:X}).", dll_file_name, base, size),
            None => println!("Warning: '{}' does NOT appear in the target process's module list — injection likely did not take effect.", dll_file_name),
        }
    }
}
