# Technical Handoff: Borderlands 4 UE5 GObjects Scan Issue

> **RESOLVED 2026-08-01 — kept for history, do not follow its "Next Steps" section.**
> This handoff's own two GObjects candidates (`0xC8CED30`/`0xC891720`) turned out to be
> wrong — neither contained gameplay pawn/controller data when tested. The actual
> GObjects RVA was `0xC891A48` (close to but distinct from the "Server" candidate here),
> found via a fast NumElements-match scan rather than the brute-force AOB approach this
> doc proposed (which, as written — a whole-module wildcard scan for a near-universal
> instruction sequence — hangs effectively forever; confirmed live, 30+ minutes with no
> result). The property-offset-shift theory in section 4 was also wrong: `AController::
> Pawn`, `bCanBeDamaged`, and `DamageDealtMultiplier` were all confirmed unchanged. The
> real bug was simply the wrong GObjects address plus an overly-generic class name match
> (`"PlayerController"` instead of `"OakPlayerController"`). See `[[project-overview]]`
> and `[[bl4-offset-discovery-lessons]]` memories, and `docs/bl4-drop-rates.md`, for the
> current confirmed state and the toolkit (`src-tauri/src/bin/`) built to redo this faster
> next time. Separately: this same session also replaced `src-tauri/src/main.rs` (the
> app's actual entry point) with a diagnostic tool, breaking the app entirely — unrelated
> to BL4, fixed by restoring the real entry point.

---

## 1. Environment & Target Context
* **Target Process**: `Borderlands4.exe` (PID `33928`)
* **Process Base Address**: `0x140000000` (ASLR disabled/fixed in the PE header on this system)
* **Engine Version**: Unreal Engine 5.5.4.1 (Gearbox Custom Fork)

---

## 2. Confirmed Global RVAs
Through real-time pattern scanning and memory sweeps, the following relative virtual addresses (RVAs) are confirmed active and functional:

* **GNames RVA**: `0xC7ABD90` (Decimal: `209370512`)
  * *Verification*: Decodes Index 0 as `"None"`. Successfully resolved Index 173536 as `'OakPlayerController'`.
* **GObjects Candidate RVAs**:
  * **RVA `0xC8CED30` (Decimal: `210562352`)**: Currently populated with `206,983` elements (typically Client-side context).
  * **RVA `0xC891720` (Decimal: `210313056`)**: Currently populated with `108,527` elements (typically Server-side context).

---

## 3. Implemented Engine Refactors (`src-tauri/src/ue5.rs`)

### A. Pointer Flags Masking (Tag Bypassing)
UE5.5 utilizes the lowest 3 bits of `UObject` and `UClass` pointers to store internal engine state flags (e.g., returning pointers like `0x14611CA16` instead of `0x14611CA10`).
* *Refactor*: Added pointer-masking math (`ptr &= !7`) inside `class_inherits_from`, `list_classes_by_keyword`, and `find_object_by_class` to clear these flags before dereferencing.

### B. SuperStruct Offset Shift (`0x30` → `0x40`)
A recursive class-hierarchy memory walk starting from a live class pointer showed that Epic Games restructured `UStruct` in UE5.5, adding 16 bytes of metadata and shifting `SuperStruct` from `0x30` to **`0x40`**.
* *Refactor*: Updated `SUPER_OFFSET` to `0x40` inside `class_inherits_from` to restore correct class-hierarchy traversal.
```rust
// Walking SuperStruct chain with offset 0x40 (Success!)
//   Step 0: Address: 0x7FF4DDF38AE0 | Class Name: 'PlayerController'
//   Step 1: Address: 0x7FF4DDF38D30 | Class Name: 'Controller'
//   Step 2: Address: 0x7FF4DDE4C840 | Class Name: 'Actor'
//   Step 3: Address: 0x7FF4DDE20000 | Class Name: 'Object'
```

### C. ObjectFlags CDO Filtering
Spawned live actors/controllers in Borderlands 4 carry `ObjectFlags` set to `0x4C87F6A8`. Since bit 5 (`0x20`) is set, any standard check against CDOs + Archetypes (`flags & 0x30 != 0`) will skip the live spawned controller.
* *Refactor*: Updated `find_object_by_class` to filter only Class Default Objects (`flags & 0x10 != 0`) to prevent skipping valid archetype-repurposed controllers.
```rust
// Skip only Class Default Objects — they have zeroed/default values.
if let Some(flags) = read_u32(pid, obj_ptr + 0x08) {
    if flags & 0x10 != 0 { continue; }
}
```

---

## 4. Current Blocker & Failure Point

### The Blocker: `Null pointer at 0x430B74D0 in pointer chain`
When activating the `ue5_prop` cheats (**Invincibility** or **One Hit Kill**), the engine successfully locates the `PlayerController` class object at `0x430B7100`. It then tries to resolve the pointer chain:
`PlayerController + PropertyOffset (976 / 0x3D0) -> Pawn + ue5Offsets[98] -> bCanBeDamaged`

The read at `0x430B7100 + 0x3D0` (`0x430B74D0`) returns `0` (Null pointer).

### Analysis of the Blocker:
1. **Property Offset Shift**: The weekly patch shifted the class member variables of `APlayerController`. A dump of `PlayerController` fields shows `APlayerCameraManager` has shifted to `0x278` (`632`), meaning `MyPawn` (which is historically defined near the camera manager in `APlayerController`) is no longer at `976` (`0x3D0`).
2. **Gameplay Context Array Isolation**:
   * Scanning GObjects array `0xC8CED30` (Client) for `Pawn` objects returned only 10 basic engine default classes (`GbxNavCharacter`, `Pawn`, etc.) and no game-specific actor instances (`BP_OakPlayerCharacter_C`).
   * Scanning GObjects array `0xC891720` (Server) also returned no spawned Pawns.
   * *Conclusion*: The game-specific player character pawn actors are spawned inside **another gameplay object pool**.

---

## 5. Next Steps for Developer

1. **Bruteforce GObjects Candidates**:
   Run the brute-force scanner written inside `src-tauri/src/main.rs` (which performs a whole-module scan for any candidate `TChunkedArray` matching GObjects structure, and walks them in real-time looking for classes containing `"OakPlayer"` or `"BP_Oak"`).
   * *Expected Outcome*: This will output the RVA of the true GObjects array that contains the gameplay character state.
2. **Calibrate Pawn Offset**:
   Once the correct gameplay-level `PlayerController` is resolved, dump its memory space to locate the offset of the active player character pawn (Pawn class RVA pointer).
3. **Calibrate `bCanBeDamaged` Offset**:
   Follow the resolved Pawn pointer to find the shifted offset of the `bCanBeDamaged` bool/bitfield inside the character instance.
