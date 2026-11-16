// Read-only: find a UFunction object by name and print its Outer (owning UClass) name,
// so we know which class's live instance to pull a vtable from before trusting a virtual
// slot resolved off an unrelated object (e.g. the player pawn).
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

fn main() {
    let target = std::env::args().nth(1).expect("Usage: find_func_owner <FunctionName>");

    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let gobjects_base = base + 171927000usize;
    let gnames_base = base + 173922192usize;
    let mut off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();
    off.gobjects_num_elements = 0x0C;
    off.gobjects_chunked = false;

    let num_elements = magic_wand_lib::engine::read_u32(pid, gobjects_base + off.gobjects_num_elements).unwrap() as usize;
    let objects_ptr = magic_wand_lib::engine::read_ptr(pid, gobjects_base + off.gobjects_objects).unwrap();

    for i in 0..num_elements {
        let item_addr = objects_ptr + i * off.fuobjectitem_size;
        let raw_ptr = match magic_wand_lib::engine::read_ptr(pid, item_addr + off.fuobjectitem_object) {
            Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p,
            _ => continue,
        };
        let obj_ptr = raw_ptr & !7;
        let oname_idx = magic_wand_lib::engine::read_u32(pid, obj_ptr + off.uobject_name).unwrap_or(0) as i32;
        let oname = match fname_to_string(pid, gnames_base, oname_idx, &off) {
            Some(s) => s,
            None => continue,
        };
        if oname != target { continue; }

        let class_ptr = magic_wand_lib::engine::read_ptr(pid, obj_ptr + off.uobject_class).unwrap_or(0);
        let cname_idx = magic_wand_lib::engine::read_u32(pid, class_ptr + off.uobject_name).unwrap_or(0) as i32;
        let cname = fname_to_string(pid, gnames_base, cname_idx, &off).unwrap_or_default();
        if cname != "Function" { continue; }

        println!("Found UFunction '{}' at 0x{:X}", oname, obj_ptr);
        let outer_ptr = magic_wand_lib::engine::read_ptr(pid, obj_ptr + 0x20).unwrap_or(0);
        println!("  Outer @ 0x{:X}", outer_ptr);
        if outer_ptr != 0 {
            let outer_name_idx = magic_wand_lib::engine::read_u32(pid, outer_ptr + off.uobject_name).unwrap_or(0) as i32;
            let outer_name = fname_to_string(pid, gnames_base, outer_name_idx, &off).unwrap_or_default();
            let outer_class_ptr = magic_wand_lib::engine::read_ptr(pid, outer_ptr + off.uobject_class).unwrap_or(0);
            let outer_class_name_idx = magic_wand_lib::engine::read_u32(pid, outer_class_ptr + off.uobject_name).unwrap_or(0) as i32;
            let outer_class_name = fname_to_string(pid, gnames_base, outer_class_name_idx, &off).unwrap_or_default();
            println!("  Outer name: '{}' (a {})", outer_name, outer_class_name);
        }
        let exec_fn = magic_wand_lib::engine::read_ptr(pid, obj_ptr + 0xD8).unwrap_or(0);
        println!("  ExecFunction RVA: 0x{:X}", exec_fn.wrapping_sub(base));

        // Also print FunctionFlags (commonly near 0xB8-0xC8 in UE5 UFunction; scan a small
        // window and flag anything with the "Native" bit (0x00400000) set) and NumParms/
        // ParmsSize if present, to sanity check this is really a native virtual dispatch.
        return;
    }
    println!("'{}' not found", target);
}
