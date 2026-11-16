//! Godot 4 (GDScript) member-variable resolver.
//!
//! GDScript instance variables live in a `Vector<Variant>` ("members") owned by a
//! `GDScriptInstance` heap object. Heap addresses change every launch, but the object's first
//! qword is the `GDScriptInstance` vtable pointer, which sits at a fixed RVA inside the game
//! exe. We scan for that vtable, then identify the right instance by its member count plus a
//! fingerprint of Variant type tags, and return the address of one member's payload.
//!
//! Layout (Godot 4.7 custom build, Find The Needle — see docs/game-find-the-needle.md):
//!   instance + 0x00 : vtable
//!   instance + 0x28 : pointer to members array data (CowData<Variant>)
//!   members  - 0x10 : element count (u64)
//!   members  + 24*i : Variant { u32 type tag, u32 pad, 16-byte union }; payload at +8
//! Variant tags: 1 bool, 2 int, 3 float (stored as f64), 4 String, ...

use crate::engine;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Result of the last full vtable scan, shared across cheats: money and stamina resolve
/// concurrently on connect, and the scan (a pass over the whole multi-GB process) is the
/// expensive part. The mutex is held for the duration of a scan so a second caller waits for
/// it and then reuses the hit list instead of scanning again.
struct ScanCache {
    pid: u32,
    vtable: usize,
    at: Instant,
    hits: Vec<usize>,
}
static SCAN_CACHE: Mutex<Option<ScanCache>> = Mutex::new(None);
const SCAN_CACHE_TTL: Duration = Duration::from_secs(120);

const INSTANCE_MEMBERS_OFFSET: usize = 0x28;
const VARIANT_SIZE: usize = 24;
const VARIANT_PAYLOAD: usize = 8;

/// Returns the members-array pointer if `inst` is a live GDScriptInstance whose member array
/// has exactly `count` entries and whose type tags match `tags` (`-1` = don't care).
fn validate(
    pid: u32,
    inst: usize,
    vtable: usize,
    count: usize,
    tags: &[i64],
) -> Option<usize> {
    if engine::read_ptr(pid, inst)? != vtable {
        return None;
    }
    let members = engine::read_ptr(pid, inst + INSTANCE_MEMBERS_OFFSET)?;
    if members < 0x10000 {
        return None;
    }
    if engine::read_ptr(pid, members - 0x10)? != count {
        return None;
    }
    let data = engine::read_memory(pid, members, count * VARIANT_SIZE).ok()?;
    for (i, &want) in tags.iter().enumerate() {
        if want < 0 || i >= count {
            continue;
        }
        let got = u32::from_le_bytes(data[i * VARIANT_SIZE..i * VARIANT_SIZE + 4].try_into().ok()?);
        if got as i64 != want {
            return None;
        }
    }
    Some(members)
}

/// Finds the GDScriptInstance matching the fingerprint and returns `(instance, member_payload_addr)`.
/// `hint` (a previously returned instance) is revalidated first — cheap — so the full-process
/// scan only runs the first time or after the instance is destroyed (scene change).
pub fn resolve_member(
    pid: u32,
    module_name: &str,
    vtable_rva: usize,
    count: usize,
    tags: &[i64],
    index: usize,
    hint: Option<usize>,
) -> Result<(usize, usize), String> {
    if index >= count {
        return Err("member index out of range".to_string());
    }
    let (base, _) = engine::get_module_info(pid, module_name)
        .ok_or_else(|| format!("Module '{}' not found", module_name))?;
    let vtable = base + vtable_rva;

    let payload = |members: usize| members + index * VARIANT_SIZE + VARIANT_PAYLOAD;

    if let Some(h) = hint {
        if let Some(m) = validate(pid, h, vtable, count, tags) {
            return Ok((h, payload(m)));
        }
    }
    let mut guard = SCAN_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    // Try the shared hit list from a recent scan first.
    if let Some(c) = guard.as_ref() {
        if c.pid == pid && c.vtable == vtable && c.at.elapsed() < SCAN_CACHE_TTL {
            for &inst in &c.hits {
                if let Some(m) = validate(pid, inst, vtable, count, tags) {
                    return Ok((inst, payload(m)));
                }
            }
        }
    }
    // Nothing usable cached (or it predates the instance): scan fresh.
    let hits = engine::scan_aligned_u64(pid, vtable as u64);
    let found = hits
        .iter()
        .find_map(|&inst| validate(pid, inst, vtable, count, tags).map(|m| (inst, payload(m))));
    *guard = Some(ScanCache { pid, vtable, at: Instant::now(), hits });
    found.ok_or_else(|| {
        "Godot script instance not found — is the game past the main menu / in a level?".to_string()
    })
}
