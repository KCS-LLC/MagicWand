// Read-only: find objects that hold a pointer to `<address>` and print holder class + field offset.
//   find_refs_to "<process>" <hex address>
use magic_wand_lib::engine;

fn cstr(pid: u32, p: usize) -> String {
    engine::read_memory(pid, p, 48).map(|b| b.iter().take_while(|c| **c != 0).map(|c| *c as char).collect()).unwrap_or_default()
}

fn class_of(pid: u32, obj: usize) -> Option<String> {
    let vt = engine::read_ptr(pid, obj).filter(|p| *p > 0x10000)?;
    let k = engine::read_ptr(pid, vt).filter(|p| *p > 0x10000)?;
    let np = engine::read_ptr(pid, k.wrapping_add(0x48)).filter(|p| *p > 0x10000)?;
    let n = cstr(pid, np);
    (n.len() >= 2 && n.chars().all(|c| c.is_ascii_graphic())).then_some(n)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 { eprintln!("usage: find_refs_to <process> <hex address>"); return; }
    let pid = engine::find_process_by_name(&a[1]).or_else(|| engine::find_process_by_name(&format!("{}.exe", a[1]))).expect("process not found");
    let target = usize::from_str_radix(a[2].trim_start_matches("0x"), 16).expect("hex");
    for hit in engine::scan_aligned_u64(pid, target as u64) {
        // Holder object starts at most 0x200 bytes before the slot; take the nearest start with a valid class.
        let holder = (0x10..0x200usize).step_by(8).filter_map(|d| hit.checked_sub(d).map(|o| (d, o)))
            .find_map(|(d, o)| class_of(pid, o).map(|c| (d, o, c)));
        match holder { Some((d, o, c)) => println!("slot 0x{:X}: {} obj=0x{:X} +0x{:X}", hit, c, o, d), None => println!("slot 0x{:X}: (no object)", hit) }
    }
}
