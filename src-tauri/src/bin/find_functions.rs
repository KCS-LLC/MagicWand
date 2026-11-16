// Read-only: locate the live UFunction objects for a list of function names, print each
// one's ExecFunction (native thunk) RVA. Used to cross-reference against candidate damage-
// multiplier sites found via AOB scanning — if a candidate sits inside/near one of these,
// that's real corroborating evidence of what code region we're in. Writes nothing.
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
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let gobjects_base = base + 171927000usize;
    let gnames_base = base + 173922192usize;
    let mut off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();
    off.gobjects_num_elements = 0x0C;
    off.gobjects_chunked = false;

    let targets: Vec<String> = std::env::args().skip(1).collect();
    if targets.is_empty() {
        eprintln!("Usage: find_functions <name1> <name2> ...");
        return;
    }

    let num_elements = magic_wand_lib::engine::read_u32(pid, gobjects_base + off.gobjects_num_elements).unwrap() as usize;
    let objects_ptr = magic_wand_lib::engine::read_ptr(pid, gobjects_base + off.gobjects_objects).unwrap();
    println!("Scanning {} objects for {} target name(s)...", num_elements, targets.len());

    let mut found: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for i in 0..num_elements {
        if found.len() == targets.len() { break; }
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
        if let Some(pos) = targets.iter().position(|t| t == &oname) {
            let class_ptr = magic_wand_lib::engine::read_ptr(pid, obj_ptr + off.uobject_class).unwrap_or(0);
            let cname_idx = magic_wand_lib::engine::read_u32(pid, class_ptr + off.uobject_name).unwrap_or(0) as i32;
            let cname = fname_to_string(pid, gnames_base, cname_idx, &off).unwrap_or_default();
            if cname != "Function" { continue; }
            let exec_fn = magic_wand_lib::engine::read_ptr(pid, obj_ptr + 0xD8).unwrap_or(0);
            if exec_fn == 0 || exec_fn < base { continue; }
            println!("{} -> ExecFunction 0x{:X} (RVA 0x{:X})", oname, exec_fn, exec_fn - base);
            found.insert(pos);
        }
    }
    for (i, t) in targets.iter().enumerate() {
        if !found.contains(&i) { println!("{} -> NOT FOUND", t); }
    }
}
