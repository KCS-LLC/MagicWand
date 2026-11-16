// Read-only. Re-verifies the two WeMod-derived "aobammo" AOB patterns (see
// tow2_trainer_setup memory, "WeMod mechanism reference: Infinite Ammo") against the
// currently-running live process, per the documented caveat that WeMod's embedded AOB
// bytes can be stale against the installed build even same-day. Both patterns here are
// long/specific (30-38 bytes, few wildcards) so match counts should be 0 or 1 each —
// unlike a short generic opcode scan, this should not flood output.
const SITE1_PATTERN: &str = "E8 ?? ?? ?? ?? 84 C0 74 ?? ?? 83 ?? 00 74 ?? ?? 8B ?? ?? ?? 00 00 ?? 85 ?? 74 ?? ?? 8B ?? E8 ?? ?? ?? ?? 48 85 C0 75 12";
const SITE1_PATCH_OFFSET: usize = 23; // WeMod calls the real patch site "aobammo+23"
const SITE1_EXPECTED: [u8; 5] = [0x48, 0x85, 0xC0, 0x75, 0x12];

const SITE2_PATTERN: &str = "48 8B 03 48 39 48 10 74 ?? EB ?? E8 ?? ?? ?? ?? ?? 8B ?? ?? 8B ?? E8 ?? ?? ?? ?? 84 C0 74";
const SITE2_EXPECTED: [u8; 7] = [0x48, 0x8B, 0x03, 0x48, 0x39, 0x48, 0x10];

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe")
        .expect("TheOuterWorlds2-Win64-Shipping.exe not found — is the game running?");
    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    println!("Base: 0x{:X} Size: 0x{:X}", base, size);

    println!("\n--- Site 1 (ammo decrement skip) ---");
    let hits1 = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, SITE1_PATTERN);
    println!("matches: {}", hits1.len());
    for m in hits1.iter().take(10) {
        let patch_addr = m + SITE1_PATCH_OFFSET;
        let rva = patch_addr - base;
        let bytes = magic_wand_lib::engine::read_memory(pid, patch_addr, 5).unwrap_or_default();
        let ok = bytes.as_slice() == SITE1_EXPECTED;
        let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        println!(
            "  match@0x{:X} -> patchSite 0x{:X} (RVA 0x{:X}): {} {}",
            m, patch_addr, rva, hex.join(" "),
            if ok { "[MATCHES expected 48 85 C0 75 12]" } else { "[MISMATCH]" }
        );
    }

    println!("\n--- Site 2 (reserve ammo pool) ---");
    let hits2 = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, SITE2_PATTERN);
    println!("matches: {}", hits2.len());
    for m in hits2.iter().take(10) {
        let rva = m - base;
        let bytes = magic_wand_lib::engine::read_memory(pid, *m, 7).unwrap_or_default();
        let ok = bytes.as_slice() == SITE2_EXPECTED;
        let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        println!(
            "  match@0x{:X} (RVA 0x{:X}): {} {}",
            m, rva, hex.join(" "),
            if ok { "[MATCHES expected 48 8B 03 48 39 48 10]" } else { "[MISMATCH]" }
        );
    }
}
