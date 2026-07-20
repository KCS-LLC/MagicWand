mod engine;
mod logger;
mod mono;
mod scanner;
mod ue5;

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
fn resolve_ue5_prop(
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
}

/// Like resolve_ue5_prop, but takes an explicit Ue5Offsets struct instead of assuming
/// ue5::Ue5Offsets::ue5_default(). Lets a trainer target UE5 games whose internal struct
/// layout (FUObjectItem size, FName entry header, etc.) differs from the default.
#[tauri::command]
fn resolve_ue5_prop_custom(
    pid: u32,
    module_name: String,
    gobjects_offset: usize,
    gnames_offset: usize,
    class_name: String,
    property_offset: usize,
    extra_offsets: Option<Vec<usize>>,
    offsets: ue5::Ue5Offsets,
) -> Result<String, String> {
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
) -> Result<(), String> {
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
            return Ok(());
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
        Ok(())
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
            resolve_ue5_prop,
            resolve_ue5_prop_custom,
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
