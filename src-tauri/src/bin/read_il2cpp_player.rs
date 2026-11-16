// Read-only check of the Outward player paths through the Rust IL2CPP walker.
use magic_wand_lib::{engine, il2cpp::{self, Hop}};

fn field(ns: &str, c: &str, f: &str) -> Hop {
    Hop::Field { namespace: ns.into(), class_name: c.into(), field: f.into() }
}

fn player_to(then: Vec<Hop>) -> Vec<Hop> {
    let mut h = vec![
        field("", "CharacterManager", "m_characters"),
        Hop::Offset(0x18), // DictionaryExt.m_values (List<Character>)
        Hop::ListFindByte { namespace: "".into(), class_name: "Character".into(), field: "m_isAI".into(), equals: 0 },
    ];
    h.extend(then);
    h
}

fn main() {
    let pid = engine::find_process_by_name("Outward Definitive Edition")
        .or_else(|| engine::find_process_by_name("Outward Definitive Edition.exe"))
        .expect("Outward process not found");
    let (base, _) = engine::get_module_info(pid, "GameAssembly.dll").expect("GameAssembly.dll not loaded");
    println!("PID {} GameAssembly 0x{:X}", pid, base);
    match il2cpp::decode_table_global(pid, base) {
        Some(g) => println!("decoded type-table global: RVA 0x{:X} (fallback RVA is 0x285A788)", g - base),
        None => println!("decode FAILED — would use fallback"),
    }

    let read = |label: &str, hops: Vec<Hop>, float: bool| {
        match il2cpp::resolve_path(pid, base, "", "CharacterManager", "Instance", &hops) {
            Ok(a) => {
                let b = engine::read_memory(pid, a, 4).unwrap();
                let raw = [b[0], b[1], b[2], b[3]];
                if float { println!("{:<22} 0x{:X} = {}", label, a, f32::from_le_bytes(raw)); }
                else { println!("{:<22} 0x{:X} = {}", label, a, i32::from_le_bytes(raw)); }
            }
            Err(e) => println!("{:<22} ERR {}", label, e),
        }
    };
    read("silver", player_to(vec![
        field("", "Character", "m_inventory"),
        field("", "CharacterInventory", "m_inventoryPouch"),
        field("", "ItemContainer", "m_silverCount"),
    ]), false);
    read("health", player_to(vec![
        field("", "Character", "m_characterStats"),
        field("", "CharacterStats", "m_health"),
    ]), true);
    for f in ["m_food", "m_drink", "m_sleep"] {
        read(f, player_to(vec![
            field("", "Character", "m_characterStats"),
            field("", "PlayerCharacterStats", f),
        ]), true);
    }

    let stats = |extra: Vec<Hop>| { let mut v = vec![field("", "Character", "m_characterStats")]; v.extend(extra); player_to(v) };
    read("maxStamina(+0x14)", stats(vec![field("", "CharacterStats", "m_maxStamina"), Hop::Offset(20)]), true);
    read("maxMana(+0x14)", stats(vec![field("", "CharacterStats", "m_maxManaStat"), Hop::Offset(20)]), true);
    read("stamina", stats(vec![field("", "CharacterStats", "m_stamina")]), true);
    read("mana", stats(vec![field("", "CharacterStats", "m_mana")]), true);
    read("burntHealth", stats(vec![field("", "CharacterStats", "m_burntHealth")]), true);
    read("burntStamina", stats(vec![field("", "CharacterStats", "m_burntStamina")]), true);
    read("burntMana", stats(vec![field("", "CharacterStats", "m_burntMana")]), true);
    read("breakthroughUsed", stats(vec![field("", "PlayerCharacterStats", "m_usedBreakthroughCount")]), false);
    read("flag@0x31 (indestr.)", player_to(vec![Hop::Offset(0x31)]), false);
    read("flag@0x33 (noweight)", player_to(vec![Hop::Offset(0x33)]), false);
}
