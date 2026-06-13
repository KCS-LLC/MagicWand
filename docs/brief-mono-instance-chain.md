# Brief: Mono Instance Chain Traversal

## Status
Unimplemented.

## Problem

The existing `mono` cheat type (brief-mono-reflection.md) only resolves **static fields** — fields stored directly in a class's vtable static data. Many games, including Cell to Singularity, hold their live currency values on a **singleton instance** whose pointer is stored in a static field on the class or its generic base class.

Cell to Singularity's Darwinium is stored at:
```
PerfectSingleton<Calculator>.instance   [static field on parent class → Calculator*]
  → Calculator._bank                    [instance field → inline Cry struct]
    → Cry.c                             [double at offset 0x30 within Cry struct]
```

`Calculator : PerfectSingleton<Calculator>` — Calculator inherits from the generic base, so the static `instance` field lives on the parent's MonoClass, reachable via Calculator's `parent` pointer in Mono metadata.

This brief adds a new `"mono_chain"` cheat type that follows this three-step chain without any per-session scanning.

---

## Discovered Field Layout (Cell to Singularity)

### Cry struct memory layout

`Cry` is a value type (struct) with the following fields in declaration order, giving this layout (BreakInfinity `BigDouble` = two doubles = 16 bytes):

```
Offset  Size  Field
0x00     8    _a (double, legacy backing)
0x08    16    backingA (BigDouble: mantissa + exponent doubles)
0x18     8    _b (double, legacy backing)
0x20    16    backingB (BigDouble: mantissa + exponent doubles)
0x30     8    c  (double) ← DARWINIUM
0x38     8    d  (double)
0x40     8    e  (double)
0x48     8    f  (double)
0x50     8    g  (double)
0x58     8    h  (double)
```

`Cry.c` is confirmed as Darwinium via the `Calculator.bank` setter:
```csharp
if (cry.c > 0.0) {
    Analytics.getSingleton().SetDarwiniumUserProp(value.c);
}
```

### Chain summary

| Step | What                            | How                                        |
|------|---------------------------------|--------------------------------------------|
| 1    | Find Calculator MonoClass       | Class hash search (existing)               |
| 2    | Walk to parent MonoClass        | Read `parent` pointer from MonoClass       |
| 3    | Find `instance` on parent       | Field search on parent class               |
| 4    | Read static vtable → ptr value  | Existing vtable traversal → dereference    |
| 5    | Find `_bank` offset on Calculator | Field search for instance field offset   |
| 6    | Add struct offset 0x30          | `instance_ptr + _bank_offset + 0x30`       |

---

## New Cheat Type JSON

```json
{
  "id": "cts-darwinium",
  "name": "Darwinium",
  "type": "mono_chain",
  "valueType": "double",
  "module": "mono-2.0-bdwgc.dll",
  "monoAssembly": "Assembly-CSharp",
  "monoNamespace": "",
  "monoClass": "Calculator",
  "monoStaticField": "instance",
  "monoStaticViaParent": true,
  "monoInstanceField": "_bank",
  "monoFinalOffset": 48,
  "offsets": [],
  "onValue": 1000000
}
```

Fields:
- `monoClass` — class whose parent holds the static singleton pointer
- `monoStaticField` — name of the static field on the parent that holds the instance pointer (`"instance"`)
- `monoStaticViaParent` — `true` means read the static field from the parent class, not `monoClass` itself
- `monoInstanceField` — name of the instance field on `monoClass` whose offset we need (`"_bank"`)
- `monoFinalOffset` — byte offset within the resolved value-type struct to the final double (`48` = `0x30` = Cry.c)
- `monoAssembly`, `monoNamespace`, `module` — same semantics as the `mono` type

---

## Implementation

### 1. Rust: Extend `MonoOffsets` — `src-tauri/src/mono.rs`

Add one new field to `MonoOffsets` and its default:

```rust
pub struct MonoOffsets {
    // ... all existing fields ...

    /// MonoClass: offset of MonoClass* parent pointer
    /// Typical Unity Mono 2019–2022 value: 0x28. Verify with a memory viewer if traversal fails.
    pub class_parent: usize,
}

impl MonoOffsets {
    pub fn unity_default() -> Self {
        MonoOffsets {
            // ... all existing defaults ...
            class_parent: 0x28,
        }
    }
}
```

### 2. Rust: Parent class helper — `src-tauri/src/mono.rs`

