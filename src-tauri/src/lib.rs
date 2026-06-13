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
    let res = engine::get_module_info(pid, module_name);
    res.map(|(addr, _)| addr.to_string())
}

#[tauri::command]
fn aob_scan(pid: u32, module_name: &str, pattern: &str) -> Result<String, String> {
    let res = engine::aob_scan(pid, module_name, pattern)?;
    Ok(res.to_string())
}

// Helper to parse address strings that might be hex or decimal
fn parse_addr(s: &str) -> Result<u64, String> {
    let clean = s.trim().to_lowercase();
    if clean.starts_with("0x") {
        u64::from_str_radix(&clean[2..], 16).map_err(|e| format!("Hex parse err: {}", e))
    } else {
        clean.parse::<u64>().map_err(|e| format!("Decimal parse err: {}", e))
    }
}

#[tauri::command]
fn resolve_pointer(
    pid: u32, 
    module_name: String, 
    base_offset: String, 
    offsets: Vec<String>
) -> Result<String, String> {
    let module_base = engine::get_module_info(pid, &module_name)
        .map(|(addr, _)| addr)
        .ok_or_else(|| format!("Module {} not found", module_name))?;

    let offset_val = parse_addr(&base_offset)?;
    let start_address = (module_base as u64) + offset_val;
    
    let mut parsed_offsets = Vec::new();
    for o in offsets {
        parsed_offsets.push(parse_addr(&o)? as usize);
    }

    let res = engine::resolve_pointer_path(pid, start_address as usize, &parsed_offsets)?;
    Ok(res.to_string())
}

#[tauri::command]
fn read_int(pid: u32, address: String) -> Result<i32, String> {
    let addr = parse_addr(&address)?;
    let data = engine::read_memory(pid, addr as usize, 4)?;
    if data.len() == 4 {
        Ok(i32::from_le_bytes(data.try_into().unwrap()))
    } else {
        Err("Failed to read 4 bytes".to_string())
    }
}

#[tauri::command]
fn read_float(pid: u32, address: String) -> Result<f32, String> {
    let addr = parse_addr(&address)?;
    let data = engine::read_memory(pid, addr as usize, 4)?;
    if data.len() == 4 {
        Ok(f32::from_le_bytes(data.try_into().unwrap()))
    } else {
        Err("Failed to read 4 bytes".to_string())
    }
}

#[tauri::command]
fn read_double(pid: u32, address: String) -> Result<f64, String> {
    let addr = parse_addr(&address)?;
    engine::read_double(pid, addr as usize)
}

#[tauri::command]
fn write_int(pid: u32, address: String, value: i32) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_memory(pid, addr as usize, &value.to_le_bytes())
}

#[tauri::command]
fn write_float(pid: u32, address: String, value: f32) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_memory(pid, addr as usize, &value.to_le_bytes())
}

#[tauri::command]
fn write_double(pid: u32, address: String, value: f64) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_double(pid, addr as usize, value)
}

#[tauri::command]
fn patch_bytes(pid: u32, address: String, bytes: Vec<u8>) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::patch_memory(pid, addr as usize, &bytes)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            scan_games,
            find_game,
            get_module_base,
            aob_scan,
            resolve_pointer,
            read_int,
            read_float,
            read_double,
            write_int,
            write_float,
            write_double,
            patch_bytes
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
