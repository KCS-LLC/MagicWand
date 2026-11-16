use crate::engine::{read_memory, aob_scan_range, read_ptr, read_u32};
use serde::Deserialize;
use std::collections::HashMap;

fn default_true() -> bool { true }

#[derive(Deserialize)]
pub struct Ue5Offsets {
    pub fuobjectitem_size: usize,
    pub fuobjectitem_object: usize,
    pub uobject_class: usize,
    pub uobject_name: usize,
    pub gobjects_chunk_size: usize,
    pub gobjects_objects: usize,
    pub gobjects_num_elements: usize,
    pub fname_stride: usize,
    pub fname_entry_header: usize,
    pub gnames_chunk_size: usize,
    // Most UE5 games (e.g. UE4.21-5.7 default TUObjectArray) store GObjects.Objects as a
    // pointer to an array of chunk pointers (FUObjectItem** — each chunk holds up to
    // gobjects_chunk_size items). Some builds instead use the older flat TUObjectArray
    // layout, where Objects points directly at one contiguous FUObjectItem array with no
    // chunk indirection at all (confirmed via a game's own Dumper-7-generated Basic.hpp:
    // `struct FUObjectItem* Objects;` used directly by `GetByIndex` with no chunk step).
    // Defaults to true (chunked) so existing BL4-style configs are unaffected.
    #[serde(default = "default_true")]
    pub gobjects_chunked: bool,
}

impl Ue5Offsets {
    pub fn ue5_default() -> Self {
        Ue5Offsets {
            fuobjectitem_size:     0x18,
            fuobjectitem_object:   0x00,
            uobject_class:         0x10,
            uobject_name:          0x18,
            gobjects_chunk_size:   0x10000,
            gobjects_objects:      0x00,
            gobjects_num_elements: 0x14,
            fname_stride:          2,
            fname_entry_header:    0x02,
            gnames_chunk_size:     0x20000,
            gobjects_chunked:      true,
        }
    }
}

fn read_i32(pid: u32, addr: usize) -> Option<i32> {
    read_u32(pid, addr).map(|v| v as i32)
}

fn resolve_rip_relative(
    pid: u32,
    module_base: usize,
    module_size: usize,
    pattern: &str,
    rip_offset: usize,
    instr_end: usize,
) -> Option<usize> {
    let match_addr = aob_scan_range(pid, module_base, module_size, pattern)?;
    let disp = read_i32(pid, match_addr + rip_offset)? as isize;
    Some(((match_addr + instr_end) as isize + disp) as usize)
}

fn fname_to_string(pid: u32, gnames_base: usize, fname_index: i32, off: &Ue5Offsets) -> Option<String> {
    if fname_index < 0 { return None; }
    let idx = fname_index as usize;
    let entries_per_chunk = off.gnames_chunk_size / off.fname_stride;
    let chunk_idx = idx / entries_per_chunk;
    let entry_off = (idx % entries_per_chunk) * off.fname_stride;
    let chunk_ptr = read_ptr(pid, gnames_base + chunk_idx * 8)?;
    let entry_addr = chunk_ptr + entry_off;

    // UE5 FNameEntryHeader (2 bytes): bit[0]=bIsWide, bits[15:6]=Len.
    // Strings are length-delimited, not null-terminated.
    let hdr_bytes = read_memory(pid, entry_addr, 2).ok()?;
    let hdr = u16::from_le_bytes([hdr_bytes[0], hdr_bytes[1]]);
    let str_len = (hdr >> 6) as usize;
    if str_len == 0 || str_len > 1024 { return None; }

    let raw = read_memory(pid, entry_addr + off.fname_entry_header, str_len).ok()?;
    String::from_utf8(raw).ok()
}

// Walk the SuperStruct chain to check if a UClass inherits from target_class.
// UStruct::SuperStruct is at 0x30: UObjectBase(0x28) + UField::Next(8) = 0x30.
fn class_inherits_from(pid: u32, gnames_base: usize, mut class_ptr: usize, target: &str, off: &Ue5Offsets) -> bool {
    const SUPER_OFFSET: usize = 0x40;
    class_ptr &= !7;
    for _ in 0..20 {
        // Reject null and non-canonical (kernel/garbage) addresses before any arithmetic.
        if class_ptr == 0 || class_ptr > 0x0000_7FFF_FFFF_FFFF { break; }
        let fname_idx = match read_i32(pid, class_ptr + off.uobject_name) {
            Some(i) if i >= 0 => i,
            _ => break,
        };
        if let Some(name) = fname_to_string(pid, gnames_base, fname_idx, off) {
            if name == target { return true; }
        }
        class_ptr = match read_ptr(pid, class_ptr + SUPER_OFFSET) {
            Some(p) => p & !7,
            None => break,
        };
    }
    false
}

