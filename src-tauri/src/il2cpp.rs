//! Read-only IL2CPP object walker for Unity 2020.3 (metadata v27) x64 builds.
//!
//! Finds classes by name via the runtime's type-definition table, resolves field offsets from
//! the class's FieldInfo array, and follows pointer paths from a static singleton. Struct
//! offsets below were taken from GameAssembly.dll's own `il2cpp_*` accessor functions
//! (e.g. `il2cpp_class_get_name` is `mov rax,[rcx+0x10]`), not guessed.

use crate::engine::{read_memory, read_ptr, read_u32};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

// Class pointers, type-table address and field offsets are stable for the life of a game
// process, and re-scanning ~14k classes per lookup made every poll tick cost ~0.7s. Keyed by
// pid so a relaunched game never reuses stale entries.
static CLASS_CACHE: LazyLock<Mutex<HashMap<(u32, String, String), usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static FIELD_CACHE: LazyLock<Mutex<HashMap<(u32, usize, String), usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static TABLE_CACHE: LazyLock<Mutex<HashMap<u32, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// Il2CppClass
const CLASS_NAME: usize = 0x10;
const CLASS_NAMESPACE: usize = 0x18;
const CLASS_PARENT: usize = 0x58;
const CLASS_FIELDS: usize = 0x80;
const CLASS_STATIC_FIELDS: usize = 0xB8;
const CLASS_FIELD_COUNT: usize = 0x120; // u16
const CLASS_METHODS: usize = 0x98;
const CLASS_METHOD_COUNT: usize = 0x11C; // u16
// MethodInfo
const METHOD_POINTER: usize = 0x0;
const METHOD_NAME: usize = 0x10;
// FieldInfo
const FIELD_STRIDE: usize = 0x20;
const FIELD_NAME: usize = 0x0;
const FIELD_OFFSET: usize = 0x18; // i32
// System.Collections.Generic.List<T> (reference-type element)
const LIST_ITEMS: usize = 0x10;
const LIST_SIZE: usize = 0x18;
const ARRAY_FIRST_ELEM: usize = 0x20;

/// RVA of `s_TypeInfoDefinitionTable` in the Outward build this was written against; only
/// used if decoding it from the game's code fails.
const FALLBACK_TABLE_RVA: usize = 0x285A788;
/// Upper bound on type-definition count (Outward has 13,910); entries are validated.
const MAX_TYPES: usize = 65536;

fn read_u16(pid: u32, addr: usize) -> Option<u16> {
    let d = read_memory(pid, addr, 2).ok()?;
    Some(u16::from_le_bytes([d[0], d[1]]))
}

fn read_cstr(pid: u32, addr: usize) -> Option<String> {
    if addr < 0x10000 { return None; }
    let buf = read_memory(pid, addr, 128).ok()?;
    let end = buf.iter().position(|&b| b == 0)?;
    String::from_utf8(buf[..end].to_vec()).ok()
}

fn plausible_ptr(p: usize) -> bool {
    p > 0x10000 && p < 0x7FFF_FFFF_FFFF
}

/// Follow `E9 rel32` tail-jumps at `addr` (exports are often one-instruction stubs).
fn follow_jmp(pid: u32, mut addr: usize) -> Option<usize> {
    for _ in 0..4 {
        let b = read_memory(pid, addr, 5).ok()?;
        if b[0] != 0xE9 { return Some(addr); }
        let rel = i32::from_le_bytes([b[1], b[2], b[3], b[4]]);
        addr = (addr as i64 + 5 + rel as i64) as usize;
    }
    Some(addr)
}

