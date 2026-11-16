# BL4 Drop Rate / Legendary — Session Brief

_Last updated: 2026-08-01_

---

## 2026-08-01 update — two 2026-07-31 game patches broke both RVAs below

BL4 shipped two patches back-to-back on 2026-07-31 (a weekly patch, then a same-day
hotfix). The binary got fully relinked — this wasn't just these two sites drifting, the
GObjects/GNames globals moved too (see `[[project-overview]]`). Status after
rediscovery:

- **Max Legendary Drop Rate: FIXED, confirmed in-game 2026-08-01.** New patch site RVA
  `0x3D9FE8E` (was `0x96C77FA`). Rediscovered by scanning for the full WeMod AoB
  fingerprint (`AND EAX,0x7FFF` + `CVTSI2SS` + `DIVSS[RIP+disp32]` + `MULSS[RDI]` +
  `LEA[RDI+0x118]` + `MOV[RDI+0x128]`) — see `src-tauri/src/bin/scan_bl4_rarity.rs`. The
  instruction layout at the new site is byte-length-identical to the old one (`AND EAX`
  5 bytes + `CVTSI2SS` 4 bytes + `DIVSS` 8 bytes = 17, matching `patchLen: 17`, with the
  DIVSS disp32 landing exactly at `ripFixup.relDispOffset: 13`), so `cavePayload`,
  `coreLen`, and `ripFixup` needed zero changes — only `patchSite` did.
  **Correction to the gate description below: it's `level >= 90` (a float compare against
  `[RDI]`), not `weight >= 90.0f`.** Same mechanism, the "weight" phrasing was imprecise.
  This means the cheat only forces legendary rolls in true endgame (level 90+) content —
  getting normal-rarity drops in earlier-level content is the gate working as designed,
  not a bug.
- **100% Drop Rate: STILL STALE, not yet re-fixed.** Live-read of `0x96CA774` after the
  patches showed neither the documented ON nor OFF byte pattern — some unrelated
  instruction now sits there. A dedicated AOB scan for the `XORPS XMM0,XMM0` +
  `CVTSI2SS` idiom (`src-tauri/src/bin/scan_bl4_droprate.rs`) returned 2000+ candidates
  with no strong discriminator to narrow them (unlike legendary's multi-instruction
  fingerprint) — needs a live iterative toggle-and-observe approach to identify the
  correct one, not a blind pattern guess. The two sites' old ~12KB relative distance did
  NOT carry over to the new build (checked directly — landed on an unrelated CALL
  instruction), so don't assume a fixed offset from the new legendary site either.
- A reusable diagnostic toolkit now lives in `src-tauri/src/bin/` (standalone cargo bins,
  run via `cargo run --bin <name>` while BL4 is running, NOT part of the Tauri app) for
  next time this happens: `inject_dll.rs` (safe Dumper-7 injection, no debugger attach —
  attaching an actual debugger is what crashes BL4, not DLL injection itself), `scan_gobjects.rs`,
  `verify_bl4.rs`, `check_bl4_patch.rs`, `scan_bl4_rarity.rs`, `scan_bl4_droprate.rs`.
- **Gotcha:** restarting the Magic Wand app (e.g. after a `Cargo.toml`/Rust edit) while a
  `code_cave` cheat is active in a live BL4 process does NOT clean up the installed cave
  — the app's cave-tracking state resets, but the actual patched bytes stay in the game's
  memory. If the site RVA turns out wrong, there's no way to cleanly recover the original
  bytes from within the running process; the only fix is fully closing and relaunching
  BL4 (reloads the .exe fresh from disk).

---

## Confirmed Working Patches (STALE as of 2026-08-01, see update above)

| Cheat          | RVA         | ON bytes               | OFF bytes              | Effect                            |
| -------------- | ----------- | ---------------------- | ---------------------- | --------------------------------- |
| 100% Drop Rate | `0x96CA774` (STALE) | `0F 57 C0 90 90 90 90` | `0F 57 C0 F3 0F 2A C5` | XORPS XMM0 → XMM0=0.0 → 100% drop |

Both were in `Borderlands4.exe`, ~12KB apart in the same loot system code region, prior
to the 2026-07-31 patches.

