# Brief: UE5 Property Cheat Type (`ue5_prop`)

## Status
Unimplemented.

## Problem
Borderlands 4 and other Unreal Engine 5 games are compiled C++ — there are no managed assemblies, so the `mono` and `mono_chain` cheat types do not apply. The existing `toggle` and `patch` types require AOB byte signatures that are version-fragile and require manual Cheat Engine research each game update.

UE5 ships its own runtime reflection system (`UObject` / `UClass` / `FProperty`) that is structurally analogous to Mono metadata: class names, property names, and field byte offsets are all present in the running process memory. This brief adds a `ue5_prop` cheat type that navigates this reflection system to find live game objects and read/write their properties by name — no AOB signature hunting required.

## Goal
A `"ue5_prop"` cheat type that:
1. Locates the global `GObjects` array via a one-time AOB pattern (stable across sessions)
2. Iterates `GObjects` to find the first live `UObject` whose class name matches `ue5ClassName`
3. Reads or writes the property value at `object_ptr + ue5PropertyOffset`
4. On toggle **ON**: writes `onValue`, then re-writes it each poll tick to maintain the state
5. On toggle **OFF**: writes `offValue` once to restore the previous state

Property offsets and class names are discovered via UE4SS and baked into the trainer JSON. They are stable across sessions (compiled struct offsets) and only change when the game patches its binary layout.

---

## New Cheat Type JSON

```json
{
  "id": "bl4-godmode",
  "name": "God Mode",
  "type": "ue5_prop",
  "valueType": "bool",
  "module": "Borderlands4.exe",
  "ue5GObjectsAob": "?? ?? ?? ?? ?? ?? ?? ??",
  "ue5ClassName": "BP_OakPlayerCharacter_C",
  "ue5PropertyOffset": 728,
  "offsets": [],
  "onValue": 0,
  "offValue": 1
}
```

Fields:
- `ue5GObjectsAob` — AOB pattern inside `module` whose RIP-relative operand resolves to the `GObjects` pointer. Discovered once via UE4SS or CE and baked in. Example: `"48 8B 05 ?? ?? ?? ?? 48 85 C0 74 ?? 8B 40 08"`
- `ue5GNamesAob` — AOB pattern whose RIP-relative operand resolves to `GNames` (the FName string pool). Same discovery method. Required for class name comparison.
- `ue5ClassName` — Exact `UClass` name to search for in `GObjects` (e.g. `"BP_OakPlayerCharacter_C"`). Obtained from UE4SS Object Dumper output.
- `ue5PropertyOffset` — Byte offset of the target property within the `UObject` struct. Obtained from UE4SS Live View. Replaces per-session property-name resolution to keep the Rust implementation simple.
- `offValue` — Value to write when the cheat is toggled OFF. Optional; if absent, nothing is written on disable and the game restores its own state.
- `valueType` — `"bool"`, `"int"`, `"float"`, or `"double"`. Controls which read/write command is used.

---

## Implementation

### 1. Rust: New file `src-tauri/src/ue5.rs`

Analogous to `src-tauri/src/mono.rs`.

#### Ue5Offsets struct

```rust
use crate::engine::{read_memory, get_module_info};

/// Offsets into UE5 internal structures.
/// Defaults target UE 5.5.x (Borderlands 4). Confirm all values against UE4SS output.
pub struct Ue5Offsets {
    /// Bytes per FUObjectItem slot in GObjects
    pub fuobjectitem_size: usize,
    /// Offset of UObject* within FUObjectItem
    pub fuobjectitem_object: usize,
    /// UObject: offset of ClassPrivate (UClass*)
    pub uobject_class: usize,
    /// UObject: offset of NamePrivate (FName: two i32s = 8 bytes)
    pub uobject_name: usize,
    /// UClass: offset of ChildProperties (first FField*)
    pub uclass_children: usize,
    /// FField: offset of Next sibling (FField*)
    pub ffield_next: usize,
    /// FField: offset of NamePrivate (FName)
    pub ffield_name: usize,
    /// FProperty: offset of Offset_Internal (i32, the byte offset within UObject)
    pub fproperty_offset_internal: usize,
    /// GObjects chunked array: items per chunk (typically 65536)
    pub gobjects_chunk_size: usize,
    /// GObjects: offset of ObjObjects.Objects (FUObjectItem** — chunk pointer array)
    pub gobjects_objects: usize,
    /// GObjects: offset of ObjObjects.NumElements (i32)
    pub gobjects_num_elements: usize,
    /// GNames FNamePool: byte stride between entries (typically 2)
    pub fname_stride: usize,
    /// Offset of the string data within a FNameEntry (past the header)
    pub fname_entry_header: usize,
    /// Bytes per GNames chunk (typically 0x20000 = 131072)
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
            gobjects_num_elements:     0x0C,
            fname_stride:              2,
            fname_entry_header:        0x02,
            gnames_chunk_size:         0x20000,
        }
    }
}
```

