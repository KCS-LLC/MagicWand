# Brief: Runtime Value Scanner

## Status
Unimplemented.

## Problem
Magic Wand currently requires a fixed AOB signature or pointer chain to locate a value in memory. This works well for native C++ games (Skyrim, etc.) where addresses are stable. It does not work for Unity/C# games like Cell to Singularity, where all game values live in the Mono garbage-collected heap — a region that relocates entirely on every game launch. No static pointer chain survives a restart, so there is nothing to put in a trainer JSON.

## Goal
Add a `"scan"` cheat type that locates a value at runtime by scanning the process's memory for it, the same way Cheat Engine's basic scan works. The user enters what their current value is, Magic Wand finds the address live, then writes the target value. The found address is cached for the rest of the session.

This makes Magic Wand capable of targeting any game that stores numeric values in readable memory, regardless of engine.

## New Cheat Type

Trainer JSON uses `"type": "scan"`:

```json
{
  "id": "cts-darwinium",
  "name": "Darwinium",
  "type": "scan",
  "valueType": "double",
  "onValue": 1000
}
```

- `valueType` — `"int"`, `"float"`, or `"double"` (determines byte width searched)
- `onValue` — the value written once the address is located
- No `signature`, `module`, `base`, or `offsets` fields needed

---

## Implementation

### 1. Rust: Memory Scanner — `src-tauri/src/engine.rs`

Add a function that enumerates all readable committed memory regions via `VirtualQueryEx`, reads each one, and searches for the target value's bytes.

```rust
use windows::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT,
    PAGE_NOACCESS, PAGE_GUARD, PAGE_PROTECTION_FLAGS,
};

pub fn scan_memory_for_bytes(pid: u32, needle: &[u8]) -> Result<Vec<usize>, String> {
    unsafe {
        let handle = OpenProcess(PROCESS_ALL_ACCESS, false, pid)
            .map_err(|e| format!("Failed to open process: {}", e))?;

        let mut results = Vec::new();
        let mut address: usize = 0;

        loop {
            let mut mbi = MEMORY_BASIC_INFORMATION::default();
            let ret = VirtualQueryEx(
                handle,
                Some(address as *const _),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            );
            if ret == 0 { break; }

            let next = mbi.BaseAddress as usize + mbi.RegionSize;

            // Only scan committed, readable, non-guarded regions
            let unreadable = PAGE_NOACCESS.0 | PAGE_GUARD.0;
            let is_committed = mbi.State == MEM_COMMIT;
            let is_readable = (mbi.Protect.0 & unreadable) == 0;

            if is_committed && is_readable && mbi.RegionSize <= 512 * 1024 * 1024 {
                if let Ok(data) = read_memory(pid, mbi.BaseAddress as usize, mbi.RegionSize) {
                    for i in 0..data.len().saturating_sub(needle.len()) {
                        if data[i..i + needle.len()] == *needle {
                            results.push(mbi.BaseAddress as usize + i);
                        }
                    }
                }
            }

            address = next;
            if address == 0 { break; } // wrapped around
        }

        let _ = CloseHandle(handle);
        Ok(results)
    }
}
```

Add three typed convenience wrappers:

```rust
pub fn scan_for_int(pid: u32, value: i32) -> Result<Vec<usize>, String> {
    scan_memory_for_bytes(pid, &value.to_le_bytes())
}

pub fn scan_for_float(pid: u32, value: f32) -> Result<Vec<usize>, String> {
    scan_memory_for_bytes(pid, &value.to_le_bytes())
}

pub fn scan_for_double(pid: u32, value: f64) -> Result<Vec<usize>, String> {
    scan_memory_for_bytes(pid, &value.to_le_bytes())
}
```

> The 512 MB region size cap skips memory-mapped files and massive heaps that are unlikely to contain game state values.

---

### 2. Rust: Tauri Command — `src-tauri/src/lib.rs`

```rust
#[tauri::command]
fn scan_value(pid: u32, value_type: String, value: f64) -> Result<Vec<String>, String> {
    let addresses = match value_type.as_str() {
        "double" => engine::scan_for_double(pid, value)?,
        "float"  => engine::scan_for_float(pid, value as f32)?,
        "int"    => engine::scan_for_int(pid, value as i32)?,
        _        => return Err(format!("Unknown value type: {}", value_type)),
    };
    Ok(addresses.iter().map(|a| format!("0x{:X}", a)).collect())
}
```

Register in `invoke_handler`:
```rust
scan_value,
```

---

### 3. TypeScript: Extend `Cheat` interface — `src/hooks/useTrainer.ts`

```typescript
export interface Cheat {
  id: string;
  name: string;
  type: 'toggle' | 'action' | 'patch' | 'scan'; // add 'scan'
  valueType?: 'int' | 'float' | 'double';
  module: string;
  // ... existing fields unchanged
}
```

---

### 4. TypeScript: Scan state — `src/App.tsx`

Add scan state alongside `customValues`:

```typescript
interface ScanState {
  status: 'idle' | 'scanning' | 'found' | 'multiple' | 'not_found';
  addresses: string[];
  cachedAddress?: string;
}

const [scanStates, setScanStates] = useState<Record<string, ScanState>>({});
const [scanInputs, setScanInputs] = useState<Record<string, string>>({});
```

Reset both when changing games:
```typescript
// In handleGameClick / selectGame, also call:
setScanStates({});
setScanInputs({});
```

---

