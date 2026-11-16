// Standalone diagnostic: bruteforce every AOB-matched GObjects candidate in a running
// Borderlands4.exe process and report which ones actually contain gameplay pawn/controller
// instances (as opposed to engine-default-only pools). Run with `cargo run --bin scan_gobjects`
// while the game is running. Not part of the Tauri app — see docs/handoff-ue5-scan.md.

fn fname_to_string(pid: u32, gnames_base: usize, fname_index: i32, off: &magic_wand_lib::ue5::Ue5Offsets) -> Option<String> {
    if fname_index < 0 { return None; }
    let idx = fname_index as usize;
    let entries_per_chunk = off.gnames_chunk_size / off.fname_stride;
    let chunk_idx = idx / entries_per_chunk;
    let entry_off = (idx % entries_per_chunk) * off.fname_stride;

    let chunk_ptr = match magic_wand_lib::engine::read_memory(pid, gnames_base + chunk_idx * 8, 8) {
        Ok(bytes) => usize::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]),
        Err(_) => return None,
    };
    if chunk_ptr == 0 { return None; }

    let entry_addr = chunk_ptr + entry_off;

    let hdr_bytes = magic_wand_lib::engine::read_memory(pid, entry_addr, 2).ok()?;
    let hdr = u16::from_le_bytes([hdr_bytes[0], hdr_bytes[1]]);
    let str_len = (hdr >> 6) as usize;
    if str_len == 0 || str_len > 1024 { return None; }

    let raw = magic_wand_lib::engine::read_memory(pid, entry_addr + off.fname_entry_header, str_len).ok()?;
    String::from_utf8(raw).ok()
}

fn scan_candidate_for_gameplay_objects(pid: u32, base: usize, gnames_base: usize, rva: usize, off: &magic_wand_lib::ue5::Ue5Offsets) -> bool {
    let gobjects_base = base + rva;
    let num_elements = match magic_wand_lib::engine::read_memory(pid, gobjects_base + off.gobjects_num_elements, 4) {
        Ok(bytes) => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize,
        Err(_) => return false,
    };
    if num_elements < 1000 || num_elements > 1000000 { return false; }

    let objects_ptr = match magic_wand_lib::engine::read_memory(pid, gobjects_base + off.gobjects_objects, 8) {
        Ok(bytes) => usize::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]),
        _ => 0,
    };
    if objects_ptr == 0 { return false; }

    let num_chunks = (num_elements + off.gobjects_chunk_size - 1) / off.gobjects_chunk_size;
    let mut found_count = 0;

    for chunk_idx in 0..num_chunks {
        let chunk_ptr = match magic_wand_lib::engine::read_memory(pid, objects_ptr + chunk_idx * 8, 8) {
            Ok(bytes) => usize::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]),
            _ => continue,
        };
        if chunk_ptr == 0 { continue; }

        let items_start = chunk_idx * off.gobjects_chunk_size;
        let items_in_chunk = (num_elements - items_start).min(off.gobjects_chunk_size);

        if let Ok(chunk_bytes) = magic_wand_lib::engine::read_memory(pid, chunk_ptr, items_in_chunk * off.fuobjectitem_size) {
            for item_idx in 0..items_in_chunk {
                let item_base = item_idx * off.fuobjectitem_size + off.fuobjectitem_object;
                let raw_ptr = usize::from_le_bytes([
                    chunk_bytes[item_base],
                    chunk_bytes[item_base + 1],
                    chunk_bytes[item_base + 2],
                    chunk_bytes[item_base + 3],
                    chunk_bytes[item_base + 4],
                    chunk_bytes[item_base + 5],
                    chunk_bytes[item_base + 6],
                    chunk_bytes[item_base + 7],
                ]);

                if raw_ptr == 0 || raw_ptr > 0x0000_7FFF_FFFF_FFFF { continue; }
                let obj_ptr = raw_ptr & !7;

                let raw_class_ptr = match magic_wand_lib::engine::read_memory(pid, obj_ptr + off.uobject_class, 8) {
                    Ok(bytes) => usize::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]),
                    _ => continue,
                };
                if raw_class_ptr == 0 || raw_class_ptr > 0x0000_7FFF_FFFF_FFFF { continue; }
                let class_ptr = raw_class_ptr & !7;

                let cname_idx = magic_wand_lib::engine::read_u32(pid, class_ptr + off.uobject_name).unwrap_or(0) as i32;
                if cname_idx >= 0 {
                    if let Some(cname) = fname_to_string(pid, gnames_base, cname_idx, off) {
                        let low_cname = cname.to_lowercase();
                        if low_cname.contains("oakplayer") || low_cname.contains("bp_oak") {
                            let oname_idx = magic_wand_lib::engine::read_u32(pid, obj_ptr + off.uobject_name).unwrap_or(0) as i32;
                            let oname = fname_to_string(pid, gnames_base, oname_idx, off).unwrap_or_else(|| "".to_string());
                            let flags = magic_wand_lib::engine::read_u32(pid, obj_ptr + 0x08).unwrap_or(0);

                            if found_count == 0 {
                                println!("\n  [SUCCESS] Match found in GObjects RVA 0x{:X} ({} elements):", rva, num_elements);
                            }
                            println!("    Obj: 0x{:X} | Class: '{}' | Name: '{}' | Flags: 0x{:08X}", obj_ptr, cname, oname, flags);
                            found_count += 1;
                            if found_count >= 10 { return true; }
                        }
                    }
                }
            }
        }
    }
    found_count > 0
}

