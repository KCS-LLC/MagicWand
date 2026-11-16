// Read-only: walk the exact same chain God Mode's ue5_prop cheat uses
// (IndianaPlayerController -> +0x368 Pawn -> +0x17E0 PlayerHealthComponent -> +0x418
// bGodMode) and print the live byte value, to check whether the app's write is actually
// landing on the field the game reads for damage immunity.
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
    println!("Controller: 0x{:X}", controller);
    let pawn = magic_wand_lib::engine::read_ptr(pid, controller + 0x368).expect("no pawn ptr") & !7;
    println!("Pawn: 0x{:X}", pawn);
    let health_comp = magic_wand_lib::engine::read_ptr(pid, pawn + 0x17E0).unwrap_or(0) & !7;
    println!("PlayerHealthComponent: 0x{:X}", health_comp);
    if health_comp == 0 { println!("NULL"); return; }
    let god_mode_addr = health_comp + 0x418;
    let val = magic_wand_lib::engine::read_memory(pid, god_mode_addr, 1).ok().map(|b| b[0]);
    println!("bGodMode @ 0x{:X} = {:?}", god_mode_addr, val);

    // Also dump a wider window around it in case the field shifted.
    if let Ok(bytes) = magic_wand_lib::engine::read_memory(pid, health_comp + 0x400, 0x40) {
        println!("HealthComponent+0x400..0x440: {:02X?}", bytes);
    }
}