/// Decode the address of the global type-definition table from the machine code of
/// `il2cpp_image_get_class`, which tail-jumps to `GetTypeInfoFromTypeDefinitionIndex`, whose
/// first memory access is `lea rbx,[rdi*8]; mov rax,[rip+disp32]` (the table pointer).
pub fn decode_table_global(pid: u32, base: usize) -> Option<usize> {
    let export = crate::mono::find_mono_export(pid, base, "il2cpp_image_get_class")?;
    let func = follow_jmp(pid, export)?;
    let body = read_memory(pid, func, 0x80).ok()?;
    // The accessor ends in `jmp GetTypeInfoFromTypeDefinitionIndex` (E9 rel32).
    for i in 0..body.len().saturating_sub(5) {
        if body[i] != 0xE9 { continue; }
        let rel = i32::from_le_bytes([body[i + 1], body[i + 2], body[i + 3], body[i + 4]]);
        let target = (func as i64 + i as i64 + 5 + rel as i64) as usize;
        let code = match read_memory(pid, target, 0x60) { Ok(c) => c, Err(_) => continue };
        // lea rbx,[rdi*8] ; mov rax,[rip+disp32]
        let pat = [0x48u8, 0x8D, 0x1C, 0xFD, 0x00, 0x00, 0x00, 0x00, 0x48, 0x8B, 0x05];
        if let Some(p) = code.windows(pat.len()).position(|w| w == pat) {
            let at = p + pat.len();
            let disp = i32::from_le_bytes([code[at], code[at + 1], code[at + 2], code[at + 3]]);
            return Some((target as i64 + at as i64 + 4 + disp as i64) as usize);
        }
    }
    None
}

/// Pointer to the `Il2CppClass*[]` type table, validated by reading a real class name.
fn type_table(pid: u32, base: usize) -> Result<usize, String> {
    if let Some(t) = TABLE_CACHE.lock().ok().and_then(|c| c.get(&pid).copied()) { return Ok(t); }
    let t = type_table_uncached(pid, base)?;
    if let Ok(mut c) = TABLE_CACHE.lock() { c.insert(pid, t); }
    Ok(t)
}

fn type_table_uncached(pid: u32, base: usize) -> Result<usize, String> {
    let candidates = [decode_table_global(pid, base), Some(base + FALLBACK_TABLE_RVA)];
    for global in candidates.into_iter().flatten() {
        if let Some(table) = read_ptr(pid, global) {
            if !plausible_ptr(table) { continue; }
            // Some entry in the first few hundred must be a class with a readable name.
            let ok = (0..512usize).any(|i| {
                read_ptr(pid, table + i * 8)
                    .filter(|k| plausible_ptr(*k))
                    .and_then(|k| read_ptr(pid, k + CLASS_NAME))
                    .and_then(|n| read_cstr(pid, n))
                    .is_some()
            });
            if ok { return Ok(table); }
        }
    }
    Err("Could not locate the IL2CPP type table (unsupported build?)".to_string())
}

pub fn find_class(pid: u32, base: usize, namespace: &str, name: &str) -> Result<usize, String> {
    let key = (pid, namespace.to_string(), name.to_string());
    if let Some(k) = CLASS_CACHE.lock().ok().and_then(|c| c.get(&key).copied()) { return Ok(k); }
    let k = find_class_uncached(pid, base, namespace, name)?;
    if let Ok(mut c) = CLASS_CACHE.lock() { c.insert(key, k); }
    Ok(k)
}

fn find_class_uncached(pid: u32, base: usize, namespace: &str, name: &str) -> Result<usize, String> {
    let table = type_table(pid, base)?;
    let raw = read_memory(pid, table, MAX_TYPES * 8)
        .or_else(|_| read_memory(pid, table, 16384 * 8))
        .map_err(|e| format!("type table read failed: {}", e))?;
    for chunk in raw.chunks_exact(8) {
        let k = usize::from_le_bytes(chunk.try_into().unwrap());
        if !plausible_ptr(k) { continue; }
        let n = match read_ptr(pid, k + CLASS_NAME).and_then(|p| read_cstr(pid, p)) { Some(n) => n, None => continue };
        if n != name { continue; }
        let ns = read_ptr(pid, k + CLASS_NAMESPACE).and_then(|p| read_cstr(pid, p)).unwrap_or_default();
        if ns == namespace { return Ok(k); }
    }
    Err(format!("IL2CPP class '{}.{}' not found", namespace, name))
}

