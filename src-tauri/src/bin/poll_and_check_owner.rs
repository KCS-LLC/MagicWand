// Read-only: poll all live WeaponMode-derived instances for ~20s; for any that change,
// immediately (same process, no gap) check the +0x20 owner chain up to 2 hops and print
// whether it resolves to a Pawn/Character/Controller, and if so, print that object's raw
// pointer so it can be compared against the live player Pawn/Controller addresses. Combines
// poll_weaponmode_attack + find_weaponmode_owner into one pass to avoid objects going stale
// between two separate tool runs.
use std::time::{Duration, Instant};

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

fn check_owner(pid: u32, gnames_base: usize, wm: usize, off: &magic_wand_lib::ue5::Ue5Offsets) {
    let hop1 = match magic_wand_lib::engine::read_ptr(pid, wm + 0x20) {
        Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p & !7,
        _ => { println!("    owner: hop1 null/invalid"); return; }
    };
    for target in ["Pawn", "Character", "Controller"] {
        let class_ptr = magic_wand_lib::engine::read_ptr(pid, hop1 + off.uobject_class).unwrap_or(0) & !7;
        if class_inherits_from(pid, gnames_base, class_ptr, target, off) {
            println!("    owner (hop1) = {} [{}]", describe(pid, gnames_base, hop1, off), target);
            return;
        }
    }
    let hop2 = match magic_wand_lib::engine::read_ptr(pid, hop1 + 0x20) {
        Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p & !7,
        _ => { println!("    owner: hop1={} (not Pawn), hop2 null/invalid", describe(pid, gnames_base, hop1, off)); return; }
    };
    for target in ["Pawn", "Character", "Controller"] {
        let class_ptr = magic_wand_lib::engine::read_ptr(pid, hop2 + off.uobject_class).unwrap_or(0) & !7;
        if class_inherits_from(pid, gnames_base, class_ptr, target, off) {
            println!("    owner (hop2) = {} [{}]  (hop1 was {})", describe(pid, gnames_base, hop2, off), target, describe(pid, gnames_base, hop1, off));
            return;
        }
    }
    println!("    owner: hop1={}, hop2={} — neither is Pawn/Character/Controller", describe(pid, gnames_base, hop1, off), describe(pid, gnames_base, hop2, off));
}

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let gobjects_base = base + 171927000usize;
    let gnames_base = base + 173922192usize;
    let mut off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();
    off.gobjects_num_elements = 0x0C;
    off.gobjects_chunked = false;

    // Print current live player controller/pawn for reference.
    let controller = magic_wand_lib::ue5::find_object_by_class(pid, gobjects_base, gnames_base, "IndianaPlayerController", &off);
    if let Ok(c) = controller {
        println!("Player controller: 0x{:X}", c);
        if let Some(p) = magic_wand_lib::engine::read_ptr(pid, c + 0x368) {
            println!("Player pawn: 0x{:X}", p);
        }
    }

    let num_elements = magic_wand_lib::engine::read_u32(pid, gobjects_base + off.gobjects_num_elements).unwrap() as usize;
    let objects_ptr = magic_wand_lib::engine::read_ptr(pid, gobjects_base + off.gobjects_objects).unwrap();

    let mut instances: Vec<usize> = Vec::new();
    for i in 0..num_elements {
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
        if let Some(flags) = magic_wand_lib::engine::read_u32(pid, obj_ptr + 0x08) {
            if flags & 0x10 != 0 { continue; }
        }
        instances.push(obj_ptr);
    }
    println!("Watching {} live WeaponMode instance(s) for ~20s. FIRE YOUR WEAPON NOW.", instances.len());

    let mut baseline: Vec<Vec<u8>> = instances.iter()
        .map(|ptr| magic_wand_lib::engine::read_memory(pid, *ptr, 0x90).unwrap_or_default())
        .collect();

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        for (idx, ptr) in instances.iter().enumerate() {
            let cur = match magic_wand_lib::engine::read_memory(pid, *ptr, 0x90) {
                Ok(b) => b,
                Err(_) => continue,
            };
            if cur != baseline[idx] {
                println!("CHANGED {}", describe(pid, gnames_base, *ptr, &off));
                check_owner(pid, gnames_base, *ptr, &off);
                baseline[idx] = cur;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    println!("Done.");
}
