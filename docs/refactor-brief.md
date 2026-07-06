# Magic Wand Refactor Brief

**Repo:** `C:\Users\renga\Claude\MagicWand`  
**Stack:** Tauri v2 (Rust backend) + React/TypeScript frontend  
**Purpose:** Simplify the codebase, remove duplication, fix latent correctness bugs introduced during iterative game-trainer development.

---

## Priority 1 — Correctness Bugs (fix before anything else)

### 1a. `CAVE_STATE` single-slot overwrites on multiple code caves

**File:** `src-tauri/src/lib.rs:11`

```rust
// Current — single slot, wrong for >1 code_cave cheat
static CAVE_STATE: Mutex<Option<CaveEntry>> = Mutex::new(None);
struct CaveEntry { cave_addr: usize }
```

If two `code_cave` cheats are enabled, the second `enable_code_cave` call overwrites the first's `cave_addr`. When either cheat is disabled, `CAVE_STATE.take()` fires and frees the wrong (or no) allocation.

**Fix:** Change to a map keyed by cheat ID:

```rust
static CAVE_STATE: Mutex<HashMap<String, CaveEntry>> = Mutex::new(HashMap::new());
```

Pass `cheat_id: String` from TypeScript in both `enable_code_cave` and `disable_code_cave`. Insert on enable, remove+free on disable.

---

### 1b. `enable_code_cave` discards original bytes — caller must remember them

**Files:** `src-tauri/src/lib.rs:387`, `src/hooks/useTrainer.ts:311-324`

The `_site_original` parameter in `enable_code_cave` is prefixed with `_` (unused). The TypeScript caller stores the original bytes in the JSON definition and passes them back to `disable_code_cave`. This means the original bytes must be correct in the JSON forever — there is no runtime verification.

**Fix:** Have `enable_code_cave` read the current bytes from the process at `site_addr` before patching, and store them in `CaveEntry`:

```rust
struct CaveEntry {
    cave_addr: usize,
    site_addr: usize,
    original_bytes: Vec<u8>,
}
```

Remove `site_original` from both command signatures. `disable_code_cave` reads `entry.original_bytes` from state instead of accepting them as a parameter. Update the TypeScript side accordingly — `siteOriginal` can be removed from the `Cheat` interface and the JSON trainer files.

---

## Priority 2 — Largest Duplication

### 2a. VirtualQueryEx page-walking loop copied 7 times

**File:** `src-tauri/src/engine.rs`

The same loop — open handle, check `MEM_COMMIT`, skip guarded/reserved pages, read chunk — appears in:

- `aob_scan_range` (~lines 79-123)
- `aob_scan_all_range` (~lines 136-182)
- `scan_memory_for_bytes` (~lines 240-277)
- `dump_module_to_file` (~lines 411-436)
- `read_module_strings` (~lines 446-486)
- `snapshot_all_pages` (~lines 569-606)
- `snapshot_executable_pages` (~lines 608-642)

**Fix:** Extract a single traversal helper:

```rust
fn walk_committed_regions(
    pid: u32,
    base: usize,
    size: usize,
    exec_only: bool,
) -> Vec<(usize, Vec<u8>)>
```

Returns a list of `(region_base, bytes)` tuples. The `exec_only` flag gates on `PAGE_EXECUTE*` protect flags. All 7 functions become consumers of this iterator. Estimated reduction: ~150-200 lines.

Once this exists, `aob_scan_range` becomes:

```rust
pub fn aob_scan_range(pid: u32, base: usize, size: usize, pattern: &str) -> Option<usize> {
    aob_scan_all_range(pid, base, size, pattern).into_iter().next()
}
```

---

### 2b. `snapshot_module` and `snapshot_by_module_name` are identical

**File:** `src-tauri/src/lib.rs` (~lines 196-209 and 613-625)

Both functions call `engine::snapshot_executable_pages`, write to `MODULE_SNAPSHOT`, and return a formatted status string. The only difference is minor wording in the return message.

**Fix:** Delete one. Update `generate_handler![]` and any TypeScript callers accordingly.

---

### 2c. App.tsx is a 1305-line god component

**File:** `src/App.tsx`

~930 of those lines are the dev panel (lines 328-1256), which also carries 6 pieces of state at the `App` level that are dead weight when `devMode` is false:

```ts
const [diffStatus, setDiffStatus] = useState<string>('');
const [diffResults, setDiffResults] = useState<string[]>([]);
const [dumpAddr, setDumpAddr] = useState<string>('');
const [classKeyword, setClassKeyword] = useState<string>('');
const [snapTarget, setSnapTarget] = useState<string>('');
const [resultsFilter, setResultsFilter] = useState<string>('');
const dropRateInterval = useRef<...>(null);
const [dropRateActive, setDropRateActive] = useState(false);
```

