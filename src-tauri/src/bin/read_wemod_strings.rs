fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("TheOuterWorlds2-Win64-Shipping.exe").unwrap();
    match magic_wand_lib::engine::read_module_strings(pid, "Trainer_49038_163ccb59f0.dll", 5) {
        Ok(lines) => {
            println!("{} strings found", lines.len());
            for l in &lines { println!("{}", l); }
        }
        Err(e) => println!("Error: {}", e),
    }
}
