# Brief: f64 (Double-Precision Float) Support

## Status
Unimplemented — the engine only reads and writes `f32` (4-byte single-precision floats). Games that store values as `f64` (8-byte doubles) cannot be targeted.

## Problem
`read_float` and `write_float` in `src-tauri/src/lib.rs` both operate on 4 bytes and interpret them as `f32`. Some games — particularly those built on 64-bit engines or using physics-accurate simulations — store health, position, speed, and other actor values as `f64`. Targeting those addresses with `f32` reads/writes produces garbage values or crashes.

The `Cheat` interface in `useTrainer.ts` already has `valueType: 'int' | 'float'` but no way to distinguish `f32` from `f64`, so trainer authors have no way to opt into double precision even if the backend supported it.

## Goal
- Add `read_double` and `write_double` Tauri commands that read/write 8 bytes as `f64`
- Extend `valueType` in the `Cheat` interface to include `'double'`
- Wire the frontend hook to dispatch `read_double`/`write_double` when `valueType === 'double'`
- No changes to existing `f32` behavior — this is purely additive

## Relevant Files
- `src-tauri/src/lib.rs` — `read_float`, `write_float`, `invoke_handler` registration
- `src/hooks/useTrainer.ts` — `Cheat` interface, `resolveCheatAddress`, polling loop, `applyCheat`
- `public/trainers/skyrim-se.json` / `public/trainers/dummy-game.json` — no changes needed; `'float'` stays `f32`

## Implementation Plan

### 1. Add Tauri commands in `lib.rs`

```rust
#[tauri::command]
fn read_double(pid: u32, address: String) -> Result<f64, String> {
    let addr = parse_addr(&address)?;
    let data = engine::read_memory(pid, addr as usize, 8)?;
    if data.len() == 8 {
        Ok(f64::from_le_bytes(data.try_into().unwrap()))
    } else {
        Err("Failed to read 8 bytes".to_string())
    }
}

#[tauri::command]
fn write_double(pid: u32, address: String, value: f64) -> Result<(), String> {
    let addr = parse_addr(&address)?;
    engine::write_memory(pid, addr as usize, &value.to_le_bytes())
}
```

Register both in `invoke_handler`:
```rust
tauri::generate_handler![
    // ... existing commands ...
    read_double,
    write_double,
]
```

### 2. Extend `Cheat.valueType` in `useTrainer.ts`

```typescript
export interface Cheat {
  // ...
  valueType?: 'int' | 'float' | 'double';
  // ...
}
```

### 3. Update the polling loop in `useTrainer.ts`

Replace the two-branch dispatch:
```typescript
const cmd = cheat.valueType === 'float' ? 'read_float' : 'read_int';
```
With a three-branch dispatch:
```typescript
function readCmd(valueType?: string) {
  if (valueType === 'float') return 'read_float';
  if (valueType === 'double') return 'read_double';
  return 'read_int';
}
// usage:
const val = await invoke<number>(readCmd(cheat.valueType), { pid: pidRef.current, address: hexAddr });
```

### 4. Update `resolveWriteValue` / `applyCheat` in `useTrainer.ts`

Replace the write dispatch:
```typescript
const cmd = cheat.valueType === 'float' ? 'write_float' : 'write_int';
```
With:
```typescript
function writeCmd(valueType?: string) {
  if (valueType === 'float') return 'write_float';
  if (valueType === 'double') return 'write_double';
  return 'write_int';
}
// usage in applyCheat (both action and toggle paths):
const cmd = writeCmd(cheat.valueType);
```

Also update `resolveWriteValue` so `double` parses as float (same as `float`):
```typescript
function resolveWriteValue(cheat: Cheat, customValueStr?: string): number {
  if (customValueStr !== undefined && customValueStr !== '') {
    const parsed = (cheat.valueType === 'float' || cheat.valueType === 'double')
      ? parseFloat(customValueStr)
      : parseInt(customValueStr, 10);
    if (!isNaN(parsed)) return parsed;
  }
  return cheat.onValue;
}
```

### 5. Update the live-value display in `App.tsx` (no change needed)

The `toFixed(2)` display branch already covers any `number` — `f64` values come back as JavaScript `number` just like `f32`. No UI change required.

## Example trainer JSON using `double`

```json
{
  "id": "some-game-speed",
  "name": "Movement Speed",
  "type": "toggle",
  "valueType": "double",
  "module": "game.exe",
  "signature": "48 8B 05 ?? ?? ?? ??",
  "offsets": ["0x2C0"],
  "onValue": 1.5
}
```

## Acceptance Criteria
- `read_double` and `write_double` Tauri commands exist and operate on 8 bytes
- `Cheat.valueType` accepts `'double'` without TypeScript errors
- Polling and write paths dispatch `read_double`/`write_double` when `valueType === 'double'`
- All existing `'float'` and `'int'` cheats behave identically to before
- No new Rust warnings or TypeScript errors introduced

## Notes
- JavaScript `number` is IEEE 754 double-precision, so passing an `f64` from Rust back to JS loses no precision beyond what JS already represents. No special handling needed on the frontend.
- The `engine::read_memory` and `engine::write_memory` functions are byte-level and already support arbitrary lengths — no engine changes needed.
- `f32` stays as `'float'` in the trainer schema; do not rename or migrate existing trainer JSON files.
