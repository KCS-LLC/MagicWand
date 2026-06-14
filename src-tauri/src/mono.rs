use crate::engine::read_memory;

fn read_ptr(pid: u32, addr: usize) -> Option<usize> {
    let data = read_memory(pid, addr, 8).ok()?;
    Some(usize::from_le_bytes(data.try_into().ok()?))
}

fn read_u32(pid: u32, addr: usize) -> Option<u32> {
    let data = read_memory(pid, addr, 4).ok()?;
    Some(u32::from_le_bytes(data.try_into().ok()?))
}

fn read_u16(pid: u32, addr: usize) -> Option<u16> {
    let data = read_memory(pid, addr, 2).ok()?;
    Some(u16::from_le_bytes(data.try_into().ok()?))
}

fn read_string(pid: u32, addr: usize, max_len: usize) -> Option<String> {
    let buf = read_memory(pid, addr, max_len).ok()?;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8(buf[..end].to_vec()).ok()
}

/// Parse the PE export directory and return the VA of the named export.
fn find_mono_export(pid: u32, module_base: usize, name: &str) -> Option<usize> {
    let e_lfanew = read_u32(pid, module_base + 0x3C)? as usize;
    let nt_header = module_base + e_lfanew;

    // OptionalHeader at +0x18, DataDirectory[0].VirtualAddress at +0x70 into it
    let export_dir_rva = read_u32(pid, nt_header + 0x18 + 0x70)? as usize;
    if export_dir_rva == 0 {
        return None;
    }
    let export_dir = module_base + export_dir_rva;

    let num_names     = read_u32(pid, export_dir + 0x18)? as usize;
    let addr_table    = module_base + read_u32(pid, export_dir + 0x1C)? as usize;
    let name_table    = module_base + read_u32(pid, export_dir + 0x20)? as usize;
    let ordinal_table = module_base + read_u32(pid, export_dir + 0x24)? as usize;

    for i in 0..num_names {
        let name_rva = match read_u32(pid, name_table + i * 4) {
            Some(v) => v as usize,
            None => continue,
        };
        let export_name = match read_string(pid, module_base + name_rva, 128) {
            Some(s) => s,
            None => continue,
        };
        if export_name == name {
            let ordinal = match read_u16(pid, ordinal_table + i * 2) {
                Some(v) => v as usize,
                None => continue,
            };
            let func_rva = match read_u32(pid, addr_table + ordinal * 4) {
                Some(v) => v as usize,
                None => continue,
            };
            return Some(module_base + func_rva);
        }
    }
    None
}

fn get_root_domain(pid: u32, mono_base: usize) -> Option<usize> {
    // Try direct data export first
    if let Some(domain_ptr_addr) = find_mono_export(pid, mono_base, "mono_root_domain") {
        if let Some(domain) = read_ptr(pid, domain_ptr_addr) {
            if domain != 0 {
                return Some(domain);
            }
        }
    }
    // Fallback: parse mono_get_root_domain — it's `mov rax, [rip+disp32]` (48 8B 05 xx xx xx xx)
    let fn_addr = find_mono_export(pid, mono_base, "mono_get_root_domain")?;
    let bytes = read_memory(pid, fn_addr, 7).ok()?;
    if bytes[0] == 0x48 && bytes[1] == 0x8B && bytes[2] == 0x05 {
        let disp = i32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]);
        let domain_ptr_addr = (fn_addr as i64 + 7 + disp as i64) as usize;
        let domain = read_ptr(pid, domain_ptr_addr)?;
        if domain != 0 { return Some(domain); }
    }
    None
}

/// Offsets into Mono internal structures for the Unity 2019–2022 Mono fork.
struct MonoOffsets {
    domain_assemblies:   usize,
    assembly_aname:      usize,
    assembly_image:      usize,
    image_class_cache:   usize,
    hash_table:          usize,
    hash_size:           usize,
    class_name:          usize,
    class_namespace:     usize,
    class_fields:        usize,
    class_runtime_info:  usize,
    field_size:          usize,
    field_name:          usize,
    field_offset:        usize,
    runtime_info_vtable: usize,
    vtable_data:         usize,
    /// MonoClass: offset of MonoClass* parent pointer
    class_parent:        usize,
}

impl MonoOffsets {
    fn unity_default() -> Self {
        MonoOffsets {
            domain_assemblies:   0xA0,
            assembly_aname:      0x10,
            assembly_image:      0x60,
            image_class_cache:   0x4D0,
            hash_table:          0x20,
            hash_size:           0x18,
            class_name:          0x48,
            class_namespace:     0x50,
            class_fields:        0x98,
            class_runtime_info:  0xD0,
            field_size:          0x20,
            field_name:          0x08,
            field_offset:        0x18,
            runtime_info_vtable: 0x08,
            vtable_data:         0x70,
            class_parent:        0x30,
        }
    }
}

