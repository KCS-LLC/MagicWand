pub mod engine;
mod logger;
pub mod mono;
pub mod il2cpp;
pub mod godot;
mod scanner;
pub mod ue5;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
struct ModuleSnapshot {
    module_name: String,
    base: usize,
    regions: Vec<(usize, Vec<u8>)>,
}
static MODULE_SNAPSHOT: Mutex<Option<ModuleSnapshot>> = Mutex::new(None);

struct CaveEntry {
    cave_addr: usize,
    site_addr: usize,
    original_bytes: Vec<u8>,
}
static CAVE_STATE: LazyLock<Mutex<HashMap<String, CaveEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Runs `$body` on the blocking thread pool and flattens the JoinError into the
/// command's `Result<_, String>`, matching every #[tauri::command] async wrapper.
macro_rules! blocking {
    ($body:expr) => {
        tauri::async_runtime::spawn_blocking(move || $body)
            .await
            .map_err(|e| e.to_string())?
    };
}

#[tauri::command]
fn scan_games() -> Vec<scanner::DetectedGame> {
    scanner::scan_all()
}

#[tauri::command]
fn find_game(name: &str) -> Option<u32> {
    engine::find_process_by_name(name)
}

#[tauri::command]
fn get_module_base(pid: u32, module_name: &str) -> Option<String> {
    engine::get_module_info(pid, module_name).map(|(addr, _)| addr.to_string())
}

/// Resolves a Godot GDScript member variable (see godot.rs). Returns [instance, address];
/// pass the instance back as `hint` on later calls so only the first call pays for a scan.
#[tauri::command]
async fn resolve_godot_member(
    pid: u32,
    module_name: String,
    vtable_rva: String,
    member_count: usize,
    tags: Vec<i64>,
    index: usize,
    hint: Option<String>,
) -> Result<Vec<String>, String> {
    blocking!({
        let rva = parse_addr(&vtable_rva)? as usize;
        let hint = match hint {
            Some(h) => Some(parse_addr(&h)? as usize),
            None => None,
        };
        godot::resolve_member(pid, &module_name, rva, member_count, &tags, index, hint)
            .map(|(i, a)| vec![format!("0x{:X}", i), format!("0x{:X}", a)])
    })
}

#[tauri::command]
async fn aob_scan_range(pid: u32, base: String, size: usize, pattern: String) -> Result<String, String> {
    blocking!({
        let base_addr = parse_addr(&base)? as usize;
        engine::aob_scan_range(pid, base_addr, size, &pattern)
            .map(|a| format!("0x{:X}", a))
            .ok_or_else(|| "Pattern not found".to_string())
    })
}

#[tauri::command]
async fn aob_scan(pid: u32, module_name: String, pattern: String) -> Result<String, String> {
    blocking!({ engine::aob_scan(pid, &module_name, &pattern).map(|r| r.to_string()) })
}

/// Like aob_scan, but returns every match in the module instead of just the first — the
/// only way to actually confirm a signature is unique before trusting it for a patch.
/// aob_scan alone can silently return a coincidental earlier match while a "verified"
/// later one sits unused, exactly the false-positive failure mode that bit the Glitch
/// Charge cheat (an 8-byte signature matched two near-identical compiled setter bodies).
#[tauri::command]
async fn aob_scan_all(pid: u32, module_name: String, pattern: String) -> Result<Vec<String>, String> {
    blocking!({
        let (base, size) = engine::get_module_info(pid, &module_name)
            .ok_or_else(|| format!("Could not find module {}", module_name))?;
        Ok(engine::aob_scan_all_range(pid, base, size, &pattern)
            .into_iter()
            .map(|a| format!("0x{:X}", a))
            .collect())
    })
}

/// Like aob_scan_range, but returns every match in the range instead of just the first.
/// Useful for hunting a generic sub-pattern (e.g. a bare SUBSS opcode) within a single
/// already-located function's byte span, where a whole-module scan would be too noisy.
#[tauri::command]
async fn aob_scan_all_range(pid: u32, base: String, size: usize, pattern: String) -> Result<Vec<String>, String> {
    blocking!({
        let base_addr = parse_addr(&base)? as usize;
        Ok(engine::aob_scan_all_range(pid, base_addr, size, &pattern)
            .into_iter()
            .map(|a| format!("0x{:X}", a))
            .collect())
    })
}

fn parse_addr(s: &str) -> Result<u64, String> {
    let clean = s.trim().to_lowercase();
    if clean.starts_with("0x") {
        u64::from_str_radix(&clean[2..], 16).map_err(|e| format!("Hex parse err: {}", e))
    } else {
        clean.parse::<u64>().map_err(|e| format!("Decimal parse err: {}", e))
    }
}

fn require_module(pid: u32, name: &str) -> Result<(usize, usize), String> {
    engine::get_module_info(pid, name)
        .ok_or_else(|| format!("Module '{}' not found", name))
}

fn read_4bytes(pid: u32, address: &str) -> Result<[u8; 4], String> {
    let addr = parse_addr(address)?;
    engine::read_memory(pid, addr as usize, 4)?
        .try_into()
        .map_err(|_| "Failed to read 4 bytes".to_string())
}

#[tauri::command]
fn resolve_pointer(
    pid: u32,
    module_name: String,
    base_offset: String,
    offsets: Vec<String>,
) -> Result<String, String> {
    let (module_base, _) = require_module(pid, &module_name)?;

    let start_address = (module_base as u64) + parse_addr(&base_offset)?;

    let mut parsed_offsets = Vec::new();
    for o in offsets {
        parsed_offsets.push(parse_addr(&o)? as usize);
    }

    engine::resolve_pointer_path(pid, start_address as usize, &parsed_offsets)
        .map(|r| r.to_string())
}

#[tauri::command]
fn read_int(pid: u32, address: String) -> Result<i32, String> {
    Ok(i32::from_le_bytes(read_4bytes(pid, &address)?))
}

#[tauri::command]
fn read_float(pid: u32, address: String) -> Result<f32, String> {
    Ok(f32::from_le_bytes(read_4bytes(pid, &address)?))
}

#[tauri::command]
fn write_int(pid: u32, address: String, value: i32) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_memory_raw(pid, addr as usize, &value.to_le_bytes())
}

#[tauri::command]
fn write_float(pid: u32, address: String, value: f32) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_memory_raw(pid, addr as usize, &value.to_le_bytes())
}

#[tauri::command]
fn read_double(pid: u32, address: String) -> Result<f64, String> {
    let addr = parse_addr(&address)?;
    engine::read_double(pid, addr as usize)
}

#[tauri::command]
fn write_double(pid: u32, address: String, value: f64) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_double(pid, addr as usize, value)
}

#[tauri::command]
fn scan_value(pid: u32, value_type: String, value: f64) -> Result<Vec<String>, String> {
    let addresses = match value_type.as_str() {
        "double" => engine::scan_for_double(pid, value)?,
        "float"  => engine::scan_for_float(pid, value as f32)?,
        "int"    => engine::scan_for_int(pid, value as i32)?,
        _        => return Err(format!("Unknown value type: {}", value_type)),
    };
    Ok(addresses.iter().map(|a| format!("0x{:X}", a)).collect())
}

#[tauri::command]
fn resolve_mono_chain(
    pid: u32,
    module_name: String,
    assembly: String,
    namespace: String,
    class_name: String,
    static_field: String,
    via_parent: bool,
    instance_field: String,
    final_offset: usize,
    instance_field_is_ref: Option<bool>,
) -> Result<String, String> {
    let (mono_base, _) = require_module(pid, &module_name)?;
    let addr = mono::resolve_mono_chain(
        pid, mono_base, &assembly, &namespace, &class_name,
        &static_field, via_parent, &instance_field, final_offset,
        instance_field_is_ref.unwrap_or(false),
    )?;
    Ok(format!("0x{:X}", addr))
}

