// Read-only: find every virtual-call site that invokes vtable slot 0x430 (GetBaseDamage's
// slot, per the existing damage-multiplier cheat's own _comment — "resolved live via the
// object's vtable slot 0x430") — i.e. `call qword ptr [rax+0x430]`, bytes FF 90 30 04 00 00.
// This is a specific 6-byte pattern, not a generic single-opcode scan (see find_functions.rs
// caution comment about flooding/hanging on this large a module), so should return a small,
// safe number of real call sites instead. For each hit, dump surrounding context for manual
// disassembly to find what register holds instigator/owner info at that call.
fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe")
        .expect("game not running");
    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    println!("Base: 0x{:X} Size: 0x{:X}", base, size);

    let hits = magic_wand_lib::engine::aob_scan_all_range(pid, base, size, "FF 90 30 04 00 00");
    println!("'FF 90 30 04 00 00' (call [rax+0x430]) matches: {}", hits.len());
    for m in hits.iter().take(30) {
        let rva = m - base;
        println!("  0x{:X} (RVA 0x{:X})", m, rva);
    }
}