> **Max Legendary Drop Rate is NOT the plain patch below.** The table above used to list
> it as a simple `B8 FE 7F 00 00` (MOV EAX,0x7FFE) in-place patch. **Do not implement it
> that way** — it was deliberately replaced with a gated `code_cave` (commit `dbbd408`,
> 2026-07-06) because the unconditional version forces every roll to legendary and
> collapses the game's weighted item/ammo/currency pool selection (same gun/shield/
> grenade, all sniper ammo, all Eridium, every time). This has already been
> re-introduced and re-fixed once (commit `8f5743b`, 2026-07-16) by an uncommitted edit
> reverting it back to the plain patch — don't repeat that. The bytes below remain
> useful as the raw instruction reference (still what the cave's own payload replicates
> internally), not as something to patch in place directly.

| Reference only — do not patch directly    | RVA (pre-2026-07-31) | Current RVA | ON bytes         | OFF bytes        | Effect                                 |
| ----------------------------------------- | ----------- | ----------- | ---------------- | ---------------- | -------------------------------------- |
| Max Legendary Drop Rate (raw instruction) | `0x96C77FA` | `0x3D9FE8E` | `B8 FE 7F 00 00` | `25 FF 7F 00 00` | MOV EAX,0x7FFE replaces AND EAX,0x7FFF |

The actual shipped cheat (`bl4-legendary-drop-rate` in `public/trainers/borderlands-4.json`)
is a `code_cave` gated on **`level >= 90`** (a float compare against `[RDI]` — corrected
2026-08-01, earlier revisions of this doc said `weight >= 90.0f`; same mechanism, more
precise description), matching WeMod's own logic (see "CE Script — Legendary cave"
below). It uses a `farJump` fallback (`patchLen: 17`, `coreLen: 5`, `ripFixup` at offset
13) — the default rel32-JMP cave needs a free 64KB-aligned memory slot within 2GB of the
patch site, which turned out to be scarce/absent unpredictably depending on what else
BL4 has mapped nearby at a given launch (not session length — verified live across
multiple launches). See the Rust engine's `enable_code_cave` (`src-tauri/src/lib.rs`)
for the far-jump mechanism, and the project memory `bl4-cave-alloc-verified-live` for
the full investigation (confirmed working in-game 2026-08-01).

---

## Re-Finding Addresses After a BL4 Update

If BL4 patches and the addresses stop working, follow this checklist in order:

### Step 1 — Verify the patch is broken

- Launch BL4, attach Magic Wand, enable "Max Legendary Drop Rate"
- Kill enemies — if drops are still white/green, the RVA is stale

### Step 2 — Run "All AND EAX" scan

- Click the **"All AND EAX"** button in the diagnostic panel
- Wait 4–5 minutes — scans entire 800MB+ executable
- Returns ~80 candidates with 80 bytes of context each

### Step 3 — Find the correct candidate

Look through the output for **two specific things** in the context bytes after each match:

**Must have `F3 0F 59 07`** (MULSS XMM0,[RDI]) somewhere after the DIVSS instruction.
This is WeMod's fixed discriminator — only 1–2 of ~80 results will have it.

**Must have `18 01 00 00` then `28 01 00 00`** shortly after the `F3 0F 59 07`.
This is the LEA/MOV tail that completes WeMod's AoB. Eliminates false positives.

The correct entry will look like this in the context bytes (starting at match offset 16):

```
25 FF 7F 00 00        ← AND EAX (match start)
F3 0F 2A ??           ← CVTSI2SS (register byte varies)
F3 0F 5E ?? ?? ?? ?? ??  ← DIVSS (5 bytes: any addressing mode)
F3 0F 59 07           ← MULSS [RDI] ← KEY
??                    ← REX prefix
8D ?? 18 01 00 00     ← LEA ← KEY
8B ?? 28 01 00 00     ← MOV ← KEY
85 ...
```

### Step 4 — Update the JSON

```json
"patches":    [{ "rva": "0xNEW_RVA", "bytes": [184, 254, 127, 0, 0] }],
"offPatches": [{ "rva": "0xNEW_RVA", "bytes": [37, 255, 127, 0, 0] }]
```

### Step 5 — Verify with "Read Legendary Func" (if available)

Read 32 bytes at the new RVA — first 5 bytes should be `B8 FE 7F 00 00` when ON, `25 FF 7F 00 00` when OFF.

### Faster alternative (requires WeMod time credits)

Enable WeMod's "Max Legendary Drop Rate," then click **"Find WeMod JMPs"**.
Scans for the cave tail `B8 FE 7F 00 00 25 FF 7F 00 00 E9` in injected memory.
Returns the patch site RVA directly. Much faster than manual AoB analysis.

---