```rust
fn get_parent_class(pid: u32, klass: usize, off: &MonoOffsets) -> Option<usize> {
    let parent = read_ptr(pid, klass + off.class_parent)?;
    if parent == 0 { None } else { Some(parent) }
}
```

### 3. Rust: Instance field offset helper — `src-tauri/src/mono.rs`

The existing `find_field` returns a raw `u32` offset. For instance fields, this offset already includes the Mono object header (0x10 bytes: vtable ptr + monitor ptr), so adding it directly to the instance pointer gives the field address.

Add a wrapper that makes this explicit:

```rust
fn get_instance_field_offset(pid: u32, klass: usize, field_name: &str, off: &MonoOffsets) -> Result<usize, String> {
    let fi = find_field(pid, klass, field_name, off)
        .ok_or_else(|| format!("Field '{}' not found", field_name))?;
    Ok(fi.offset as usize)
}
```

### 4. Rust: Chain resolver — `src-tauri/src/mono.rs`

```rust
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

    // Step 1: Get the class that owns the static `instance` field
    let static_class = if via_parent {
        get_parent_class(pid, klass, &off)
            .ok_or_else(|| format!("No parent class found for '{}'", class_name))?
    } else {
        klass
    };

    // Step 2: Read the static field → instance pointer
    let static_fi = find_field(pid, static_class, static_field, &off)
        .ok_or_else(|| format!("Static field '{}' not found on parent class", static_field))?;
    let static_addr = get_static_field_address(pid, static_class, static_fi.offset, &off)
        .ok_or("Could not resolve static vtable for parent class")?;
    let instance_ptr = read_ptr(pid, static_addr)
        .ok_or("Static field pointer is null — game may not be fully loaded yet")?;
    if instance_ptr == 0 {
        return Err("Instance pointer is null — singleton not yet initialized".to_string());
    }

    // Step 3: Get the instance field offset on the original class
    let field_offset = get_instance_field_offset(pid, klass, instance_field, &off)?;

    // Step 4: Compute final address
    // instance_ptr + field_offset gives start of the value-type struct (_bank / Cry)
    // + final_offset gives the primitive field within that struct (Cry.c)
    Ok(instance_ptr + field_offset + final_offset)
}
```

### 5. Rust: Tauri command — `src-tauri/src/lib.rs`

```rust
#[tauri::command]
fn resolve_mono_chain(
    pid: u32,
    module_name: String,
    assembly: String,
    namespace: String,
    class_name: String,
    static_field: String,
    via_parent: bool,
    instance_field: String,
    final_offset: usize,
) -> Result<String, String> {
    let (mono_base, _) = engine::get_module_info(pid, &module_name)
        .ok_or_else(|| format!("Module '{}' not found in process", module_name))?;
    let addr = mono::resolve_mono_chain(
        pid, mono_base, &assembly, &namespace, &class_name,
        &static_field, via_parent, &instance_field, final_offset,
    )?;
    Ok(format!("0x{:X}", addr))
}
```

Register in `invoke_handler`:
```rust
resolve_mono_chain,
```

### 6. TypeScript: Extend `Cheat` interface — `src/hooks/useTrainer.ts`

Add new optional fields to the `Cheat` interface alongside the existing mono fields:

```typescript
export interface Cheat {
  // ... all existing fields ...

  // mono_chain-specific fields:
  monoStaticField?: string;       // static field on the class (or parent) holding instance ptr
  monoStaticViaParent?: boolean;  // true = look for static field on parent class
  monoInstanceField?: string;     // instance field whose offset we add to the instance ptr
  monoFinalOffset?: number;       // byte offset within value-type struct to the primitive
}
```

Update `resolveCheatAddress` to handle `mono_chain`:

```typescript
if (cheat.type === 'mono_chain') {
  finalAddr = await invoke<string>('resolve_mono_chain', {
    pid: pidRef.current,
    moduleName: cheat.module,
    assembly: cheat.monoAssembly ?? 'Assembly-CSharp',
    namespace: cheat.monoNamespace ?? '',
    className: cheat.monoClass ?? '',
    staticField: cheat.monoStaticField ?? 'instance',
    viaParent: cheat.monoStaticViaParent ?? false,
    instanceField: cheat.monoInstanceField ?? '',
    finalOffset: cheat.monoFinalOffset ?? 0,
  });
}
```

Update the `type` union in the `Cheat` interface:
```typescript
type: 'toggle' | 'action' | 'patch' | 'scan' | 'mono' | 'mono_chain';
```

### 7. UI — `src/App.tsx` and `src/App.css`

