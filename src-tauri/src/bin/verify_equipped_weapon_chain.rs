// Read-only: walk the static chain recovered from the real Dumper-7 SDK dump
// (Arkansas_classes.hpp) instead of scanning/guessing:
//   IndianaPlayerController -> +0x368 (Pawn)
//     -> +0x17D8 (AIndianaPlayerCharacter::PlayerEquipmentComponent)
//     -> +0x530  (UEquipmentComponent::EquippedWeapon, a UWeapon*)
//     -> +0x1E10 (UWeapon::PrimaryMode, a UWeaponMode*)
// Confirms this resolves to a live WeaponMode instance, and prints whether it's the
// same instance our own tow2-damage-multiplier cave hooks via GetBaseDamage().
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

    let controller = magic_wand_lib::ue5::find_object_by_class(pid, gobjects_base, gnames_base, "IndianaPlayerController", &off)
        .expect("controller not found");
    let pawn = magic_wand_lib::engine::read_ptr(pid, controller + 0x368).expect("no pawn ptr") & !7;
    println!("Pawn: 0x{:X}", pawn);

    let equip_comp = magic_wand_lib::engine::read_ptr(pid, pawn + 0x17D8).unwrap_or(0) & !7;
    println!("PlayerEquipmentComponent: 0x{:X}", equip_comp);
    if equip_comp == 0 { println!("NULL - not equipped/spawned yet"); return; }

    let cname = class_name_of(pid, gnames_base, equip_comp, &off);
    println!("  class: {:?}", cname);

    let weapon = magic_wand_lib::engine::read_ptr(pid, equip_comp + 0x530).unwrap_or(0) & !7;
    println!("EquippedWeapon: 0x{:X}", weapon);
    if weapon == 0 { println!("NULL - no weapon equipped right now"); return; }
    println!("  class: {:?}", class_name_of(pid, gnames_base, weapon, &off));

    let weapon_mode = magic_wand_lib::engine::read_ptr(pid, weapon + 0x1E10).unwrap_or(0) & !7;
    println!("PrimaryMode (WeaponMode): 0x{:X}", weapon_mode);
    if weapon_mode == 0 { println!("NULL"); return; }
    println!("  class: {:?}", class_name_of(pid, gnames_base, weapon_mode, &off));

    let base_dmg_118 = magic_wand_lib::engine::read_memory(pid, weapon_mode + 0x118, 4).ok()
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()));
    let base_dmg_178 = magic_wand_lib::engine::read_memory(pid, weapon_mode + 0x178, 4).ok()
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()));
    println!("  BaseDamage @ +0x118 = {:?}, @ +0x178 = {:?}", base_dmg_118, base_dmg_178);

    println!("(module base=0x{:X})", base);
}
