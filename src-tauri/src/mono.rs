use crate::engine::{read_memory, read_ptr, read_u32};

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
pub(crate) fn find_mono_export(pid: u32, module_base: usize, name: &str) -> Option<usize> {
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
    /// MonoClass: offset of static data index (read as u32)
    vtable_sd_index:     usize,
    /// MonoVTable: base offset for static data array
    vtable_sd_base:      usize,
    /// MonoClass: offset of MonoClass* parent pointer
    class_parent:        usize,
}

impl MonoOffsets {
    /// Unity 2019.4's Mono keeps `MonoDomain::domain_assemblies` at 0xC8 instead of 0xA0; if the
    /// default doesn't lead to mscorlib, scan the domain struct for the assembly list.
    fn detect(pid: u32, domain: usize) -> Self {
        let mut off = Self::unity_default();
        let has_mscorlib = |d_off: usize, a_off: usize| {
            read_ptr(pid, domain + d_off).map_or(false, |head| {
                gslist_iter(pid, head).into_iter().take(512).any(|asm| {
                    read_ptr(pid, asm.wrapping_add(a_off))
                        .and_then(|p| read_string(pid, p, 32))
                        .map_or(false, |s| s.eq_ignore_ascii_case("mscorlib"))
                })
            })
        };
        if !has_mscorlib(off.domain_assemblies, off.assembly_aname) {
            if let Some(d) = (0x80..0x200).step_by(8).find(|&d| has_mscorlib(d, off.assembly_aname)) {
                off.domain_assemblies = d;
            }
        }
        off
    }

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
            vtable_sd_index:     0x5C,
            vtable_sd_base:      0x48,
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
        node = read_ptr(pid, node.wrapping_add(8)).unwrap_or(0);
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

/// Diagnostic: names of every assembly loaded in the root domain.
pub fn list_assembly_names(pid: u32, mono_module_base: usize) -> Result<Vec<String>, String> {
    let domain = get_root_domain(pid, mono_module_base).ok_or("Could not locate Mono root domain")?;
    let off = MonoOffsets::detect(pid, domain);
    let head = read_ptr(pid, domain + off.domain_assemblies).ok_or("Could not read domain assemblies")?;
    Ok(gslist_iter(pid, head)
        .into_iter()
        .filter_map(|asm| read_ptr(pid, asm + off.assembly_aname).and_then(|p| read_string(pid, p, 128)))
        .collect())
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

    // If the chain decoder can't read the bucket's next_value helper, fall back to head-only scan.
    let next_fn = read_ptr(pid, cache_base.saturating_add(0x10)).unwrap_or(0);
    let (chain_off, max_steps) = match decode_next_value_offset(pid, next_fn) {
        Some(co) => (co, 32),
        None => (0, 1),
    };

    for i in 0..raw_size {
        let mut klass = read_ptr(pid, table_ptr.saturating_add(i * 8)).unwrap_or(0);
        let mut steps = 0;
        while klass >= 0x10000 && steps < max_steps {
            let name_ptr = read_ptr(pid, klass.saturating_add(off.class_name)).unwrap_or(0);
            let ns_ptr   = read_ptr(pid, klass.saturating_add(off.class_namespace)).unwrap_or(0);
            let name = read_string(pid, name_ptr, 128).unwrap_or_default();
            let ns   = read_string(pid, ns_ptr, 128).unwrap_or_default();
            if name == class_name && ns == namespace {
                return Some(klass);
            }
            klass = read_ptr(pid, klass.saturating_add(chain_off)).unwrap_or(0);
            steps += 1;
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
    // Tried in order: the configured default first, then known alternates for other Unity Mono builds.
    const IMAGE_OFFSET_CANDIDATES: &[usize] = &[0x60, 0x68, 0x70, 0x78];
    const CACHE_OFFSET_CANDIDATES: &[usize] = &[0x4D0, 0x2D0, 0x378, 0x3D8, 0x400, 0x440, 0x480, 0x500, 0x550, 0x600];

    let img_candidates = std::iter::once(off.assembly_image)
        .chain(IMAGE_OFFSET_CANDIDATES.iter().copied().filter(|o| *o != off.assembly_image));
    for img_off in img_candidates {
        let image = match read_ptr(pid, assembly.saturating_add(img_off)) {
            Some(p) if p > 0x10000 => p,
            _ => continue,
        };
        let cache_candidates = std::iter::once(off.image_class_cache)
            .chain(CACHE_OFFSET_CANDIDATES.iter().copied().filter(|o| *o != off.image_class_cache));
        for cache_off in cache_candidates {
            if let Some(klass) = try_find_class_at_cache(pid, image, cache_off, namespace, class_name, off) {
                return Some(klass);
            }
        }
        // Unknown Mono build: scan the whole image for hash-table-shaped regions.
        for cache_off in (0x100..0x900usize).step_by(8) {
            if let Some(klass) = try_find_class_at_cache(pid, image, cache_off, namespace, class_name, off) {
                return Some(klass);
            }
        }
    }
    None
}

/// Find a class in `assembly`, falling back to every other loaded assembly (game code often splits
/// types across several DLLs, e.g. a shared `ToolsAssembly`).
fn find_class_any(pid: u32, domain: usize, assembly: usize, namespace: &str, class_name: &str, off: &MonoOffsets) -> Option<usize> {
    if let Some(k) = find_class(pid, assembly, namespace, class_name, off) { return Some(k); }
    let head = read_ptr(pid, domain + off.domain_assemblies)?;
    gslist_iter(pid, head).into_iter().filter(|a| *a != assembly)
        .find_map(|a| find_class(pid, a, namespace, class_name, off))
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

/// True if `p` points at a Mono object whose vtable->klass has a readable ASCII name.
fn looks_like_object(pid: u32, p: usize, off: &MonoOffsets) -> bool {
    if p < 0x10000 || p > 0x7FFFFFFFFFFF { return false; }
    let Some(vt) = read_ptr(pid, p) else { return false };
    let Some(k) = read_ptr(pid, vt) else { return false };
    if vt == p || k == p || k == vt || k < 0x10000 { return false; }
    // MonoVTable::klass must point back at a class whose own vtable belongs to a class chain.
    let Some(np) = read_ptr(pid, k.wrapping_add(off.class_name)) else { return false };
    if np < 0x10000 || np == p { return false; }
    read_string(pid, np, 64).map_or(false, |n| n.len() >= 2 && n.bytes().all(|c| c.is_ascii_graphic()))
}

fn get_static_field_address(pid: u32, klass: usize, field_offset: u32, off: &MonoOffsets) -> Option<usize> {
    let runtime_info = read_ptr(pid, klass.saturating_add(off.class_runtime_info))?;
    let vtable       = read_ptr(pid, runtime_info.saturating_add(off.runtime_info_vtable))?;
    let vt_klass     = read_ptr(pid, vtable)?;
    let sd_index     = read_u32(pid, vt_klass.saturating_add(off.vtable_sd_index))? as usize;
    let static_data  = read_ptr(pid, vtable.saturating_add(sd_index * 8 + off.vtable_sd_base))?;
    let default_addr = static_data.saturating_add(field_offset as usize);
    // The default offsets match Unity 2020+ builds; Unity 2019.4 shifts MonoClass/MonoVTable members.
    // Accept the default if it yields an object (or null); otherwise search for offsets that do.
    match read_ptr(pid, default_addr) {
        Some(0) => return Some(default_addr),
        Some(v) if looks_like_object(pid, v, off) => return Some(default_addr),
        _ => {}
    }
    for ri in (0xB0..0x110usize).step_by(8) {
        let Some(rinfo) = read_ptr(pid, klass.saturating_add(ri)).filter(|p| *p > 0x10000 && *p <= 0x7FFFFFFFFFFF) else { continue };
        let Some(vt) = read_ptr(pid, rinfo.saturating_add(off.runtime_info_vtable)).filter(|p| *p > 0x10000 && *p <= 0x7FFFFFFFFFFF) else { continue };
        if read_ptr(pid, vt) != Some(klass) { continue; }
        for sd_idx_off in (0x40..0x90usize).step_by(4) {
            let Some(idx) = read_u32(pid, klass.saturating_add(sd_idx_off)).map(|v| v as usize).filter(|v| *v < 4096) else { continue };
            for base in (0x30..0x70usize).step_by(8) {
                let Some(sd) = read_ptr(pid, vt.saturating_add(idx * 8 + base)).filter(|p| *p > 0x10000 && *p <= 0x7FFFFFFFFFFF) else { continue };
                let addr = sd.saturating_add(field_offset as usize);
                if read_ptr(pid, addr).map_or(false, |v| looks_like_object(pid, v, off)) {
                    return Some(addr);
                }
            }
        }
    }
    Some(default_addr)
}

fn probe_static_field_offsets(pid: u32, klass: usize, field_offset: u32, _off: &MonoOffsets) -> String {
    let mut report = String::new();
    let ri_candidates = [0xC8, 0xD0, 0xD8, 0xE0, 0xE8, 0xF0];
    for &ri in &ri_candidates {
        if let Some(runtime_info) = read_ptr(pid, klass.saturating_add(ri)) {
            if runtime_info == 0 || runtime_info > 0x7FFFFFFFFFFF { continue; }
            if let Some(vtable) = read_ptr(pid, runtime_info.saturating_add(0x08)) {
                if vtable == 0 || vtable > 0x7FFFFFFFFFFF { continue; }
                if let Some(vt_klass) = read_ptr(pid, vtable) {
                    if let Some(sd_idx) = read_u32(pid, vt_klass.saturating_add(0x5C)) {
                        let sd_off = (sd_idx as usize) * 8 + 0x48;
                        if let Some(static_data) = read_ptr(pid, vtable.saturating_add(sd_off)) {
                            if static_data > 0 && static_data < 0x7FFFFFFFFFFF {
                                let addr = static_data.saturating_add(field_offset as usize);
                                if let Some(val) = read_ptr(pid, addr) {
                                    if val > 0 && val < 0x7FFFFFFFFFFF {
                                        report.push_str(&format!(
                                            "HIT ri=0x{:X} sd_idx={} → ptr=0x{:X}\n",
                                            ri, sd_idx, val
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if report.is_empty() { "No valid offset combination found".to_string() } else { report }
}

fn get_parent_class(pid: u32, klass: usize, off: &MonoOffsets) -> Option<usize> {
    let parent = read_ptr(pid, klass.saturating_add(off.class_parent))?;
    if parent == 0 { None } else { Some(parent) }
}

fn get_instance_field_offset(pid: u32, klass: usize, field_name: &str, off: &MonoOffsets) -> Result<usize, String> {
    let mut current = Some(klass);
    while let Some(k) = current {
        if let Some(o) = find_field_offset(pid, k, field_name, off) {
            return Ok(o as usize);
        }
        current = get_parent_class(pid, k, off);
    }
    Err(format!("Field '{}' not found", field_name))
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
    instance_field_is_ref: bool,
) -> Result<usize, String> {

    let domain = get_root_domain(pid, mono_module_base)
        .ok_or("Could not locate Mono root domain")?;
    let off = MonoOffsets::detect(pid, domain);

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
    if instance_ptr == 0 || instance_ptr > 0x7FFFFFFFFFFF {
        let probe = probe_static_field_offsets(pid, static_class, static_off, &off);
        return Err(format!("Instance pointer invalid (0x{:X}). Probe results:\n{}", instance_ptr, probe));
    }

    // Step 3: get the instance field offset on the original class
    let field_offset = get_instance_field_offset(pid, klass, instance_field, &off)?;

    let field_addr = instance_ptr.saturating_add(field_offset);
    let base = if instance_field_is_ref {
        read_ptr(pid, field_addr)
            .ok_or_else(|| format!(
                "Instance field read failed at 0x{:X} (instance=0x{:X} + offset=0x{:X})",
                field_addr, instance_ptr, field_offset
            ))?
    } else {
        field_addr
    };
    if base == 0 {
        return Err(format!(
            "Instance field reference is null at 0x{:X} (instance=0x{:X} + offset=0x{:X})",
            field_addr, instance_ptr, field_offset
        ));
    }
    Ok(base.saturating_add(final_offset))
}

/// Result of `locate_method_jit`: where a managed method's native (JIT) body lives.
#[derive(Debug, Clone)]
pub struct MethodJit {
    pub method: usize,
    pub jit_info: usize,
    pub code_start: usize,
    pub code_size: usize,
    /// MonoClass offset at which the `methods` array was found (for diagnostics).
    pub methods_off: usize,
}

/// Find the MonoMethod* for `method_name` on `klass` by scanning the class's `methods`
/// array. The array's offset in MonoClass varies by Mono build, so candidates are tried and
/// each hit is validated (method->klass == klass, method->name == method_name).
fn find_method_ptr(pid: u32, klass: usize, method_name: &str) -> Option<(usize, usize)> {
    const METHODS_OFF_CANDIDATES: &[usize] = &[0xA0, 0xA8, 0x98, 0xB0, 0xB8, 0xC0, 0x90, 0x88, 0xC8, 0xD8];
    for &moff in METHODS_OFF_CANDIDATES {
        let arr = match read_ptr(pid, klass.saturating_add(moff)) {
            Some(p) if p > 0x10000 => p,
            _ => continue,
        };
        for i in 0..2048usize {
            let m = match read_ptr(pid, arr.saturating_add(i * 8)) {
                Some(p) if p > 0x10000 => p,
                _ => continue,
            };
            // MonoMethod: flags u16, iflags u16, token u32, klass* @0x8, signature* @0x10, name* @0x18
            if read_ptr(pid, m + 0x8) != Some(klass) { continue; }
            let name_ptr = match read_ptr(pid, m + 0x18) {
                Some(p) if p > 0x10000 => p,
                _ => continue,
            };
            if read_string(pid, name_ptr, 128).as_deref() == Some(method_name) {
                return Some((m, moff));
            }
        }
    }
    None
}

/// Locate the JIT-compiled body of `class_name.method_name`.
///
/// MonoJitInfo layout (x64): d.method @0x0, next_jit_code_hash @0x8, code_start @0x10,
/// unwind_info u32 @0x18, code_size i32 @0x1C. We find it by scanning process memory for
/// pointers equal to the MonoMethod* and keeping the ones shaped like a JitInfo (code_start in
/// an executable region, plausible code_size). The method must already have been compiled —
/// i.e. executed at least once in this session.
pub fn locate_method_jit(
    pid: u32,
    mono_module_base: usize,
    assembly_name: &str,
    namespace: &str,
    class_name: &str,
    method_name: &str,
) -> Result<Vec<MethodJit>, String> {
    let domain = get_root_domain(pid, mono_module_base)
        .ok_or("Could not locate Mono root domain")?;
    let off = MonoOffsets::detect(pid, domain);
    let assembly = find_assembly(pid, domain, assembly_name, &off)
        .ok_or_else(|| format!("Assembly '{}' not found", assembly_name))?;
    let klass = find_class(pid, assembly, namespace, class_name, &off)
        .ok_or_else(|| format!("Class '{}.{}' not found", namespace, class_name))?;
    let (method, methods_off) = find_method_ptr(pid, klass, method_name)
        .ok_or_else(|| format!("Method '{}' not found on '{}'", method_name, class_name))?;

    let exec_regions: Vec<(usize, usize)> = crate::engine::list_executable_regions(pid)?
        .into_iter()
        .map(|(b, s, _)| (b, s))
        .collect();
    let in_exec = |p: usize| exec_regions.iter().any(|(b, s)| p >= *b && p < b + s);

    let mut out = Vec::new();
    for hit in crate::engine::scan_aligned_u64(pid, method as u64) {
        let code_start = match read_ptr(pid, hit + 0x10) { Some(p) if p > 0x10000 => p, _ => continue };
        if !in_exec(code_start) { continue; }
        let code_size = match read_u32(pid, hit + 0x1C) { Some(s) => s as usize, None => continue };
        if code_size < 8 || code_size > 0x20000 { continue; }
        out.push(MethodJit { method, jit_info: hit, code_start, code_size, methods_off });
    }
    if out.is_empty() {
        return Err(format!(
            "'{}' hasn't been compiled yet — Mono compiles a method the first time the game runs it. \
             Trigger it once in-game (for a damage method: take one hit), then toggle again. (MonoMethod 0x{:X})",
            method_name, method
        ));
    }
    Ok(out)
}

/// Byte offset of an instance field within its object (inherited fields included).
pub fn instance_field_offset(
    pid: u32,
    mono_module_base: usize,
    assembly_name: &str,
    namespace: &str,
    class_name: &str,
    field: &str,
) -> Result<usize, String> {
    let domain = get_root_domain(pid, mono_module_base).ok_or("Could not locate Mono root domain")?;
    let off = MonoOffsets::detect(pid, domain);
    let assembly = find_assembly(pid, domain, assembly_name, &off)
        .ok_or_else(|| format!("Assembly '{}' not found", assembly_name))?;
    let klass = find_class(pid, assembly, namespace, class_name, &off)
        .ok_or_else(|| format!("Class '{}.{}' not found", namespace, class_name))?;
    get_instance_field_offset(pid, klass, field, &off)
}

/// Find x64 `movss [reg+disp], xmmN` stores (F3 [REX] 0F 11 /r) whose displacement equals
/// `field_off` in a JIT'd body. Returns (index into `code`, instruction length). This is how
/// an inlined float property setter (e.g. `set_Hp`) appears in native code.
pub fn find_movss_stores(code: &[u8], field_off: i32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 8 < code.len() {
        if code[i] == 0xF3 {
            let mut j = i + 1;
            if code[j] & 0xF0 == 0x40 { j += 1; }
            if j + 3 < code.len() && code[j] == 0x0F && code[j + 1] == 0x11 {
                let modrm = code[j + 2];
                let (mode, rm) = (modrm >> 6, modrm & 7);
                let mut k = j + 3;
                if mode != 3 && rm == 4 { k += 1; } // SIB byte
                let disp = match mode {
                    2 if k + 4 <= code.len() => Some((i32::from_le_bytes([code[k], code[k+1], code[k+2], code[k+3]]), 4)),
                    1 if k + 1 <= code.len() => Some((code[k] as i8 as i32, 1)),
                    _ => None,
                };
                if let Some((d, dlen)) = disp {
                    if d == field_off {
                        out.push((i, k + dlen - i));
                        i = k + dlen;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

/// Absolute addresses + lengths of every store to `field` inside the JIT body of
/// `class_name.method_name`. `field_class_*` names the class that declares the field.
pub fn find_field_store_sites(
    pid: u32,
    mono_module_base: usize,
    assembly_name: &str,
    namespace: &str,
    class_name: &str,
    method_name: &str,
    field_name: &str,
) -> Result<Vec<(usize, usize)>, String> {
    let field_off = instance_field_offset(pid, mono_module_base, assembly_name, namespace, class_name, field_name)?;
    let hits = locate_method_jit(pid, mono_module_base, assembly_name, namespace, class_name, method_name)?;
    if hits.len() != 1 {
        return Err(format!("'{}.{}' has {} compiled bodies — refusing to guess", class_name, method_name, hits.len()));
    }
    let h = &hits[0];
    let code = read_memory(pid, h.code_start, h.code_size)?;
    let sites: Vec<(usize, usize)> = find_movss_stores(&code, field_off as i32)
        .into_iter()
        .map(|(idx, len)| (h.code_start + idx, len))
        .collect();
    if sites.is_empty() {
        return Err(format!("No stores to '{}' (offset 0x{:X}) found in '{}.{}'", field_name, field_off, class_name, method_name));
    }
    Ok(sites)
}

/// True if `err` is the "method hasn't been JIT-compiled yet" error from `locate_method_jit`.
pub fn is_not_compiled(err: &str) -> bool {
    err.contains("hasn't been compiled yet")
}

/// A byte-level edit inside a method's IL: at `offset` from the signature start, the bytes
/// must equal `expect`, and are replaced by `replace` (same length).
pub struct IlPatch {
    pub offset: usize,
    pub expect: Vec<u8>,
    pub replace: Vec<u8>,
}

/// Find the in-memory copy of a method's IL by its leading `signature` and return the
/// (address, original bytes, replacement) edits to apply. Mono compiles from this copy the
/// first time the method runs, so editing it before then changes what gets compiled. Every
/// copy whose `expect` bytes all verify is returned; none verifying is an error (the game
/// was likely updated), so we never edit unverified memory.
pub fn il_patch_sites(pid: u32, signature: &[u8], patches: &[IlPatch]) -> Result<Vec<(usize, Vec<u8>, Vec<u8>)>, String> {
    let mut out = Vec::new();
    for base in crate::engine::scan_bytes_streaming(pid, signature) {
        let mut copy = Vec::new();
        let mut ok = true;
        for p in patches {
            let at = base + p.offset;
            match read_memory(pid, at, p.expect.len()) {
                Ok(b) if b == p.expect => copy.push((at, b, p.replace.clone())),
                _ => { ok = false; break; }
            }
        }
        if ok { out.extend(copy); }
    }
    if out.is_empty() {
        return Err("Could not find this method's IL in memory (game updated? signature in the trainer JSON is stale)".to_string());
    }
    Ok(out)
}

/// One step of a `resolve_mono_path` walk: the class that declares `field`.
pub struct MonoHop {
    pub namespace: String,
    pub class_name: String,
    pub field: String,
    /// When set, the object reached so far is a `Dictionary<int-like key, reference>`; it is replaced
    /// by the value stored under this key before `field` is read from it.
    pub dict_key: Option<i32>,
}

/// Value stored under `key` in a Mono `Dictionary<K, V>` (4-byte key, reference value).
/// Entry layout is {hashCode:i32, next:i32, key, value} with 8-byte alignment: stride 0x18.
fn dict_lookup_ref(pid: u32, dict: usize, key: i32) -> Option<usize> {
    let entries = read_ptr(pid, dict.checked_add(0x18)?).filter(|p| *p > 0x10000)?;
    let len = read_u32(pid, entries.checked_add(0x18)?)? as usize;
    if len > 4096 { return None; }
    (0..len).find_map(|i| {
        let e = entries + 0x20 + i * 0x18;
        let hash = read_u32(pid, e)? as i32;
        let k = read_u32(pid, e + 8)? as i32;
        if hash >= 0 && k == key { read_ptr(pid, e + 0x10).filter(|p| *p > 0x10000) } else { None }
    })
}

/// Resolve a path of reference fields starting from a static singleton.
///
/// `class_name.static_field` supplies the first object. Every hop except the last reads a
/// reference field off the current object; the last hop's field address is returned
/// (the primitive value we want to read/write).
pub fn resolve_mono_path(
    pid: u32,
    mono_module_base: usize,
    assembly_name: &str,
    namespace: &str,
    class_name: &str,
    static_field: &str,
    hops: &[MonoHop],
) -> Result<usize, String> {
    if hops.is_empty() {
        return Err("resolve_mono_path needs at least one hop".to_string());
    }

    let domain = get_root_domain(pid, mono_module_base)
        .ok_or("Could not locate Mono root domain")?;
    let off = MonoOffsets::detect(pid, domain);
    let assembly = find_assembly(pid, domain, assembly_name, &off)
        .ok_or_else(|| format!("Assembly '{}' not found", assembly_name))?;
    let klass = find_class(pid, assembly, namespace, class_name, &off)
        .ok_or_else(|| format!("Class '{}.{}' not found", namespace, class_name))?;

    let static_off = find_field_offset(pid, klass, static_field, &off)
        .ok_or_else(|| format!("Static field '{}' not found on '{}'", static_field, class_name))?;
    let static_addr = get_static_field_address(pid, klass, static_off, &off)
        .ok_or("Could not resolve static vtable")?;
    let mut obj = read_ptr(pid, static_addr)
        .ok_or("Static field pointer is null — game may not be fully loaded yet")?;
    if obj == 0 || obj > 0x7FFFFFFFFFFF {
        return Err(format!("Singleton pointer invalid (0x{:X})", obj));
    }

    for (i, hop) in hops.iter().enumerate() {
        let hop_class = find_class_any(pid, domain, assembly, &hop.namespace, &hop.class_name, &off)
            .ok_or_else(|| format!("Class '{}.{}' not found", hop.namespace, hop.class_name))?;
        if let Some(key) = hop.dict_key {
            obj = dict_lookup_ref(pid, obj, key)
                .ok_or_else(|| format!("Hop {}: dictionary key {} not found", i, key))?;
        }
        let field_off = get_instance_field_offset(pid, hop_class, &hop.field, &off)?;
        let field_addr = obj.saturating_add(field_off);
        if i + 1 == hops.len() {
            return Ok(field_addr);
        }
        obj = read_ptr(pid, field_addr)
            .filter(|p| *p != 0 && *p <= 0x7FFFFFFFFFFF)
            .ok_or_else(|| format!(
                "Hop {} ('{}.{}') is null — not in a run yet?",
                i, hop.class_name, hop.field
            ))?;
    }
    unreachable!()
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


    let domain = get_root_domain(pid, mono_module_base)
        .ok_or("Could not locate Mono root domain — is this a Mono/Unity game?")?;
    let off = MonoOffsets::detect(pid, domain);

    let assembly = find_assembly(pid, domain, assembly_name, &off)
        .ok_or_else(|| format!("Assembly '{}' not found", assembly_name))?;

    let klass = find_class(pid, assembly, namespace, class_name, &off)
        .ok_or_else(|| format!("Class '{}.{}' not found", namespace, class_name))?;

    let field_off = find_field_offset(pid, klass, field_name, &off)
        .ok_or_else(|| format!("Field '{}' not found on class '{}'", field_name, class_name))?;

    get_static_field_address(pid, klass, field_off, &off)
        .ok_or_else(|| format!("Could not read static vtable for '{}'", class_name))
}
