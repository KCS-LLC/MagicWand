fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let addr = base + 0x12D3A18usize + 0x1Ausize;
    let bytes = magic_wand_lib::engine::read_memory(pid, addr, 6).unwrap();
    let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
    println!("RVA 0x{:X} (6 bytes): {}", 0x12D3A18usize + 0x1Ausize, hex.join(" "));
}
