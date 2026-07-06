# BL4 Legendary Drop Rate — Investigation Brief

## Goal
Find the exact memory address(es) WeMod patches for "Max Legendary Drop Rate" and implement as a `code_patch` in `public/trainers/borderlands-4.json`.

---

## What Is Working
- **100% Drop Rate** at RVA `0x96CA774` — confirmed working, in production.
  - Original: `0F 57 C0 F3 0F 2A C5` (XORPS XMM0,XMM0 + CVTSI2SS XMM0,EBP)
  - Patch: `0F 57 C0 90 90 90 90` (XORPS + NOP×4)

## What Has Been Tried and Ruled Out

### Code Patch Addresses — All Confirmed Non-Working
| RVA | Instruction | Patch Attempted | Result |
|-----|-------------|-----------------|--------|
| `0x6F66A1A` | `AND EAX, 0x7FFF` | → `MOV EAX, 0x7FFE` | No effect on rarity |
| `0x24E1782` | `AND EAX, 0x7FFF` | → `MOV EAX, 0x7FFE` | No effect on rarity |
| `0x24E179F` | `CMP EAX, 247` (83 F8 F7) | → `CMP EAX, 0` (83 F8 00) | No effect on rarity |

All three can be patched simultaneously and Verify Patches confirms they are active — still white/green drops.

### Snapshot Diff Investigation — Confirmed Dead End
- Snapshot BL4 executable pages → enable WeMod legendary → diff
- Addresses that changed: `0x1F8F83A5`, `0x1F8F83ED`, `0x1F8F83F5`, `0x1F8F83FD/FE`, `0x1F902DB0-DB3`, `0x1F914937-93A`
- **Toggle diff confirmed these are Denuvo noise**: same addresses appear in both ON→OFF and OFF→ON diffs, but with completely different byte values each time. WeMod produces consistent before/after values; Denuvo does not.
- **Conclusion**: WeMod does NOT patch any executable code page in BL4. The executable snapshot cannot find WeMod's patch.

### Cave Signature Scan — No Result
- Scanned all executable regions ≤4MB for `83 3D ?? ?? ?? ?? 01 75 ?? B8 FE 7F 00 00` (CMP [flag],1; JNE; MOV EAX,0x7FFE)
- Found nothing
- WeMod's current trainer does not use a CE-style code cave for this cheat in the current BL4 build

### AoB Scan — Pattern Not Found in Current BL4
WeMod's CE script (read from injected DLL at offset 0x8B81A) has this rarity AoB:
```
25 FF 7F 00 00 F3 0F 2A ?? F3 0F 5E ?? ?? ?? ?? ?? F3 0F 59 07 ?? 8D ?? 18 01 00 00 8B ?? 28 01 00 00 85
```
- Full 35-byte pattern: **NOT FOUND** in current BL4
- 18-byte partial (through `F3 0F 59`): found at RVA `0x24E1782` — but patching there has no effect (see above)
- **Conclusion**: BL4 was updated after this trainer build. The AoB no longer fully matches.

### CE Script — What Was Read From Memory
Three cheat scripts exist in the DLL at offset 0x8B81A:
1. **Drop rate v1** AoB: `0F 57 C0 F3 0F 2A C5 F3 0F 5E * * 0F 2E * 76 * * 0F 2E * 73`
2. **Drop rate v2** AoB: `0F 57 C0 F3 0F 2A C5 F3 0F 5E * * 0F 2E * 0F 86 * * * * * 0F 2E`
3. **Rarity** AoB: (the 35-byte pattern above) — does NOT match current BL4

The CE script's `[ENABLE]` block for rarity was structured as:
```
aobscanmodule(aobdroprarity, Borderlands4.exe, <35-byte pattern>)
alloc(newmem, $1000, aobdroprarity)
newmem:
  cmp [legendaryDropFlag], 1
  jne @f
  mov eax, 7FFE
  @@:
  and eax, 7FFF
  [rest of original instructions]
  jmp return
[aobdroprarity]:
  jmp newmem
```
**Distinction**: drop rate cave uses `JE` (0x74); legendary uses `JNE` (0x75) — WeMod runs MOV EAX,0x7FFE then falls through to AND EAX,0x7FFF so the AND masks 0x7FFE → 0x7FFE (stays legendary when cheat is on, AND executes original mask when cheat is off).

### NexusConfigStore Data Patching — Dead End
- `NexusConfigStoreInventoryRarity`, `NexusConfigStoreLuckCategory`, `NexusConfigStoreLootConfig` — all explored
- Writing 1.0 to probability/weight fields in these objects has no effect on drop rarity
- These objects control drop PROBABILITY (whether something drops) not QUALITY (what rarity it is)

### Luck Multipliers, V2 Force — No Effect
- Boosting luck category multipliers to 100.0 → no rarity improvement
- Forcing V2 rarity multiplier to 32767.0 → no observed effect

### Cheat Engine — Ruled Out (Hard Blocker)
- Any debugger attachment to BL4 (CE, x64dbg, WinDbg targeting BL4) causes immediate game crash
- Denuvo detects debugger presence
- **Do not attempt**

### API Monitor / Process Monitor — Ruled Out
- WeMod uses its own dedicated DLL infrastructure
- WriteProcessMemory calls from WeMod do not surface usably via API Monitor because the calls go through WeMod's DLL chain, not wand.exe directly

### List Modules — Not Useful
- Tried multiple times, doesn't lead to actionable addresses

---

## The DLL on Disk — What We Know

