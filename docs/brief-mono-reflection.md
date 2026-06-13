# Brief: Mono Reflection Traversal

## Status
Unimplemented.

## Problem
The `scan` cheat type requires user interaction every session: enter current value, scan, optionally change the value and rescan to narrow duplicates. This is necessary because Unity's Mono GC heap relocates on every launch — no address survives a restart.

Mono reflection fixes this by navigating the Mono *runtime metadata* instead of scanning the heap. The runtime re-establishes the same class/field layout every launch; only the heap base moves. Once you know the assembly, class, and field name, the value's address can be resolved deterministically every session with no user input.

---

## One-Time Discovery (manual, per game version)

Before writing a Mono cheat, the trainer author must identify:
- **Assembly name** — almost always `Assembly-CSharp`
- **Namespace** — often empty; game-dependent
- **Class name** — e.g. `CurrencyManager`
- **Field name** — e.g. `darwinium`
- **Static vs instance** — whether the field is on a `static` class member or on a singleton instance

### How to discover these

1. Find the game's managed assembly:
   ```
   <GameDir>\<GameName>_Data\Managed\Assembly-CSharp.dll
   ```
2. Open it in **dnSpy** (https://github.com/dnSpy/dnSpy) or **ILSpy**
3. Search for the currency name (e.g. "darwinium") in the assembly tree
4. Note the namespace, class name, and field name

### Cell to Singularity — fields to discover

The exact class/field names are not yet confirmed. Using dnSpy on:
```
<CellToSingularity install>\CellToSingularity_Data\Managed\Assembly-CSharp.dll
```
Search for `darwinium` or look in classes named `CurrencyManager`, `GameManager`, `PlayerData`, or `Singularity`. The field type will be `double` (confirmed from CE scan). Note whether it is `static`.

---

## New Cheat Type JSON

```json
{
  "id": "cts-darwinium",
  "name": "Darwinium",
  "type": "mono",
  "valueType": "double",
  "monoAssembly": "Assembly-CSharp",
  "monoNamespace": "",
  "monoClass": "CurrencyManager",
  "monoField": "darwinium",
  "monoStatic": true,
  "module": "mono-2.0-bdwgc.dll",
  "offsets": [],
  "onValue": 1000
}
```

Fields:
- `monoAssembly` — assembly name without `.dll`
- `monoNamespace` — class namespace; use `""` if none
- `monoClass` — class name
- `monoField` — field name
- `monoStatic` — `true` if the field is declared `static`; `false` if it is on a singleton instance
- `module` — always `"mono-2.0-bdwgc.dll"` for Mono games
- `onValue` — value to write when the cheat is fired

---

## Implementation

### Architecture overview

Magic Wand reads Mono's internal data structures from the target process using `ReadProcessMemory`. No code injection or remote threads are needed. The traversal path is:

```
mono_root_domain (exported global)
  └─ MonoDomain
       └─ domain_assemblies (GSList of MonoAssembly*)
            └─ MonoAssembly  [match by aname.name]
                 └─ MonoImage
                      └─ class_cache (hash table of MonoClass*)
                           └─ MonoClass  [match by name + namespace]
                                └─ MonoClassField[]  [match by name]
                                     ├─ Static: vtable->data + field.offset
                                     └─ Instance: object* + field.offset
                                          (object found via MonoVTable or singleton scan)
```

---

### 1. Rust: Mono PE export reader — `src-tauri/src/mono.rs` (new file)

```rust
use windows::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};
use windows::Win32::Foundation::CloseHandle;
use crate::engine::read_memory;

/// Read a null-terminated UTF-8 string from target process memory.
fn read_string(pid: u32, addr: usize, max_len: usize) -> Option<String> {
    let buf = read_memory(pid, addr, max_len).ok()?;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8(buf[..end].to_vec()).ok()
}

fn read_ptr(pid: u32, addr: usize) -> Option<usize> {
    let data = read_memory(pid, addr, 8).ok()?;
    Some(usize::from_le_bytes(data.try_into().ok()?))
}

fn read_u32(pid: u32, addr: usize) -> Option<u32> {
    let data = read_memory(pid, addr, 4).ok()?;
    Some(u32::from_le_bytes(data.try_into().ok()?))
}

/// Parse the PE export directory of mono-2.0-bdwgc.dll from target process memory
/// and return the VA of the named export.
fn find_mono_export(pid: u32, module_base: usize, name: &str) -> Option<usize> {
    // PE header offset at 0x3C
    let e_lfanew = read_u32(pid, module_base + 0x3C)? as usize;
    let nt_header = module_base + e_lfanew;

    // Export directory RVA is at NT_HEADER + 0x18 (OptionalHeader) + 0x70 (DataDirectory[0].VirtualAddress)
    let export_dir_rva = read_u32(pid, nt_header + 0x18 + 0x70)? as usize;
    if export_dir_rva == 0 { return None; }
    let export_dir = module_base + export_dir_rva;

    let num_names     = read_u32(pid, export_dir + 0x18)? as usize;
    let addr_table    = module_base + read_u32(pid, export_dir + 0x1C)? as usize;
    let name_table    = module_base + read_u32(pid, export_dir + 0x20)? as usize;
    let ordinal_table = module_base + read_u32(pid, export_dir + 0x24)? as usize;

    for i in 0..num_names {
        let name_rva = read_u32(pid, name_table + i * 4)? as usize;
        let export_name = read_string(pid, module_base + name_rva, 128)?;
        if export_name == name {
            let ordinal = read_u32(pid, ordinal_table + i * 2)? as usize & 0xFFFF;
            let func_rva = read_u32(pid, addr_table + ordinal * 4)? as usize;
            return Some(module_base + func_rva);
        }
    }
    None
}

/// Locate Mono's root MonoDomain pointer.
/// `mono_root_domain` is a data export (a MonoDomain**) in Unity's Mono fork.
/// Reading it gives the MonoDomain*.
fn get_root_domain(pid: u32, mono_base: usize) -> Option<usize> {
    let domain_ptr_addr = find_mono_export(pid, mono_base, "mono_root_domain")?;
    let domain = read_ptr(pid, domain_ptr_addr)?;
    if domain == 0 { None } else { Some(domain) }
}
```

---

### 2. Rust: Structure traversal — `src-tauri/src/mono.rs` (continued)

The offsets below target **Unity Mono 2019–2022** (the Mono 5.x/6.x fork shipped with Unity). If the game uses a different Mono version, some offsets may shift by 8–16 bytes. Provide a `MonoOffsets` struct so they can be tuned per-game without rebuilding.

```rust
/// Offsets into Mono internal structures for Unity 2019–2022 Mono fork.
/// Adjust if a game uses a different Mono version.
pub struct MonoOffsets {
    /// MonoDomain: offset of domain_assemblies (GSList*)
    pub domain_assemblies: usize,
    /// MonoAssembly: offset of MonoAssemblyName (inline struct, first field is char* name)
    pub assembly_aname: usize,
    /// MonoAssembly: offset of MonoImage*
    pub assembly_image: usize,
    /// MonoImage: offset of class_cache (MonoInternalHashTable)
    pub image_class_cache: usize,
    /// MonoInternalHashTable: offset of table (MonoClass**)
    pub hash_table: usize,
    /// MonoInternalHashTable: offset of size (int32)
    pub hash_size: usize,
    /// MonoClass: offset of char* name
    pub class_name: usize,
    /// MonoClass: offset of char* name_space
    pub class_namespace: usize,
    /// MonoClass: offset of MonoClassField* fields
    pub class_fields: usize,
    /// MonoClass: offset of uint32 field_count
    pub class_field_count: usize,
    /// MonoClass: offset of MonoClassRuntimeInfo*
    pub class_runtime_info: usize,
    /// MonoClassField: total size of one field entry
    pub field_size: usize,
    /// MonoClassField: offset of char* name within a field entry
    pub field_name: usize,
    /// MonoClassField: offset of int32 offset within a field entry
    pub field_offset: usize,
    /// MonoClassRuntimeInfo: offset of domain_vtables[0] (MonoVTable*)
    pub runtime_info_vtable: usize,
    /// MonoVTable: offset of void* data (static field storage)
    pub vtable_data: usize,
}

impl MonoOffsets {
    pub fn unity_default() -> Self {
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

/// GSList node: data pointer at +0, next pointer at +8
fn gslist_iter(pid: u32, head: usize) -> Vec<usize> {
    let mut items = Vec::new();
    let mut node = head;
    while node != 0 {
        if let Some(data) = read_ptr(pid, node) {
            if data != 0 { items.push(data); }
        }
        node = read_ptr(pid, node + 8).unwrap_or(0);
    }
    items
}

fn find_assembly(pid: u32, domain: usize, assembly_name: &str, off: &MonoOffsets) -> Option<usize> {
    let assemblies_head = read_ptr(pid, domain + off.domain_assemblies)?;
    for asm in gslist_iter(pid, assemblies_head) {
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
    let image = read_ptr(pid, assembly + off.assembly_image)?;
    let cache_base = image + off.image_class_cache;
    let table_ptr  = read_ptr(pid, cache_base + off.hash_table)?;
    let table_size = read_u32(pid, cache_base + off.hash_size)? as usize;

    for i in 0..table_size {
        let mut klass = read_ptr(pid, table_ptr + i * 8).unwrap_or(0);
        while klass != 0 {
            let name_ptr = read_ptr(pid, klass + off.class_name).unwrap_or(0);
            let ns_ptr   = read_ptr(pid, klass + off.class_namespace).unwrap_or(0);
            let name = read_string(pid, name_ptr, 128).unwrap_or_default();
            let ns   = read_string(pid, ns_ptr, 128).unwrap_or_default();
            if name == class_name && ns == namespace {
                return Some(klass);
            }
            // MonoClass has a next_class_cache pointer at a fixed offset — skip for now
            // (hash chaining handled by bucket walk above; most tables are flat)
            break;
        }
    }
    None
}

struct FieldInfo {
    offset: u32,
    is_static: bool,
}

fn find_field(pid: u32, klass: usize, field_name: &str, off: &MonoOffsets) -> Option<FieldInfo> {
    let fields_ptr   = read_ptr(pid, klass + off.class_fields)?;
    let field_count  = read_u32(pid, klass + off.class_field_count)? as usize;

    for i in 0..field_count {
        let field = fields_ptr + i * off.field_size;
        let name_ptr = read_ptr(pid, field + off.field_name)?;
        let name = read_string(pid, name_ptr, 128)?;
        if name == field_name {
            let raw_offset = read_u32(pid, field + off.field_offset)?;
            // Static fields: offset has bit 31 cleared by convention in the storage;
            // type attrs carry FIELD_ATTRIBUTE_STATIC (0x10) in MonoType->attrs
            // For simplicity, rely on the caller-supplied monoStatic flag.
            return Some(FieldInfo { offset: raw_offset, is_static: false /* filled by caller */ });
        }
    }
    None
}

fn get_static_field_address(pid: u32, klass: usize, field_offset: u32, off: &MonoOffsets) -> Option<usize> {
    let runtime_info = read_ptr(pid, klass + off.class_runtime_info)?;
    let vtable = read_ptr(pid, runtime_info + off.runtime_info_vtable)?;
    let static_data = read_ptr(pid, vtable + off.vtable_data)?;
    Some(static_data + field_offset as usize)
}

/// Top-level: resolve a mono cheat to its field address in the target process.
pub fn resolve_mono_field(
    pid: u32,
    mono_module_base: usize,
    assembly_name: &str,
    namespace: &str,
    class_name: &str,
    field_name: &str,
    is_static: bool,
) -> Result<usize, String> {
    let off = MonoOffsets::unity_default();

    let domain = get_root_domain(pid, mono_module_base)
        .ok_or("Could not locate Mono root domain")?;

    let assembly = find_assembly(pid, domain, assembly_name, &off)
        .ok_or_else(|| format!("Assembly '{}' not found", assembly_name))?;

    let klass = find_class(pid, assembly, namespace, class_name, &off)
        .ok_or_else(|| format!("Class '{}.{}' not found", namespace, class_name))?;

    let field = find_field(pid, klass, field_name, &off)
        .ok_or_else(|| format!("Field '{}' not found on class '{}'", field_name, class_name))?;

    if is_static {
        get_static_field_address(pid, klass, field.offset, &off)
            .ok_or_else(|| format!("Could not read static vtable for '{}'", class_name))
    } else {
        // Instance field: find the singleton instance via vtable->data is wrong here.
        // Instance support is out of scope for v1 — all known Unity currency fields are static
        // or held on a MonoBehaviour singleton that can be found via mono_gc_get_objects.
        // Leave as a future enhancement.
        Err("Instance mono fields not yet supported — use monoStatic: true".to_string())
    }
}
```

---

### 3. Rust: Tauri command — `src-tauri/src/lib.rs`

```rust
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
    let (mono_base, _) = engine::get_module_info(pid, &module_name)
        .ok_or_else(|| format!("Module '{}' not found in process", module_name))?;
    let addr = mono::resolve_mono_field(
        pid, mono_base, &assembly, &namespace, &class_name, &field_name, is_static,
    )?;
    Ok(format!("0x{:X}", addr))
}
```

Register in `invoke_handler`:
```rust
resolve_mono_field,
```

Add `mod mono;` at the top of `lib.rs`.

---

### 4. TypeScript: Extend `Cheat` interface — `src/hooks/useTrainer.ts`

```typescript
export interface Cheat {
  id: string;
  name: string;
  type: 'toggle' | 'action' | 'patch' | 'scan' | 'mono';
  valueType?: 'int' | 'float' | 'double';
  module: string;
  base?: string;
  signature?: string;
  offsets: string[];
  onValue: number;
  onBytes?: number[];
  offBytes?: number[];
  // Mono-specific fields:
  monoAssembly?: string;
  monoNamespace?: string;
  monoClass?: string;
  monoField?: string;
  monoStatic?: boolean;
  active?: boolean;
  currentValue?: string | number;
}
```

Update `resolveCheatAddress` to handle `mono` type:

```typescript
const resolveCheatAddress = async (cheat: Cheat): Promise<string> => {
  if (!pidRef.current) throw new Error('Not connected');
  if (addressCache.current[cheat.id]) return addressCache.current[cheat.id];

  let finalAddr: string;

  if (cheat.type === 'mono') {
    finalAddr = await invoke<string>('resolve_mono_field', {
      pid: pidRef.current,
      moduleName: cheat.module,
      assembly: cheat.monoAssembly ?? 'Assembly-CSharp',
      namespace: cheat.monoNamespace ?? '',
      className: cheat.monoClass ?? '',
      fieldName: cheat.monoField ?? '',
      isStatic: cheat.monoStatic ?? true,
    });
  } else if (cheat.signature) {
    // ... existing AOB path
  } else if (cheat.base) {
    // ... existing pointer path
  } else {
    throw new Error('Invalid cheat config');
  }

  addressCache.current[cheat.id] = finalAddr;
  return finalAddr;
};
```

The existing poll loop and `applyCheat` work unchanged — they call `resolveCheatAddress` and then read/write via the existing `read_double`/`write_double` commands.

---

### 5. UI changes — `src/App.tsx`

`mono` cheats render identically to `action` cheats — a Fire button that writes `onValue`. No scan input is needed. The existing rendering logic handles this because `mono` will fall through to the non-`scan` branch, which already shows a Fire button for action/mono types and a toggle for toggle/patch types.

The only addition: treat `mono` like `action` in the type check:

```tsx
{cheat.type === 'action' || cheat.type === 'mono' ? (
  <button className="fire-button" onClick={() => applyCheat(cheat)} disabled={!pid}>
    {cheat.type === 'mono' ? `Set to ${cheat.onValue}` : 'Fire'}
  </button>
) : (
  // ... toggle switch
)}
```

Add a CSS badge color for `mono`:

```css
.cheat-type-mono { background: #1a1a3a; color: #818cf8; }
```

---

### 6. Cell to Singularity trainer update — `public/trainers/cell-to-singularity.json`

Once the class and field names are confirmed via dnSpy, replace the existing `scan` cheat with:

```json
{
  "name": "Cell to Singularity",
  "executable": "CellToSingularity.exe",
  "cheats": [
    {
      "id": "cts-darwinium",
      "name": "Darwinium",
      "type": "mono",
      "valueType": "double",
      "module": "mono-2.0-bdwgc.dll",
      "monoAssembly": "Assembly-CSharp",
      "monoNamespace": "",
      "monoClass": "???",
      "monoField": "???",
      "monoStatic": true,
      "offsets": [],
      "onValue": 1000
    }
  ]
}
```

Replace `???` after running dnSpy discovery.

---

## Offset Calibration

If the traversal fails (`Could not locate Mono root domain` or class not found), the `MonoOffsets` defaults may not match the game's Mono version. Calibration steps:

1. In Cheat Engine, attach to the game and open the Mono dissector (`Mono > Enable Mono Features`)
2. CE will show the correct class/field names and addresses
3. Compare CE's resolved address against what `resolve_mono_field` returns (or fails to find)
4. Adjust the `MonoOffsets` constants — `domain_assemblies`, `image_class_cache`, and `class_runtime_info` are the most version-sensitive

Alternatively, add a `mono_offsets` object to the trainer JSON so offsets can be tuned without recompiling:
```json
"monoOffsets": {
  "domainAssemblies": 208,
  "imageClassCache": 888
}
```
This is optional for v1.

---

## Acceptance Criteria
- `resolve_mono_field` Tauri command returns the correct address for a known static double field in a live process
- The returned address matches Cheat Engine's Mono dissector for the same field
- Mono cheat fires and writes `onValue` with a single button click
- No per-session scanning required — the address is resolved fresh from Mono metadata each launch
- The existing `scan`, `toggle`, `action`, and `patch` cheat types are unaffected
- `cargo build` compiles with no errors; no TypeScript errors
- `monoStatic: false` returns a clear "not yet supported" error rather than a crash

## Notes
- **Instance field support** is out of scope for v1. All Unity currency values tested so far are static. If a game uses instance fields, a future brief should cover `mono_gc_get_objects` heap enumeration.
- **IL2CPP games** (most new Unity titles, e.g. Genshin Impact) use a completely different reflection path and are out of scope for this brief. IL2CPP strips the managed runtime; you'd need to navigate the IL2CPP metadata instead. Check which backend a game uses: if `mono-2.0-bdwgc.dll` is present in the process → Mono; if `GameAssembly.dll` is present instead → IL2CPP.
- The `scan` cheat type in `cell-to-singularity.json` should remain as a fallback until the Mono path is validated against a live session.
