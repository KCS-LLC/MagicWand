fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let addr = base + 0x6AD7A4Dusize;
    let bytes = magic_wand_lib::engine::read_memory(pid, addr, 32).unwrap();
    let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
    println!("At RVA 0x6AD7A4D: {}", hex.join(" "));

    // Also check the original site1 region for comparison / surrounding context.
    let addr1 = base + 0x6AD7A43usize;
    let bytes1 = magic_wand_lib::engine::read_memory(pid, addr1, 40).unwrap();
    let hex1: Vec<String> = bytes1.iter().map(|b| format!("{:02X}", b)).collect();
    println!("At RVA 0x6AD7A43 (40 bytes): {}", hex1.join(" "));
}
