// One-shot verification that the updated borderlands-4.json offsets actually resolve
// end-to-end against the live process, using the exact same resolve_ue5_prop_static path
// the real Tauri app uses. Read-only — does not write/toggle anything.

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("Borderlands4")
        .or_else(|| magic_wand_lib::engine::find_process_by_name("Borderlands4.exe"))
        .expect("Could not find running process Borderlands4.exe");
    println!("PID: {}", pid);

    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "Borderlands4.exe")
        .expect("Could not find module info for Borderlands4.exe");
    println!("Base: 0x{:X}", base);

    let gobjects_offset = 210311752usize;
    let gnames_offset = 209370512usize;

    // Invincibility: PlayerController -> +976 (Pawn) -> +98 (bCanBeDamaged byte, bit 2)
    match magic_wand_lib::ue5::resolve_ue5_prop_static(pid, base, size, gobjects_offset, gnames_offset, "OakPlayerController", 976, &[98]) {
        Ok(addr) => {
            let byte = magic_wand_lib::engine::read_memory(pid, addr, 1).ok().map(|b| b[0]);
            println!("[Invincibility] resolved addr=0x{:X} current byte={:?} (bCanBeDamaged bit2 -> {:?})",
                addr, byte, byte.map(|b| (b & 0x04) != 0));
        }
        Err(e) => println!("[Invincibility] FAILED to resolve: {}", e),
    }

    // One Hit Kill: PlayerController -> +976 (Pawn) -> +19416 (DamageDealtMultiplier.Value)
    match magic_wand_lib::ue5::resolve_ue5_prop_static(pid, base, size, gobjects_offset, gnames_offset, "OakPlayerController", 976, &[19416]) {
        Ok(addr) => {
            let bytes = magic_wand_lib::engine::read_memory(pid, addr, 4).ok();
            let val = bytes.map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            println!("[One Hit Kill] resolved addr=0x{:X} current DamageDealtMultiplier.Value={:?}", addr, val);
        }
        Err(e) => println!("[One Hit Kill] FAILED to resolve: {}", e),
    }
}