fn gslist_iter(pid: u32, head: usize) -> Vec<usize> {
    let mut items = Vec::new();
    let mut node = head;
    let mut seen = std::collections::HashSet::new();
    while node != 0 && seen.insert(node) && items.len() < 4096 {
        if let Some(data) = read_ptr(pid, node) {
            if data != 0 {
                items.push(data);
            }
        }
        node = read_ptr(pid, node + 8).unwrap_or(0);
    }
    items
}

fn find_assembly(pid: u32, domain: usize, assembly_name: &str, off: &MonoOffsets) -> Option<usize> {
    let head = read_ptr(pid, domain + off.domain_assemblies)?;
    for asm in gslist_iter(pid, head) {
        let name_ptr = read_ptr(pid, asm + off.assembly_aname)?;
        if let Some(name) = read_string(pid, name_ptr, 128) {
            if name.eq_ignore_ascii_case(assembly_name) {
                return Some(asm);
            }
        }
    }
    None
}

fn looks_like_hash_table(pid: u32, cache_base: usize) -> bool {
    // MonoInternalHashTable starts with 3 function pointers (hash_func, key_extract, next_value)
    // In 64-bit Windows, DLL function addresses are in 0x7FF000000000-0x7FFFFFFFFFFF range
    for slot in 0..3 {
        let p = match read_ptr(pid, cache_base.saturating_add(slot * 8)) {
            Some(v) => v,
            None => return false,
        };
        if p < 0x7FF000000000 || p > 0x800000000000 { return false; }
    }
    true
}

/// Read `next_value(MonoClass*)` function bytes (`mov rax, [rcx+disp32]; ret`)
/// and extract the chain-next offset within MonoClass.
fn decode_next_value_offset(pid: u32, fn_addr: usize) -> Option<usize> {
    let bytes = read_memory(pid, fn_addr, 8).ok()?;
    // Both `mov rax, [rcx+disp32]` (48 8B 81) and `lea rax, [rcx+disp32]` (48 8D 81)
    // give us the chain offset within MonoClass.
    if bytes[0] == 0x48 && (bytes[1] == 0x8B || bytes[1] == 0x8D) && bytes[2] == 0x81 {
        let disp = i32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]);
        if disp >= 0 && disp < 0x10000 { return Some(disp as usize); }
    }
    None
}

fn try_find_class_at_cache(
    pid: u32,
    image: usize,
    cache_off: usize,
    namespace: &str,
    class_name: &str,
    off: &MonoOffsets,
) -> Option<usize> {
    let cache_base = image.saturating_add(cache_off);
    if !looks_like_hash_table(pid, cache_base) { return None; }
    let table_ptr = read_ptr(pid, cache_base.saturating_add(off.hash_table))?;
    if table_ptr < 0x10000 { return None; }
    let raw_size = read_u32(pid, cache_base.saturating_add(off.hash_size))? as usize;
    if raw_size < 16 || raw_size > 65536 { return None; }

    // Decode chain offset from the next_value function pointer at cache_base+0x10
    let next_fn = read_ptr(pid, cache_base.saturating_add(0x10)).unwrap_or(0);
    let chain_off = decode_next_value_offset(pid, next_fn);

    for i in 0..raw_size {
        let mut klass = read_ptr(pid, table_ptr.saturating_add(i * 8)).unwrap_or(0);
        let mut steps = 0;
        while klass >= 0x10000 && steps < 32 {
            let name_ptr = read_ptr(pid, klass.saturating_add(off.class_name)).unwrap_or(0);
            let ns_ptr   = read_ptr(pid, klass.saturating_add(off.class_namespace)).unwrap_or(0);
            let name = read_string(pid, name_ptr, 128).unwrap_or_default();
            let ns   = read_string(pid, ns_ptr, 128).unwrap_or_default();
            if name == class_name && ns == namespace {
                return Some(klass);
            }
            match chain_off {
                Some(co) => {
                    klass = read_ptr(pid, klass.saturating_add(co)).unwrap_or(0);
                    steps += 1;
                }
                None => break,
            }
        }
    }
    None
}

fn find_class(
    pid: u32,
    assembly: usize,
    namespace: &str,
    class_name: &str,
    off: &MonoOffsets,
) -> Option<usize> {
    for &img_off in &[off.assembly_image, 0x60usize, 0x68, 0x70, 0x78] {
        let image = match read_ptr(pid, assembly.saturating_add(img_off)) {
            Some(p) if p > 0x10000 => p,
            _ => continue,
        };
        for &cache_off in &[off.image_class_cache, 0x4D0usize, 0x2D0, 0x378, 0x3D8, 0x400, 0x440, 0x480, 0x500, 0x550, 0x600] {
            if let Some(klass) = try_find_class_at_cache(pid, image, cache_off, namespace, class_name, off) {
                return Some(klass);
            }
        }
    }
    None
}

