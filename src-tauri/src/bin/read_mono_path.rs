// Read-only: resolve a Mono static-singleton field path and print the i32/f32 at the end.
//   [ASM=<assembly>] [NS=<singleton namespace>] read_mono_path "<process>" <Class> <staticField> <Class.field> [<Class.field> ...]
// Class may be Namespace.Class.field (last dot = field, previous dot = class).
use magic_wand_lib::{engine, mono};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 5 { eprintln!("usage: read_mono_path <process> <Class> <staticField> <Class.field>..."); return; }
    let pid = engine::find_process_by_name(&a[1]).or_else(|| engine::find_process_by_name(&format!("{}.exe", a[1]))).expect("process not found");
    let (base, _) = engine::get_module_info(pid, "mono-2.0-bdwgc.dll").expect("no mono module");
    let hops: Vec<mono::MonoHop> = a[4..].iter().map(|s| {
        // Optional `@<int>` suffix = dictionary key (see MonoHop::dict_key).
        let (s, dict_key) = match s.rsplit_once('@') { Some((a, k)) => (a, Some(k.parse::<i32>().expect("key"))), None => (s.as_str(), None) };
        let (cls, field) = s.rsplit_once('.').expect("Class.field");
        let (ns, class_name) = cls.rsplit_once('.').unwrap_or(("", cls));
        mono::MonoHop { namespace: ns.to_string(), class_name: class_name.to_string(), field: field.to_string(), dict_key }
    }).collect();
    match mono::resolve_mono_path(pid, base, &std::env::var("ASM").unwrap_or("Assembly-CSharp".into()), &std::env::var("NS").unwrap_or_default(), &a[2], &a[3], &hops) {
        Ok(addr) => {
            if let Some(n) = std::env::var("DUMP").ok().and_then(|v| v.parse::<usize>().ok()) {
                // Treat the resolved field as a reference and dump the object it points at.
                let p = u64::from_le_bytes(engine::read_memory(pid, addr, 8).unwrap().try_into().unwrap()) as usize;
                println!("field@0x{:X} -> obj 0x{:X}", addr, p);
                if let Ok(d) = engine::read_memory(pid, p, n) {
                    for (i, c) in d.chunks(8).enumerate() { println!("  +0x{:02X}: {:016X}", i * 8, u64::from_le_bytes(c.try_into().unwrap_or([0; 8]))); }
                }
            }
            let b = engine::read_memory(pid, addr, 4).unwrap();
            let raw = [b[0], b[1], b[2], b[3]];
            println!("addr=0x{:X} i32={} f32={}", addr, i32::from_le_bytes(raw), f32::from_le_bytes(raw));
        }
        Err(e) => println!("ERR: {}", e),
    }
}