### Critical Lessons Learned

- **81 addresses** match `AND EAX,0x7FFF + CVTSI2SS` in BL4 — most are unrelated math
- The 8-byte prefix alone is not enough to identify the right address
- `F3 0F 59 07` (MULSS [RDI]) is the key differentiator — only the correct function uses it
- If patching an address has **zero in-game effect even after NOPing a branch**, it's the wrong address — don't keep patching it
- The legendary patch and drop rate patch are ~12KB apart in the same loot region — look for them near each other

---

## How We Found 0x96C77FA

WeMod's full AoB for legendary (from extracted CE script):

```
25 FF 7F 00 00   AND EAX,0x7FFF      (patch site)
F3 0F 2A *       CVTSI2SS XMM0, reg
F3 0F 5E * * * * *  DIVSS XMM0, [mem]  (5 wildcards — allows RIP-relative or [RDI])
F3 0F 59 07      MULSS XMM0, [RDI]   (FIXED — key differentiator)
*                REX prefix wildcard
8D * 18 01 00 00 LEA reg, [reg+0x118]
8B * 28 01 00 00 MOV reg, [reg+0x128]
85               TEST
```

The `scan_rarity_candidates` command scanned all 81 occurrences of `AND EAX + CVTSI2SS` in BL4.
Only **two** had `F3 0F 59 07` (MULSS XMM0,[RDI]):

- `0x6F66A1A` — partial match, tail has `D8 00 00 00` ≠ required `18 01 00 00` ✗
- `0x96C77FA` — **complete match**, all AoB bytes confirmed ✓ → **correct address**

---

## What Didn't Work (Dead Ends)

### Wrong address: 0x24E1782

- Has `AND EAX + CVTSI2SS + DIVSS[RIP] + MULSS[RIP]`
- MULSS uses `F3 0F 59 05` (RIP-relative), not `F3 0F 59 07` ([RDI])
- WeMod's AoB **never matched** this address
- Patching MOV EAX,0x7FFE there, plus CMP→0 at 0x24E179F, plus NOP-JNZ at 0x24E17AF: **zero effect**
- This function is simply not on the call path for regular enemy drops

### Wrong address: 0x6F66A1A

- Has `F3 0F 59 07` but the LEA offset is `D8 00 00 00` ≠ `18 01 00 00`
- WeMod AoB doesn't match; patching there had no effect

---

## WeMod Architecture (confirmed)

- WeMod injects three DLLs into BL4: `Trainer_49051_*.dll`, `TrainerLib_x64.dll`, `CELib_x64.dll`
- Patches are applied from inside BL4 via CE auto-assembler caves
- No external WriteProcessMemory — scanning from outside will not catch WeMod's activity

### CE Script — Drop Rate cave

```
newmem:
  xorps xmm0,xmm0        ; zero XMM0
  cmp [drop_rate],1       ; WeMod enable flag
  je code                 ; if ON: XMM0=0.0 → 100% drop
  cvtsi2ss xmm0,ebp       ; if OFF: normal
code:
  jmp return              ; back to BL4 at aobdroprate+7
```

Our patch = `0F 57 C0 90 90 90 90` (XORPS + 4 NOPs) — same net result, unconditional.

### CE Script — Legendary cave

```
newmem:
  cmp [drop_rarity],1     ; WeMod enable flag
  jne code                ; if OFF: skip
  cmp [rdi],(float)90     ; level >= 90?
  jl code                 ; if < 90: skip
  mov eax,00007FFE        ; force legendary roll
code:
  and eax,00007FFF        ; ORIGINAL instruction (always runs)
  jmp return
```

Our patch = `B8 FE 7F 00 00` (MOV EAX,0x7FFE) — same net result, unconditional (no level check).

---

## Key Addresses

| Symbol          | Address       | Status                    |
| --------------- | ------------- | ------------------------- |
| BL4 base        | 0x140000000   | Confirmed fixed (no ASLR) |
| GObjects RVA    | 0xC891A48     | Confirmed working as of 2026-08-01 (see `[[project-overview]]`; drifts most patches, re-verify before trusting) |
| GNames RVA      | 0xC7ABD90     | Confirmed working as of 2026-08-01 |
| Drop rate patch | RVA 0x96CA774 | **STALE as of 2026-07-31 patches — not yet re-derived** |
| Legendary patch | RVA 0x3D9FE8E | **Confirmed in-game 2026-08-01** (was 0x96C77FA, stale after 2026-07-31 patches) |
