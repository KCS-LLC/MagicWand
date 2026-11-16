// Read-only: walk the live (flat, non-chunked) GObjects array and print every object whose
// own name contains a given substring (case-insensitive), along with its class name. Used
// to get real ground truth on what "damage"-related classes/functions/properties actually
// exist in this build, instead of guessing from a raw AOB hit with no semantic context.
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

const MAX_RESULTS: usize = 150;

fn main() {
    let needle = std::env::args().nth(1).unwrap_or_else(|| "damage".to_string()).to_lowercase();

    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let gobjects_base = base + 171927000usize;
    let gnames_base = base + 173922192usize;
    let mut off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();
    off.gobjects_num_elements = 0x0C;
    off.gobjects_chunked = false;

    let num_elements = magic_wand_lib::engine::read_u32(pid, gobjects_base + off.gobjects_num_elements).unwrap() as usize;
    let objects_ptr = magic_wand_lib::engine::read_ptr(pid, gobjects_base + off.gobjects_objects).unwrap();
    println!("Scanning {} objects for name substring '{}'...", num_elements, needle);

    let mut printed = 0usize;
    // De-dupe by (object name, class name) pair — many gameplay instances share the same
    // class (e.g. per-actor "DamageEvent" objects), and we only care about distinct shapes.
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    for i in 0..num_elements {
        if printed >= MAX_RESULTS { break; }
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
        if !oname.to_lowercase().contains(&needle) { continue; }

        let class_ptr = magic_wand_lib::engine::read_ptr(pid, obj_ptr + off.uobject_class).unwrap_or(0);
        let cname_idx = magic_wand_lib::engine::read_u32(pid, class_ptr + off.uobject_name).unwrap_or(0) as i32;
        let cname = fname_to_string(pid, gnames_base, cname_idx, &off).unwrap_or_default();

        let key = (oname.clone(), cname.clone());
        if !seen.insert(key) { continue; }

        println!("{} -> (class {})", oname, cname);
        printed += 1;
    }
    if printed >= MAX_RESULTS {
        println!("(stopped at {} results — narrow the substring for more precision)", MAX_RESULTS);
    }
    println!("Done. {} distinct name/class pairs printed.", printed);
}
