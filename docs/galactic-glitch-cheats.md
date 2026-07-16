# Galactic Glitch — Cheat Source Brief

_Last updated: 2026-07-14_

Galactic Glitch is Unity/IL2CPP. All game logic lives in `GameAssembly.dll`, loaded
inside `GalacticGlitch.exe` — every cheat's `module` is `GameAssembly.dll`, not the exe.

Manual scanning (unknown-initial-value HP scan, since HP is only shown as a bar, not a
number) kept converging on 5 decoy addresses that didn't affect damage/death when frozen.
Switched to public Cheat Engine tables instead of continuing to scan.

## Status: game updated mid-project (v2.22, 2026-06-26) — only Invincibility is re-verified

The source CT table below is dated June 2025; the installed game is v2.22 (June 26, 2026).
That's enough drift that the original `Invincibility` AOB (`F3 0F 5C C1 44 38 BB 71 01 00 00`)
no longer matched anywhere in `GameAssembly.dll` — confirmed via Magic Wand's own AOB Scan
diagnostic tool, not just an in-game failure. **`Invincibility` has since been re-derived,
rebuilt as a two-site patch, and confirmed working against the live v2.22 process across
multiple full runs — HP bar does not move at all.** (See below for how.) **The other 6
cheats (Energy, Dodge, Glitch Charge, Glitch Token, Orb x50, Shard x50) are still built
from the stale June 2025 table and have not been re-verified against v2.22** — do not
trust them without re-running the same process used for Invincibility. One prior attempt
to enable all 6 unverified cheats at once caused severe system-wide sluggishness (see
"Runaway retry-storm bug" below) — verify each one individually with the AOB Scan tool
before re-enabling.

**Also learned the hard way**: both `code_cave` and `code_patch` write directly into the
target game's own process memory (a JMP+cave, or raw NOP'd bytes, inside
`GalacticGlitch.exe`'s address space). Once installed, that stays there for as long as the
*game* process stays alive — closing Magic Wand does **not** undo it, for either type.
`code_patch`'s restoration (writing the original bytes back from `offPatches`) only
happens inside `stopCodePatch`, which only runs when a cheat is explicitly toggled off; if
Magic Wand closes while a `code_patch` cheat is still active, its 50ms re-apply interval
just dies mid-flight and the last-written (patched) bytes are never restored.
`code_cave`'s equivalent problem: the app's record of the original bytes (needed to
cleanly remove the JMP) only lives in memory while Magic Wand is running, so closing it
loses that record even though the JMP itself is still installed in the game.

Practical effect either way: Magic Wand's UI resets to "off" on every relaunch (fresh
app state), but that's disconnected from what's *actually* sitting in the game's memory —
a cheat can show as off while its patch (or orphaned NOPs) is still live from a previous
session. This showed up concretely when re-verifying Glitch Token's signature: an "AOB
Scan All" for the original bytes came back with zero matches even though the cheat looked
off in the UI, because the previous session's NOP patch was still physically in place and
had never been restored.

Always disable `code_cave`/`code_patch` cheats before closing Magic Wand; if you forget,
a full game restart (not just Magic Wand) clears whatever was left behind.

### How Invincibility was re-derived (repeatable process for the other 6)

1. Cheat Engine's Mono/Il2Cpp dissector (built into CE, appears as an extra menu once
   attached to a Mono/IL2CPP process) browses the game's own IL2CPP metadata — every
   class, field, and method by name, with live addresses. This is how the original CT
   table's disassembly comments had real C# names in them; it isn't CE-version-specific
   magic, it's reading data the game ships with.
2. Found the `Health` class, expanded its `methods` list, found
   `<TakeDamage>g__ApplyDamageToHealth|249_1` (same local-function shape as the old
   table's `|224_1` — same method, just recompiled with a different compiler-generated
   index) at `MethodInfo*` address `0x1de87ee9500`.
