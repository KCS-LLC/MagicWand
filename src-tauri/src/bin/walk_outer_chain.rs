// Read-only: given a live UObject address, walk its Outer chain up to the top, printing
// each link's class and name. Cheap way to learn real ownership structure instead of
// guessing field offsets by brute-force pointer scanning.
fn fname_to_string(pid: u32, gnames_base: usize, fname_index: i32, off: &magic_wand_lib::ue5::Ue5Offsets) -> Option<String> {
    if fname_index < 0 { return None; }
    let idx = fname_index as usize;
    let entries_per_chunk = off.gnames_chunk_size / off.fname_stride;
    let chunk_idx = idx / entries_per_chunk;
    let entry_off = (idx % entries_per_chunk) * off.fname_stride;
    let chunk_ptr = magic_wand_lib::engine::read_ptr(pid, gnames_base + chunk_idx * 8)?;
    if chunk_ptr == 0 { return None; }
    let entry_addr = chunk_ptr + entry_off;
    let hdr_bytes = magic_wand_lib::engine::read_memory(pid, entry_addr, 2).ok()?;
    let hdr = u16::from_le_bytes([hdr_bytes[0], hdr_bytes[1]]);
    let str_len = (hdr >> 6) as usize;
    if str_len == 0 || str_len > 1024 { return None; }
    let raw = magic_wand_lib::engine::read_memory(pid, entry_addr + off.fname_entry_header, str_len).ok()?;
    String::from_utf8(raw).ok()
}

fn describe(pid: u32, gnames_base: usize, obj_ptr: usize, off: &magic_wand_lib::ue5::Ue5Offsets) -> String {
    let name_idx = magic_wand_lib::engine::read_u32(pid, obj_ptr + off.uobject_name).unwrap_or(0) as i32;
    let name = fname_to_string(pid, gnames_base, name_idx, off).unwrap_or_default();
    let class_ptr = magic_wand_lib::engine::read_ptr(pid, obj_ptr + off.uobject_class).unwrap_or(0) & !7;
    let cname_idx = magic_wand_lib::engine::read_u32(pid, class_ptr + off.uobject_name).unwrap_or(0) as i32;
    let cname = fname_to_string(pid, gnames_base, cname_idx, off).unwrap_or_default();
    format!("'{}' (class {})", name, cname)
}

fn main() {
    let addr_arg = std::env::args().nth(1).expect("Usage: walk_outer_chain <hex_addr>");
    let mut ptr = usize::from_str_radix(addr_arg.trim_start_matches("0x"), 16).unwrap();

    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let gnames_base = {
        let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
        base + 173922192usize
    };
    let off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();

    for depth in 0..15 {
        println!("[{}] 0x{:X} -> {}", depth, ptr, describe(pid, gnames_base, ptr, &off));
        let outer = magic_wand_lib::engine::read_ptr(pid, ptr + 0x20).unwrap_or(0);
        if outer == 0 { println!("  (no further outer)"); break; }
        ptr = outer;
    }
}