// Fast rediscovery: instead of a byte-pattern AOB scan (which chokes on common instruction
// sequences across an 883MB module), scan committed, non-executable regions once and look
// for the live GObjects.NumElements value sitting at the expected struct offset. This is a
// single bulk read + local int32 compare per candidate, not a windowed re-scan-per-match.
fn find_gobjects_by_count(pid: u32, base: usize, size: usize, off: &magic_wand_lib::ue5::Ue5Offsets, expected_count_range: (u32, u32)) -> Vec<usize> {
    let mut candidates = Vec::new();
    let regions = magic_wand_lib::engine::walk_committed_regions(pid, base, size, false, 64 * 1024 * 1024);
    for (region_base, data) in regions {
        if data.len() < off.gobjects_num_elements + 4 { continue; }
        let mut pos = 0usize;
        while pos + 4 <= data.len() {
            let v = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            if v >= expected_count_range.0 && v <= expected_count_range.1 && pos >= off.gobjects_num_elements {
                let candidate_addr = region_base + pos - off.gobjects_num_elements;
                if candidate_addr >= base {
                    candidates.push(candidate_addr - base);
                }
            }
            pos += 4;
        }
    }
    candidates
}

fn main() {
    let pid = magic_wand_lib::engine::find_process_by_name("Borderlands4")
        .or_else(|| magic_wand_lib::engine::find_process_by_name("Borderlands4.exe"))
        .expect("Could not find running process Borderlands4.exe");
    println!("Found Borderlands4 process with PID: {}", pid);

    let (base, size) = magic_wand_lib::engine::get_module_info(pid, "Borderlands4.exe")
        .expect("Could not find module info for Borderlands4.exe");
    println!("Base Address: 0x{:X}, Size: 0x{:X}", base, size);

    let off = magic_wand_lib::ue5::Ue5Offsets::ue5_default();
    let gnames_base = base + 0xC7ABD90;

    // Sanity check: is the previously-confirmed GNames RVA (from before today's second
    // patch) still valid? Index 0 must decode to "None".
    match fname_to_string(pid, gnames_base, 0, &off) {
        Some(s) if s == "None" => println!("GNames RVA 0xC7ABD90 still valid (index 0 = 'None')."),
        other => println!("GNames RVA 0xC7ABD90 NO LONGER VALID (index 0 decoded as {:?}) — needs rediscovery too.", other),
    }

    // The live GObjects-Dump.txt from the fresh Dumper-7 run reported Count: 208787 —
    // search for candidates whose NumElements falls in a tight band around that.
    println!("\n--- Scanning committed regions for GObjects.NumElements ~= 208787 ---");
    let candidates = find_gobjects_by_count(pid, base, size, &off, (208000, 209500));
    println!("Found {} raw candidate RVA(s). Validating each for gameplay data...", candidates.len());

    let mut found_any = false;
    for rva in candidates {
        println!("\n[candidate] RVA 0x{:X}:", rva);
        if scan_candidate_for_gameplay_objects(pid, base, gnames_base, rva, &off) {
            found_any = true;
        }
    }

    if !found_any {
        println!("\n  [FAILURE] No candidate in range contained gameplay pawn/controller data.");
    }
}
