use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};
use crate::engine::read_memory;

fn read_ptr(pid: u32, addr: usize) -> Option<usize> {
    let data = read_memory(pid, addr, 8).ok()?;
    Some(usize::from_le_bytes(data.try_into().ok()?))
}

fn read_u32(pid: u32, addr: usize) -> Option<u32> {
    let data = read_memory(pid, addr, 4).ok()?;
    Some(u32::from_le_bytes(data.try_into().ok()?))
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
        let name_rva = read_u32(pid, name_table + i * 4)? as usize;
        let export_name = read_string(pid, module_base + name_rva, 128)?;
        if export_name == name {
            let ordinal = (read_u32(pid, ordinal_table + i * 2)? & 0xFFFF) as usize;
            let func_rva = read_u32(pid, addr_table + ordinal * 4)? as usize;
            return Some(module_base + func_rva);
        }
    }
    None
}

fn get_root_domain(pid: u32, mono_base: usize) -> Option<usize> {
    let domain_ptr_addr = find_mono_export(pid, mono_base, "mono_root_domain")?;
    let domain = read_ptr(pid, domain_ptr_addr)?;
    if domain == 0 { None } else { Some(domain) }
}

/// Offsets into Mono internal structures for the Unity 2019–2022 Mono fork.
struct MonoOffsets {
    domain_assemblies:  usize,
    assembly_aname:     usize,
    assembly_image:     usize,
    image_class_cache:  usize,
    hash_table:         usize,
    hash_size:          usize,
    class_name:         usize,
    class_namespace:    usize,
    class_fields:       usize,
    class_field_count:  usize,
    class_runtime_info: usize,
    field_size:         usize,
    field_name:         usize,
    field_offset:       usize,
    runtime_info_vtable: usize,
    vtable_data:        usize,
}

impl MonoOffsets {
    fn unity_default() -> Self {
        MonoOffsets {
            domain_assemblies:   0xD0,
            assembly_aname:      0x10,
            assembly_image:      0x60,
            image_class_cache:   0x378,
            hash_table:          0x20,
            hash_size:           0x18,
            class_name:          0x40,
            class_namespace:     0x48,
            class_fields:        0x98,
            class_field_count:   0xA0,
            class_runtime_info:  0xD0,
            field_size:          0x20,
            field_name:          0x08,
            field_offset:        0x18,
            runtime_info_vtable: 0x08,
            vtable_data:         0x40,
        }
    }
}

fn gslist_iter(pid: u32, head: usize) -> Vec<usize> {
    let mut items = Vec::new();
    let mut node = head;
    while node != 0 {
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

fn find_class(
    pid: u32,
    assembly: usize,
    namespace: &str,
    class_name: &str,
    off: &MonoOffsets,
) -> Option<usize> {
    let image      = read_ptr(pid, assembly + off.assembly_image)?;
    let cache_base = image + off.image_class_cache;
    let table_ptr  = read_ptr(pid, cache_base + off.hash_table)?;
    let table_size = read_u32(pid, cache_base + off.hash_size)? as usize;

    for i in 0..table_size {
        let klass = read_ptr(pid, table_ptr + i * 8).unwrap_or(0);
        if klass == 0 {
            continue;
        }
        let name_ptr = read_ptr(pid, klass + off.class_name).unwrap_or(0);
        let ns_ptr   = read_ptr(pid, klass + off.class_namespace).unwrap_or(0);
        let name = read_string(pid, name_ptr, 128).unwrap_or_default();
        let ns   = read_string(pid, ns_ptr, 128).unwrap_or_default();
        if name == class_name && ns == namespace {
            return Some(klass);
        }
    }
    None
}

fn find_field_offset(pid: u32, klass: usize, field_name: &str, off: &MonoOffsets) -> Option<u32> {
    let fields_ptr  = read_ptr(pid, klass + off.class_fields)?;
    let field_count = read_u32(pid, klass + off.class_field_count)? as usize;

    for i in 0..field_count {
        let field    = fields_ptr + i * off.field_size;
        let name_ptr = read_ptr(pid, field + off.field_name)?;
        let name     = read_string(pid, name_ptr, 128)?;
        if name == field_name {
            return read_u32(pid, field + off.field_offset);
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

/// Verify the Mono module is present in the target process and return its base address.
pub fn find_mono_module(pid: u32, module_name: &str) -> Option<usize> {
    unsafe {
        let handle = OpenProcess(PROCESS_ALL_ACCESS, false, pid).ok()?;
        let base = crate::engine::get_module_info(pid, module_name).map(|(b, _)| b);
        let _ = CloseHandle(handle);
        base
    }
}
