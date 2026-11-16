// Read-only rediscovery of the "100% Drop Rate" (XORPS-based) patch site, companion to
// scan_bl4_rarity.rs. Original site was ~12KB from the legendary site; try that offset
// from the newly-found legendary RVA first, then fall back to a dedicated AOB scan.

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("Borderlands4")
        .or_else(|| magic_wand_lib::engine::find_process_by_name("Borderlands4.exe"))
        .expect("Could not find running process Borderlands4.exe");
    println!("PID: {}", pid);

    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "Borderlands4.exe")
        .expect("Could not find module info for Borderlands4.exe");
    println!("Base: 0x{:X} Size: 0x{:X}", base, size);

    let new_legendary_rva = 0x3D9FE8Eusize;
    let old_legendary_rva = 0x96C77FAusize;
    let old_droprate_rva = 0x96CA774usize;
    let guess_offset = old_droprate_rva as isize - old_legendary_rva as isize;
    let guess_rva = (new_legendary_rva as isize + guess_offset) as usize;
    println!("Guess (same relative offset as before, 0x{:X}): RVA 0x{:X}", guess_offset, guess_rva);
    if let Ok(bytes) = magic_wand_lib::engine::read_memory(pid, base + guess_rva, 16) {
        let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        println!("  bytes: {}", hex.join(" "));
    }

    println!("\n--- Dedicated AOB scan for XORPS XMM0,XMM0 + CVTSI2SS (0F 57 C0 F3 0F 2A) ---");
    let matches = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, "0F 57 C0 F3 0F 2A");
    println!("Found {} candidate(s):", matches.len());
    for addr in &matches {
        let rva = addr.wrapping_sub(base);
        let ctx_start = addr.saturating_sub(8);
        let ctx = magic_wand_lib::engine::read_memory(pid, ctx_start, 64).unwrap_or_default();
        let hex: Vec<String> = ctx.iter().map(|b| format!("{:02X}", b)).collect();
        println!("RVA=0x{:X}  ctx[-8..+56]: {}", rva, hex.join(" "));
    }
}