pub fn lookup_fnames(pid: u32, gnames_base: usize, indices: &[u32], off: &Ue5Offsets) -> Vec<String> {
    indices.iter().map(|&idx| {
        fname_to_string(pid, gnames_base, idx as i32, off)
            .unwrap_or_else(|| format!("?{}", idx))
    }).collect()
}

pub fn list_classes_by_keyword(
    pid: u32,
    gobjects_base: usize,
    gnames_base: usize,
    keyword: &str,
    off: &Ue5Offsets,
) -> Result<Vec<String>, String> {
    let num_elements = read_u32(pid, gobjects_base + off.gobjects_num_elements)
        .ok_or("Could not read GObjects.NumElements")? as usize;
    if num_elements == 0 || num_elements > 2_000_000 {
        return Err(format!("GObjects.NumElements={} looks invalid", num_elements));
    }
    let objects_ptr = read_ptr(pid, gobjects_base + off.gobjects_objects)
        .ok_or("Could not read GObjects.Objects")?;
    let kw_lower = keyword.to_lowercase();
    let num_chunks = (num_elements + off.gobjects_chunk_size - 1) / off.gobjects_chunk_size;
    let mut seen: HashMap<usize, ()> = HashMap::new();
    let mut results: Vec<String> = Vec::new();
    for chunk_idx in 0..num_chunks {
        let chunk_ptr = match read_ptr(pid, objects_ptr + chunk_idx * 8) {
            Some(p) if p != 0 => p,
            _ => continue,
        };
        let items_start = chunk_idx * off.gobjects_chunk_size;
        let items_in_chunk = (num_elements - items_start).min(off.gobjects_chunk_size);
        let chunk_bytes = match read_memory(pid, chunk_ptr, items_in_chunk * off.fuobjectitem_size) {
            Ok(b) => b,
            Err(_) => continue,
        };
        for item_idx in 0..items_in_chunk {
            let item_base = item_idx * off.fuobjectitem_size + off.fuobjectitem_object;
            if item_base + 8 > chunk_bytes.len() { continue; }
            let mut obj_ptr = usize::from_le_bytes(
                match chunk_bytes[item_base..item_base + 8].try_into() {
                    Ok(b) => b,
                    Err(_) => continue,
                }
            );
            if obj_ptr == 0 || obj_ptr > 0x0000_7FFF_FFFF_FFFF { continue; }
            obj_ptr &= !7;
            let mut class_ptr = match read_ptr(pid, obj_ptr + off.uobject_class) {
                Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p,
                _ => continue,
            };
            class_ptr &= !7;
            if seen.contains_key(&class_ptr) { continue; }
            seen.insert(class_ptr, ());
            if let Some(i) = read_i32(pid, class_ptr + off.uobject_name) {
                if i >= 0 {
                    if let Some(name) = fname_to_string(pid, gnames_base, i, off) {
                        if name.to_lowercase().contains(&kw_lower) {
                            results.push(name);
                        }
                    }
                }
            }
        }
    }
    results.sort();
    Ok(results)
}

