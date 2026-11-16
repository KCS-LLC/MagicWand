// Read-only: resolves Find The Needle's money + stamina via godot::resolve_member and prints
// the live values (twice: cold scan, then hinted revalidation). Writes nothing.
use magic_wand_lib::{engine, godot};
fn main() {
    let exe = "FindTheNeedle.exe";
    let pid = engine::find_process_by_name(exe).expect("process not found");
    let mut money_tags = vec![-1i64; 40];
    money_tags[0] = 4;
    for i in [31, 35, 36, 37, 38, 39] { money_tags[i] = 3; }
    let stam_tags = [3, 3, 3, 3, 1, 2, 2, -1, 1, 1];
    for (name, count, tags, idx) in [
        ("money", 115usize, money_tags.as_slice(), 31usize),
        ("stamina", 10, &stam_tags[..], 0),
    ] {
        let t = std::time::Instant::now();
        let (inst, addr) = godot::resolve_member(pid, exe, 0x5B13B90, count, tags, idx, None).expect("resolve");
        let cold = t.elapsed();
        let t = std::time::Instant::now();
        let again = godot::resolve_member(pid, exe, 0x5B13B90, count, tags, idx, Some(inst)).expect("hint");
        println!("{name}: inst 0x{inst:X} addr 0x{addr:X} value {:?} (cold {:?}, hinted {:?}, same={})",
            engine::read_double(pid, addr), cold, t.elapsed(), again.1 == addr);
        if name == "money" {
            println!("  mirror (+192): {:?}", engine::read_double(pid, addr + 192));
        }
    }
}
