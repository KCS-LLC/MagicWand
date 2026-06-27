mod engine;
mod logger;
mod mono;
mod scanner;
mod ue5;

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
    engine::get_module_info(pid, module_name).map(|(addr, _)| addr.to_string())
}

#[tauri::command]
fn aob_scan(pid: u32, module_name: &str, pattern: &str) -> Result<String, String> {
    engine::aob_scan(pid, module_name, pattern).map(|r| r.to_string())
}

fn parse_addr(s: &str) -> Result<u64, String> {
    let clean = s.trim().to_lowercase();
    if clean.starts_with("0x") {
        u64::from_str_radix(&clean[2..], 16).map_err(|e| format!("Hex parse err: {}", e))
    } else {
        clean.parse::<u64>().map_err(|e| format!("Decimal parse err: {}", e))
    }
}

fn read_4bytes(pid: u32, address: &str) -> Result<[u8; 4], String> {
    let addr = parse_addr(address)?;
    engine::read_memory(pid, addr as usize, 4)?
        .try_into()
        .map_err(|_| "Failed to read 4 bytes".to_string())
}

#[tauri::command]
fn resolve_pointer(
    pid: u32,
    module_name: String,
    base_offset: String,
    offsets: Vec<String>,
) -> Result<String, String> {
    let module_base = engine::get_module_info(pid, &module_name)
        .map(|(addr, _)| addr)
        .ok_or_else(|| format!("Module {} not found", module_name))?;

    let start_address = (module_base as u64) + parse_addr(&base_offset)?;

    let mut parsed_offsets = Vec::new();
    for o in offsets {
        parsed_offsets.push(parse_addr(&o)? as usize);
    }

    engine::resolve_pointer_path(pid, start_address as usize, &parsed_offsets)
        .map(|r| r.to_string())
}

#[tauri::command]
fn read_int(pid: u32, address: String) -> Result<i32, String> {
    Ok(i32::from_le_bytes(read_4bytes(pid, &address)?))
}

#[tauri::command]
fn read_float(pid: u32, address: String) -> Result<f32, String> {
    Ok(f32::from_le_bytes(read_4bytes(pid, &address)?))
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
fn read_double(pid: u32, address: String) -> Result<f64, String> {
    let addr = parse_addr(&address)?;
    engine::read_double(pid, addr as usize)
}

#[tauri::command]
fn write_double(pid: u32, address: String, value: f64) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_double(pid, addr as usize, value)
}

#[tauri::command]
fn scan_value(pid: u32, value_type: String, value: f64) -> Result<Vec<String>, String> {
    let addresses = match value_type.as_str() {
        "double" => engine::scan_for_double(pid, value)?,
        "float"  => engine::scan_for_float(pid, value as f32)?,
        "int"    => engine::scan_for_int(pid, value as i32)?,
        _        => return Err(format!("Unknown value type: {}", value_type)),
    };
    Ok(addresses.iter().map(|a| format!("0x{:X}", a)).collect())
}

#[tauri::command]
fn resolve_mono_chain(
    pid: u32,
    module_name: String,
    assembly: String,
    namespace: String,
    class_name: String,
    static_field: String,
    via_parent: bool,
    instance_field: String,
    final_offset: usize,
    instance_field_is_ref: Option<bool>,
) -> Result<String, String> {
    let (mono_base, _) = engine::get_module_info(pid, &module_name)
        .ok_or_else(|| format!("Module '{}' not found in process", module_name))?;
    let addr = mono::resolve_mono_chain(
        pid, mono_base, &assembly, &namespace, &class_name,
        &static_field, via_parent, &instance_field, final_offset,
        instance_field_is_ref.unwrap_or(false),
    )?;
    Ok(format!("0x{:X}", addr))
}

#[tauri::command]
fn resolve_mono_field(
    pid: u32,
    module_name: String,
    assembly: String,
    namespace: String,
    class_name: String,
    field_name: String,
    is_static: bool,
) -> Result<String, String> {
    let (mono_base, _) = engine::get_module_info(pid, &module_name)
        .ok_or_else(|| format!("Module '{}' not found in process", module_name))?;
    let addr = mono::resolve_mono_field(
        pid, mono_base, &assembly, &namespace, &class_name, &field_name, is_static,
    )?;
    Ok(format!("0x{:X}", addr))
}

#[tauri::command]
fn resolve_ue5_prop(
    pid: u32,
    module_name: String,
    gobjects_aob: String,
    gnames_aob: String,
    gobjects_offset: Option<usize>,
    gnames_offset: Option<usize>,
    class_name: String,
    property_offset: usize,
    extra_offsets: Option<Vec<usize>>,
) -> Result<String, String> {
    let (base, size) = engine::get_module_info(pid, &module_name)
        .ok_or_else(|| format!("Module '{}' not found", module_name))?;
    let chain = extra_offsets.as_deref().unwrap_or(&[]);
    let addr = if let (Some(go), Some(gn)) = (gobjects_offset, gnames_offset) {
        ue5::resolve_ue5_prop_static(pid, base, go, gn, &class_name, property_offset, chain)?
    } else {
        ue5::resolve_ue5_prop(pid, base, size, &gobjects_aob, &gnames_aob, &class_name, property_offset, chain)?
    };
    Ok(format!("0x{:X}", addr))
}

#[tauri::command]
fn write_byte(pid: u32, address: String, value: u8) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::patch_memory(pid, addr as usize, &[value])
}

#[tauri::command]
fn read_byte(pid: u32, address: String) -> Result<u8, String> {
    let addr = parse_addr(&address)?;
    let data = engine::read_memory(pid, addr as usize, 1)
        .map_err(|e| e.to_string())?;
    Ok(data[0])
}

#[tauri::command]
fn patch_bytes(pid: u32, address: String, bytes: Vec<u8>) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::patch_memory(pid, addr as usize, &bytes)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("magic-wand.log")))
        .unwrap_or_else(|| std::path::PathBuf::from("magic-wand.log"));
    logger::init(&log_path);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
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
            scan_value,
            patch_bytes,
            resolve_mono_field,
            resolve_mono_chain,
            resolve_ue5_prop,
            write_byte,
            read_byte
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
