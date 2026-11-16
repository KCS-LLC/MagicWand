// Read-only: dump a raw byte range from the live process to a local file, for offline
// disassembly with objdump. Usage: dump_range <rva_hex> <before> <after> <out_path>
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rva = usize::from_str_radix(args[1].trim_start_matches("0x"), 16).unwrap();
    let before: usize = args[2].parse().unwrap();
    let after: usize = args[3].parse().unwrap();
    let out_path = &args[4];

    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let start = base + rva - before;
    let total = before + after;
    let bytes = magic_wand_lib::engine::read_memory(pid, start, total).unwrap();
    std::fs::write(out_path, &bytes).unwrap();
    println!("Wrote {} bytes to {} (covers RVA 0x{:X} to 0x{:X}, target RVA 0x{:X} at file offset 0x{:X})",
        bytes.len(), out_path, rva - before, rva + after, rva, before);
}
