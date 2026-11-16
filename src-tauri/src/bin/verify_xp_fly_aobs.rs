// Read-only: independently verify (not trust blindly) the WeMod-derived aobexp x2 and
// aobflymode AOB patterns against the live module, since WeMod's own byte patterns have
// been stale before (ammo, crit chance). Prints match addresses/RVAs and the actual live
// bytes at each site, plus the extracted s1 displacement for aobflymode.
fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    println!("Module base: 0x{:X}  size: 0x{:X}", base, size);

    let exp1 = "45 ?? ?? 8B FA 48 8B D9 85 D2 7F ?? 32 C0";
    let exp2 = "45 ?? ?? 8B F2 48 8B D9 85 D2 7F ?? 32 C0";
    let flymode = "8A 83 ?? ?? 00 00 3C 06 0F 84 ?? ?? 00 00 3C 03 0F 84 ?? ?? 00 00 ?? ?? ?? 0F 85 ?? ?? 00 00";

    for (name, pat) in [("aobexp#1", exp1), ("aobexp#2", exp2), ("aobflymode", flymode)] {
        let matches = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, pat);
        println!("\n{}: {} match(es)", name, matches.len());
        for m in &matches {
            let rva = m - base;
            println!("  addr=0x{:X} rva=0x{:X}", m, rva);
            if let Ok(bytes) = magic_wand_lib::engine::read_memory(pid, *m, 24) {
                println!("    bytes: {:02X?}", bytes);
                if name == "aobflymode" {
                    let s1 = u16::from_le_bytes([bytes[2], bytes[3]]);
                    println!("    s1 displacement = 0x{:X} ({})", s1, s1);
                }
            }
        }
    }
}