### 5. TypeScript: Scan handler — `src/App.tsx`

```typescript
const handleScan = async (cheat: Cheat) => {
  if (!pid) return;
  const currentValueStr = scanInputs[cheat.id];
  if (!currentValueStr) return;

  const value = cheat.valueType === 'int'
    ? parseInt(currentValueStr, 10)
    : parseFloat(currentValueStr);

  if (isNaN(value)) return;

  setScanStates(prev => ({ ...prev, [cheat.id]: { status: 'scanning', addresses: [] } }));

  try {
    const addresses = await invoke<string[]>('scan_value', {
      pid,
      valueType: cheat.valueType ?? 'int',
      value,
    });

    if (addresses.length === 0) {
      setScanStates(prev => ({ ...prev, [cheat.id]: { status: 'not_found', addresses: [] } }));
    } else if (addresses.length === 1) {
      setScanStates(prev => ({ ...prev, [cheat.id]: { status: 'found', addresses, cachedAddress: addresses[0] } }));
    } else {
      setScanStates(prev => ({ ...prev, [cheat.id]: { status: 'multiple', addresses } }));
    }
  } catch (err) {
    setScanStates(prev => ({ ...prev, [cheat.id]: { status: 'not_found', addresses: [] } }));
  }
};

const handleScanWrite = async (cheat: Cheat) => {
  if (!pid) return;
  const state = scanStates[cheat.id];
  if (!state?.cachedAddress) return;

  const cmd = cheat.valueType === 'double' ? 'write_double'
            : cheat.valueType === 'float'  ? 'write_float'
            : 'write_int';
  await invoke(cmd, { pid, address: state.cachedAddress, value: cheat.onValue });
};
```

---

### 6. UI: Scan cheat render — `src/App.tsx`

Inside the cheat list, add a branch for `type === 'scan'`:

```tsx
{cheat.type === 'scan' ? (
  <div className="scan-cheat">
    <div className="scan-row">
      <input
        className="value-input"
        type="number"
        placeholder="Current value in game"
        value={scanInputs[cheat.id] ?? ''}
        onChange={e => setScanInputs(prev => ({ ...prev, [cheat.id]: e.target.value }))}
        disabled={!pid || scanStates[cheat.id]?.status === 'scanning'}
      />
      <button
        className="fire-button"
        onClick={() => handleScan(cheat)}
        disabled={!pid || !scanInputs[cheat.id]}
      >
        {scanStates[cheat.id]?.status === 'scanning' ? 'Scanning...' : 'Scan'}
      </button>
    </div>
    {scanStates[cheat.id]?.status === 'found' && (
      <div className="scan-result found">
        ✓ Found. <button className="fire-button" onClick={() => handleScanWrite(cheat)}>Set to {cheat.onValue}</button>
      </div>
    )}
    {scanStates[cheat.id]?.status === 'multiple' && (
      <div className="scan-result multiple">
        {scanStates[cheat.id].addresses.length} matches — change the value in-game then scan again
      </div>
    )}
    {scanStates[cheat.id]?.status === 'not_found' && (
      <div className="scan-result not-found">Not found — check the value and try again</div>
    )}
  </div>
) : /* existing toggle/action/patch render */ }
```

---

### 7. CSS — `src/App.css`

```css
.scan-cheat {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.scan-row {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

.scan-result {
  font-size: 0.8rem;
  padding: 0.25rem 0.5rem;
  border-radius: 0.375rem;
}

.scan-result.found {
  color: #4ade80;
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.scan-result.multiple {
  color: #f97316;
}

.scan-result.not-found {
  color: #ef4444;
}
```

---

## Cell to Singularity Trainer JSON

Once this is implemented, create `public/trainers/cell-to-singularity.json`:

```json
{
  "name": "Cell to Singularity",
  "executable": "CellToSingularity.exe",
  "cheats": [
    {
      "id": "cts-darwinium",
      "name": "Darwinium",
      "type": "scan",
      "valueType": "double",
      "module": "",
      "offsets": [],
      "onValue": 1000
    }
  ]
}
```

User flow:
1. Open Magic Wand → select Cell to Singularity
2. Note current Darwinium amount (e.g. `47`)
3. Type `47` in the scan input → click **Scan**
4. If multiple results: spend/earn Darwinium, update the input, scan again
5. Once found: click **Set to 1000**

## Acceptance Criteria
- `scan_value` Tauri command returns correct addresses for a known double value in a live process
- Scan cheat UI shows scanning state, found/multiple/not-found feedback
- "Set to 1000" button appears only after a single address is confirmed
- Multiple-match state prompts user to change value and rescan
- Found address is cached — user doesn't need to rescan unless they deselect the game
- `cargo build` compiles with no errors; no TypeScript errors
- Existing cheat types (toggle, action, patch) are unaffected

## Notes
- The 512 MB region cap in `scan_memory_for_bytes` skips pathological regions. Lower it to 256 MB if scans are too slow.
- `f64::to_le_bytes()` on a whole number like `47.0` produces a specific 8-byte pattern. Even one Darwinium decimal (e.g. `47.3`) will not match a scan for `47.0` — the user must enter their exact current value. Consider displaying a note in the UI: "Enter the exact whole number shown in-game."
- This feature intentionally does NOT persist the address between game sessions — that is the correct behavior for managed-heap games.
- The `module` field in the trainer JSON is left empty (`""`) for scan cheats — it is unused. A future refactor could make the field optional in the interface, but that is out of scope here.
