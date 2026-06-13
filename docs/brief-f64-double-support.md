# Brief: Add f64 (Double) Read/Write Support

## Status
Unimplemented — engine only supports f32 (float) and i32 (int).

## Problem
Some games (including Cell to Singularity) store numeric values as 64-bit doubles (`f64`) rather than 32-bit floats (`f32`) or 32-bit integers (`i32`). Writing a 32-bit value to a double address corrupts the adjacent 4 bytes of memory, which can crash the game or produce garbage values. There is currently no way to correctly read or write double-precision values in Magic Wand.

## Goal
Add `read_double` and `write_double` Tauri commands to the Rust backend, and wire up `'double'` as a valid `valueType` throughout the TypeScript layer so trainer JSON files can target double-precision memory values.

## Scope
This is a contained, additive change. No existing commands are modified — only new ones are added. The change touches four locations:

1. `src-tauri/src/engine.rs` — two new public functions
2. `src-tauri/src/lib.rs` — two new Tauri command wrappers + registration
3. `src/hooks/useTrainer.ts` — `valueType` union, poll loop, write dispatch
4. Trainer JSON files (when authored) — use `"valueType": "double"`

---

## Implementation

### 1. `src-tauri/src/engine.rs`

Add after the existing `write_memory` function. No other changes to this file.

```rust
pub fn read_double(pid: u32, address: usize) -> Result<f64, String> {
    let data = read_memory(pid, address, 8)?;
    data.try_into()
        .map(f64::from_le_bytes)
        .map_err(|_| "Failed to read 8 bytes for double".to_string())
}

pub fn write_double(pid: u32, address: usize, value: f64) -> Result<(), String> {
    write_memory(pid, address, &value.to_le_bytes())
}
```

> These intentionally delegate to the existing `read_memory`/`write_memory` functions so all handle/close logic stays in one place.

---

### 2. `src-tauri/src/lib.rs`

Add two new command functions alongside the existing `read_float`/`write_float` pair:

```rust
#[tauri::command]
fn read_double(pid: u32, address: String) -> Result<f64, String> {
    let addr = parse_addr(&address)?;
    engine::read_double(pid, addr as usize)
}

#[tauri::command]
fn write_double(pid: u32, address: String, value: f64) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_double(pid, addr as usize, value)
}
```

Register them in the `invoke_handler` macro:

```rust
.invoke_handler(tauri::generate_handler![
    scan_games,
    find_game,
    get_module_base,
    aob_scan,
    resolve_pointer,
    read_int,
    read_float,
    read_double,   // ADD
    write_int,
    write_float,
    write_double,  // ADD
    patch_bytes
])
```

---

### 3. `src/hooks/useTrainer.ts`

#### 3a. Extend the `valueType` union in the `Cheat` interface (line 8)

```typescript
// Before
valueType?: 'int' | 'float';

// After
valueType?: 'int' | 'float' | 'double';
```

#### 3b. Poll loop — command selection (inside the `setInterval` callback, ~line 170)

```typescript
// Before
const cmd = cheat.valueType === 'float' ? 'read_float' : 'read_int';

// After
const cmd =
  cheat.valueType === 'float'  ? 'read_float'  :
  cheat.valueType === 'double' ? 'read_double' :
  'read_int';
```

#### 3c. Write dispatch in `toggleCheat` / `applyCheat` (~line 210)

```typescript
// Before
const cmd = cheat.valueType === 'float' ? 'write_float' : 'write_int';

// After
const cmd =
  cheat.valueType === 'float'  ? 'write_float'  :
  cheat.valueType === 'double' ? 'write_double' :
  'write_int';
```

#### 3d. Live value display formatting (`App.tsx`, ~line 77)

The existing `toFixed(2)` call handles doubles correctly since JavaScript's `number` type is already f64. No change needed here.

---

## Trainer JSON Usage

Once implemented, trainer files can use:

```json
{
  "id": "cts-darwinium",
  "name": "Darwinium",
  "type": "action",
  "valueType": "double",
  "module": "CellToSingularity.exe",
  "signature": "...",
  "offsets": [],
  "onValue": 9999
}
```

The `onValue` field is a JSON number, which JavaScript reads as a native f64. Rust receives it as `f64` from the Tauri command — no conversion needed.

---

## Acceptance Criteria
- `read_double` returns the correct f64 value from a game process address
- `write_double` writes all 8 bytes correctly without corrupting adjacent memory
- A trainer cheat with `"valueType": "double"` polls and displays the live value
- Toggling or firing such a cheat writes the correct value
- Existing `float` and `int` cheats are unaffected
- `cargo build` succeeds with no warnings on the new functions

## Notes
- `f64::from_le_bytes` requires `[u8; 8]` — the `.try_into()` will only fail if `read_memory` returns fewer than 8 bytes (i.e. a bad address), which surfaces as a clean error string rather than a panic.
- JavaScript's `number` is already IEEE 754 double precision, so there is no precision loss when the value travels from Rust → JSON → TypeScript.
- This change is safe to ship independently before any Cell to Singularity trainer JSON is authored — it adds no new UI and does not alter existing behavior.
