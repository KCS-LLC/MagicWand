// One-shot read-only check: locate a Mono method's JIT-compiled body in a running Unity
// game and dump its first bytes. Usage:
//   [ASM=<assembly>] find_mono_method_jit "<process name>" <Class> <Method> [Namespace]
use magic_wand_lib::{engine, mono};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: find_mono_method_jit <process> <Class> <Method> [Namespace]");
        return;
    }
    let (proc_name, class, method) = (&args[1], &args[2], &args[3]);
    let namespace = args.get(4).map(String::as_str).unwrap_or("");

    let pid = engine::find_process_by_name(proc_name)
        .or_else(|| engine::find_process_by_name(&format!("{}.exe", proc_name)))
        .expect("process not found");
    println!("PID: {}", pid);
    let (mono_base, _) = engine::get_module_info(pid, "mono-2.0-bdwgc.dll")
        .expect("mono-2.0-bdwgc.dll not loaded");
    println!("mono base: 0x{:X}", mono_base);

    if args.len() > 5 {
        match mono::find_field_store_sites(pid, mono_base, "Assembly-CSharp", namespace, class, method, &args[5]) {
            Ok(sites) => {
                println!("{} store site(s) to '{}':", sites.len(), args[5]);
                for (a, l) in sites {
                    let b = engine::read_memory(pid, a, l).unwrap_or_default();
                    println!("  0x{:X} len={}  {}", a, l, b.iter().map(|x| format!("{:02X}", x)).collect::<Vec<_>>().join(" "));
                }
            }
            Err(e) => println!("store scan: {}", e),
        }
    }
    // Dry run of the IL pre-patch finder (reads only): env ILSIG = hex signature.
    if let Ok(sig) = std::env::var("ILSIG") {
        let bytes: Vec<u8> = sig.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect();
        let patches: Vec<mono::IlPatch> = [0xf3usize, 0x140, 0x35b, 0x3d5, 0x40f, 0x430].iter().map(|&o| mono::IlPatch {
            offset: o, expect: vec![0x28, 0xa9, 0x0b, 0x00, 0x06], replace: vec![0x26, 0x26, 0, 0, 0] }).collect();
        match mono::il_patch_sites(pid, &bytes, &patches) {
            Ok(sites) => { println!("IL sites ({}):", sites.len()); for (a, o, _) in sites { println!("  0x{:X}  {:02X?}", a, o); } }
            Err(e) => println!("IL sites: {}", e),
        }
    }
    for f in args.iter().skip(5) {
        match mono::instance_field_offset(pid, mono_base, "Assembly-CSharp", namespace, class, f) {
            Ok(o) => println!("field {} offset = 0x{:X} ({})", f, o, o),
            Err(e) => println!("field {} : {}", f, e),
        }
    }
    match mono::locate_method_jit(pid, mono_base, &std::env::var("ASM").unwrap_or("Assembly-CSharp".into()), namespace, class, method) {
        Ok(hits) => {
            for h in &hits {
                println!(
                    "MonoMethod=0x{:X} (klass+0x{:X})  JitInfo=0x{:X}  code_start=0x{:X}  size={}",
                    h.method, h.methods_off, h.jit_info, h.code_start, h.code_size
                );
                if let Ok(code) = engine::read_memory(pid, h.code_start, h.code_size) {
                    let path = format!("{}/jit_{}_{}.bin", std::env::temp_dir().display(), class, method);
                    std::fs::write(&path, &code).ok();
                    println!("  full body -> {}", path);
                }
                match engine::read_memory(pid, h.code_start, 48) {
                    Ok(b) => println!(
                        "  bytes: {}",
                        b.iter().map(|x| format!("{:02X}", x)).collect::<Vec<_>>().join(" ")
                    ),
                    Err(e) => println!("  read failed: {}", e),
                }
            }
        }
        Err(e) => println!("FAILED: {}", e),
    }
}
