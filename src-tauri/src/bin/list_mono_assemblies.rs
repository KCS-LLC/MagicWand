// Read-only: print every assembly name loaded in a Mono game's root domain.
//   list_mono_assemblies "<process>"
use magic_wand_lib::{engine, mono};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 { eprintln!("usage: list_mono_assemblies <process>"); return; }
    let pid = engine::find_process_by_name(&a[1]).or_else(|| engine::find_process_by_name(&format!("{}.exe", a[1]))).expect("process not found");
    let (base, _) = engine::get_module_info(pid, "mono-2.0-bdwgc.dll").expect("no mono module");
    match mono::list_assembly_names(pid, base) {
        Ok(names) => { println!("{} assemblies", names.len()); for n in names { println!("  {}", n); } }
        Err(e) => println!("ERR: {}", e),
    }
}
