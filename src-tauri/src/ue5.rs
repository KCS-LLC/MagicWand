use crate::engine::{read_memory, aob_scan_range, read_ptr, read_u32};
use std::collections::HashMap;

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
    const SUPER_OFFSET: usize = 0x30;
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
            Some(p) => p,
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
            let obj_ptr = usize::from_le_bytes(
                match chunk_bytes[item_base..item_base + 8].try_into() {
                    Ok(b) => b,
                    Err(_) => continue,
                }
            );
            if obj_ptr == 0 || obj_ptr > 0x0000_7FFF_FFFF_FFFF { continue; }
            let class_ptr = match read_ptr(pid, obj_ptr + off.uobject_class) {
                Some(p) if p != 0 => p,
                _ => continue,
            };
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
    let num_chunks = (num_elements + off.gobjects_chunk_size - 1) / off.gobjects_chunk_size;

    for chunk_idx in 0..num_chunks {
        let chunk_ptr = match read_ptr(pid, objects_ptr + chunk_idx * 8) {
            Some(p) if p != 0 => p,
            _ => continue,
        };

        let items_start = chunk_idx * off.gobjects_chunk_size;
        let items_in_chunk = (num_elements - items_start).min(off.gobjects_chunk_size);

        // Bulk-read the entire chunk: one RPM call instead of one per item.
        let chunk_bytes = match read_memory(pid, chunk_ptr, items_in_chunk * off.fuobjectitem_size) {
            Ok(b) => b,
            Err(_) => continue,
        };

        for item_idx in 0..items_in_chunk {
            let item_base = item_idx * off.fuobjectitem_size + off.fuobjectitem_object;
            if item_base + 8 > chunk_bytes.len() { continue; }

            let obj_ptr = usize::from_le_bytes(
                match chunk_bytes[item_base..item_base + 8].try_into() {
                    Ok(b) => b,
                    Err(_) => continue,
                }
            );
            if obj_ptr == 0 || obj_ptr > 0x0000_7FFF_FFFF_FFFF { continue; }

            // Skip Class Default Objects and Archetypes — they have zeroed/default values.
            // ObjectFlags at UObject+0x08: RF_ClassDefaultObject=0x10, RF_ArchetypeObject=0x20.
            if let Some(flags) = read_u32(pid, obj_ptr + 0x08) {
                if flags & 0x30 != 0 { continue; }
            }

            let class_ptr = match read_ptr(pid, obj_ptr + off.uobject_class) {
                Some(p) if p != 0 => p,
                _ => continue,
            };

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
