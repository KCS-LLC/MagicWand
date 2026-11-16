// Read-only: resolve the player's currently-equipped weapon's AmmoPool object, read its
// vtable, and print the real virtual ConsumeAmmo implementation at vtable+0x300 (the
// generic execConsumeAmmo Kismet thunk at a fixed RVA just calls this virtually per
// ammo-pool subclass, so THIS is the address that actually needs disassembling).
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
    let equip_comp = magic_wand_lib::engine::read_ptr(pid, pawn + 0x17D8).expect("no equip comp") & !7;
    let weapon = magic_wand_lib::engine::read_ptr(pid, equip_comp + 0x530).expect("no weapon") & !7;
    println!("EquippedWeapon: 0x{:X} class={:?}", weapon, class_name_of(pid, gnames_base, weapon, &off));

    let ammo_pool = magic_wand_lib::engine::read_ptr(pid, weapon + 0x1E38).unwrap_or(0) & !7;
    println!("AmmoPool: 0x{:X} class={:?}", ammo_pool, class_name_of(pid, gnames_base, ammo_pool, &off));
    if ammo_pool == 0 { println!("NULL - weapon may not use ammo pool"); return; }

    let vtable = magic_wand_lib::engine::read_ptr(pid, ammo_pool).unwrap_or(0);
    println!("vtable: 0x{:X}", vtable);
    let impl_addr = magic_wand_lib::engine::read_ptr(pid, vtable + 0x300).unwrap_or(0);
    println!("ConsumeAmmo virtual impl: 0x{:X}  (RVA 0x{:X})", impl_addr, impl_addr - base);

    // Also print CurrentPoolSize (GetCurrentPoolSize's likely backing field) region for
    // later cross-reference once we see the real decrement instruction's target offset.
    if let Ok(bytes) = magic_wand_lib::engine::read_memory(pid, ammo_pool, 0x40) {
        println!("AmmoPool+0x0..0x40: {:02X?}", bytes);
    }
    let pool_size = magic_wand_lib::engine::read_u32(pid, ammo_pool + 0x140);
    println!("AmmoPool+0x140 (CurrentPoolSize): {:?}", pool_size);
}