/// Resolve the native (JIT) entry address of a managed method, for code_patch targets
/// that have no fixed RVA. The method must have executed at least once this session.
// async + spawn_blocking: this scans the whole process and must not run on the UI thread
// (a sync command would freeze the window for the duration).
#[tauri::command]
async fn resolve_mono_method(
    pid: u32,
    module_name: String,
    assembly: String,
    namespace: String,
    class_name: String,
    method: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (mono_base, _) = require_module(pid, &module_name)?;
        let hits = mono::locate_method_jit(pid, mono_base, &assembly, &namespace, &class_name, &method)?;
        // Mono has one JitInfo per compiled body; more than one means a generic/shared
        // instantiation, which we can't pick between safely.
        if hits.len() > 1 {
            return Err(format!(
                "'{}.{}' has {} compiled bodies — refusing to guess",
                class_name, method, hits.len()
            ));
        }
        Ok(format!("0x{:X}", hits[0].code_start))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Saved original bytes of IL2CPP patches, keyed by (pid, cheat id), so each toggle restores
/// only its own sites: Vec of (patched address, original bytes).
static IL2CPP_PATCH_ORIGINALS: LazyLock<Mutex<HashMap<(u32, String), Vec<(usize, Vec<u8>)>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn restore_il2cpp_patch(pid: u32, key: &str) -> Result<(), String> {
    let mut saved = IL2CPP_PATCH_ORIGINALS.lock().map_err(|e| e.to_string())?;
    if let Some(sites) = saved.remove(&(pid, key.to_string())) {
        for (addr, orig) in sites {
            engine::write_memory_rw(pid, addr, &orig)?;
        }
    }
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MethodRefArg {
    namespace: Option<String>,
    class_name: String,
    method: String,
}

/// Make IL2CPP methods (every overload) return immediately, or restore them. Only valid for
/// void methods: `ret` at byte 0 runs before the method touches anything, so it is safe for
/// any void method — the trainer author must confirm the method is void.
#[tauri::command]
async fn set_il2cpp_method_ret(
    pid: u32,
    module_name: String,
    key: String,
    methods: Vec<MethodRefArg>,
    enable: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !enable { return restore_il2cpp_patch(pid, &key); }
        let (base, _) = require_module(pid, &module_name)?;
        {
            let saved = IL2CPP_PATCH_ORIGINALS.lock().map_err(|e| e.to_string())?;
            if saved.contains_key(&(pid, key.clone())) { return Ok(()); }
        }
        let mut sites: Vec<(usize, Vec<u8>)> = Vec::new();
        for m in &methods {
            let ns = m.namespace.clone().unwrap_or_default();
            for addr in il2cpp::find_method_addresses(pid, base, &ns, &m.class_name, &m.method)? {
                let orig = engine::read_memory(pid, addr, 1)?;
                if orig[0] == 0xC3 {
                    return Err(format!("{}::{} already starts with `ret` (left over from an earlier session? restart the game)", m.class_name, m.method));
                }
                sites.push((addr, orig));
            }
        }
        for (addr, _) in &sites { engine::write_memory_rw(pid, *addr, &[0xC3])?; }
        IL2CPP_PATCH_ORIGINALS.lock().map_err(|e| e.to_string())?.insert((pid, key), sites);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// NOP the *decrement* store to `field_class.field` — the first store after a `subss` — in
/// every overload of each listed method, or restore. Used for "lock this stat" toggles: the
/// stat can still be raised (eating, resting) but the decay is gone. At least one site must
/// be found across all methods, and each overload contributes at most one.
#[tauri::command]
async fn set_il2cpp_decay_nop(
    pid: u32,
    module_name: String,
    key: String,
    methods: Vec<MethodRefArg>,
    field_class: String,
    field: String,
    enable: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !enable { return restore_il2cpp_patch(pid, &key); }
        let (base, _) = require_module(pid, &module_name)?;
        {
            let saved = IL2CPP_PATCH_ORIGINALS.lock().map_err(|e| e.to_string())?;
            if saved.contains_key(&(pid, key.clone())) { return Ok(()); }
        }
        let entries = il2cpp::all_method_entries(pid, base)?;
        let off = il2cpp::field_offset_of(pid, base, "", &field_class, &field)? as i32;
        let mut sites: Vec<(usize, Vec<u8>)> = Vec::new();
        for m in &methods {
            let ns = m.namespace.clone().unwrap_or_default();
            for start in il2cpp::find_method_addresses(pid, base, &ns, &m.class_name, &m.method)? {
                let end = match entries.binary_search(&start) {
                    Ok(i) => entries.get(i + 1).copied(),
                    Err(i) => entries.get(i).copied(),
                }.ok_or("could not bound method size")?;
                let code = engine::read_memory(pid, start, (end - start).min(0x4000))?;
                let after_subss = |idx: usize| -> bool {
                    (idx.saturating_sub(0x30)..idx).any(|i| {
                        code[i] == 0xF3 && (
                            (code.get(i + 1) == Some(&0x0F) && code.get(i + 2) == Some(&0x5C)) ||
                            (code.get(i + 1).map(|b| b & 0xF0 == 0x40).unwrap_or(false)
                                && code.get(i + 2) == Some(&0x0F) && code.get(i + 3) == Some(&0x5C))
                        )
                    })
                };
                if let Some((idx, len)) = mono::find_movss_stores(&code, off).into_iter().find(|(i, _)| after_subss(*i)) {
                    sites.push((start + idx, code[idx..idx + len].to_vec()));
                }
            }
        }
        if sites.is_empty() {
            return Err(format!("No decrement store to {} found in the listed methods (game updated?)", field));
        }
        for (addr, orig) in &sites { engine::write_memory_rw(pid, *addr, &vec![0x90u8; orig.len()])?; }
        IL2CPP_PATCH_ORIGINALS.lock().map_err(|e| e.to_string())?.insert((pid, key), sites);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Il2cppHopArg {
    /// "field" | "offset" | "listItem" | "listFindByte"
    kind: String,
    namespace: Option<String>,
    class_name: Option<String>,
    field: Option<String>,
    offset: Option<usize>,
    index: Option<usize>,
    equals: Option<u8>,
}

/// Resolve a value inside an IL2CPP game by walking from a static singleton through `hops`.
/// Async + spawn_blocking because it reads the whole type table (~100 KB) and many fields.
#[tauri::command]
async fn resolve_il2cpp_path(
    pid: u32,
    module_name: String,
    namespace: String,
    class_name: String,
    static_field: String,
    hops: Vec<Il2cppHopArg>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (base, _) = require_module(pid, &module_name)?;
        let mut path = Vec::new();
        for h in hops {
            let ns = h.namespace.clone().unwrap_or_default();
            let cn = h.class_name.clone().unwrap_or_default();
            path.push(match h.kind.as_str() {
                "field" => il2cpp::Hop::Field { namespace: ns, class_name: cn, field: h.field.clone().ok_or("field hop needs `field`")? },
                "offset" => il2cpp::Hop::Offset(h.offset.ok_or("offset hop needs `offset`")?),
                "listItem" => il2cpp::Hop::ListItem(h.index.unwrap_or(0)),
                "listFindByte" => il2cpp::Hop::ListFindByte {
                    namespace: ns,
                    class_name: cn,
                    field: h.field.clone().ok_or("listFindByte needs `field`")?,
                    equals: h.equals.unwrap_or(0),
                },
                other => return Err(format!("unknown hop kind '{}'", other)),
            });
        }
        let addr = il2cpp::resolve_path(pid, base, &namespace, &class_name, &static_field, &path)?;
        Ok(format!("0x{:X}", addr))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Original bytes of every site we edited for the store-NOP cheat (native NOPs and IL
/// edits), keyed by (pid, address), so toggling off restores exactly what the game had.
static STORE_NOP_ORIGINALS: LazyLock<Mutex<HashMap<(u32, usize), Vec<u8>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// Processes where we edited a method's IL (before it was compiled); see `set_mono_store_nop`.
static IL_PATCHED_PIDS: LazyLock<Mutex<std::collections::HashSet<u32>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct IlPatchArg {
    offset: usize,
    /// Space-separated hex of the bytes that must be present.
    expect: String,
    /// Space-separated hex written in their place (same length).
    replace: String,
}

fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).map_err(|_| format!("bad hex byte '{}'", b)))
        .collect()
}

/// NOP (enable) or restore (disable) every store to `class.field` inside `class.method`, so
/// the method keeps running (on-hit triggers, popups) but the stat never changes. Returns a
/// short status for the UI: "native" | "il" | "restored" | "restored-baked".
///
/// If the method has already been JIT-compiled the inlined native stores are NOPped (fully
/// reversible). If it hasn't — and the trainer supplies `il_signature` + `il_patches` — the
/// method's IL is edited in memory *before* it compiles, so even the first call is harmless.
/// The catch: a method compiled from edited IL stays edited; disabling restores the IL but
/// cannot un-compile it ("restored-baked": restart the game to get damage back).
/// Runs whole-process memory searches, so it is async + off-thread.
#[tauri::command]
async fn set_mono_store_nop(
    pid: u32,
    module_name: String,
    assembly: String,
    namespace: String,
    class_name: String,
    method: String,
    field: String,
    il_signature: Option<String>,
    il_patches: Option<Vec<IlPatchArg>>,
    enable: bool,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut saved = STORE_NOP_ORIGINALS.lock().map_err(|e| e.to_string())?;
        if !enable {
            let keys: Vec<(u32, usize)> = saved.keys().filter(|(p, _)| *p == pid).cloned().collect();
            for key in &keys {
                if let Some(orig) = saved.remove(key) {
                    engine::write_memory_rw(pid, key.1, &orig)?;
                }
            }
            drop(saved);
            let was_il = IL_PATCHED_PIDS.lock().map_err(|e| e.to_string())?.remove(&pid);
            if was_il {
                // Did the game compile the method while its IL was edited?
                let (mono_base, _) = require_module(pid, &module_name)?;
                if mono::locate_method_jit(pid, mono_base, &assembly, &namespace, &class_name, &method).is_ok() {
                    return Ok("restored-baked".to_string());
                }
            }
            return Ok("restored".to_string());
        }
        if saved.keys().any(|(p, _)| *p == pid) {
            return Ok("already".to_string());
        }
        let (mono_base, _) = require_module(pid, &module_name)?;
        match mono::find_field_store_sites(pid, mono_base, &assembly, &namespace, &class_name, &method, &field) {
            Ok(sites) => {
                for (addr, len) in &sites {
                    let current = engine::read_memory(pid, *addr, *len)?;
                    saved.insert((pid, *addr), current);
                    engine::write_memory_rw(pid, *addr, &vec![0x90u8; *len])?;
                }
                Ok("native".to_string())
            }
            Err(e) if mono::is_not_compiled(&e) => {
                let (sig, patches) = match (il_signature, il_patches) {
                    (Some(s), Some(p)) if !p.is_empty() => (parse_hex(&s)?, p),
                    _ => return Err(e),
                };
                let patches: Vec<mono::IlPatch> = patches
                    .iter()
                    .map(|p| Ok(mono::IlPatch { offset: p.offset, expect: parse_hex(&p.expect)?, replace: parse_hex(&p.replace)? }))
                    .collect::<Result<_, String>>()?;
                for p in &patches {
                    if p.expect.len() != p.replace.len() { return Err("IL patch expect/replace length mismatch".to_string()); }
                }
                let sites = mono::il_patch_sites(pid, &sig, &patches)?;
                for (addr, orig, repl) in &sites {
                    saved.insert((pid, *addr), orig.clone());
                    engine::write_memory_rw(pid, *addr, repl)?;
                }
                IL_PATCHED_PIDS.lock().map_err(|e| e.to_string())?.insert(pid);
                Ok("il".to_string())
            }
            Err(e) => Err(e),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Original bytes per (pid, cheat id) for `set_mono_method_ret`, as (address, bytes) pairs.
static METHOD_RET_ORIGINALS: LazyLock<Mutex<HashMap<(u32, String), Vec<(usize, Vec<u8>)>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// (pid, cheat id) pairs whose method IL was edited before it compiled.
static METHOD_RET_IL: LazyLock<Mutex<std::collections::HashSet<(u32, String)>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

/// Make a Mono method do nothing and return 0 while enabled (e.g. a stat-reduction method), and
/// restore it when disabled. Works for void and int-returning methods. Native path writes
/// `xor eax,eax; ret` over the first bytes of the JIT body; if the method hasn't compiled yet and
/// the trainer supplies `il_signature` + `il_patches`, its IL is edited instead (same trade-off
/// as `set_mono_store_nop`). Returns "native" | "il" | "restored" | "restored-baked" | "already".
#[tauri::command]
async fn set_mono_method_ret(
    pid: u32,
    cheat_id: String,
    module_name: String,
    assembly: String,
    namespace: String,
    class_name: String,
    method: String,
    il_signature: Option<String>,
    il_patches: Option<Vec<IlPatchArg>>,
    enable: bool,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = (pid, cheat_id);
        let mut saved = METHOD_RET_ORIGINALS.lock().map_err(|e| e.to_string())?;
        if !enable {
            if let Some(sites) = saved.remove(&key) {
                for (addr, orig) in sites {
                    engine::write_memory_rw(pid, addr, &orig)?;
                }
            }
            drop(saved);
            if METHOD_RET_IL.lock().map_err(|e| e.to_string())?.remove(&key) {
                let (mono_base, _) = require_module(pid, &module_name)?;
                if mono::locate_method_jit(pid, mono_base, &assembly, &namespace, &class_name, &method).is_ok() {
                    return Ok("restored-baked".to_string());
                }
            }
            return Ok("restored".to_string());
        }
        if saved.contains_key(&key) {
            return Ok("already".to_string());
        }
        let (mono_base, _) = require_module(pid, &module_name)?;
        match mono::locate_method_jit(pid, mono_base, &assembly, &namespace, &class_name, &method) {
            Ok(hits) => {
                if hits.len() != 1 {
                    return Err(format!("'{}.{}' has {} compiled bodies — refusing to guess", class_name, method, hits.len()));
                }
                const RET0: [u8; 3] = [0x31, 0xC0, 0xC3]; // xor eax,eax; ret
                let at = hits[0].code_start;
                let orig = engine::read_memory(pid, at, RET0.len())?;
                if orig == RET0 {
                    return Err("already patched (stale patch from an earlier session?) — restart the game".to_string());
                }
                engine::write_memory_rw(pid, at, &RET0)?;
                saved.insert(key, vec![(at, orig)]);
                Ok("native".to_string())
            }
            Err(e) if mono::is_not_compiled(&e) => {
                let (sig, patches) = match (il_signature, il_patches) {
                    (Some(s), Some(p)) if !p.is_empty() => (parse_hex(&s)?, p),
                    _ => return Err(e),
                };
                let patches: Vec<mono::IlPatch> = patches
                    .iter()
                    .map(|p| Ok(mono::IlPatch { offset: p.offset, expect: parse_hex(&p.expect)?, replace: parse_hex(&p.replace)? }))
                    .collect::<Result<_, String>>()?;
                for p in &patches {
                    if p.expect.len() != p.replace.len() { return Err("IL patch expect/replace length mismatch".to_string()); }
                }
                let sites = mono::il_patch_sites(pid, &sig, &patches)?;
                let mut done = Vec::new();
                for (addr, orig, repl) in &sites {
                    engine::write_memory_rw(pid, *addr, repl)?;
                    done.push((*addr, orig.clone()));
                }
                saved.insert(key.clone(), done);
                METHOD_RET_IL.lock().map_err(|e| e.to_string())?.insert(key);
                Ok("il".to_string())
            }
            Err(e) => Err(e),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonoMethodArg {
    assembly: String,
    #[serde(default)]
    namespace: String,
    class_name: String,
    method: String,
}

/// Skip one specific call while leaving the rest of the caller running: inside the JIT body of
/// `caller`, find the direct call to `callee` (`mov r11, <callee code>; call r11` = 49 BB imm64
/// 41 FF D3) and NOP the 3-byte `call r11`. The call's arguments are already loaded in registers
/// and Win64 has no caller cleanup, so skipping a void call is safe. Both methods must already be
/// compiled. Returns "native" | "restored" | "already".
#[tauri::command]
async fn set_mono_call_nop(
    pid: u32,
    cheat_id: String,
    module_name: String,
    caller: MonoMethodArg,
    callee: MonoMethodArg,
    enable: bool,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = (pid, cheat_id);
        let mut saved = METHOD_RET_ORIGINALS.lock().map_err(|e| e.to_string())?;
        if !enable {
            if let Some(sites) = saved.remove(&key) {
                for (addr, orig) in sites {
                    engine::write_memory_rw(pid, addr, &orig)?;
                }
            }
            return Ok("restored".to_string());
        }
        if saved.contains_key(&key) {
            return Ok("already".to_string());
        }
        let (mono_base, _) = require_module(pid, &module_name)?;
        let one = |m: &MonoMethodArg| -> Result<mono::MethodJit, String> {
            let mut hits = mono::locate_method_jit(pid, mono_base, &m.assembly, &m.namespace, &m.class_name, &m.method)?;
            if hits.len() != 1 {
                return Err(format!("'{}.{}' has {} compiled bodies — refusing to guess", m.class_name, m.method, hits.len()));
            }
            Ok(hits.remove(0))
        };
        let callee_jit = one(&callee)?;
        let caller_jit = one(&caller)?;
        let code = engine::read_memory(pid, caller_jit.code_start, caller_jit.code_size)?;
        let want = (callee_jit.code_start as u64).to_le_bytes();
        let mut sites = Vec::new();
        for i in 0..code.len().saturating_sub(13) {
            if code[i] == 0x49 && code[i + 1] == 0xBB && code[i + 2..i + 10] == want && code[i + 10..i + 13] == [0x41, 0xFF, 0xD3] {
                sites.push(caller_jit.code_start + i + 10);
            }
        }
        if sites.is_empty() {
            return Err(format!(
                "No direct call to '{}.{}' found in '{}.{}' (game updated, or the callee was not compiled when the caller was)",
                callee.class_name, callee.method, caller.class_name, caller.method
            ));
        }
        let mut done = Vec::new();
        for at in sites {
            let orig = engine::read_memory(pid, at, 3)?;
            engine::write_memory_rw(pid, at, &[0x90, 0x90, 0x90])?;
            done.push((at, orig));
        }
        saved.insert(key, done);
        Ok("native".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonoHopArg {
    namespace: String,
    class_name: String,
    field: String,
    #[serde(default)]
    dict_key: Option<i32>,
}

#[tauri::command]
fn resolve_mono_path(
    pid: u32,
    module_name: String,
    assembly: String,
    namespace: String,
    class_name: String,
    static_field: String,
    hops: Vec<MonoHopArg>,
) -> Result<String, String> {
    let (mono_base, _) = require_module(pid, &module_name)?;
    let hops: Vec<mono::MonoHop> = hops
        .into_iter()
        .map(|h| mono::MonoHop { namespace: h.namespace, class_name: h.class_name, field: h.field, dict_key: h.dict_key })
        .collect();
    let addr = mono::resolve_mono_path(
        pid, mono_base, &assembly, &namespace, &class_name, &static_field, &hops,
    )?;
    Ok(format!("0x{:X}", addr))
}

#[tauri::command]
fn resolve_mono_field(
    pid: u32,
    module_name: String,
    assembly: String,
    namespace: String,
    class_name: String,
    field_name: String,
    is_static: bool,
) -> Result<String, String> {
    let (mono_base, _) = require_module(pid, &module_name)?;
    let addr = mono::resolve_mono_field(
        pid, mono_base, &assembly, &namespace, &class_name, &field_name, is_static,
    )?;
    Ok(format!("0x{:X}", addr))
}

#[tauri::command]
async fn resolve_ue5_prop(
    pid: u32,
    module_name: String,
    gobjects_aob: Option<String>,
    gnames_aob: Option<String>,
    gobjects_offset: Option<usize>,
    gnames_offset: Option<usize>,
    class_name: String,
    property_offset: usize,
    extra_offsets: Option<Vec<usize>>,
) -> Result<String, String> {
    blocking!({
        let (base, size) = require_module(pid, &module_name)?;
        let chain = extra_offsets.as_deref().unwrap_or(&[]);
        let addr = if let (Some(go), Some(gn)) = (gobjects_offset, gnames_offset) {
            ue5::resolve_ue5_prop_static(pid, base, size, go, gn, &class_name, property_offset, chain)?
        } else {
            let aob_go = gobjects_aob.as_deref().unwrap_or("");
            let aob_gn = gnames_aob.as_deref().unwrap_or("");
            ue5::resolve_ue5_prop(pid, base, size, aob_go, aob_gn, &class_name, property_offset, chain)?
        };
        Ok(format!("0x{:X}", addr))
    })
}

/// Like resolve_ue5_prop, but takes an explicit Ue5Offsets struct instead of assuming
/// ue5::Ue5Offsets::ue5_default(). Lets a trainer target UE5 games whose internal struct
/// layout (FUObjectItem size, FName entry header, etc.) differs from the default.
#[tauri::command]
async fn resolve_ue5_prop_custom(
    pid: u32,
    module_name: String,
    gobjects_offset: usize,
    gnames_offset: usize,
    class_name: String,
    property_offset: usize,
    extra_offsets: Option<Vec<usize>>,
    offsets: ue5::Ue5Offsets,
) -> Result<String, String> {
    blocking!({
        let (base, _) = require_module(pid, &module_name)?;
        let chain = extra_offsets.as_deref().unwrap_or(&[]);
        let gobjects_base = base + gobjects_offset;
        let gnames_base = base + gnames_offset;
        let obj_ptr = ue5::find_object_by_class(pid, gobjects_base, gnames_base, &class_name, &offsets)?;
        let initial = obj_ptr + property_offset;
        let addr = if chain.is_empty() {
            initial
        } else {
            engine::resolve_pointer_path(pid, initial, chain)?
        };
        Ok(format!("0x{:X}", addr))
    })
}

/// Cheap re-walk of the tail of a ue5_prop chain from an already-known object pointer,
/// skipping the expensive find_object_by_class GObjects scan entirely. For chains where
/// the class-object itself (e.g. the player controller) is effectively stable for the
/// whole session but a downstream hop (e.g. equipped weapon) can legitimately repoint to
/// a different live object at any time — re-doing the full scan every poll tick to catch
/// that is needlessly expensive (and, on a debug build against a 500k+-object GObjects
/// array, slow enough to occasionally fail outright). Pair with a one-time
/// resolve_ue5_prop_custom (property_offset: 0, extra_offsets: None) to get obj_addr.
#[tauri::command]
async fn resolve_pointer_chain_from(
    pid: u32,
    obj_addr: String,
    property_offset: usize,
    extra_offsets: Vec<usize>,
) -> Result<String, String> {
    blocking!({
        let obj_ptr = parse_addr(&obj_addr)? as usize;
        let initial = obj_ptr + property_offset;
        let addr = if extra_offsets.is_empty() {
            initial
        } else {
            engine::resolve_pointer_path(pid, initial, &extra_offsets)?
        };
        Ok(format!("0x{:X}", addr))
    })
}

/// Cheap sanity check for a ue5NoCache cheat's cached class-object pointer (see
/// ue5::verify_object_class) — a single read + SuperStruct walk, not a GObjects scan.
/// Lets the frontend catch a stale-but-non-faulting pointer (e.g. after a level
/// transition/fast-travel recreates the controller) proactively every tick, instead of
/// only invalidating the cache when a read/write actually errors — which a stale-but
/// -still-committed pointer may never do.
#[tauri::command]
async fn verify_object_class(
    pid: u32,
    module_name: String,
    gnames_offset: usize,
    obj_addr: String,
    class_name: String,
    offsets: ue5::Ue5Offsets,
) -> Result<bool, String> {
    blocking!({
        let (base, _) = require_module(pid, &module_name)?;
        let gnames_base = base + gnames_offset;
        let obj_ptr = parse_addr(&obj_addr)? as usize;
        Ok(ue5::verify_object_class(pid, gnames_base, obj_ptr, &class_name, &offsets))
    })
}

/// Resolves the address of a value field inside a dynamic TArray element (e.g. a currency
/// stack inside an inventory's `TArray<FItemStack>`), by walking a pointer chain to the
/// container object then scanning its array for the first element matching `match_class`.
/// See `ue5::resolve_ue5_array_item` for the exact semantics. `offsets` is optional —
/// omit it to use `Ue5Offsets::ue5_default()` (BL4-style chunked GObjects).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn resolve_ue5_array_item(
    pid: u32,
    module_name: String,
    gobjects_offset: usize,
    gnames_offset: usize,
    class_name: String,
    property_offset: usize,
    extra_offsets: Option<Vec<usize>>,
    array_offset: usize,
    item_stride: usize,
    item_ptr_offset: usize,
    match_class: String,
    value_offset: usize,
    offsets: Option<ue5::Ue5Offsets>,
) -> Result<String, String> {
    blocking!({
        let (base, _) = require_module(pid, &module_name)?;
        let gobjects_base = base + gobjects_offset;
        let gnames_base = base + gnames_offset;
        let chain = extra_offsets.as_deref().unwrap_or(&[]);
        let off = offsets.unwrap_or_else(ue5::Ue5Offsets::ue5_default);
        let addr = ue5::resolve_ue5_array_item(
            pid, gobjects_base, gnames_base, &class_name, property_offset, chain,
            array_offset, item_stride, item_ptr_offset, &match_class, value_offset, &off,
        )?;
        Ok(format!("0x{:X}", addr))
    })
}

#[tauri::command]
async fn diff_snapshot(pid: u32) -> Result<Vec<String>, String> {
    blocking!({ diff_snapshot_inner(pid) })
}

fn diff_snapshot_inner(pid: u32) -> Result<Vec<String>, String> {
    let snap = MODULE_SNAPSHOT.lock().unwrap();
    let snapshot = snap.as_ref().ok_or("No snapshot — take a snapshot first")?;
    let (current_base, _) = engine::get_module_info(pid, &snapshot.module_name)
        .ok_or("Module not found")?;
    let mut diffs: Vec<String> = Vec::new();
    for (region_base, old_data) in &snapshot.regions {
        if let Ok(new_data) = engine::read_memory(pid, *region_base, old_data.len()) {
            for (i, (old, new)) in old_data.iter().zip(new_data.iter()).enumerate() {
                if old != new {
                    let abs = region_base + i;
                    let rva = abs.wrapping_sub(snapshot.base);
                    diffs.push(format!("RVA 0x{:X}  abs 0x{:X}  {:02X} -> {:02X}", rva, abs, old, new));
                }
            }
        }
    }
    if diffs.is_empty() {
        diffs.push(format!("No changes detected (base 0x{:X})", current_base));
    }
    Ok(diffs)
}



#[tauri::command]
fn read_snapshot_region(rva: usize, size: usize) -> Result<String, String> {
    let snap = MODULE_SNAPSHOT.lock().unwrap();
    let snapshot = snap.as_ref().ok_or("No snapshot")?;
    let target = snapshot.base + rva;
    for (region_base, data) in &snapshot.regions {
        let region_end = region_base + data.len();
        if *region_base <= target && target < region_end {
            let start = target - region_base;
            let end = (start + size).min(data.len());
            let hex: Vec<String> = data[start..end].iter().map(|b| format!("{:02X}", b)).collect();
            return Ok(hex.join(" "));
        }
    }
    Err(format!("RVA 0x{:X} not found in snapshot", rva))
}

#[tauri::command]
fn lookup_fnames(pid: u32, module_name: String, gnames_offset: usize, indices: Vec<u32>) -> Result<Vec<String>, String> {
    let (base, _) = require_module(pid, &module_name)?;
    let off = ue5::Ue5Offsets::ue5_default();
    Ok(ue5::lookup_fnames(pid, base + gnames_offset, &indices, &off))
}

#[tauri::command]
fn list_ue5_classes(pid: u32, module_name: String, gobjects_offset: usize, gnames_offset: usize, keyword: String) -> Result<Vec<String>, String> {
    let (base, _) = require_module(pid, &module_name)?;
    let off = ue5::Ue5Offsets::ue5_default();
    ue5::list_classes_by_keyword(pid, base + gobjects_offset, base + gnames_offset, &keyword, &off)
}

#[tauri::command]
fn dump_floats_at(pid: u32, address: String, count: usize) -> Result<Vec<String>, String> {
    let addr = parse_addr(&address)? as usize;
    let bytes = engine::read_memory(pid, addr, count * 4)?;
    let lines = bytes.chunks(4).enumerate().map(|(i, chunk)| {
        let b = [chunk[0], chunk[1], chunk[2], chunk[3]];
        let f = f32::from_le_bytes(b);
        let u = u32::from_le_bytes(b);
        let hex: String = b.iter().map(|x| format!("{:02X}", x)).collect::<Vec<_>>().join(" ");
        format!("+0x{:03X}  f:{:>14.6}  i:{:>10}  [{}]", i * 4, f, u, hex)
    }).collect();
    Ok(lines)
}

#[tauri::command]
fn toggle_bit_flag(pid: u32, address: String, bit: u8, value: bool) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::set_bit_at(pid, addr as usize, bit, value)
}

#[tauri::command]
fn write_byte(pid: u32, address: String, value: u8) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_memory_rw(pid, addr as usize, &[value])
}

#[tauri::command]
fn read_byte(pid: u32, address: String) -> Result<u8, String> {
    let addr = parse_addr(&address)?;
    let data = engine::read_memory(pid, addr as usize, 1)
        .map_err(|e| e.to_string())?;
    Ok(data[0])
}

#[tauri::command]
fn read_raw_bytes(pid: u32, address: String, count: usize) -> Result<String, String> {
    let addr = parse_addr(&address)?;
    let bytes = engine::read_memory(pid, addr as usize, count)?;
    Ok(bytes.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" "))
}

/// Stage 1: scan for the exact WeMod AoB (AND EAX + CVTSI2SS + DIVSS[RDI] + MULSS[RDI]).
/// If found: those are WeMod's original targets. If zero results: BL4 updated all [RDI] forms
/// to [RIP+rel32] and WeMod changed approach. Falls back to broader 8-byte prefix scan.
#[tauri::command]
async fn scan_rarity_candidates(pid: u32, module_name: String) -> Result<Vec<String>, String> {
    blocking!({
        let (base, size) = require_module(pid, &module_name)?;

        // Primary: full WeMod AoB — AND EAX + CVTSI2SS(any reg) + DIVSS[RDI] + MULSS[RDI]
        // F3 0F 5E 07 = DIVSS XMM0,[RDI]  (4-byte [RDI] form)
        // F3 0F 59 07 = MULSS XMM0,[RDI]  (4-byte [RDI] form, fixed in WeMod's AoB)
        let wemod_aob = "25 FF 7F 00 00 F3 0F 2A ?? F3 0F 5E 07 F3 0F 59 07";
        let exact = engine::aob_scan_all_range(pid, base, size, wemod_aob);
        if !exact.is_empty() {
            let mut results = vec![format!(
                "WEMOD AOB EXACT: {} hit(s) — these are WeMod's original [RDI]-form targets:", exact.len()
            )];
            for addr in &exact {
                let rva = addr.wrapping_sub(base);
                let ctx_start = addr.saturating_sub(16);
                let ctx = engine::read_memory(pid, ctx_start, 96).unwrap_or_default();
                let hex: Vec<String> = ctx.iter().map(|b| format!("{:02X}", b)).collect();
                results.push(format!("RVA=0x{:X}  ctx[-16..+80]: {}", rva, hex.join(" ")));
            }
            return Ok(results);
        }

        // Secondary: 8-byte prefix (AND EAX + CVTSI2SS prefix) — annotate with what DIVSS form follows
        let all = engine::aob_scan_all_range(pid, base, size, "25 FF 7F 00 00 F3 0F 2A");
        if all.is_empty() {
            return Ok(vec!["No WeMod AoB match and no AND EAX+CVTSI2SS found — BL4 code layout changed significantly".to_string()]);
        }
        let mut results = vec![format!(
            "WeMod [RDI] AoB: 0 hits. Found {} candidate(s) of AND EAX+CVTSI2SS — checking DIVSS form:", all.len()
        )];
        for addr in &all {
            let rva = addr.wrapping_sub(base);
            let ctx_start = addr.saturating_sub(16);
            // Read 96 bytes: 16 before + 80 after so we can see DIVSS+MULSS form
            let ctx = engine::read_memory(pid, ctx_start, 96).unwrap_or_default();
            let hex: Vec<String> = ctx.iter().map(|b| format!("{:02X}", b)).collect();
            // Bytes 16..20 of ctx = match start; bytes 25..29 = after CVTSI2SS(4 bytes) = DIVSS
            let divss_form = if ctx.len() >= 32 {
                match ctx[25] {
                    0x07 => "[RDI]",       // F3 0F 5E 07
                    0x05 => "[RIP+rel32]", // F3 0F 5E 05 xx xx xx xx
                    _    => "?",
                }
            } else { "?" };
            results.push(format!("RVA=0x{:X}  DIVSS={} ctx[-16..+80]: {}", rva, divss_form, hex.join(" ")));
        }
        Ok(results)
    })
}

/// Install a code cave for the legendary gate cheat.
/// `cheat_id`     — key into CAVE_STATE; lets multiple code_cave cheats coexist
/// `site_rva`     — RVA of the instruction(s) to replace with JMP
/// `cave_payload` — cave bytes WITHOUT the final JMP back (engine appends E9 rel32)
/// `site_len`     — total bytes to overwrite at the site (>=5). The JMP (5 bytes) is
///                  written first; any remaining bytes up to site_len are NOP-padded, and
///                  execution resumes at site_addr + site_len. Needed when the instruction(s)
///                  being neutralized are longer than a single 5-byte JMP (e.g. a multi-byte
///                  cmp/subss pair) — overwriting only 5 bytes would leave dangling original
///                  bytes that the CPU would misdecode on return.
/// Describes a RIP-relative memory operand within the bytes a far-jump cave swallows
/// beyond what `cave_payload` itself replicates (see `enable_code_cave`'s `far_jump`
/// branch). The referenced data gets copied live into the cave and the displacement
/// rewritten to point at that local copy — copying the original disp32 as-is would
/// resolve relative to wherever the cave lands, which is wrong once the cave isn't
/// necessarily near the original site.
///
/// Only supports the instruction being the *last* one in the swallowed region (its
/// disp32 field must end exactly at site_len) — that's the case this exists for
/// (BL4's legendary drop-rate DIVSS); a general multi-instruction relocator would need
/// a real disassembler and isn't worth building for one call site.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RipFixup {
    rel_disp_offset: usize, // offset of the 4-byte disp32 field, relative to site_addr
    data_len: usize,        // size of the data the operand references
}

#[tauri::command]
async fn enable_code_cave(
    pid: u32,
    cheat_id: String,
    module_name: String,
    site_rva: String,
    cave_payload: Vec<u8>,
    site_len: usize,
    far_jump: bool,
    core_len: usize,
    rip_fixup: Option<RipFixup>,
) -> Result<String, String> {
    blocking!({
        let (base, _) = require_module(pid, &module_name)?;
        let rva = parse_addr(&site_rva)? as usize;
        let site_addr = base + rva;

        if far_jump {
            // `jmp qword ptr [rip+0]` (6 bytes) + an embedded 8-byte absolute pointer
            // right after it (14 bytes total) — reaches anywhere in the process, so the
            // cave can go in freely-available memory instead of needing a free 64KB slot
            // within 2GB of the site. Trades the compact rel32 JMP for site_len needing
            // to land on a real instruction boundary >= 14 bytes out (verify by reading
            // the live bytes before setting this on a cheat — a wrong cut corrupts the
            // instruction stream instead of just failing to find memory).
            const FAR_JMP_LEN: usize = 14;
            let site_len = site_len.max(FAR_JMP_LEN);
            let original_bytes = engine::read_memory(pid, site_addr, site_len)?;

            // Bytes beyond what cave_payload's own replicated instruction covers (core_len)
            // still need to execute — carried into the cave verbatim, except for any
            // RIP-relative operand, which gets repointed at an embedded copy (see RipFixup).
            let core_len = core_len.min(site_len);
            let mut extra_bytes = original_bytes[core_len..].to_vec();

            let mut embedded_data: Vec<u8> = Vec::new();
            if let Some(fixup) = rip_fixup {
                let rel_off = fixup.rel_disp_offset.checked_sub(core_len)
                    .ok_or("rip_fixup.rel_disp_offset before core_len")?;
                let disp_bytes = extra_bytes.get(rel_off..rel_off + 4)
                    .ok_or("rip_fixup.rel_disp_offset out of range")?;
                let orig_disp = i32::from_le_bytes(disp_bytes.try_into().unwrap());
                // This instruction's own RIP (address of the next instruction) is
                // site_addr + site_len, since the fixup only supports it being last.
                let orig_target = (site_addr + site_len) as i64 + orig_disp as i64;
                embedded_data = engine::read_memory(pid, orig_target as usize, fixup.data_len)?;

                // New displacement: embedded_data is placed right after the far
                // jump-back block, which is always exactly FAR_JMP_LEN bytes past where
                // this instruction's RIP now points (the far-jump opcode immediately
                // follows extra_bytes in the cave — see layout below).
                let new_disp = FAR_JMP_LEN as i32;
                extra_bytes[rel_off..rel_off + 4].copy_from_slice(&new_disp.to_le_bytes());
            }

            let total = cave_payload.len() + extra_bytes.len() + FAR_JMP_LEN + embedded_data.len();
            let cave_addr = engine::alloc_executable_anywhere(pid, total)?;
            let return_addr = site_addr + site_len;

            let mut cave_bytes = cave_payload;
            cave_bytes.extend_from_slice(&extra_bytes);
            cave_bytes.extend_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
            cave_bytes.extend_from_slice(&(return_addr as u64).to_le_bytes());
            cave_bytes.extend_from_slice(&embedded_data);
            engine::write_memory_raw(pid, cave_addr, &cave_bytes)?;

            let mut jmp_bytes = vec![0xFFu8, 0x25, 0, 0, 0, 0];
            jmp_bytes.extend_from_slice(&(cave_addr as u64).to_le_bytes());
            jmp_bytes.resize(site_len, 0x90);
            engine::write_memory_rw(pid, site_addr, &jmp_bytes)?;

            CAVE_STATE.lock().unwrap().insert(cheat_id, CaveEntry { cave_addr, site_addr, original_bytes });
            crate::mwlog!("[enable_code_cave/far] cave=0x{:X} site=0x{:X} len={} (RVA 0x{})", cave_addr, site_addr, site_len, site_rva);
            return Ok(format!("0x{:X}", cave_addr));
        }

        let site_len = site_len.max(5);

        // Snapshot the bytes about to be overwritten by the JMP, so disable can restore them
        // without relying on a caller-supplied (and potentially stale) copy.
        let original_bytes = engine::read_memory(pid, site_addr, site_len)?;

        // Allocate RWX memory near the patch site
        let total = cave_payload.len() + 5; // payload + E9 rel32
        let cave_addr = engine::alloc_executable_near(pid, site_addr, total)?;

        // Compute return address: execution resumes at site_addr + site_len (after the JMP
        // and any NOP padding we write)
        let return_addr = site_addr + site_len;
        let jmp_back_from = cave_addr + cave_payload.len() + 1; // E9 is 1 byte, rel32 follows
        let rel32_back = (return_addr as i64 - (jmp_back_from as i64 + 4)) as i32;

        // Build full cave: payload + E9 + rel32
        let mut cave_bytes = cave_payload;
        cave_bytes.push(0xE9);
        cave_bytes.extend_from_slice(&rel32_back.to_le_bytes());

        // Write cave
        engine::write_memory_raw(pid, cave_addr, &cave_bytes)?;

        // Write JMP from patch site to cave (E9 rel32, 5 bytes), NOP-padding out to site_len
        let jmp_from = site_addr + 1; // E9 is 1 byte, rel32 follows
        let rel32_fwd = (cave_addr as i64 - (jmp_from as i64 + 4)) as i32;
        let mut jmp_bytes = vec![0xE9u8];
        jmp_bytes.extend_from_slice(&rel32_fwd.to_le_bytes());
        jmp_bytes.resize(site_len, 0x90);
        engine::write_memory_rw(pid, site_addr, &jmp_bytes)?;

        CAVE_STATE.lock().unwrap().insert(cheat_id, CaveEntry { cave_addr, site_addr, original_bytes });
        crate::mwlog!("[enable_code_cave] cave=0x{:X} site=0x{:X} len={} (RVA 0x{})", cave_addr, site_addr, site_len, site_rva);
        Ok(format!("0x{:X}", cave_addr))
    })
}

#[tauri::command]
async fn disable_code_cave(pid: u32, cheat_id: String) -> Result<(), String> {
    blocking!({
        let entry = CAVE_STATE.lock().unwrap().remove(&cheat_id)
            .ok_or("No active code cave for this cheat")?;

        // Restore original bytes at patch site
        engine::write_memory_rw(pid, entry.site_addr, &entry.original_bytes)?;

        // Free the cave allocation
        let _ = engine::free_alloc(pid, entry.cave_addr);
        crate::mwlog!("[disable_code_cave] freed cave=0x{:X}", entry.cave_addr);
        Ok(())
    })
}

#[tauri::command]
async fn find_wemod_drop_cave(pid: u32) -> Result<Vec<String>, String> {
    blocking!({
        let regions = engine::list_executable_regions(pid)?;
        const PAGE: usize = 4096;
        for (base, size, _) in &regions {
            // Skip massive module-backed regions (>32MB) — caves are small anonymous allocs
            if *size > 32 * 1024 * 1024 { continue; }
            // Scan at 4KB page boundaries within the region
            let pages = size.div_ceil(PAGE);
            for p in 0..pages {
                let page_addr = base + p * PAGE;
                let bytes = match engine::read_memory(pid, page_addr, 25) {
                    Ok(b) if b.len() >= 25 => b,
                    _ => continue,
                };
                // Drop rate cave: 0F 57 C0 83 3D (xorps xmm0,xmm0; cmp [RIP+rel32],1)
                if bytes[0..5] != [0x0F, 0x57, 0xC0, 0x83, 0x3D] { continue; }
                // CMP [RIP+rel32],1: 83 3D XX XX XX XX 01 (7 bytes), RIP_after = page+10
                let rel32_cmp = i32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
                let drop_rate_flag = (page_addr as i64) + 10 + (rel32_cmp as i64);
                // JMP return at offset 16: E9 rel32, return = page+21+rel32, aobdroprate = return-7
                if bytes[16] != 0xE9 { continue; }
                let rel32_jmp = i32::from_le_bytes([bytes[17], bytes[18], bytes[19], bytes[20]]);
                let aobdroprate = (page_addr as i64) + 21 + (rel32_jmp as i64) - 7;
                return Ok(vec![
                    format!("0x{:X}", drop_rate_flag as u64),
                    format!("0x{:X}", aobdroprate as u64),
                    format!("0x{:X}", page_addr),
                ]);
            }
        }
        Err("WeMod drop rate cave not found — WeMod may not be injected or drop rate was never enabled".to_string())
    })
}

#[tauri::command]
async fn find_outside_jmps(pid: u32, module_name: String) -> Result<Vec<String>, String> {
    // Scan anonymous regions ≤4MB for WeMod's legendary cave tail:
    //   B8 FE 7F 00 00  — MOV EAX, 0x7FFE
    //   25 FF 7F 00 00  — AND EAX, 0x7FFF  (original BL4 instruction)
    //   E9 XX XX XX XX  — JMP return (back to BL4)
    // The cave starts with CMP [drop_rarity],1 ; JNE ; float-compare vs 90.0, then this tail.
    // The tail is unique and 15 bytes — no wildcards needed.
    // Enable WeMod's "Max Legendary Drop Rate" cheat before running this.
    blocking!({
        let (mod_base, mod_size) = require_module(pid, &module_name)?;
        let mod_end = mod_base + mod_size;
        let regions = engine::list_executable_regions(pid)?;
        let mut results = Vec::new();

        for (region_base, region_size, _) in &regions {
            if *region_size > 4 * 1024 * 1024 { continue; }
            let bytes = match engine::read_memory(pid, *region_base, *region_size) {
                Ok(b) => b,
                Err(_) => continue,
            };

            for i in 0..bytes.len().saturating_sub(15) {
                // MOV EAX, 0x7FFE
                if bytes[i] != 0xB8 || bytes[i+1] != 0xFE || bytes[i+2] != 0x7F
                    || bytes[i+3] != 0x00 || bytes[i+4] != 0x00 { continue; }
                // AND EAX, 0x7FFF immediately after
                if bytes[i+5] != 0x25 || bytes[i+6] != 0xFF || bytes[i+7] != 0x7F
                    || bytes[i+8] != 0x00 || bytes[i+9] != 0x00 { continue; }
                // JMP (E9) return trampoline
                if bytes[i+10] != 0xE9 { continue; }

                let mov_addr = *region_base + i;
                let jmp_addr = *region_base + i + 10;
                let jmp_rel = i32::from_le_bytes([bytes[i+11], bytes[i+12], bytes[i+13], bytes[i+14]]);
                let return_addr = (jmp_addr as i64 + 5 + jmp_rel as i64) as usize;

                let location = if mov_addr >= mod_base && mov_addr < mod_end {
                    format!("BL4 RVA 0x{:X}", mov_addr - mod_base)
                } else {
                    format!("cave 0x{:X}", mov_addr)
                };

                let patch_rva = if return_addr >= mod_base + 5 && return_addr < mod_end {
                    // return_addr is where execution resumes after the AND EAX,0x7FFF
                    // The jmp from BL4 goes to newmem, return goes back to BL4 at return_addr
                    // BL4 patch site (where jmp newmem was written) = return_addr - 5
                    Some(return_addr - mod_base - 5)
                } else {
                    None
                };

                let ctx_end = (i + 32).min(bytes.len());
                let ctx: Vec<String> = bytes[i..ctx_end].iter().map(|b| format!("{:02X}", b)).collect();

                let patch_info = patch_rva
                    .map(|r| format!("patch_site=RVA_0x{:X}", r))
                    .unwrap_or_else(|| format!("return=0x{:X} (not in BL4?)", return_addr));

                results.push(format!(
                    "[{}] {} | ctx: {}",
                    location, patch_info, ctx.join(" ")
                ));
            }
        }

        if results.is_empty() {
            results.push("No legendary cave (MOV_7FFE+AND_7FFF+JMP) found — enable WeMod's Max Legendary cheat first".to_string());
        }
        Ok(results)
    })
}

#[tauri::command]
async fn dump_module_to_file(pid: u32, module_name: String, out_path: String) -> Result<String, String> {
    blocking!({
        let bytes = engine::dump_module_to_file(pid, &module_name, &out_path)?;
        Ok(format!("Dumped {:.2}MB to {}", bytes as f64 / 1_048_576.0, out_path))
    })
}

#[tauri::command]
async fn read_module_strings(pid: u32, module_name: String, min_len: usize) -> Result<Vec<String>, String> {
    blocking!({ engine::read_module_strings(pid, &module_name, min_len) })
}

/// Dumps arbitrary diagnostic text (e.g. the results pane) to disk so it can be handed
/// off without going through clipboard/chat copy-paste for large outputs.
#[tauri::command]
async fn write_text_file(out_path: String, contents: String) -> Result<String, String> {
    blocking!({
        std::fs::write(&out_path, contents.as_bytes()).map_err(|e| e.to_string())?;
        Ok(format!("Wrote {} bytes to {}", contents.len(), out_path))
    })
}

fn dumps_dir() -> Result<std::path::PathBuf, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Could not resolve project root")?
        .join("dumps");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
}

fn dump_timestamp() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())
        .map(|d| d.as_secs())
}

/// Saves diagnostic text into `<project_root>/dumps/`, one file per call (timestamp-named,
/// never overwritten), so a series of DevPanel results can be captured and diffed by hand
/// without round-tripping through the clipboard.
#[tauri::command]
async fn save_dev_dump(contents: String) -> Result<String, String> {
    blocking!({
        let path = dumps_dir()?.join(format!("dump_{}.txt", dump_timestamp()?));
        std::fs::write(&path, contents.as_bytes()).map_err(|e| e.to_string())?;
        Ok(format!("Wrote {} bytes to {}", contents.len(), path.display()))
    })
}

/// Dumps a raw address+size range (not a registered module — for manually-mapped code
/// invisible to EnumProcessModules) into `<project_root>/dumps/` as a binary file.
#[tauri::command]
async fn dump_range_to_file(pid: u32, address: String, size: usize) -> Result<String, String> {
    blocking!({
        let addr = parse_addr(&address)? as usize;
        let path = dumps_dir()?.join(format!("dump_{}.bin", dump_timestamp()?));
        let path_str = path.to_string_lossy().to_string();
        let bytes = engine::dump_range_to_file(pid, addr, size, &path_str)?;
        Ok(format!("Wrote {} bytes to {}", bytes, path_str))
    })
}

#[tauri::command]
fn list_modules(pid: u32) -> Result<Vec<String>, String> {
    let mut mods = engine::list_all_modules(pid)?;
    mods.sort_by(|a, b| b.2.cmp(&a.2));
    Ok(mods.iter().map(|(name, base, size)| {
        format!("{:<52}  base=0x{:016X}  size={:.1}MB", name, base, *size as f64 / 1_048_576.0)
    }).collect())
}

#[tauri::command]
async fn snapshot_full(pid: u32, module_name: String) -> Result<String, String> {
    blocking!({
        let (base, size) = require_module(pid, &module_name)?;
        let regions = engine::snapshot_all_pages(pid, base, size)?;
        let total: usize = regions.iter().map(|(_, d)| d.len()).sum();
        let count = regions.len();
        *MODULE_SNAPSHOT.lock().unwrap() = Some(ModuleSnapshot { module_name: module_name.clone(), base, regions });
        Ok(format!("Full snapshot {} pages ({:.1}MB) — {} @ 0x{:X}", count, total as f64 / 1_048_576.0, module_name, base))
    })
}

#[tauri::command]
async fn snapshot_by_module_name(pid: u32, module_name: String) -> Result<String, String> {
    blocking!({
        let (base, size) = require_module(pid, &module_name)?;
        let regions = engine::snapshot_executable_pages(pid, base, size)?;
        let total: usize = regions.iter().map(|(_, d)| d.len()).sum();
        let count = regions.len();
        *MODULE_SNAPSHOT.lock().unwrap() = Some(ModuleSnapshot { module_name: module_name.clone(), base, regions });
        Ok(format!("Snapshotted {} bytes across {} exec regions — {} @ 0x{:X}", total, count, module_name, base))
    })
}

#[tauri::command]
fn list_exec_regions(pid: u32) -> Result<Vec<String>, String> {
    let regions = engine::list_executable_regions(pid)?;
    Ok(regions.iter().map(|(base, size, prot)| {
        format!("0x{:016X}  size={:.2}MB  prot={}", base, *size as f64 / 1_048_576.0, prot)
    }).collect())
}

#[tauri::command]
fn patch_bytes(pid: u32, address: String, bytes: Vec<u8>) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_memory_rw(pid, addr as usize, &bytes)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("magic-wand.log")))
        .unwrap_or_else(|| std::path::PathBuf::from("magic-wand.log"));
    logger::init(&log_path);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            scan_games,
            find_game,
            get_module_base,
            aob_scan,
            resolve_godot_member,
            resolve_pointer,
            read_int,
            read_float,
            read_double,
            write_int,
            write_float,
            write_double,
            scan_value,
            patch_bytes,
            resolve_mono_field,
            resolve_mono_chain,
            resolve_mono_path,
            resolve_mono_method,
            set_mono_store_nop,
            set_mono_method_ret,
            set_mono_call_nop,
            resolve_il2cpp_path,
            set_il2cpp_method_ret,
            set_il2cpp_decay_nop,
            resolve_ue5_prop,
            resolve_ue5_prop_custom,
            resolve_pointer_chain_from,
            verify_object_class,
            resolve_ue5_array_item,
            write_byte,
            read_byte,
            toggle_bit_flag,
            dump_floats_at,
            list_ue5_classes,
            lookup_fnames,
            snapshot_by_module_name,
            snapshot_full,
            diff_snapshot,
            read_snapshot_region,
            read_raw_bytes,
            aob_scan_range,
            aob_scan_all_range,
            aob_scan_all,
            list_modules,
            list_exec_regions,
            dump_module_to_file,
            read_module_strings,
            write_text_file,
            save_dev_dump,
            dump_range_to_file,
            find_wemod_drop_cave,
            find_outside_jmps,
            scan_rarity_candidates,
            enable_code_cave,
            disable_code_cave
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