3. That address is a `MethodInfo` struct, not code — offset `0x00` (`methodPointer`) is
   the actual native entry point: `0x00007FF82FBB0EA0` (RVA `0x3D0EA0` relative to
   `GameAssembly.dll`'s base — confirmed offset `0x18` in the same struct pointed back at
   the `Health` class's own address, cross-verifying the struct layout).
4. Read 512 bytes from that entry point (Magic Wand's diagnostic panel "Read Bytes"
   tool) — mostly IL2CPP static-init boilerplate at the top, real logic starts after.
5. Used the new `aob_scan_all_range`-backed "Scan 16KB From Addr" tool to find every
   occurrence of the generic `subss` opcode prefix (`F3 0F 5C`) within a 16KB window
   from the function entry, instead of a whole-module scan (too noisy — `subss` alone is
   extremely common). Got 4 candidates; read context around each.
6. Three of the four didn't structurally match the original confirmed pattern (wrong
   destination register, or no clamp-call sequence following). The fourth
   (`subss xmm0,[rsi+0x10]` at RVA `0x3D16EB`) was followed by
   `cmp [rbx+0x189],r15b → je+0x18 → const-load → xor r9d,r9d → const-load → call →
   movss [rbx+0x170],xmm0` — byte-for-byte the same shape as the original confirmed
   `subss → cmp[rbx+0x171],r15b → je+0x18 → ... → movss [rbx+0x158],xmm0` sequence, just
   with the class's fields shifted to new offsets (it picked up new fields over the year).
7. Needed a way to gate the cheat to the player only (this is a combat-only game — a
   blanket "nobody takes damage" cheat would also make enemies unkillable and break the
   game entirely). The old table's player/enemy check (`[rbx+0x120]`) was the *cheat
   author's own discovery*, unrelated to the damage instruction itself — so it had to be
   re-found separately. Expanded the `Health` class's **`fields`** list (not `methods`)
   in the same Mono dissector and found `<IsPlayer>k__BackingField` (boolean) directly by
   name at offset `0x130`. Much more reliable than reverse-engineering it from instruction
   context.

## Source Tables

- `Galactic Glitch v1.0_Table v1.0_ColonelRVH.ct` (thecheatscript.com) — full-featured,
  actively maintained. Source for every cheat below except none; all 7 trace back to this table.
- `GalacticGlitchfrf.CT` (fearlessrevolution.com) — single simpler cheat (pointer-based
  Orbs value via a captured `rbx` at a different AOB). Not ported: Magic Wand's schema
  has no concept of "capture a pointer into a persistent cave symbol, read it elsewhere"
  the way CE's named allocations do. Revisit if direct Orb-value editing (vs. the
  multiplier cheat below) is wanted later.

## Cheats in `public/trainers/galactic-glitch.json`

All are `code_cave`/`code_patch` entries with a `patchSignature`/`signature` (live AOB
scan, not a hardcoded RVA) — so these should keep working across GameAssembly.dll
relocations and minor patches, as long as the byte patterns themselves don't change.