pub fn find_object_by_class(
    pid: u32,
    gobjects_base: usize,
    gnames_base: usize,
    target_class: &str,
    off: &Ue5Offsets,
) -> Result<usize, String> {
    let num_elements = read_u32(pid, gobjects_base + off.gobjects_num_elements)
        .ok_or("Could not read GObjects.NumElements")? as usize;

    if num_elements == 0 || num_elements > 2_000_000 {
        return Err(format!("GObjects.NumElements={} looks invalid", num_elements));
    }

    let objects_ptr = read_ptr(pid, gobjects_base + off.gobjects_objects)
        .ok_or("Could not read GObjects.Objects")?;

    crate::mwlog!("[ue5] scanning {} objects for class '{}'", num_elements, target_class);

    // Cache class_ptr → matches so each unique class is walked only once.
    let mut class_cache: HashMap<usize, bool> = HashMap::new();
    // Collect direct class names for diagnostics (populated per unique class_ptr).
    let mut seen_names: Vec<String> = Vec::new();
    let num_chunks = if off.gobjects_chunked {
        (num_elements + off.gobjects_chunk_size - 1) / off.gobjects_chunk_size
    } else {
        1
    };

    for chunk_idx in 0..num_chunks {
        // Flat layout: objects_ptr already points directly at the one contiguous
        // FUObjectItem array — no per-chunk pointer to dereference.
        let chunk_ptr = if off.gobjects_chunked {
            match read_ptr(pid, objects_ptr + chunk_idx * 8) {
                Some(p) if p != 0 => p,
                _ => continue,
            }
        } else {
            objects_ptr
        };

        let items_start = chunk_idx * off.gobjects_chunk_size;
        let items_in_chunk = if off.gobjects_chunked {
            (num_elements - items_start).min(off.gobjects_chunk_size)
        } else {
            num_elements
        };

        // Bulk-read the entire chunk: one RPM call instead of one per item.
        let chunk_bytes = match read_memory(pid, chunk_ptr, items_in_chunk * off.fuobjectitem_size) {
            Ok(b) => b,
            Err(_) => continue,
        };

        for item_idx in 0..items_in_chunk {
            let item_base = item_idx * off.fuobjectitem_size + off.fuobjectitem_object;
            if item_base + 8 > chunk_bytes.len() { continue; }

            let mut obj_ptr = usize::from_le_bytes(
                match chunk_bytes[item_base..item_base + 8].try_into() {
                    Ok(b) => b,
                    Err(_) => continue,
                }
            );
            if obj_ptr == 0 || obj_ptr > 0x0000_7FFF_FFFF_FFFF { continue; }
            obj_ptr &= !7;

            // Skip only Class Default Objects — they have zeroed/default values.
            // ObjectFlags at UObject+0x08: RF_ClassDefaultObject=0x10.
            if let Some(flags) = read_u32(pid, obj_ptr + 0x08) {
                if flags & 0x10 != 0 { continue; }
            }

            let mut class_ptr = match read_ptr(pid, obj_ptr + off.uobject_class) {
                Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p,
                _ => continue,
            };
            class_ptr &= !7;

            // Manual entry to allow collecting names alongside the cache insert.
            let matches = if let Some(&cached) = class_cache.get(&class_ptr) {
                cached
            } else {
                // Collect direct FName for diagnostic output.
                if let Some(i) = read_i32(pid, class_ptr + off.uobject_name) {
                    if i >= 0 {
                        if let Some(name) = fname_to_string(pid, gnames_base, i, off) {
                            seen_names.push(name);
                        }
                    }
                }
                let result = class_inherits_from(pid, gnames_base, class_ptr, target_class, off);
                class_cache.insert(class_ptr, result);
                result
            };

            if matches {
                let flags = read_u32(pid, obj_ptr + 0x08).unwrap_or(0);
                crate::mwlog!("[ue5] found '{}' object at 0x{:X} (ObjFlags=0x{:08X})", target_class, obj_ptr, flags);
                return Ok(obj_ptr);
            }
        }
    }

    seen_names.sort();
    seen_names.dedup();
    // Print a short filtered list of character/pawn/player-like names first so they're easy to spot.
    let char_names: Vec<&str> = seen_names.iter()
        .filter(|n| {
            let low = n.to_lowercase();
            low.contains("character") || low.contains("pawn") || low.contains("player")
                || low.contains("vault") || low.contains("hunter") || low.contains("oak")
        })
        .map(|s| s.as_str())
        .collect();
    crate::mwlog!("[ue5] '{}' not found. Character/Player-like classes: {:?}", target_class, char_names);
    let preview: Vec<&str> = seen_names.iter().take(100).map(|s| s.as_str()).collect();
    crate::mwlog!("[ue5] Class list (first 100 of {} unique): {:?}", seen_names.len(), preview);
    Err(format!("No object with class '{}' found in GObjects", target_class))
}

// Cheap sanity check for a single, already-known object pointer: does it still look like a
// live instance of target_class? Unlike find_object_by_class, this never scans GObjects —
// just a couple of reads plus a SuperStruct walk — so it's safe to run every poll tick even
// for a ue5NoCache cheat's cached class-object pointer. Exists because a stale-but-still
// -committed pointer (e.g. after a level transition recreates the controller) doesn't
// necessarily fault on read/write, so relying on an actual error to trigger cache
// invalidation misses this case entirely — this catches it proactively instead.
pub fn verify_object_class(
    pid: u32,
    gnames_base: usize,
    obj_ptr: usize,
    target_class: &str,
    off: &Ue5Offsets,
) -> bool {
    if obj_ptr == 0 || obj_ptr > 0x0000_7FFF_FFFF_FFFF {
        return false;
    }
    let class_ptr = match read_ptr(pid, obj_ptr + off.uobject_class) {
        Some(p) if p != 0 && p <= 0x0000_7FFF_FFFF_FFFF => p,
        _ => return false,
    };
    class_inherits_from(pid, gnames_base, class_ptr, target_class, off)
}

