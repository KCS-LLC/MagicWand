# Find The Needle (Godot 4.7, GDScript)

Trainer: `public/trainers/find-the-needle.json` (registered in `index.json`). Process `FindTheNeedle.exe`, Steam app 5160800.
Custom Godot 4.7.2 build; the PCK is encrypted (no `GDPC` magic) so scripts can't be read statically — everything here was found live.

## Status

| Cheat | Type | Status |
|---|---|---|
| Set Money (textbox, default 9999) | `action` + `godot` member, `double` | **Confirmed live** 2026-11-15, incl. after full game + app relaunch (per-launch lookup works) |
| Unlimited Stamina | `code_cave` + `godot` scratch (conditional store-skip) | **Confirmed live** 2026-11-16 for sprinting AND shoveling/raking (real no-drain, not a poll-write) |

## How it works (new `godot` cheat field, `src-tauri/src/godot.rs`, command `resolve_godot_member`)

- GDScript member vars live in a `Vector<Variant>` owned by a `GDScriptInstance` heap object. Heap addresses move each launch.
- Anchor: the instance's first qword is the GDScriptInstance **vtable**, at exe RVA `0x5B13B90` (exe base `0x7FF6D8B10000` on the dev machine).
  The engine scans the process for that qword (~35 s cold, ~0.15 ms when the cached instance is revalidated), then keeps the instance whose
  member count and Variant tag fingerprint match.
- Layout: instance `+0x28` = members data ptr; members `-0x10` = count; element stride **24** bytes: `u32 tag, pad, payload@+8`.
  Tags: 1 bool, 2 int, 3 float (**stored as f64**), 4 String. GDScript floats are therefore `double` in the trainer JSON.
- The resolver returns the payload address; the instance is cached per cheat (`godotInstances` in `useTrainer.ts`) and revalidated every call, so a
  scene change that destroys it just triggers a re-scan. The full-process scan result is shared across cheats (120 s TTL, mutex-serialized), so money + stamina cost one ~35 s scan on connect.

## Verified facts

- Money: instance with **115 members**, member **31** (displayed value is rounded: 19.3888 shows as $19.39). Member 39 often equals it but is NOT a mirror
  (diverged once: 25.23 vs 16.91) — do not write it. Money can reach billions (double is fine).
- Stamina: instance with **10 members** (tags `3,3,3,3,1,2,2,?,1,1`), member **0**: 100.0 full, 0.0 drained, back to 100.0 on refill.
  Members 1-3 are other floats (member 3 looked like a timer) — untested whether the "exhausted" lockout needs more than topping member 0 up.

## Re-finding after a game update

1. Vtable RVA: scan for any GDScriptInstance (object whose `+0x28` points at a Variant array) and take its first qword minus the exe base.
2. Money: change money, scan Variant-tagged doubles (tag 3 at `addr-8`) for the displayed value ±0.005, re-read after changing money; find the owning
   instance by searching for pointers to the array start. Stamina: snapshot all tagged doubles full → drained → refilled and keep the round-trip
   values that live inside a script instance's member array (enumerated through the vtable).
3. Diagnostic (read-only): `cargo run --bin verify_godot_ftn` in `src-tauri/`.

## Stamina drain hook (found 2026-11-16 with a DR0 hardware write-watch, `hwwatch.py`-style: DebugActiveProcess + DR7 write/8 bytes on every thread, DRs cleared before detach)

- Exactly **one** instruction writes the stamina member: exe RVA `0x2DF33D6`, `F2 0F 11 41 08` = `movsd [rcx+8], xmm0`, the engine's generic float-Variant copy
  (so it can't simply be NOPed - every float assignment in the game goes through it).
- Values are frame-based (~60/s): sprint drain **-0.2/frame**, regen **+0.3667/frame**; no idle rewrites.
- Cave (`cavePayload`, 43 bytes incl. an 8-byte scratch qword at offset 35): save flags+rax; if `rcx+8 == scratch` (live stamina address, refreshed from the
  `godot` resolver by `refreshGodotScratch` on install and every poll tick) and `ucomisd xmm0,[rcx+8]` says new < old, skip the store; else run the original store.
  Until the scratch is filled it is zero, so the cave is inert.
- Re-find after an update: watch the stamina address with a hardware write breakpoint while sprinting (needs the debugger-attach to work - it did on 2026-11-16).
