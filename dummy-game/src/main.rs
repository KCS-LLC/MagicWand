use std::thread::sleep;
use std::time::Duration;

#[repr(C)]
struct GameData {
    marker: [u8; 16],
    health: i32,
    gold: i32,
}

static mut DATA: GameData = GameData {
    marker: [
        0xDE, 0xAD, 0xBE, 0xEF, 0x13, 0x37, 0x13, 0x37, 
        0x42, 0x42, 0x42, 0x42, 0xAA, 0xBB, 0xCC, 0xDD
    ],
    health: 100,
    gold: 500,
};

extern "C" {
    fn GetModuleHandleW(lpModuleName: *const u16) -> *mut std::ffi::c_void;
}

fn main() {
    let base_address = unsafe { GetModuleHandleW(std::ptr::null()) as usize };
    let data_address = unsafe { &DATA as *const _ as usize };
    
    println!("=== MAGIC WAND DUMMY GAME (AOB EDITION) ===");
    println!("Process ID: {}", std::process::id());
    println!("Base Address: 0x{:X}", base_address);
    println!("Data Address (Marker): 0x{:X}", data_address);
    println!("Signature: DE AD BE EF 13 37 13 37 42 42 42 42 AA BB CC DD");
    
    println!("---------------------------------------------");
    println!("Health is at Marker + 16 bytes (0x10)");
    println!("Gold is at Marker + 20 bytes (0x14)");
    println!("---------------------------------------------");
    println!("Press Ctrl+C to exit.");

    loop {
        unsafe {
            println!("Status | Health: {} | Gold: {}", DATA.health, DATA.gold);
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
