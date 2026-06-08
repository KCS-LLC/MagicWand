use std::thread::sleep;
use std::time::Duration;

static mut HEALTH: i32 = 100;
static mut GOLD: i32 = 500;

extern "C" {
    fn GetModuleHandleW(lpModuleName: *const u16) -> *mut std::ffi::c_void;
}

fn main() {
    let base_address = unsafe { GetModuleHandleW(std::ptr::null()) as usize };
    
    println!("=== MAGIC WAND DUMMY GAME ===");
    println!("Process ID: {}", std::process::id());
    println!("Base Address: 0x{:X}", base_address);
    unsafe {
        println!("Health Offset: 0x{:X}", (&raw const HEALTH as usize) - base_address);
        println!("Gold Offset: 0x{:X}", (&raw const GOLD as usize) - base_address);
    }
    println!("=============================");
    println!("Press Ctrl+C to exit.");

    loop {
        unsafe {
            println!("Status | Health: {} | Gold: {}", HEALTH, GOLD);
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
