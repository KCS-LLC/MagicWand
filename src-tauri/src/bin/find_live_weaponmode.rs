// Read-only: scan all live GObjects for instances whose class inherits from WeaponMode
// (not just exact-name matches, and not the CDO), print each one plus its Outer chain (up
// to 6 hops) and BaseDamage fields — used to find which live WeaponMode instance belongs to
// the player's currently-equipped weapon, since find_weapon_chain's pointer-scan approach
// didn't find a directly-named "Weapon" field on the Pawn.
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

fn class_inherits_from(pid: u32, gnames_base: usize, mut class_ptr: usize, target: &str, off: &magic_wand_lib::ue5::Ue5Offsets) -> bool {
    const SUPER_OFFSET: usize = 0x40;
    class_ptr &= !7;
    for _ in 0..20 {
        if class_ptr == 0 || class_ptr > 0x0000_7FFF_FFFF_FFFF { break; }
        let fname_idx = match magic_wand_lib::engine::read_u32(pid, class_ptr + off.uobject_name) {
            Some(i) if (i as i32) >= 0 => i as i32,
            _ => break,
        };
        if let Some(name) = fname_to_string(pid, gnames_base, fname_idx, off) {
            if name == target { return true; }
        }
        class_ptr = match magic_wand_lib::engine::read_ptr(pid, class_ptr + SUPER_OFFSET) {
            Some(p) => p & !7,
            None => break,
        };
    }
    false
}

fn describe(pid: u32, gnames_base: usize, obj_ptr: usize, off: &magic_wand_lib::ue5::Ue5Offsets) -> String {
    let name_idx = magic_wand_lib::engine::read_u32(pid, obj_ptr + off.uobject_name).unwrap_or(0) as i32;
    let name = fname_to_string(pid, gnames_base, name_idx, off).unwrap_or_default();
    let class_ptr = magic_wand_lib::engine::read_ptr(pid, obj_ptr + off.uobject_class).unwrap_or(0) & !7;
    let cname_idx = magic_wand_lib::engine::read_u32(pid, class_ptr + off.uobject_name).unwrap_or(0) as i32;
    let cname = fname_to_string(pid, gnames_base, cname_idx, off).unwrap_or_default();
    format!("0x{:X} '{}' (class {})", obj_ptr, name, cname)
}

const MAX_INSTANCES: usize = 20;

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let gobjects_base = base + 171927000usize;
    let gnames_base = base + 173922192usize;
    let mut off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();
    off.gobjects_num_elements = 0x0C;
    off.gobjects_chunked = false;

    let num_elements = magic_wand_lib::engine::read_u32(pid, gobjects_base + off.gobjects_num_elements).unwrap() as usize;
    let objects_ptr = magic_wand_lib::engine::read_ptr(pid, gobjects_base + off.gobjects_objects).unwrap();
    println!("Scanning {} objects for live WeaponMode-derived instances...", num_elements);

    let mut printed = 0usize;
    for i in 0..num_elements {
        if printed >= MAX_INSTANCES { break; }
        let item_addr = objects_ptr + i * off.fuobjectitem_size;
        let raw_ptr = match magic_wand_lib::engine::read_ptr(pid, item_addr + off.fuobjectitem_object) {
            Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p,
            _ => continue,
        };
        let obj_ptr = raw_ptr & !7;

        let class_ptr = match magic_wand_lib::engine::read_ptr(pid, obj_ptr + off.uobject_class) {
            Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p & !7,
            _ => continue,
        };
        if !class_inherits_from(pid, gnames_base, class_ptr, "WeaponMode", &off) { continue; }

        let cname_idx = magic_wand_lib::engine::read_u32(pid, class_ptr + off.uobject_name).unwrap_or(0) as i32;
        let cname = fname_to_string(pid, gnames_base, cname_idx, &off).unwrap_or_default();
        if cname == "WeaponMode" { continue; } // skip the CDO's own exact class match noise if any

        // Skip Class Default Objects (flags & 0x10) - not live instances.
        if let Some(flags) = magic_wand_lib::engine::read_u32(pid, obj_ptr + 0x08) {
            if flags & 0x10 != 0 { continue; }
        }
        let base_dmg_118 = magic_wand_lib::engine::read_memory(pid, obj_ptr + 0x118, 4).ok()
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()));
        let base_dmg_178 = magic_wand_lib::engine::read_memory(pid, obj_ptr + 0x178, 4).ok()
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()));

        println!("{} | BaseDamage@0x118={:?} @0x178={:?}", describe(pid, gnames_base, obj_ptr, &off), base_dmg_118, base_dmg_178);
        // Outer chain, up to 6 hops
        let mut ptr = obj_ptr;
        for depth in 0..6 {
            let outer = magic_wand_lib::engine::read_ptr(pid, ptr + 0x20).unwrap_or(0);
            if outer == 0 { break; }
            println!("    outer[{}] -> {}", depth, describe(pid, gnames_base, outer, &off));
            ptr = outer;
        }
        printed += 1;
    }
    println!("Done. {} instance(s) printed.", printed);
}
