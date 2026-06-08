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
fn get_module_base(pid: u32, module_name: &str) -> Option<usize> {
    engine::get_module_base_address(pid, module_name)
}

#[tauri::command]
fn resolve_pointer(pid: u32, base_address: usize, offsets: Vec<usize>) -> Result<usize, String> {
    engine::resolve_pointer_path(pid, base_address, &offsets)
}

#[tauri::command]
fn read_int(pid: u32, address: usize) -> Result<i32, String> {
    let data = engine::read_memory(pid, address, 4)?;
    if data.len() == 4 {
        Ok(i32::from_le_bytes(data.try_into().unwrap()))
    } else {
        Err("Failed to read 4 bytes".to_string())
    }
}

#[tauri::command]
fn write_int(pid: u32, address: usize, value: i32) -> Result<(), String> {
    engine::write_memory(pid, address, &value.to_le_bytes())
}

#[tauri::command]
fn write_float(pid: u32, address: usize, value: f32) -> Result<(), String> {
    engine::write_memory(pid, address, &value.to_le_bytes())
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
            write_int,
            write_float
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
