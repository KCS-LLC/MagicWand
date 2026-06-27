use crate::engine::{read_memory, aob_scan_range};

pub struct Ue5Offsets {
    pub fuobjectitem_size: usize,
    pub fuobjectitem_object: usize,
    pub uobject_class: usize,
    pub uobject_name: usize,
    pub uclass_children: usize,
    pub ffield_next: usize,
    pub ffield_name: usize,
    pub fproperty_offset_internal: usize,
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
            fuobjectitem_size:         0x18,
            fuobjectitem_object:       0x00,
            uobject_class:             0x10,
            uobject_name:              0x18,
            uclass_children:           0x50,
            ffield_next:               0x20,
            ffield_name:               0x28,
            fproperty_offset_internal: 0x4C,
            gobjects_chunk_size:       0x10000,
            gobjects_objects:          0x00,
            gobjects_num_elements:     0x14,
            fname_stride:              2,
            fname_entry_header:        0x02,
            gnames_chunk_size:         0x20000,
        }
    }
}

fn read_ptr(pid: u32, addr: usize) -> Option<usize> {
    let data = read_memory(pid, addr, 8).ok()?;
    Some(usize::from_le_bytes(data.try_into().ok()?))
}

fn read_u32(pid: u32, addr: usize) -> Option<u32> {
    let data = read_memory(pid, addr, 4).ok()?;
    Some(u32::from_le_bytes(data.try_into().ok()?))
}

fn read_i32(pid: u32, addr: usize) -> Option<i32> {
    read_u32(pid, addr).map(|v| v as i32)
}

fn read_string(pid: u32, addr: usize, max_len: usize) -> Option<String> {
    let buf = read_memory(pid, addr, max_len).ok()?;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8(buf[..end].to_vec()).ok()
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
    read_string(pid, entry_addr + off.fname_entry_header, 128)
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
    let objects_ptr = read_ptr(pid, gobjects_base + off.gobjects_objects)
        .ok_or("Could not read GObjects.Objects")?;

    for i in 0..num_elements {
        let chunk_idx = i / off.gobjects_chunk_size;
        let item_idx = i % off.gobjects_chunk_size;
        let chunk_ptr = match read_ptr(pid, objects_ptr + chunk_idx * 8) {
            Some(p) if p != 0 => p,
            _ => continue,
        };
        let item_addr = chunk_ptr + item_idx * off.fuobjectitem_size;
        let obj_ptr = match read_ptr(pid, item_addr + off.fuobjectitem_object) {
            Some(p) if p != 0 => p,
            _ => continue,
        };

        let class_ptr = match read_ptr(pid, obj_ptr + off.uobject_class) {
            Some(p) if p != 0 => p,
            _ => continue,
        };
        let class_fname_idx = match read_i32(pid, class_ptr + off.uobject_name) {
            Some(i) if i >= 0 => i,
            _ => continue,
        };
        if let Some(name) = fname_to_string(pid, gnames_base, class_fname_idx, off) {
            if name == target_class {
                return Ok(obj_ptr);
            }
        }
    }

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

    if extra_offsets.is_empty() {
        Ok(initial)
    } else {
        crate::engine::resolve_pointer_path(pid, initial, extra_offsets)
    }
}

// Uses static module offsets instead of AOB scanning.
// gobjects_offset: offset of GObjects global from module base
// gnames_offset:   offset of FNamePool.Blocks from module base (= FNamePool + 0x10)
// extra_offsets:   optional pointer chain applied after obj_ptr + property_offset
pub fn resolve_ue5_prop_static(
    pid: u32,
    module_base: usize,
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
