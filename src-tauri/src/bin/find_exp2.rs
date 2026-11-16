fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let pat = "8B F2 48 8B D9 85 D2 7F ?? 32 C0";
    let matches = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, pat);
    println!("{} match(es)", matches.len());
    for m in &matches {
        let bytes = magic_wand_lib::engine::read_memory(pid, m - 3, 8).unwrap_or_default();
        println!("  0x{:X} (RVA 0x{:X})  preceding+match: {:02X?}", m, m - base, bytes);
    }
}