#### Low-level helpers (add after Ue5Offsets)

```rust
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
```

#### AOB-to-address resolver

Finds a global pointer (GObjects or GNames) from an AOB pattern containing a RIP-relative operand.

```rust
/// Scan `module` for `pattern`, then follow the RIP-relative i32 at `rip_offset`
/// bytes into the match to resolve the final pointer address.
/// rip_offset: byte index within the match where the 4-byte RIP-relative displacement begins.
/// instr_end_offset: total length of the instruction (displacement end = instr_end_offset).
fn resolve_rip_relative(
    pid: u32,
    module_base: usize,
    module_size: usize,
    pattern: &str,
    rip_offset: usize,
    instr_end: usize,
) -> Option<usize> {
    use crate::engine::aob_scan_range;
    let match_addr = aob_scan_range(pid, module_base, module_size, pattern)?;
    let disp = read_i32(pid, match_addr + rip_offset)? as isize;
    Some(((match_addr + instr_end) as isize + disp) as usize)
}
```

> **Note:** `aob_scan_range` is an engine helper that returns the VA of the first match. If the existing `aob_scan` Tauri command only returns a String, add an internal version that returns `usize` directly, callable from Rust without going through Tauri.

#### GNames string lookup

```rust
fn fname_to_string(pid: u32, gnames_base: usize, fname_index: i32, off: &Ue5Offsets) -> Option<String> {
    if fname_index < 0 { return None; }
    let idx = fname_index as usize;
    // GNames FNamePool: outer array of chunk pointers
    let chunk_idx  = idx / (off.gnames_chunk_size / off.fname_stride);
    let entry_off  = (idx % (off.gnames_chunk_size / off.fname_stride)) * off.fname_stride;
    let chunk_ptr  = read_ptr(pid, gnames_base + chunk_idx * 8)?;
    let entry_addr = chunk_ptr + entry_off;
    read_string(pid, entry_addr + off.fname_entry_header, 128)
}
```

#### GObjects iteration

```rust
/// Return the address of the first live UObject in GObjects whose class name == target_class.
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
        let chunk_idx  = i / off.gobjects_chunk_size;
        let item_idx   = i % off.gobjects_chunk_size;
        let chunk_ptr  = match read_ptr(pid, objects_ptr + chunk_idx * 8) {
            Some(p) if p != 0 => p,
            _ => continue,
        };
        let item_addr  = chunk_ptr + item_idx * off.fuobjectitem_size;
        let obj_ptr    = match read_ptr(pid, item_addr + off.fuobjectitem_object) {
            Some(p) if p != 0 => p,
            _ => continue,
        };

        // Read the object's class and its name
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
```

#### Top-level resolver

```rust
/// Resolve a ue5_prop cheat to its field address in the target process.
pub fn resolve_ue5_prop(
    pid: u32,
    module_base: usize,
    module_size: usize,
    gobjects_aob: &str,
    gnames_aob: &str,
    class_name: &str,
    property_offset: usize,
) -> Result<usize, String> {
    let off = Ue5Offsets::ue5_default();

    let gobjects_base = resolve_rip_relative(pid, module_base, module_size, gobjects_aob, 3, 7)
        .ok_or("Could not resolve GObjects from AOB")?;

    let gnames_base = resolve_rip_relative(pid, module_base, module_size, gnames_aob, 3, 7)
        .ok_or("Could not resolve GNames from AOB")?;

    let obj_ptr = find_object_by_class(pid, gobjects_base, gnames_base, class_name, &off)?;

    Ok(obj_ptr + property_offset)
}
```

