// Read-only: scan the live player Pawn's own memory for a pointer to a live WeaponMode
// instance (or an intermediate Weapon-class object that itself points to one), to recover
// the pointer chain PlayerController -> Pawn -> ... -> WeaponMode for a direct property
// write cheat (same pattern as God Mode/Money) instead of a fragile code_cave.
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

fn class_name_of(pid: u32, gnames_base: usize, obj_ptr: usize, off: &magic_wand_lib::ue5::Ue5Offsets) -> Option<String> {
    let class_ptr = magic_wand_lib::engine::read_ptr(pid, obj_ptr + off.uobject_class)? & !7;
    if class_ptr == 0 { return None; }
    let idx = magic_wand_lib::engine::read_u32(pid, class_ptr + off.uobject_name)? as i32;
    fname_to_string(pid, gnames_base, idx, off)
}

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let gobjects_base = base + 171927000usize;
    let gnames_base = base + 173922192usize;
    let mut off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();
    off.gobjects_num_elements = 0x0C;
    off.gobjects_chunked = false;

    let controller = magic_wand_lib::ue5::find_object_by_class(
        pid, gobjects_base, gnames_base, "IndianaPlayerController", &off,
    ).expect("controller not found");
    let pawn = magic_wand_lib::engine::read_ptr(pid, controller + 0x368).expect("no pawn ptr");
    println!("Pawn @ 0x{:X}", pawn);

    let inventory = magic_wand_lib::engine::read_ptr(pid, pawn + 0x17D0).unwrap_or(0);
    println!("InventoryComponent @ 0x{:X}", inventory);

    // Scan Pawn's own fields, and the InventoryComponent's fields, (8-byte aligned) for any
    // pointer that looks like a live UObject whose class name contains "Weapon".
    let scan_len: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0x800);
    let mut regions = vec![("Pawn", pawn)];
    if inventory != 0 && inventory <= 0x0000_7FFF_FFFF_FFFF {
        regions.push(("Inventory", inventory));
    }
    for (region_name, region_ptr) in regions {
    let pawn_bytes = magic_wand_lib::engine::read_memory(pid, region_ptr, scan_len).unwrap();
    for off_bytes in (0..pawn_bytes.len() - 8).step_by(8) {
        let ptr = usize::from_le_bytes(pawn_bytes[off_bytes..off_bytes + 8].try_into().unwrap());
        if ptr == 0 || ptr > 0x0000_7FFF_FFFF_FFFF || ptr & 7 != 0 { continue; }
        let cname = match class_name_of(pid, gnames_base, ptr, &off) {
            Some(c) => c,
            None => continue,
        };
        if cname.to_lowercase().contains("weapon") {
            println!("{}+0x{:X} -> 0x{:X} (class {})", region_name, off_bytes, ptr, cname);
            // One level deeper: scan this weapon object's own fields for a WeaponMode ptr.
            if let Ok(wbytes) = magic_wand_lib::engine::read_memory(pid, ptr, 0x400) {
                for woff in (0..wbytes.len() - 8).step_by(8) {
                    let wptr = usize::from_le_bytes(wbytes[woff..woff + 8].try_into().unwrap());
                    if wptr == 0 || wptr > 0x0000_7FFF_FFFF_FFFF || wptr & 7 != 0 { continue; }
                    if let Some(wcname) = class_name_of(pid, gnames_base, wptr, &off) {
                        if wcname.to_lowercase().contains("weaponmode") {
                            println!("  {}+0x{:X} -> 0x{:X} (class {})", cname, woff, wptr, wcname);
                            let base_dmg_118 = magic_wand_lib::engine::read_memory(pid, wptr + 0x118, 4).ok()
                                .map(|b| f32::from_le_bytes(b.try_into().unwrap()));
                            let base_dmg_178 = magic_wand_lib::engine::read_memory(pid, wptr + 0x178, 4).ok()
                                .map(|b| f32::from_le_bytes(b.try_into().unwrap()));
                            println!("    BaseDamage @ +0x118 = {:?}, @ +0x178 = {:?}", base_dmg_118, base_dmg_178);
                        }
                    }
                }
            }
        }
    }
    }
    println!("(base=0x{:X})", base);
}
