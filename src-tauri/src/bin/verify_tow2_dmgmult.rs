fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();

    for sig in ["F3 0F 59 B6 7C 01 00 00", "F3 0F 59 B6 78 01 00 00"] {
        let matches = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, sig);
        println!("'{}' -> {} match(es)", sig, matches.len());
        for a in &matches {
            println!("  0x{:X} (RVA 0x{:X})", a, a - base);
        }
    }
}
