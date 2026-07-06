# BL4 Drop Rate / Legendary — Session Brief

_Last updated: 2026-06-30_

---

## Confirmed Working Patches

| Cheat | RVA | ON bytes | OFF bytes | Effect |
|---|---|---|---|---|
| 100% Drop Rate | `0x96CA774` | `0F 57 C0 90 90 90 90` | `0F 57 C0 F3 0F 2A C5` | XORPS XMM0 → XMM0=0.0 → 100% drop |
| Max Legendary Drop Rate | `0x96C77FA` | `B8 FE 7F 00 00` | `25 FF 7F 00 00` | MOV EAX,0x7FFE replaces AND EAX,0x7FFF |

Both are in `Borderlands4.exe`, ~12KB apart in the same loot system code region.

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

| Symbol | Address | Status |
|---|---|---|
| BL4 base | 0x140000000 | Confirmed fixed (no ASLR) |
| GObjects RVA | 0x11765A30 | Confirmed working |
| GNames RVA | 0x1167FDD0 | Confirmed working |
| Drop rate patch | RVA 0x96CA774 | **Confirmed in-game** |
| Legendary patch | RVA 0x96C77FA | **Confirmed in-game** |