fn find_field_offset(pid: u32, klass: usize, field_name: &str, off: &MonoOffsets) -> Option<u32> {
    let fields_ptr = read_ptr(pid, klass.saturating_add(off.class_fields))?;
    if fields_ptr < 0x10000 { return None; }
    // Iterate without relying on class_field_count — stop on first invalid name
    for i in 0..256usize {
        let field = fields_ptr.saturating_add(i * off.field_size);
        let name_ptr = read_ptr(pid, field.saturating_add(off.field_name))?;
        if name_ptr < 0x10000 { return None; }
        let name = read_string(pid, name_ptr, 128)?;
        if name.is_empty() || !name.is_ascii() { return None; }
        if name == field_name {
            return read_u32(pid, field.saturating_add(off.field_offset));
        }
    }
    None
}

fn get_static_field_address(pid: u32, klass: usize, field_offset: u32, off: &MonoOffsets) -> Option<usize> {
    let runtime_info = read_ptr(pid, klass + off.class_runtime_info)?;
    let vtable       = read_ptr(pid, runtime_info + off.runtime_info_vtable)?;
    let static_data  = read_ptr(pid, vtable + off.vtable_data)?;
    Some(static_data + field_offset as usize)
}

fn get_parent_class(pid: u32, klass: usize, off: &MonoOffsets) -> Option<usize> {
    let parent = read_ptr(pid, klass + off.class_parent)?;
    if parent == 0 { None } else { Some(parent) }
}

fn get_instance_field_offset(pid: u32, klass: usize, field_name: &str, off: &MonoOffsets) -> Result<usize, String> {
    find_field_offset(pid, klass, field_name, off)
        .map(|o| o as usize)
        .ok_or_else(|| format!("Field '{}' not found", field_name))
}

/// Resolve a three-step instance chain:
///   (parent class static field → instance ptr) + (instance field offset) + (struct offset)
///
/// Returns the address of the final primitive value in the target process.
pub fn resolve_mono_chain(
    pid: u32,
    mono_module_base: usize,
    assembly_name: &str,
    namespace: &str,
    class_name: &str,
    static_field: &str,
    via_parent: bool,
    instance_field: &str,
    final_offset: usize,
) -> Result<usize, String> {
    let off = MonoOffsets::unity_default();

    let domain = get_root_domain(pid, mono_module_base)
        .ok_or("Could not locate Mono root domain")?;

    let assembly = find_assembly(pid, domain, assembly_name, &off)
        .ok_or_else(|| format!("Assembly '{}' not found", assembly_name))?;

    let klass = find_class(pid, assembly, namespace, class_name, &off)
        .ok_or_else(|| format!("Class '{}.{}' not found", namespace, class_name))?;

    // Step 1: get the class that owns the static instance pointer
    let static_class = if via_parent {
        get_parent_class(pid, klass, &off)
            .ok_or_else(|| format!("No parent class found for '{}'", class_name))?
    } else {
        klass
    };

    // Step 2: read the static field → instance pointer
    let static_off = find_field_offset(pid, static_class, static_field, &off)
        .ok_or_else(|| format!("Static field '{}' not found on parent class", static_field))?;
    let static_addr = get_static_field_address(pid, static_class, static_off, &off)
        .ok_or("Could not resolve static vtable for parent class")?;
    let instance_ptr = read_ptr(pid, static_addr)
        .ok_or("Static field pointer is null — game may not be fully loaded yet")?;
    if instance_ptr == 0 {
        return Err("Instance pointer is null — singleton not yet initialized".to_string());
    }

    // Step 3: get the instance field offset on the original class
    let field_offset = get_instance_field_offset(pid, klass, instance_field, &off)?;

    // instance_ptr + field_offset = start of value-type struct (_bank / Cry)
    // + final_offset = primitive field within that struct (Cry.c = Darwinium)
    Ok(instance_ptr.saturating_add(field_offset).saturating_add(final_offset))
}

/// Resolve a Mono static field to its runtime address in the target process.
pub fn resolve_mono_field(
    pid: u32,
    mono_module_base: usize,
    assembly_name: &str,
    namespace: &str,
    class_name: &str,
    field_name: &str,
    is_static: bool,
) -> Result<usize, String> {
    if !is_static {
        return Err("Instance mono fields are not yet supported — use monoStatic: true".to_string());
    }

    let off = MonoOffsets::unity_default();

    let domain = get_root_domain(pid, mono_module_base)
        .ok_or("Could not locate Mono root domain — is this a Mono/Unity game?")?;

    let assembly = find_assembly(pid, domain, assembly_name, &off)
        .ok_or_else(|| format!("Assembly '{}' not found", assembly_name))?;

    let klass = find_class(pid, assembly, namespace, class_name, &off)
        .ok_or_else(|| format!("Class '{}.{}' not found", namespace, class_name))?;

    let field_off = find_field_offset(pid, klass, field_name, &off)
        .ok_or_else(|| format!("Field '{}' not found on class '{}'", field_name, class_name))?;

    get_static_field_address(pid, klass, field_off, &off)
        .ok_or_else(|| format!("Could not read static vtable for '{}'", class_name))
}
