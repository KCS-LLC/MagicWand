// One-shot read-only check of the raw bytes at the BL4 drop-rate patch sites, to tell
// whether a code_cave/code_patch toggle actually landed vs silently no-op'd vs is now
// pointed at stale (post-patch-relinked) code entirely.

fn dump(pid: u32, base: usize, label: &str, rva: usize, len: usize) {
    match magic_wand_lib::engine::read_memory(pid, base + rva, len) {
        Ok(bytes) => {
            let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
            println!("[{}] RVA 0x{:X}: {}", label, rva, hex.join(" "));
        }
        Err(e) => println!("[{}] RVA 0x{:X}: READ FAILED: {}", label, rva, e),
    }
}

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("Borderlands4")
        .or_else(|| magic_wand_lib::engine::find_process_by_name("Borderlands4.exe"))
        .expect("Could not find running process Borderlands4.exe");
    println!("PID: {}", pid);

    let (base, _size) = magic_wand_lib::engine::get_module_info(pid, "Borderlands4.exe")
        .expect("Could not find module info for Borderlands4.exe");
    println!("Base: 0x{:X}", base);

    dump(pid, base, "100% Drop Rate site", 0x96CA774, 7);
    dump(pid, base, "Legendary cave site", 0x96C77FA, 17);
}
