// One-shot read-only verification of the planned outer-worlds-2.json ue5_prop offsets
// against the live process. TheOuterWorlds2-Win64-Shipping.exe uses a FLAT (non-chunked)
// TUObjectArray layout (confirmed via its own Dumper-7-generated Basic.hpp), unlike BL4's
// chunked layout, so this uses find_object_by_class directly with a custom Ue5Offsets
// (gobjects_chunked: false) instead of the chunked-only default. Does not write anything.

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe")
        .expect("Could not find running process TheOuterWorlds2-Win64-Shipping.exe");
    println!("PID: {}", pid);

    let (base, _size) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe")
        .expect("Could not find module info");
    println!("Base: 0x{:X}", base);

    let gobjects_base = base + 171927000usize;
    let gnames_base = base + 173922176usize + 0x10;

    let mut off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();
    off.gobjects_objects = 0x00;
    off.gobjects_num_elements = 0x0C;
    off.gobjects_chunked = false;

    match magic_wand_lib::ue5::find_object_by_class(pid, gobjects_base, gnames_base, "IndianaPlayerController", &off) {
        Ok(obj_ptr) => {
            println!("[GObjects] Found IndianaPlayerController at 0x{:X}", obj_ptr);

            // Controller -> +0x368 (Pawn, deref) -> +0x17E0 (PlayerHealthComponent, deref) -> +0x418 (bGodMode byte)
            let initial = obj_ptr + 0x368;
            match magic_wand_lib::engine::resolve_pointer_path(pid, initial, &[0x17E0, 0x418]) {
                Ok(addr) => {
                    let byte = magic_wand_lib::engine::read_memory(pid, addr, 1).ok().map(|b| b[0]);
                    println!("[God Mode] resolved addr=0x{:X} current byte={:?} (bGodMode -> {:?})",
                        addr, byte, byte.map(|b| b != 0));
                }
                Err(e) => println!("[God Mode] chain FAILED: {}", e),
            }
        }
        Err(e) => println!("[GObjects] FAILED to find IndianaPlayerController: {}", e),
    }
}
