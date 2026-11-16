// Read-only: find Mono objects of a given class that hold a float value.
//   find_mono_stat "<process>" <ClassName> <float> [<float>...]
// Scans memory for each f32, then keeps hits that sit 0x10..0x40 bytes into an object whose
// vtable->klass name matches (class name offset 0x48, same as mono.rs).
use magic_wand_lib::engine;

fn cstr(pid: u32, p: usize) -> String {
    engine::read_memory(pid, p, 48).map(|b| b.iter().take_while(|c| **c != 0).map(|c| *c as char).collect()).unwrap_or_default()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 { eprintln!("usage: find_mono_stat <process> <Class> <float>..."); return; }
    let pid = engine::find_process_by_name(&a[1]).or_else(|| engine::find_process_by_name(&format!("{}.exe", a[1]))).expect("process not found");
    for v in &a[3..] {
        let f: f32 = v.parse().expect("float");
        let raw = std::env::var("INT").is_ok();
        let hits = engine::scan_bytes_streaming(pid, &if raw { (f as i32).to_le_bytes() } else { f.to_le_bytes() });
        println!("{} hits for {}", hits.len(), f);
        for h in hits {
            let fixed = std::env::var("FO").ok().and_then(|v| usize::from_str_radix(v.trim_start_matches("0x"), 16).ok());
            for fo in (0x10..0x90usize).step_by(4).filter(|f| fixed.map_or(true, |x| *f == x)) {
                let Some(obj) = h.checked_sub(fo) else { continue };
                let Some(vt) = engine::read_ptr(pid, obj).filter(|p| *p > 0x10000) else { continue };
                let Some(k) = engine::read_ptr(pid, vt).filter(|p| *p > 0x10000) else { continue };
                let Some(np) = engine::read_ptr(pid, k.wrapping_add(0x48)).filter(|p| *p > 0x10000) else { continue };
                let n = cstr(pid, np); if a[2] == "*" && n.chars().all(|c| c.is_ascii_graphic()) && n.len() > 2 || n == a[2] { println!("  {} obj=0x{:X} field_off=0x{:X} value_addr=0x{:X}", n, obj, fo, h); }
            }
        }
    }
}
