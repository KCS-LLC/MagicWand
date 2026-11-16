// Read-only: resolve the actual virtual-function implementation behind GetBaseDamage's
// vtable slot (offset 0x430, confirmed via disassembly of the exec thunk: `mov rax,[rcx];
// call [rax+0x430]`) on the live player character instance, and print its RVA so we can
// disassemble the real damage-calculation code instead of guessing from a raw AOB hit.
fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    let gobjects_base = base + 171927000usize;
    let gnames_base = base + 173922192usize;
    let mut off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();
    off.gobjects_num_elements = 0x0C;
    off.gobjects_chunked = false;

    let class_name = std::env::args().nth(2).unwrap_or_else(|| "IndianaPlayerController".to_string());
    let instance = if class_name == "IndianaPlayerController" {
        let controller = magic_wand_lib::ue5::find_object_by_class(
            pid, gobjects_base, gnames_base, "IndianaPlayerController", &off,
        ).expect("controller not found");
        println!("IndianaPlayerController @ 0x{:X}", controller);
        let pawn = magic_wand_lib::engine::read_ptr(pid, controller + 0x368).expect("no pawn ptr");
        println!("Pawn @ 0x{:X}", pawn);
        assert!(pawn != 0, "Pawn is null - player must be loaded into a level with a possessed pawn");
        pawn
    } else {
        let inst = magic_wand_lib::ue5::find_object_by_class(
            pid, gobjects_base, gnames_base, &class_name, &off,
        ).expect("instance not found");
        println!("{} instance @ 0x{:X}", class_name, inst);
        inst
    };

    let vtable = magic_wand_lib::engine::read_ptr(pid, instance).expect("no vtable ptr");
    println!("VTable @ 0x{:X}", vtable);

    let slot: usize = std::env::args().nth(1)
        .map(|s| usize::from_str_radix(s.trim_start_matches("0x"), 16).unwrap())
        .unwrap_or(0x430);
    let func = magic_wand_lib::engine::read_ptr(pid, vtable + slot).expect("no func ptr");
    println!("VTable[0x{:X}] = 0x{:X} (RVA 0x{:X})", slot, func, func.wrapping_sub(base));
}