> The `rip_offset: 3` and `instr_end: 7` values assume the pattern begins with a 3-byte opcode prefix (`48 8B 05`) followed by a 4-byte displacement. Confirm against the actual AOB bytes from UE4SS research.

---

### 2. Rust: Add to `src-tauri/src/lib.rs`

Add `mod ue5;` at the top alongside `mod mono;`.

```rust
#[tauri::command]
fn resolve_ue5_prop(
    pid: u32,
    module_name: String,
    gobjects_aob: String,
    gnames_aob: String,
    class_name: String,
    property_offset: usize,
) -> Result<String, String> {
    let (base, size) = engine::get_module_info(pid, &module_name)
        .ok_or_else(|| format!("Module '{}' not found", module_name))?;
    let addr = ue5::resolve_ue5_prop(
        pid, base, size,
        &gobjects_aob, &gnames_aob,
        &class_name, property_offset,
    )?;
    Ok(format!("0x{:X}", addr))
}
```

Register in `invoke_handler`:
```rust
resolve_ue5_prop,
```

---

### 3. TypeScript: Extend `Cheat` interface — `src/hooks/useTrainer.ts`

Add to the `Cheat` interface:

```typescript
export interface Cheat {
  // ... all existing fields ...

  // ue5_prop-specific fields:
  ue5GObjectsAob?: string;
  ue5GNamesAob?: string;
  ue5ClassName?: string;
  ue5PropertyOffset?: number;
  offValue?: number;
}
```

Update the `type` union:
```typescript
type: 'toggle' | 'action' | 'patch' | 'scan' | 'mono' | 'mono_chain' | 'ue5_prop';
```

---

### 4. TypeScript: Address resolution — `src/hooks/useTrainer.ts`

In `resolveCheatAddress`, add a new branch after the `mono_chain` case:

```typescript
} else if (cheat.type === 'ue5_prop') {
  finalAddr = await invoke<string>('resolve_ue5_prop', {
    pid: pidRef.current,
    moduleName: cheat.module,
    gobjectsAob: cheat.ue5GObjectsAob ?? '',
    gnamesAob: cheat.ue5GNamesAob ?? '',
    className: cheat.ue5ClassName ?? '',
    propertyOffset: cheat.ue5PropertyOffset ?? 0,
  });
}
```

Note: `ue5_prop` addresses are NOT cached (same as `mono_chain`) because the player pawn pointer changes between sessions and on respawn. Remove `ue5_prop` from the cache guard at the top of `resolveCheatAddress`:

```typescript
// existing line — extend the exclusion:
if (cheat.type !== 'mono_chain' && cheat.type !== 'ue5_prop' && addressCache.current[cheat.id]) {
  return addressCache.current[cheat.id];
}
```

---

### 5. TypeScript: Poll loop — `src/hooks/useTrainer.ts`

The poll loop (around line 148) already handles `toggle` and `mono_chain` for active cheats. Extend the condition to include `ue5_prop`:

```typescript
// existing condition (line ~149):
if ((cheat.type === 'toggle' || cheat.type === 'mono_chain') && cheat.active && cheat.valueType) {

// becomes:
if ((cheat.type === 'toggle' || cheat.type === 'mono_chain' || cheat.type === 'ue5_prop') && cheat.active && cheat.valueType) {
```

---

### 6. TypeScript: Toggle off with offValue — `src/hooks/useTrainer.ts`

In `applyCheat`, find the section that handles toggling off (sets `willBeActive = false`). After the existing patch/write logic, add a branch for `ue5_prop` that writes `offValue` when disabling:

