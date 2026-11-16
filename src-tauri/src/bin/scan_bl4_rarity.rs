// Read-only rediscovery of the BL4 legendary-drop-rate patch site, replicating the same
// AOB logic as the app's `scan_rarity_candidates` Tauri command (src-tauri/src/lib.rs),
// run standalone so results can be processed directly instead of read off the DevPanel UI.
// See docs/bl4-drop-rates.md "Re-Finding Addresses After a BL4 Update".

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("Borderlands4")
        .or_else(|| magic_wand_lib::engine::find_process_by_name("Borderlands4.exe"))
        .expect("Could not find running process Borderlands4.exe");
    println!("PID: {}", pid);

    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "Borderlands4.exe")
        .expect("Could not find module info for Borderlands4.exe");
    println!("Base: 0x{:X} Size: 0x{:X}", base, size);

    let wemod_aob = "25 FF 7F 00 00 F3 0F 2A ?? F3 0F 5E 07 F3 0F 59 07";
    let exact = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, wemod_aob);
    if !exact.is_empty() {
        println!("\nWEMOD AOB EXACT: {} hit(s):", exact.len());
        for addr in &exact {
            let rva = addr.wrapping_sub(base);
            let ctx_start = addr.saturating_sub(16);
            let ctx = magic_wand_lib::engine::read_memory(pid, ctx_start, 96).unwrap_or_default();
            let hex: Vec<String> = ctx.iter().map(|b| format!("{:02X}", b)).collect();
            println!("RVA=0x{:X}  ctx[-16..+80]: {}", rva, hex.join(" "));
        }
        return;
    }

    println!("\nNo exact [RDI]-form hit. Falling back to AND EAX + CVTSI2SS prefix scan...");
    let all = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, "25 FF 7F 00 00 F3 0F 2A");
    println!("Found {} candidate(s):", all.len());
    for addr in &all {
        let rva = addr.wrapping_sub(base);
        let ctx_start = addr.saturating_sub(16);
        let ctx = magic_wand_lib::engine::read_memory(pid, ctx_start, 96).unwrap_or_default();
        let hex: Vec<String> = ctx.iter().map(|b| format!("{:02X}", b)).collect();
        let divss_form = if ctx.len() >= 32 {
            match ctx[25] {
                0x07 => "[RDI]",
                0x05 => "[RIP+rel32]",
                _ => "?",
            }
        } else { "?" };
        // Highlight candidates matching the LEA/MOV tail discriminator from bl4-drop-rates.md:
        // `8D * 18 01 00 00` then `8B * 28 01 00 00` shortly after MULSS.
        let has_tail = ctx.windows(6).any(|w| w[0] == 0x8D && w[2..6] == [0x18, 0x01, 0x00, 0x00])
            && ctx.windows(6).any(|w| w[0] == 0x8B && w[2..6] == [0x28, 0x01, 0x00, 0x00]);
        println!("RVA=0x{:X}  DIVSS={} tail_match={} ctx[-16..+80]: {}", rva, divss_form, has_tail, hex.join(" "));
    }
}