Plus two functions (`applyDropRatePatch`, `applyLegendaryPatch`) that are only called from inside the dev panel.

**Fix:** Split into three files:

- `src/components/TrainerDashboard.tsx` — the active-game cheat list (lines 221-327)
- `src/components/DevPanel.tsx` — the entire devMode block (lines 328-1256) with its own local state
- `src/App.tsx` — reduced to navigation, library view, page routing, and game selection

`DevPanel` accepts `pid`, `activeGame`, and `snapTarget`/module input as props or manages them internally.

---

## Priority 3 — Smaller Repeated Patterns

### 3a. `require_module` helper missing from lib.rs

**File:** `src-tauri/src/lib.rs`

This pattern appears ~13 times:

```rust
let (base, size) = engine::get_module_info(pid, &module_name)
    .ok_or_else(|| format!("Module '{}' not found", module_name))?;
```

**Fix:** Add to `lib.rs` (or `engine.rs`):

```rust
fn require_module(pid: u32, name: &str) -> Result<(usize, usize), String> {
    engine::get_module_info(pid, name)
        .ok_or_else(|| format!("Module '{}' not found", name))
}
```

---

### 3b. `read_ptr` and `read_u32` defined in two modules

**Files:** `src-tauri/src/mono.rs:3-10`, `src-tauri/src/ue5.rs:34-42`

Both modules define identical private helpers:

```rust
fn read_ptr(pid: u32, addr: usize) -> Option<usize> { ... }
fn read_u32(pid: u32, addr: usize) -> Option<u32> { ... }
```

**Fix:** Move both to `engine.rs` as `pub fn`. Delete from `mono.rs` and `ue5.rs`.

---

### 3c. `spawn_blocking` boilerplate repeated 13 times

**File:** `src-tauri/src/lib.rs`

Every async command wraps its body the same way:

```rust
tauri::async_runtime::spawn_blocking(move || { ... })
    .await
    .map_err(|e| e.to_string())?
```

**Fix:** A local macro:

```rust
macro_rules! blocking {
    ($body:expr) => {
        tauri::async_runtime::spawn_blocking(move || $body)
            .await
            .map_err(|e| e.to_string())?
    };
}
```

---

### 3d. `bl4Prop()` inline helper missing from dev panel

**File:** `src/App.tsx` (dev panel section)

`invoke('resolve_ue5_prop', { pid, moduleName: 'Borderlands4.exe', gobjectsAob: '', gnamesAob: '', gobjectsOffset: 0x11765A30, gnamesOffset: 0x1167FDD0, propertyOffset: 0, extraOffsets: null, className: ... })` is copy-pasted ~12 times.

**Fix:** Add at the top of `DevPanel.tsx`:

```ts
const bl4Prop = (className: string) =>
  invoke<string>('resolve_ue5_prop', {
    pid,
    moduleName: 'Borderlands4.exe',
    gobjectsAob: '',
    gnamesAob: '',
    gobjectsOffset: 0x11765A30,
    gnamesOffset: 0x1167FDD0,
    className,
    propertyOffset: 0,
    extraOffsets: null,
  });
```

---

### 3e. 8-byte little-endian BigInt assembly repeated 7+ times

**File:** `src/App.tsx` (dev panel section)

```ts
const lo = BigInt(b[0]) | (BigInt(b[1]) << 8n) | (BigInt(b[2]) << 16n) | (BigInt(b[3]) << 24n);
const hi = BigInt(b[4]) | (BigInt(b[5]) << 8n) | (BigInt(b[6]) << 16n) | (BigInt(b[7]) << 24n);
const ptr = (hi << 32n) | lo;
```

**Fix:** Add to a `src/utils/memory.ts` file:

```ts
export function parseLePtr64(hexBytes: string[]): bigint {
  const b = hexBytes.map(h => parseInt(h, 16));
  const lo = BigInt(b[0]) | (BigInt(b[1]) << 8n) | (BigInt(b[2]) << 16n) | (BigInt(b[3]) << 24n);
  const hi = BigInt(b[4]) | (BigInt(b[5]) << 8n) | (BigInt(b[6]) << 16n) | (BigInt(b[7]) << 24n);
  return (hi << 32n) | lo;
}
```

---

## Priority 4 — Structural / Dead Code

### 4a. `scan_local` ships test code in release builds

**File:** `src-tauri/src/scanner.rs:132-172`

Scans for `dummy-game/target/debug/dummy-game.exe` on every game library refresh in production.

**Fix:** Wrap the function body and its call in `scan_all` with `#[cfg(debug_assertions)]`, or delete entirely.

---

### 4b. `MODULE_SNAPSHOT` is an unnamed tuple

**File:** `src-tauri/src/lib.rs:8`