File: `C:\Users\renga\AppData\Roaming\WeMod\App\trainers\Trainer_49051_20e30ce373.dll`
- Size on disk: 2,417,992 bytes
- PE sections: 7 unnamed sections + `.edata`, `.idata`, `.tls`, `.rsrc`, `.wemod` (empty on disk), `.boot` (2MB), `.reloc`
- `.boot` section starts at raw offset `0x45000`, size `0x206E00` (~2MB) — **custom encrypted**, not standard compression
- First 4 bytes of `.boot`: `72 74 4C 6B` — does not match LZ4, LZMA, zstd, zlib, or any known format
- Imports: only `kernel32.dll`, `USER32.dll`, `VERSION.dll` — self-contained loader
- The encryption key is supplied by WeMod at injection time
- **Cannot be read statically** — decrypted content only available when loaded in BL4's memory

WeMod rebranded to "Wand". App is at: `C:\Users\renga\AppData\Local\Wand\app-12.34.2\resources\app.asar`
- This is Electron JS (UI only) — no patch addresses in it
- Only useful info: confirms cheat name "Max Legendary Drop Rate" maps to trainer ID 49051

---

## The Plan: Dump the Decrypted DLL

**WeMod can attach to BL4 without using credits (time-limited cheats only)**. The DLL is injected and decrypted in BL4's memory even without cheats enabled.

### Step 1: Dump While WeMod Is Attached
1. Open WeMod/Wand, attach to BL4 (no cheats needed)
2. Open Magic Wand, attach to BL4
3. In the diagnostic panel: click **"Dump DLL"** button
   - This calls `dump_module_to_file` for `Trainer_49051_20e30ce373.dll`
   - Output: `C:\Users\renga\Desktop\wemod_trainer_dump.dll`
4. The output file will be the **fully decrypted DLL** — much more readable than the on-disk version

### Step 2: Verify the Dump
- Expected size: significantly larger than 2,417,992 bytes (the `.wemod` virtual section gets populated)
- Run `strings` on the dump and search for `aobdroprarity`, `legendary`, `7FFE`, `and eax`
- CE script should still be readable at in-memory offset `0x8B81A` from DLL base (use "Read CE Script" button too)

### Step 3: Load in Ghidra (or x64dbg offline)
- Ghidra: free from https://ghidra-sre.org
- Import `wemod_trainer_dump.dll`, auto-analyze, PE format
- Search for:
  1. **String**: `aobdroprarity` — leads directly to the CE script init code
  2. **Bytes**: `25 FF 7F 00 00` (AND EAX, 0x7FFF) — the original instruction being hooked
  3. **Bytes**: `B8 FE 7F 00 00` (MOV EAX, 0x7FFE) — the forced legendary value
  4. **Bytes**: `83 3D` (CMP [RIP+rel32]) — the cheat flag check pattern
  5. **Function**: `WriteProcessMemory` — follow references to see what address/bytes it writes to BL4
  6. **Function**: `VirtualProtectEx` — called just before patching BL4 code

### What the Analysis Should Reveal
The key question is whether WeMod's CURRENT approach (for current BL4 build) is:

**a) Updated AoB pattern** — a new pattern that DOES match current BL4 (different from the 35-byte one in the script). We'd find a new AoB string → scan that pattern in BL4 → patch site found.

**b) Hardcoded RVA fallback** — when AoB fails, WeMod falls back to a known offset. We'd find a hardcoded address like `0xXXXXXXX` in the DLL code → that's our patch RVA.

**c) Data patch to non-executable memory** — WeMod writes to a `.rdata` or heap address. Executable snapshot diff can't see this. We'd find a WriteProcessMemory call with an address not in `.text`.

**d) Something else** — vtable patch, IAT hook, etc.

---

## Untested Single-Byte Patch (Worth Trying Before Dumping DLL)

The `applyLegendaryPatch` button patches 5 addresses but mixes a real code address with Denuvo noise:
- `0x128DFFEE` → `[0x79]` — **REAL CODE**, ~295MB into BL4, short conditional jump instruction
- `0x1F8F83A5` → `[0x42]` — Denuvo noise, ignore
- `0x1F8F83ED` → `[0xC3]` — Denuvo noise, ignore
- `0x1F8F83FD` → `[0x1C, 0xB3, 0x88]` — Denuvo noise, ignore
- `0x1F902DB0` → `[0xD8, 0xB4, 0xAB, 0x57, 0xF8, 0x51]` — Denuvo noise, ignore

**The 0x128DFFEE patch has NEVER been tested in isolation.** The Denuvo writes alongside it may be actively interfering. Before dumping the DLL, test:
1. Fresh BL4 session
2. Use `patch_bytes` to write `0x79` to `abs(0x140000000 + 0x128DFFEE) = 0x1528DFFEE`
3. No other patches applied
4. Kill enemies and check drop rarity

The "Read Code Block" button reads 512 bytes at `0x128DFF00` which covers this area. Read it to understand context before patching.

---

## Key Constants
- **BL4 base address**: `0x140000000` (fixed, ASLR not applied)
- **GObjects offset**: `0x11765A30`
- **GNames offset**: `0x1167FDD0`
- **Working drop rate RVA**: `0x96CA774`
- **WeMod CE script in-memory offset**: `0x8B81A` from DLL base

## JSON Patch Format (Once Address Is Found)
```json
{
  "id": "bl4-legendary-drop-rate",
  "name": "Max Legendary Drop Rate",
  "type": "code_patch",
  "module": "Borderlands4.exe",
  "offsets": [],
  "onValue": 0,
  "patches": [
    { "rva": "0xXXXXXXX", "bytes": [XX, XX, XX, XX, XX] }
  ],
  "offPatches": [
    { "rva": "0xXXXXXXX", "bytes": [XX, XX, XX, XX, XX] }
  ]
}
```
The `offPatches` bytes must be the **original bytes** read before any patch is applied.
