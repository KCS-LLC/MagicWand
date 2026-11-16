// Read-only: look for WeMod's injected module in the live TOW2 process, either as a
// named module or (more likely, per prior BL4 investigation) a manually-mapped hidden
// executable region invisible to normal module enumeration.
fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    println!("PID: {}", pid);

    println!("\n--- Named modules matching 'wemod'/'trainer' ---");
    if let Ok(mods) = magic_wand_lib::engine::list_all_modules(pid) {
        for (name, base, size) in &mods {
            if name.to_lowercase().contains("wemod") || name.to_lowercase().contains("trainer") {
                println!("  {} base=0x{:X} size={:.1}MB", name, base, *size as f64 / 1_048_576.0);
            }
        }
        println!("  ({} total modules)", mods.len());
    }

    println!("\n--- Executable regions outside normal module base ranges ---");
    if let Ok(regions) = magic_wand_lib::engine::list_executable_regions(pid) {
        println!("  {} total exec regions", regions.len());
        for (base, size, prot) in &regions {
            // Flag anything under 2MB and 32MB-ish (typical trainer DLL size) not in the
            // usual high 0x7FF... module range, or just print all and eyeball it.
            if *size > 500_000 && *size < 10_000_000 {
                println!("  0x{:X}  size={:.2}MB  prot={}", base, *size as f64 / 1_048_576.0, prot);
            }
        }
    }
}
