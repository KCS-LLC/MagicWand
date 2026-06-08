mod engine;
mod scanner;

#[tauri::command]
fn scan_games() -> Vec<scanner::DetectedGame> {
    scanner::scan_all()
}

#[tauri::command]
fn find_game(name: &str) -> Option<u32> {
    engine::find_process_by_name(name)
}

#[tauri::command]
fn get_module_base(pid: u32, module_name: &str) -> Option<String> {
    let res = engine::get_module_base_address(pid, module_name);
    res.map(|addr| addr.to_string())
}

#[tauri::command]
fn resolve_pointer(
    pid: u32, 
    module_name: String, 
    base_offset: String, 
    offsets: Vec<String>
) -> Result<String, String> {
    // 1. Get Module Base
    let module_base = engine::get_module_base_address(pid, &module_name)
        .ok_or_else(|| format!("Module {} not found", module_name))?;

    // 2. Parse Base Offset (handle 0x prefix)
    let clean_base = base_offset.trim_start_matches("0x");
    let offset_val = u64::from_str_radix(clean_base, 16)
        .map_err(|e| format!("Failed to parse base offset {}: {}", base_offset, e))?;

    let start_address = (module_base as u64) + offset_val;
    
    // 3. Parse Sub-offsets
    let mut parsed_offsets = Vec::new();
    for o in offsets {
        let clean_o = o.trim_start_matches("0x");
        let val = u64::from_str_radix(clean_o, 16)
            .map_err(|e| format!("Failed to parse offset {}: {}", o, e))?;
        parsed_offsets.push(val as usize);
    }

    // 4. Resolve Path
    let res = engine::resolve_pointer_path(pid, start_address as usize, &parsed_offsets);
    
    match res {
        Ok(addr) => {
            if addr < 0x10000 {
                println!("DEBUG: resolve_pointer(module={}, base={}) -> Resolved to Null/Invalid (0x{:X})", module_name, base_offset, addr);
                Err("Resolved to null or invalid address".to_string())
            } else {
                println!("DEBUG: resolve_pointer(module={}, base={}) -> 0x{:X}", module_name, base_offset, addr);
                Ok(addr.to_string())
            }
        }
        Err(e) => {
            println!("DEBUG: resolve_pointer ERROR: {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
fn read_int(pid: u32, address: String) -> Result<i32, String> {
    let addr = address.parse::<u64>().map_err(|e| e.to_string())?;
    let data = engine::read_memory(pid, addr as usize, 4)?;
    if data.len() == 4 {
        Ok(i32::from_le_bytes(data.try_into().unwrap()))
    } else {
        Err("Failed to read 4 bytes".to_string())
    }
}

#[tauri::command]
fn read_float(pid: u32, address: String) -> Result<f32, String> {
    let addr = address.parse::<u64>().map_err(|e| e.to_string())?;
    let data = engine::read_memory(pid, addr as usize, 4)?;
    if data.len() == 4 {
        Ok(f32::from_le_bytes(data.try_into().unwrap()))
    } else {
        Err("Failed to read 4 bytes".to_string())
    }
}

#[tauri::command]
fn write_int(pid: u32, address: String, value: i32) -> Result<(), String> {
    let addr = address.parse::<u64>().map_err(|e| e.to_string())?;
    engine::write_memory(pid, addr as usize, &value.to_le_bytes())
}

#[tauri::command]
fn write_float(pid: u32, address: String, value: f32) -> Result<(), String> {
    let addr = address.parse::<u64>().map_err(|e| e.to_string())?;
    engine::write_memory(pid, addr as usize, &value.to_le_bytes())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            scan_games,
            find_game,
            get_module_base,
            resolve_pointer,
            read_int,
            read_float,
            write_int,
            write_float
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