| Cheat | Site (AOB) | Mechanism |
|---|---|---|
| Invincibility ✅ re-verified v2.22 | Two sites in `Health.<TakeDamage>g__ApplyDamageToHealth\|249_1`, both the same `subss xmm0,[rsi+0x10]` instruction: RVA `0x3D16EB` (context `44 38 BB 89 01 00 00 74 18`) and RVA `0x3D13D0` (context `44 0F 2F C8`) | Checks `[rbx+0x130]` (`<IsPlayer>k__BackingField`) at each site; if player, skips the subtract entirely. First site gates the lethal/final HP field (`[rbx+0x170]`) — prevented death but left a second, separate HP-like field (`[rbx+0x178]`) still ticking down on normal hits, bottoming out near zero (only recovered by rare healing items, no passive regen). Second site patches that field too via `caveSites` (multi-site `code_cave`, `src/hooks/useTrainer.ts`) — confirmed 2026-07-14 across multiple full runs: HP bar does not move at all. Re-derived 2026-07-13 directly from the running v2.22 process (CE's Mono/Il2Cpp dissector + Magic Wand's AOB Scan/Read Bytes/Scan-16KB tools) — the June 2025 table's version of this patch no longer matched anything. |
| Unlimited Glitch Energy 🗑️ dropped | — | User doesn't want it. Left in this doc for history only; not in the trainer JSON reasoning below (still physically present in the JSON but should be treated as abandoned — not re-verified and not wanted). |
| Unlimited Dodge 🗑️ dropped | — | User doesn't want it. Same as above. |
| Glitch Token Never Decreases ✅ re-verified v2.22 | `PlayerStatBonuses.RemoveUpgradeProtocol`, `29 78 2C EB 47` (patches first 3 bytes only) | NOPs the `sub [rax+2C],edi`. Direct byte patch. Re-checked 2026-07-14 with the AOB Scan tool — pattern matched exactly, and the surrounding context (`48 8B 83 70 01 00 00 48 85 C0 74 74 ...`) is structurally consistent with the original disassembly (some field offsets/branch targets shifted, as expected from a recompile, but same shape). No changes needed. |
| Glitch Charge Never Decreases ✅ re-verified v2.22 (redesigned) | `Player.SetGlitchChargesAmount`, RVA `0x380AC2`, `89 7B 30 48 85 C9 74 0E` (patches first 6 bytes: `mov [rbx+0x30],edi` + `test rcx,rcx`) | The original inline-decrement site inside `OnGlitchBomb` no longer exists — that method was restructured (now takes an `isStarted: bool` parameter that didn't exist before, so this wasn't just a recompile). Rather than chase the new decrement site, patched the **setter** instead: `SetGlitchChargesAmount(chargesAmount)` writes `edi` (the parameter) straight to `[rbx+0x30]` with no clamping. Since every charge change (pickups *and* usage) funnels through this one setter, the cave now compares the incoming value against the current one and only stores it if it's not lower — pickups still raise the count normally, usage no longer lowers it. Centralizing on the setter is arguably more robust than the original table's approach of patching one specific call site. |
| Orb Pickup x50 ⚠️ unverified v2.22 | `OrbsWorld.PickupOrbs`, `45 33 C0 03 53 18 ...` (patches first 6 bytes: `xor r8d,r8d` + `add edx,[rbx+18]`) | Reproduces the original `xor r8d,r8d` unchanged (downstream code depends on r8 being zeroed), captures `[rbx+18]` into `ecx`, multiplies by a hardcoded 50, adds the result to `edx` instead of the raw pickup amount. From the stale June 2025 table — not yet re-checked against v2.22, and likely to be replaced with a direct master-value read/write instead (see below) rather than re-derived as a multiplier cave. |
| Shard/Credit Pickup x50 ⚠️ unverified v2.22 | `Player.set_Credits`, `01 BB ?? ?? 00 00 01 7B 20` (patches first 6 bytes: `add [rbx+688],edi`) | Multiplies `edi` by a hardcoded 50, then reproduces the original `add [rbx+688],edi` unchanged. **The `+0x688` offset is hardcoded** (the CT table's wildcards only cover the low 16 bits of the displacement; this was resolved against the June 2025 build, not v2.22) — from the stale table, not yet re-checked, and likely to be replaced with a direct master-value read/write instead (see below). |

**Orb/Shard direction change**: rather than re-deriving these two as pickup-multiplier
caves (hand-assembled x86, `code_cave`), the plan is to find a stable pointer to the
player/save-data object plus the Orbs/Shards field offsets (same technique as `IsPlayer`
— browse the relevant class's fields in the Mono dissector) and expose them as plain
`scan`/`toggle`-type value cheats instead, the same way BL4's Cash/Eridium cheats work.
Simpler, no assembly, easier to verify. Not yet done.

**Before re-enabling any `⚠️ unverified` cheat**: run its `signature` through the AOB Scan
tool first. A clean "not found" is safe (the cheat just won't turn on). The dangerous case
is a signature that still finds *a* match — possibly a coincidental wrong location in the
updated binary — since installing a cave/patch there hijacks whatever unrelated code
actually lives at that address. Read the context bytes and sanity-check them the same way
Invincibility was re-derived above before trusting a "found" result.

## Why no Instant Kill

The CT table's "Instant Kill" (enemies die in one hit) uses the *same* patch site as
Invincibility, distinguished at runtime by a CE-side flag byte the Lua/hotkey layer
toggles. Magic Wand has no per-cheat flag-byte mechanism — each `code_cave` cheat
independently installs/removes its own JMP at enable/disable, keyed by `cheat_id` in
`CAVE_STATE` (`src-tauri/src/lib.rs`). Two cheats sharing one patch site would stomp each
other's "original bytes" snapshot: whichever enables second would snapshot the *first
cheat's* JMP as "original," and disabling it later would restore corrupted bytes instead
of the real original code.

Since Invincibility was the actual goal (not one-shotting enemies — this is a combat-only
game, so a global damage-blocker isn't useful anyway), Instant Kill was dropped rather
than solved. If it's wanted later, options are: (a) a single combined "HP Control" cheat
with a 3-way mode (Off/GodMode/InstaKill) that installs one cave whose branch depends on
which mode is selected, or (b) extending the trainer schema with a shared-flag-byte
primitive. Don't enable a from-scratch Instant Kill cheat on the same site without one of
those — it will corrupt the Invincibility cheat's restore path if both are ever toggled
independently.

## Backend Changes Made For This

`code_cave` previously only supported a hardcoded `patchSite` RVA and unconditionally
overwrote exactly 5 bytes (`enable_code_cave` in `src-tauri/src/lib.rs`). Two of this
game's patch sites are 6 and 11 bytes — a bare 5-byte JMP would've left dangling
original bytes that the CPU would misdecode on return. Extended:

- `code_cave`/`code_patch` cheats can now supply a `signature`/`patchSignature` AOB
  pattern (resolved via the existing `aob_scan` command at enable time) instead of a
  fixed RVA — see `patchSite`/`patchSignature` and `patches[].rva`/`patches[].signature`
  in `src/hooks/useTrainer.ts`.
- `enable_code_cave` takes a `site_len` (JSON: `patchLen`, default 5). Bytes beyond the
  5-byte JMP up to `site_len` are NOP-padded, and the JMP back resumes at
  `site_addr + site_len` instead of always `site_addr + 5`.
- `code_patch` signature-based addresses are resolved once and cached per cheat (not
  re-scanned on every 50ms re-apply tick — an AOB scan walks the whole module).
- `code_cave` cheats can now list multiple `caveSites` (each with its own
  `patchSite`/`patchSignature`/`patchLen`/`cavePayload`) instead of a single site. Needed
  once we found the "Invincibility" toggle actually needed to patch two separate call
  sites of the same instruction to fully stop HP from moving (see below) — one UI toggle
  now installs/removes N independent caves, keyed internally as `${cheat.id}#0`,
  `${cheat.id}#1`, etc. If any site fails to enable, the ones that already succeeded are
  rolled back so the cheat doesn't end up half-installed.

**Runaway retry-storm bug (found and fixed 2026-07-13):** the address cache above was
only populated on a *successful* resolution. If a `code_patch`'s signature failed to
resolve (stale, as happened here), the cache never got set, and the silently-swallowed
50ms re-apply interval would retry a full, uncached module scan on every single tick,
forever. Enabling 3 stale `code_patch` cheats at once this way caused severe whole-PC
sluggishness (no crash/BSOD — consistent with several CPU-bound full-module scans firing
20×/second indefinitely). Fixed in `useTrainer.ts`: a `code_patch` cheat no longer flips
to "active" until its patch resolves and applies at least once, and a cheat that starts
failing mid-session now stops its own interval and flips back off instead of retrying
forever.

**Diagnostic tools added to `DevPanel.tsx`** (all gated behind Settings → Dev Mode) for
re-deriving stale patches without leaving the app:
- **AOB Scan** — module + pattern → first match's absolute address, RVA, and 32 bytes of context.
- **Read Bytes** — absolute address → 512 raw bytes, plus RVA if a module name is set in the AOB Scan row.
- **Scan 16KB From Addr** — like AOB Scan, but returns *every* match within a 16KB window
  from a given address (`aob_scan_all_range`, `src-tauri/src/lib.rs`) instead of just the
  first hit in the whole module. Useful for hunting a generic sub-pattern (e.g. a bare
  `subss` opcode) once a containing function's real entry point is already known — a
  whole-module scan for something that generic would be too noisy to use.