```rust
static MODULE_SNAPSHOT: Mutex<Option<(String, usize, Vec<(usize, Vec<u8>)>)>> = Mutex::new(None);
```

Every consumer destructures as `(module_name, snap_base, regions)` without any field names.

**Fix:**

```rust
struct ModuleSnapshot {
    module_name: String,
    base: usize,
    regions: Vec<(usize, Vec<u8>)>,
}
static MODULE_SNAPSHOT: Mutex<Option<ModuleSnapshot>> = Mutex::new(None);
```

---

### 4c. `Ue5Offsets` is never constructed with non-default values

**File:** `src-tauri/src/ue5.rs`

`Ue5Offsets::ue5_default()` is the only call site. The struct is `pub` with public fields but nothing outside the module varies them.

**Fix (option A):** Make it a private constant and remove the struct:
```rust
const UE5: Ue5Offsets = Ue5Offsets { fuobjectitem_size: 24, ... };
```

**Fix (option B):** Keep the struct but add a Tauri command accepting custom offsets, enabling multi-game UE5 support without hardcoded values.

---

### 4d. `write_memory` vs `patch_memory` are indistinguishable by signature

**File:** `src-tauri/src/engine.rs`

`write_memory` calls `WriteProcessMemory` directly (fails on protected pages). `patch_memory` calls `VirtualProtectEx` first. The names don't communicate this. Callers in `lib.rs` use both interchangeably; some write to code sections using `write_memory` and silently fail.

**Fix:** Rename to make intent explicit:
- `write_memory` → `write_memory_raw` (data sections, caller guarantees writeability)
- `patch_memory` → `write_memory_rw` (code sections, forces RW then restores protection)

---

### 4e. Polling loop reads `code_patch`/`code_cave` cheats unnecessarily

**File:** `src/hooks/useTrainer.ts:216-218`

The interval filter is `c.valueType != null`, which includes `code_patch` and `code_cave` cheats. Their live values are never displayed (explicitly excluded in `App.tsx:238`), so the reads are wasted.

**Fix:**

```ts
.filter(c => c.valueType != null && c.type !== 'code_patch' && c.type !== 'code_cave')
```

---

### 4f. `ue5GObjectsOffset` is `string` in TypeScript, `usize` in Rust

**File:** `src/hooks/useTrainer.ts:31`, `src-tauri/src/lib.rs` (`resolve_ue5_prop` command)

The field is declared as `string` in the `Cheat` interface and parsed with `parseInt(..., 16)` at the call site. The Rust command accepts `Option<usize>`.

**Fix:** Declare the field as `number` in the TypeScript interface. Store numeric values in the trainer JSON. Eliminates the `parseInt` at the call site.

---

## Summary Table

| # | Item | File(s) | Effort | Impact |
|---|---|---|---|---|
| 1a | Multi-cave `CAVE_STATE` HashMap | lib.rs | Small | Correctness fix |
| 1b | Store original bytes in `CaveEntry` | lib.rs, useTrainer.ts | Small | Correctness fix + simpler API |
| 2a | Extract `walk_committed_regions` | engine.rs | Medium | ~200 lines removed |
| 2b | Delete duplicate snapshot command | lib.rs | Trivial | ~13 lines |
| 2c | Extract `TrainerDashboard` + `DevPanel` | App.tsx | Medium | ~950 lines moved out of App |
| 3a | `require_module` helper | lib.rs | Trivial | 13 repetitions collapsed |
| 3b | Move `read_ptr`/`read_u32` to engine.rs | mono.rs, ue5.rs | Trivial | ~20 lines, no more duplication |
| 3c | `blocking!` macro | lib.rs | Trivial | Noise reduction across 13 commands |
| 3d | `bl4Prop()` helper | App.tsx / DevPanel.tsx | Trivial | 12 copy-pastes → 1 |
| 3e | `parseLePtr64` utility | App.tsx / utils/memory.ts | Trivial | 7 copy-pastes → 1 |
| 4a | Gate `scan_local` on debug | scanner.rs | Trivial | Test code out of release |
| 4b | Name the `MODULE_SNAPSHOT` tuple | lib.rs | Trivial | Readability |
| 4c | Collapse or promote `Ue5Offsets` | ue5.rs | Small | Remove false configurability |
| 4d | Rename `write_memory`/`patch_memory` | engine.rs, lib.rs | Small | Prevent silent write failures |
| 4e | Fix polling filter | useTrainer.ts | Trivial | Wasted reads per tick |
| 4f | `ue5GObjectsOffset` as `number` | useTrainer.ts, JSON files | Trivial | Type mismatch at boundary |

Items 1a and 1b are the only ones that affect runtime correctness. Everything else is cleanup with no user-visible behavior change.