/// Offset of `field` in `klass` or its parents (instance offset, or offset into static data).
fn find_field_offset(pid: u32, klass: usize, field: &str) -> Result<usize, String> {
    let key = (pid, klass, field.to_string());
    if let Some(o) = FIELD_CACHE.lock().ok().and_then(|c| c.get(&key).copied()) { return Ok(o); }
    let o = find_field_offset_uncached(pid, klass, field)?;
    if let Ok(mut c) = FIELD_CACHE.lock() { c.insert(key, o); }
    Ok(o)
}

fn find_field_offset_uncached(pid: u32, klass: usize, field: &str) -> Result<usize, String> {
    let mut cur = Some(klass);
    while let Some(k) = cur {
        let arr = read_ptr(pid, k + CLASS_FIELDS).unwrap_or(0);
        let count = read_u16(pid, k + CLASS_FIELD_COUNT).unwrap_or(0) as usize;
        if plausible_ptr(arr) {
            for i in 0..count {
                let f = arr + i * FIELD_STRIDE;
                let name = read_ptr(pid, f + FIELD_NAME).and_then(|p| read_cstr(pid, p));
                if name.as_deref() == Some(field) {
                    let off = read_u32(pid, f + FIELD_OFFSET).ok_or("field offset unreadable")? as i32;
                    if off < 0 { return Err(format!("Field '{}' has no fixed offset", field)); }
                    return Ok(off as usize);
                }
            }
        }
        cur = read_ptr(pid, k + CLASS_PARENT).filter(|p| plausible_ptr(*p));
    }
    Err(format!("Field '{}' not found", field))
}

/// One step of a path from the singleton to the final value.
pub enum Hop {
    /// Read `field` (declared on `namespace.class_name` or a parent) from the current object.
    Field { namespace: String, class_name: String, field: String },
    /// Read the pointer/value at a raw byte offset from the current object.
    Offset(usize),
    /// Current object is a `List<T>` of references; take element `index`.
    ListItem(usize),
    /// Current object is a `List<T>`; take the first element whose byte at
    /// `class_name.field` equals `equals` (e.g. the first non-AI `Character`).
    ListFindByte { namespace: String, class_name: String, field: String, equals: u8 },
}

fn list_elements(pid: u32, list: usize) -> Result<(usize, usize), String> {
    let items = read_ptr(pid, list + LIST_ITEMS).filter(|p| plausible_ptr(*p)).ok_or("List._items is null")?;
    let size = read_u32(pid, list + LIST_SIZE).ok_or("List._size unreadable")? as usize;
    if size > 1_000_000 { return Err("List size implausible".to_string()); }
    Ok((items, size))
}

