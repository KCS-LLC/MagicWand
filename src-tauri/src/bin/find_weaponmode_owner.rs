// Read-only: for given WeaponMode instance addresses, scan bytes [0x0, 0xB0) (the static
// portion, before the transient +0xB0 attack-context struct) for any 8-byte-aligned pointer
// that resolves to a live UObject whose class inherits from Pawn, Character, or Controller —
// i.e. an owner reference set once at equip time, not per-attack.
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

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let gnames_base = base + 173922192usize;
    let off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();

    let addrs: Vec<usize> = std::env::args().skip(1)
        .map(|s| usize::from_str_radix(s.trim_start_matches("0x"), 16).unwrap())
        .collect();

    for wm in addrs {
        println!("=== WeaponMode 0x{:X} ===", wm);
        let bytes = match magic_wand_lib::engine::read_memory(pid, wm, 0xB0) {
            Ok(b) => b,
            Err(e) => { println!("  read failed: {}", e); continue; }
        };
        for off_bytes in (0..bytes.len() - 8).step_by(8) {
            let ptr = usize::from_le_bytes(bytes[off_bytes..off_bytes + 8].try_into().unwrap());
            if ptr == 0 || ptr > 0x0000_7FFF_FFFF_FFFF || ptr & 7 != 0 { continue; }
            let class_ptr = match magic_wand_lib::engine::read_ptr(pid, ptr + off.uobject_class) {
                Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p & !7,
                _ => continue,
            };
            for target in ["Pawn", "Character", "Controller", "IndianaCharacter", "IndianaPlayerController"] {
                if class_inherits_from(pid, gnames_base, class_ptr, target, &off) {
                    println!("  +0x{:X} -> {} [matches '{}']", off_bytes, describe(pid, gnames_base, ptr, &off), target);
                    break;
                }
            }
            // Also walk this pointer's own Outer chain (offset +0x20) up to 3 hops, in case
            // +0x20 here is a Weapon actor (not the Pawn directly) whose own Outer/chain
            // eventually reaches a Pawn.
            let mut chain_ptr = ptr;
            for depth in 0..3 {
                let outer = match magic_wand_lib::engine::read_ptr(pid, chain_ptr + 0x20) {
                    Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p & !7,
                    _ => break,
                };
                let outer_class = match magic_wand_lib::engine::read_ptr(pid, outer + off.uobject_class) {
                    Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p & !7,
                    _ => break,
                };
                for target in ["Pawn", "Character", "Controller"] {
                    if class_inherits_from(pid, gnames_base, outer_class, target, &off) {
                        println!("  +0x{:X} (outer chain depth {}) -> {} [matches '{}']", off_bytes, depth + 1, describe(pid, gnames_base, outer, &off), target);
                    }
                }
                chain_ptr = outer;
            }
        }
    }
}