```typescript
if (cheat.type === 'patch') {
  const bytes = willBeActive ? cheat.onBytes : cheat.offBytes;
  await invoke('patch_bytes', { pid, address: hexAddr, bytes });
} else if (cheat.type === 'ue5_prop' && !willBeActive && cheat.offValue !== undefined) {
  await invoke(memCmd('write', cheat.valueType), {
    pid,
    address: hexAddr,
    value: cheat.offValue,
  });
} else if (willBeActive) {
  await invoke(memCmd('write', cheat.valueType), {
    pid,
    address: hexAddr,
    value: resolveWriteValue(cheat, customValueStr),
  });
}
```

---

### 7. UI — `src/App.tsx`

`ue5_prop` cheats render as toggle switches (same as `toggle`). No changes needed to the render logic — `ue5_prop` falls through to the default switch branch automatically.

Exclude `ue5_prop` from live value display (the pawn pointer re-resolves each tick; showing the raw numeric value of a bool is not useful):

```tsx
// existing line (around line 183):
{cheat.type !== 'scan' && cheat.type !== 'mono' && (

// becomes:
{cheat.type !== 'scan' && cheat.type !== 'mono' && cheat.type !== 'ue5_prop' && (
```

Add a CSS badge in `src/App.css`:

```css
.cheat-type-ue5_prop { background: #1a2a1a; color: #86efac; }
```

---

## Offset Calibration

All `Ue5Offsets` defaults are starting points. Verify against UE4SS output for BL4 (UE 5.5.4.1):

| Field | How to verify |
|---|---|
| `uobject_class` | In UE4SS Live View, note the byte offset of the class pointer in any object |
| `uclass_children` | Check the offset of the first FProperty in a known class |
| `ffield_next` | Follow two consecutive properties and verify the Next pointer offset |
| `fproperty_offset_internal` | Cross-check `ue5PropertyOffset` in JSON against what UE4SS reports |
| `gobjects_chunk_size` | UE4SS will report number of objects; divide by chunks to confirm |
| GNames layout | Resolve a known FName (e.g. index 0 = "None") and confirm string reads correctly |

If `find_object_by_class` returns null for a known-valid class name, the most likely culprits are `uobject_name`, `fname_entry_header`, or `fname_stride`. Log the raw bytes at those offsets and compare against UE4SS's reported name for the same object.

---

## Acceptance Criteria

- `resolve_ue5_prop` returns the correct address for `bCanBeDamaged` on `BP_OakPlayerCharacter_C` in a live BL4 session
- Toggling God Mode ON writes `0` to that address; the player takes no damage
- Toggling God Mode OFF writes `1`; the player takes damage normally
- The cheat re-resolves on each poll tick (player pawn can change on respawn without requiring trainer restart)
- `cargo build` compiles with no errors; no TypeScript errors
- The `ue5_prop` badge renders correctly in the UI
- Existing cheat types (`toggle`, `patch`, `mono_chain`, `scan`) are unaffected

## Notes

- **`bool` in UE5**: `bCanBeDamaged` is a `bool` stored as a single byte (0 or 1). Use `valueType: "int"` and `write_int` — Magic Wand's `write_int` writes a 4-byte int, which will overwrite the bool and 3 adjacent bytes. This is safe here (the adjacent bytes are padding or a bitfield that UE5 handles). If precise single-byte writes are needed in future, a `write_byte` Tauri command should be added.
- **Multiple instances**: `find_object_by_class` returns the FIRST matching object. In co-op, multiple player pawns exist. A future `ue5PlayerOnly` flag could filter by checking if the object's outer chain contains a `PlayerController` associated with a `LocalPlayer`. For single-player BL4, the first match is always the local player.
- **GObjects AOB source**: UE4SS logs the GObjects address on startup. Compare that address against the module base to derive the RIP-relative displacement, then work backward to find the instruction pattern. Alternatively, search for the standard UE5 GObjects access pattern: `48 8B 05 ?? ?? ?? ?? 48 85 C0` in the module.
- **`engine::get_module_info` return type**: The function currently returns `(base: usize, size: usize)`. Confirm this is already the case in `src-tauri/src/engine.rs` before using `size` in the AOB scan range call.