/// Resolve `class.static_field` (a singleton) then follow `hops`. All hops but the last
/// dereference; the last must be a `Field` or `Offset` and yields the value's address.
pub fn resolve_path(
    pid: u32,
    base: usize,
    namespace: &str,
    class_name: &str,
    static_field: &str,
    hops: &[Hop],
) -> Result<usize, String> {
    if hops.is_empty() { return Err("path needs at least one hop".to_string()); }
    let klass = find_class(pid, base, namespace, class_name)?;
    let sf_off = find_field_offset(pid, klass, static_field)?;
    let sdata = read_ptr(pid, klass + CLASS_STATIC_FIELDS)
        .filter(|p| plausible_ptr(*p))
        .ok_or("Class has no static data yet")?;
    let mut cur = read_ptr(pid, sdata + sf_off)
        .filter(|p| plausible_ptr(*p))
        .ok_or_else(|| format!("{}.{} is null — game not loaded yet?", class_name, static_field))?;

    for (i, hop) in hops.iter().enumerate() {
        let last = i + 1 == hops.len();
        let addr = match hop {
            Hop::Field { namespace, class_name, field } => {
                let k = find_class(pid, base, namespace, class_name)?;
                cur + find_field_offset(pid, k, field)?
            }
            Hop::Offset(o) => cur + *o,
            Hop::ListItem(idx) => {
                let (items, size) = list_elements(pid, cur)?;
                if *idx >= size { return Err(format!("List has {} items, wanted #{}", size, idx)); }
                let a = items + ARRAY_FIRST_ELEM + idx * 8;
                if last { return Err("ListItem cannot be the last hop".to_string()); }
                cur = read_ptr(pid, a).filter(|p| plausible_ptr(*p)).ok_or("List element is null")?;
                continue;
            }
            Hop::ListFindByte { namespace, class_name, field, equals } => {
                if last { return Err("ListFindByte cannot be the last hop".to_string()); }
                let k = find_class(pid, base, namespace, class_name)?;
                let off = find_field_offset(pid, k, field)?;
                let (items, size) = list_elements(pid, cur)?;
                let mut found = None;
                for n in 0..size {
                    let el = match read_ptr(pid, items + ARRAY_FIRST_ELEM + n * 8) {
                        Some(p) if plausible_ptr(p) => p,
                        _ => continue,
                    };
                    if read_memory(pid, el + off, 1).map(|b| b[0] == *equals).unwrap_or(false) {
                        found = Some(el);
                        break;
                    }
                }
                cur = found.ok_or_else(|| format!("No list element with {}.{} == {}", class_name, field, equals))?;
                continue;
            }
        };
        if last { return Ok(addr); }
        cur = read_ptr(pid, addr)
            .filter(|p| plausible_ptr(*p))
            .ok_or_else(|| format!("Hop {} is null — not in a loaded game yet?", i))?;
    }
    unreachable!()
}

/// Native entry address of `namespace.class_name::method`. Errors if the name is ambiguous
/// (overloads), because patching the wrong overload would be silent and hard to diagnose.
pub fn find_method_address(pid: u32, base: usize, namespace: &str, class_name: &str, method: &str) -> Result<usize, String> {
    let klass = find_class(pid, base, namespace, class_name)?;
    let arr = read_ptr(pid, klass + CLASS_METHODS).filter(|p| plausible_ptr(*p)).ok_or("class has no methods array")?;
    let count = read_u16(pid, klass + CLASS_METHOD_COUNT).unwrap_or(0) as usize;
    let mut found = Vec::new();
    for i in 0..count {
        let m = match read_ptr(pid, arr + i * 8) { Some(m) if plausible_ptr(m) => m, _ => continue };
        let name = read_ptr(pid, m + METHOD_NAME).and_then(|p| read_cstr(pid, p));
        if name.as_deref() == Some(method) {
            if let Some(fp) = read_ptr(pid, m + METHOD_POINTER).filter(|p| plausible_ptr(*p)) {
                found.push(fp);
            }
        }
    }
    match found.len() {
        1 => Ok(found[0]),
        0 => Err(format!("Method '{}::{}' not found (or has no native body)", class_name, method)),
        n => Err(format!("Method '{}::{}' has {} overloads — refusing to guess", class_name, method, n)),
    }
}

/// Offset of `class_name.field` (instance offset), for callers that scan native code for stores.
pub fn field_offset_of(pid: u32, base: usize, namespace: &str, class_name: &str, field: &str) -> Result<usize, String> {
    let k = find_class(pid, base, namespace, class_name)?;
    find_field_offset(pid, k, field)
}

