// Read-only: try progressively shorter/looser anchors for the XP and fly-mode sites,
// since the full WeMod-derived patterns came back with 0 matches (same-day staleness,
// same lesson as ammo Site 2 / crit chance).
fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    println!("Module base: 0x{:X}  size: 0x{:X}", base, size);

    let candidates = [
        ("exp-core-A", "85 D2 7F ?? 32 C0"),
        ("exp-core-B", "8B FA 48 8B D9 85 D2"),
        ("exp-core-C", "8B F2 48 8B D9 85 D2"),
        ("fly-core-A", "3C 06 0F 84 ?? ?? 00 00 3C 03 0F 84 ?? ?? 00 00"),
        ("fly-core-B", "8A 83 ?? ?? 00 00 3C 06"),
    ];
    for (name, pat) in candidates {
        let matches = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, pat);
        println!("{}: {} match(es)", name, matches.len());
        for m in matches.iter().take(5) {
            println!("  0x{:X} (RVA 0x{:X})", m, m - base);
        }
    }
}
