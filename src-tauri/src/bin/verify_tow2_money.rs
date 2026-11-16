// One-shot read-only verification of the tow2-money array-item resolver against the
// exact chain/offsets used in outer-worlds-2.json. Does not write anything.
fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe")
        .expect("process not found");
    let (base, _) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe")
        .expect("module not found");

    let gobjects_base = base + 171927000usize;
    let gnames_base = base + 173922192usize;

    let mut off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();
    off.gobjects_num_elements = 0x0C;
    off.gobjects_chunked = false;

    // IndianaPlayerController -> +0x368 (Pawn, deref) -> +0x17D0 (PlayerInventoryComponent,
    // deref) -> scan TArray<FItemStack> at +0x1C0 for the first entry whose Item's class
    // inherits from CurrencyItem (live instance found: Bit_Cartridge_C) -> +0x08 (Count).
    match magic_wand_lib::ue5::resolve_ue5_array_item(
        pid, gobjects_base, gnames_base,
        "IndianaPlayerController",
        0x368,
        &[0x17D0],
        0x1C0,
        0x18,
        0x00,
        "CurrencyItem",
        0x08,
        &off,
    ) {
        Ok(addr) => {
            let val = magic_wand_lib::engine::read_memory(pid, addr, 4)
                .ok()
                .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            println!("[Money] resolved addr=0x{:X} current Count={:?}", addr, val);
        }
        Err(e) => println!("[Money] FAILED to resolve: {}", e),
    }
}