Treat `mono_chain` identically to `mono` in the UI — a Fire button that writes `onValue`, no live value display, no scan input.

In App.tsx, wherever `mono` is checked, extend to include `mono_chain`:

```tsx
// Live value display: skip for scan, mono, and mono_chain
{cheat.type !== 'scan' && cheat.type !== 'mono' && cheat.type !== 'mono_chain' && (
  <span className="live-value">...</span>
)}

// Render: mono and mono_chain → Fire button
{cheat.type === 'mono' || cheat.type === 'mono_chain' ? (
  <button className="fire-button" onClick={() => applyCheat(cheat)} disabled={!pid}>
    Set to {cheat.onValue}
  </button>
) : /* ... toggle/action/scan ... */}
```

Add a CSS badge for `mono_chain` in `src/App.css`:

```css
.cheat-type-mono_chain { background: #1a1a3a; color: #c4b5fd; }
```

### 8. Cell to Singularity trainer — `public/trainers/cell-to-singularity.json`

Replace the placeholder `mono` entry and keep the `scan` fallback:

```json
{
  "name": "Cell to Singularity",
  "executable": "CellToSingularity.exe",
  "cheats": [
    {
      "id": "cts-darwinium",
      "name": "Darwinium",
      "type": "mono_chain",
      "valueType": "double",
      "module": "mono-2.0-bdwgc.dll",
      "monoAssembly": "Assembly-CSharp",
      "monoNamespace": "",
      "monoClass": "Calculator",
      "monoStaticField": "instance",
      "monoStaticViaParent": true,
      "monoInstanceField": "_bank",
      "monoFinalOffset": 48,
      "offsets": [],
      "onValue": 1000000
    },
    {
      "id": "cts-darwinium-scan",
      "name": "Darwinium (scan fallback)",
      "type": "scan",
      "valueType": "double",
      "module": "",
      "offsets": [],
      "onValue": 1000000
    }
  ]
}
```

---

## Offset Calibration

If the chain resolves to a wrong address or null:

1. **Parent pointer offset** (`class_parent: 0x28`): Open Cheat Engine, attach to the game, open Mono dissector. Find the Calculator class. Compare the address CE shows for the class descriptor against the address `find_class` resolves. Step through offsets at +0x28, +0x30 looking for a valid pointer to a MonoClass that has `name = "PerfectSingleton\`1"`.

2. **Instance field offset**: The `_bank` offset returned by `find_field` must match what you'd calculate manually. In CE's Mono dissector, find the Calculator instance and look at its field list — `_bank` should show its byte offset from the object start.

3. **Cry.c offset** (`monoFinalOffset: 48`): Double-check by reading the Calculator instance in CE at `instance_ptr + _bank_offset` and confirming a `double` there matches the Darwinium value shown in-game. If not, increment by 8 (one field) until it matches.

A quick validation command — run from Tauri dev console once connected:
```
resolve_mono_chain(pid, "mono-2.0-bdwgc.dll", "Assembly-CSharp", "", "Calculator",
  "instance", true, "_bank", 48)
→ should return a hex address
read_double(pid, <that address>)
→ should return current Darwinium value shown in-game
```

---

## Acceptance Criteria

- `resolve_mono_chain` returns the correct address for Darwinium in a live Cell to Singularity session
- `read_double` at that address matches the value displayed in-game
- Firing the cheat writes `onValue` and the in-game value updates immediately
- No per-session scanning required — resolves fresh each launch from Mono metadata
- The `scan` fallback cheat remains functional and unaffected
- `cargo build` passes with no errors; no TypeScript errors
- `mono_chain` badge renders correctly in the UI

## Notes

- **`_bank` is a private field** — Mono metadata retains the raw field name regardless of C# access modifiers, so `find_field` will find it by name.
- **Value type vs reference type**: `_bank` is a `Cry` struct (value type), stored *inline* in the Calculator object — not a heap pointer. This is why we add the field offset directly and then add the struct offset, rather than dereferencing again.
- **Null check timing**: If the game is mid-load, the singleton `instance` pointer may be null. The resolver returns a clear error in that case. The frontend should retry on the next poll tick (existing behavior — `addressCache` is not populated on error).
- **Generic base class name**: In Mono's class hash, `PerfectSingleton<Calculator>` is stored as `PerfectSingleton\`1`. We don't need to search for it by name — we reach it via Calculator's `parent` pointer, which avoids generic class name complexity entirely.
- **`class_parent` offset verification**: If traversal fails, this is the most likely culprit. Log the parent pointer value and check it against CE's Mono dissector.
