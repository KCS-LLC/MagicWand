// Read-only. Re-verifies the two WeMod-derived AOB patterns needed for player-vs-enemy
// damage detection (see tow2_trainer_setup memory, "Damage Multiplier fix: WeMod's real
// player-vs-enemy detection mechanism") against the currently-running live process.
// CE's "s1.2" (a 2-byte captured displacement) and "*" (wildcard byte) are translated to
// this engine's "??" wildcard syntax. Both patterns are long/specific, so should match 0-1
// times each — not a flood risk like a short generic opcode scan.
const SITE_A_PATTERN: &str = "48 3B 87 ?? ?? 00 00 0F 85 ?? ?? ?? ?? ?? ?? ?? 0F 85 ?? ?? ?? ?? 8B ?? ?? 0F BA";
const SITE_B_PATTERN: &str = "FF 90 ?? ?? 00 00 0F 28 ?? 0F 2E ?? ?? ?? ?? ?? 74 ?? F6 ?? ?? ?? 00 00 08";

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe")
        .expect("TheOuterWorlds2-Win64-Shipping.exe not found — is the game running?");
    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    println!("Base: 0x{:X} Size: 0x{:X}", base, size);

    println!("\n--- Site A (aobplayerpawn) ---");
    let hitsa = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, SITE_A_PATTERN);
    println!("matches: {}", hitsa.len());
    for m in hitsa.iter().take(10) {
        let rva = m - base;
        let bytes = magic_wand_lib::engine::read_memory(pid, *m, 27).unwrap_or_default();
        let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        println!("  match@0x{:X} (RVA 0x{:X}): {}", m, rva, hex.join(" "));
    }

    println!("\n--- Site B (aobdmgcheck) ---");
    let hitsb = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, SITE_B_PATTERN);
    println!("matches: {}", hitsb.len());
    for m in hitsb.iter().take(10) {
        let rva = m - base;
        let bytes = magic_wand_lib::engine::read_memory(pid, *m, 25).unwrap_or_default();
        let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        println!("  match@0x{:X} (RVA 0x{:X}): {}", m, rva, hex.join(" "));
    }
}