/// Resolves the address of a value field inside a dynamic TArray element, by walking a
/// pointer chain to a container object (e.g. an inventory component) then linear-scanning
/// its TArray for the first element whose sub-object class matches `match_class` (matched
/// via the same is-a inheritance check `find_object_by_class` uses, not exact string
/// equality). Built for stacks like `TArray<FItemStack>` where the target's index isn't
/// fixed (items get added/removed, reordering the array) — a plain fixed-offset ue5_prop
/// chain can't reach a moving target like that. Assumes the standard UE `TArray<T>` ABI:
/// `T* Data` at +0x00, `int32 Num` at +0x08 (Max at +0x0C, unused here).
///
/// `extra_offsets` (same semantics as `resolve_ue5_prop`'s chain) resolves down to the
/// ADDRESS of the pointer-to-container-object slot; this function then dereferences that
/// slot once more itself to land on the container object's own address (unlike
/// `resolve_ue5_prop`, whose chain ends on a value slot, not an object to read further
/// fields from).
#[allow(clippy::too_many_arguments)]
pub fn resolve_ue5_array_item(
    pid: u32,
    gobjects_base: usize,
    gnames_base: usize,
    class_name: &str,
    property_offset: usize,
    extra_offsets: &[usize],
    array_offset: usize,
    item_stride: usize,
    item_ptr_offset: usize,
    match_class: &str,
    value_offset: usize,
    off: &Ue5Offsets,
) -> Result<usize, String> {
    let obj_ptr = find_object_by_class(pid, gobjects_base, gnames_base, class_name, off)?;
    let initial = obj_ptr + property_offset;
    let container_slot = if extra_offsets.is_empty() {
        initial
    } else {
        crate::engine::resolve_pointer_path(pid, initial, extra_offsets)?
    };
    let container_ptr = read_ptr(pid, container_slot)
        .filter(|&p| p != 0)
        .ok_or("Null/unreadable container pointer while resolving array item")?;

    let arr_data = read_ptr(pid, container_ptr + array_offset)
        .filter(|&p| p != 0)
        .ok_or("Could not read TArray.Data")?;
    let arr_num = read_u32(pid, container_ptr + array_offset + 8)
        .ok_or("Could not read TArray.Num")? as usize;
    if arr_num > 100_000 {
        return Err(format!("TArray.Num={} looks invalid", arr_num));
    }

    for i in 0..arr_num {
        let item_addr = arr_data + i * item_stride;
        let item_obj_ptr = match read_ptr(pid, item_addr + item_ptr_offset) {
            Some(p) if p != 0 => p,
            _ => continue,
        };
        let class_ptr = match read_ptr(pid, item_obj_ptr + off.uobject_class) {
            Some(p) if p != 0 => p,
            _ => continue,
        };
        if class_inherits_from(pid, gnames_base, class_ptr, match_class, off) {
            return Ok(item_addr + value_offset);
        }
    }

    Err(format!(
        "No item of class '{}' found in array ({} entries)",
        match_class, arr_num
    ))
}

pub fn resolve_ue5_prop(
    pid: u32,
    module_base: usize,
    module_size: usize,
    gobjects_aob: &str,
    gnames_aob: &str,
    class_name: &str,
    property_offset: usize,
    extra_offsets: &[usize],
) -> Result<usize, String> {
    let off = Ue5Offsets::ue5_default();

    let gobjects_base = resolve_rip_relative(pid, module_base, module_size, gobjects_aob, 3, 7)
        .ok_or("Could not resolve GObjects from AOB")?;

    let gnames_base = resolve_rip_relative(pid, module_base, module_size, gnames_aob, 3, 7)
        .ok_or("Could not resolve GNames from AOB")?;

    let obj_ptr = find_object_by_class(pid, gobjects_base, gnames_base, class_name, &off)?;
    let initial = obj_ptr + property_offset;
    crate::mwlog!("[ue5/aob] obj_ptr=0x{:X}  initial=0x{:X}  extra_offsets={:?}", obj_ptr, initial, extra_offsets);

    if extra_offsets.is_empty() {
        Ok(initial)
    } else {
        crate::engine::resolve_pointer_path(pid, initial, extra_offsets)
    }
}


pub fn resolve_ue5_prop_static(
    pid: u32,
    module_base: usize,
    _module_size: usize,
    gobjects_offset: usize,
    gnames_offset: usize,
    class_name: &str,
    property_offset: usize,
    extra_offsets: &[usize],
) -> Result<usize, String> {
    let off = Ue5Offsets::ue5_default();
    let gobjects_base = module_base + gobjects_offset;
    let gnames_base   = module_base + gnames_offset;
    let obj_ptr = find_object_by_class(pid, gobjects_base, gnames_base, class_name, &off)?;
    let initial = obj_ptr + property_offset;

    if extra_offsets.is_empty() {
        Ok(initial)
    } else {
        crate::engine::resolve_pointer_path(pid, initial, extra_offsets)
    }
}