/// Smallest method entry address greater than `start` across every class — an upper bound on
/// where the method at `start` ends (IL2CPP has no per-method size record to read).
pub fn next_method_after(pid: u32, base: usize, start: usize) -> Result<usize, String> {
    let table = type_table(pid, base)?;
    let raw = read_memory(pid, table, MAX_TYPES * 8)
        .or_else(|_| read_memory(pid, table, 16384 * 8))
        .map_err(|e| format!("type table read failed: {}", e))?;
    let mut best = usize::MAX;
    for chunk in raw.chunks_exact(8) {
        let k = usize::from_le_bytes(chunk.try_into().unwrap());
        if !plausible_ptr(k) { continue; }
        let arr = match read_ptr(pid, k + CLASS_METHODS) { Some(a) if plausible_ptr(a) => a, _ => continue };
        let count = read_u16(pid, k + CLASS_METHOD_COUNT).unwrap_or(0) as usize;
        if count == 0 || count > 20000 { continue; }
        let ptrs = match read_memory(pid, arr, count * 8) { Ok(p) => p, Err(_) => continue };
        for m in ptrs.chunks_exact(8) {
            let mi = usize::from_le_bytes(m.try_into().unwrap());
            if !plausible_ptr(mi) { continue; }
            if let Some(fp) = read_ptr(pid, mi + METHOD_POINTER) {
                if fp > start && fp < best { best = fp; }
            }
        }
    }
    if best == usize::MAX { Err("could not bound the method's size".to_string()) } else { Ok(best) }
}

/// Native entry addresses of every overload of `namespace.class_name::method`.
pub fn find_method_addresses(pid: u32, base: usize, namespace: &str, class_name: &str, method: &str) -> Result<Vec<usize>, String> {
    let klass = find_class(pid, base, namespace, class_name)?;
    let arr = read_ptr(pid, klass + CLASS_METHODS).filter(|p| plausible_ptr(*p)).ok_or("class has no methods array")?;
    let count = read_u16(pid, klass + CLASS_METHOD_COUNT).unwrap_or(0) as usize;
    let mut found = Vec::new();
    for i in 0..count {
        let m = match read_ptr(pid, arr + i * 8) { Some(m) if plausible_ptr(m) => m, _ => continue };
        let name = read_ptr(pid, m + METHOD_NAME).and_then(|p| read_cstr(pid, p));
        if name.as_deref() == Some(method) {
            if let Some(fp) = read_ptr(pid, m + METHOD_POINTER).filter(|p| plausible_ptr(*p)) {
                found.push(fp);
            }
        }
    }
    if found.is_empty() {
        return Err(format!("Method '{}::{}' not found (or has no native body)", class_name, method));
    }
    Ok(found)
}

/// Every method entry address in the module, sorted and de-duplicated. A method's size is
/// bounded by the next entry in this list.
pub fn all_method_entries(pid: u32, base: usize) -> Result<Vec<usize>, String> {
    let table = type_table(pid, base)?;
    let raw = read_memory(pid, table, MAX_TYPES * 8)
        .or_else(|_| read_memory(pid, table, 16384 * 8))
        .map_err(|e| format!("type table read failed: {}", e))?;
    let mut out = Vec::new();
    for chunk in raw.chunks_exact(8) {
        let k = usize::from_le_bytes(chunk.try_into().unwrap());
        if !plausible_ptr(k) { continue; }
        let arr = match read_ptr(pid, k + CLASS_METHODS) { Some(a) if plausible_ptr(a) => a, _ => continue };
        let count = read_u16(pid, k + CLASS_METHOD_COUNT).unwrap_or(0) as usize;
        if count == 0 || count > 20000 { continue; }
        let ptrs = match read_memory(pid, arr, count * 8) { Ok(p) => p, Err(_) => continue };
        for m in ptrs.chunks_exact(8) {
            let mi = usize::from_le_bytes(m.try_into().unwrap());
            if !plausible_ptr(mi) { continue; }
            if let Some(fp) = read_ptr(pid, mi + METHOD_POINTER) {
                if plausible_ptr(fp) { out.push(fp); }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}
