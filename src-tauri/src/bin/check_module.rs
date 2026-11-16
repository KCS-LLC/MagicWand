// Read-only. NOTE: "F3 0F 59 B6" is a generic 4-byte opcode prefix (MULSS) that matches
// thousands of times in a large UE5 module — a whole-module scan for it, printing +
// re-reading every hit individually, previously flooded/crashed the terminal (see
// bl4-offset-discovery-lessons: generic short AOB patterns can hang/flood on big modules).
// Always cap how many hits get the extra per-match read+print; use a longer/more specific
// signature or a narrower [base, size) range when possible instead of raising this cap.
const MAX_PRINTED_MATCHES: usize = 50;

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    println!("Base: 0x{:X} Size: 0x{:X} ({} bytes)", base, size, size);

    let anchor = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, "F3 0F 59 B6");
    println!("'F3 0F 59 B6' anchor matches: {}", anchor.len());
    if anchor.len() > MAX_PRINTED_MATCHES {
        println!(
            "  (too many hits to print individually — showing first {} of {}; narrow the \
             signature or range instead of scrolling through the rest)",
            MAX_PRINTED_MATCHES, anchor.len()
        );
    }
    for a in anchor.iter().take(MAX_PRINTED_MATCHES) {
        let rva = a - base;
        let bytes = magic_wand_lib::engine::read_memory(pid, *a, 8).unwrap_or_default();
        let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        println!("  0x{:X} (RVA 0x{:X}): {}", a, rva, hex.join(" "));
    }
}
