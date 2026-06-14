# Brief: Polling-Write Toggle Cheats

## Status
Unimplemented.

## Problem

Toggle cheats with `type: "toggle"` and a `valueType` (int / float / double) currently write `onValue` exactly once per click and then go idle. That works for one-shot stats but not for anything the game continuously recalculates.

Real example: Skyrim's Unlimited Carry Weight (`public/trainers/skyrim-se.json`). Click → carry weight = 99999 → equip a piece of armour, level up, exit a menu, fast travel → game recalculates the derived stat → back to your real max → the cheat appears to have "worn off." The same problem exists for Infinite Health (regen ticks rewrite the value), the Skyrim Gold cheat if it were a toggle instead of action, and any future Cell to Singularity toggle that maps to a value the game's update loop touches.

WeMod / Wand-style trainers handle this by **continuously re-asserting the cheated value** on a timer for as long as the toggle is on. This brief adds that behaviour to Magic Wand.

It does **not** change `action`, `patch`, `scan`, `mono`, or `mono_chain` cheats — only the `toggle` path with a `valueType`.

---

## Behaviour

- `applyCheat` on a toggle now treats the click as a state flip, not as the write itself:
  - Toggling **on** flips `active: true` and issues one immediate write (snappy UX — user sees the value change without waiting for the next poll).
  - Toggling **off** flips `active: false` and writes **nothing**. The game's own update loop is allowed to take over again.
- The existing poll loop, which already runs every `pollInterval` ms to refresh `currentValue`, now **also writes `onValue` to every active toggle's address before reading** it. The read step still runs so the live-value display continues to show the value (which should now match `onValue`).
- Patch toggles (`type: "toggle"` is not used for patches — `type: "patch"` is) are unaffected. So is everything else.
- Errors from polling-writes are surfaced through the existing `onCheatError` plumbing (added by [[brief-cheat-error-feedback]]); de-dup is automatic because the error string is keyed by cheat id.

---

## Implementation

### 1. `applyCheat` — `src/hooks/useTrainer.ts`

Today the non-patch toggle branch writes on every click regardless of direction. Skip the write when the new active state is `false`:

```typescript
const applyCheat = async (cheat: Cheat, customValueStr?: string, onError?: (id: string, msg: string) => void) => {
  if (!pid || !activeGame) return;

  if (cheat.type === 'action' || cheat.type === 'mono' || cheat.type === 'mono_chain') {
    // ... unchanged ...
    return;
  }

  const willBeActive = !cheat.active;
  setActiveGame(prev => {
    if (!prev) return null;
    return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: willBeActive } : c) };
  });
  try {
    const addr = await resolveCheatAddress(cheat);
    const hexAddr = "0x" + BigInt(addr).toString(16);
    if (cheat.type === 'patch') {
      const bytes = !cheat.active ? cheat.onBytes : cheat.offBytes;
      await invoke('patch_bytes', { pid, address: hexAddr, bytes });
    } else if (willBeActive) {
      // Only write on toggle-on. Toggle-off just stops the poll loop from re-asserting.
      const writeValue = resolveWriteValue(cheat, customValueStr);
      const cmd = cheat.valueType === 'double' ? 'write_double'
                : cheat.valueType === 'float'  ? 'write_float'
                : 'write_int';
      await invoke(cmd, { pid, address: hexAddr, value: writeValue });
    }
  } catch (err) {
    setActiveGame(prev => {
      if (!prev) return null;
      return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: !willBeActive } : c) };
    });
    onError?.(cheat.id, err instanceof Error ? err.message : String(err));
  }
};
```

The existing catch already reverts the optimistic state flip on error — keep it.

### 2. Poll loop write-then-read — `src/hooks/useTrainer.ts`

Inside the existing `setInterval` callback, for each cheat resolve the address, and **if the cheat is an active non-patch toggle, write `onValue` before reading**:

```typescript
const results = await Promise.all(currentActive.cheats.map(async (cheat) => {
  try {
    const addr = await resolveCheatAddress(cheat);
    const hexAddr = "0x" + BigInt(addr).toString(16);

    // Re-assert active toggle cheats (skip patch — those toggle bytes, not values)
    if (cheat.type === 'toggle' && cheat.active && cheat.valueType) {
      const writeCmd = cheat.valueType === 'double' ? 'write_double'
                     : cheat.valueType === 'float'  ? 'write_float'
                     : 'write_int';
      await invoke(writeCmd, { pid: pidRef.current, address: hexAddr, value: cheat.onValue });
    }

    const readCmd = cheat.valueType === 'double' ? 'read_double'
                  : cheat.valueType === 'float'  ? 'read_float'
                  : 'read_int';
    const val = await invoke<number>(readCmd, { pid: pidRef.current, address: hexAddr });
    return { id: cheat.id, val };
  } catch (e) {
    onCheatError?.(cheat.id, e instanceof Error ? e.message : String(e));
    return { id: cheat.id, val: '???' };
  }
}));
```

Note: the read still happens after the write, so the displayed `currentValue` reflects what we just wrote (good signal to the user that the cheat is sticking).

### 3. Custom value support (optional, nice to have)

Right now `customValueStr` only flows into `applyCheat`. After this change, polling-writes use `cheat.onValue` literally — they won't pick up a user-typed custom value from the value-input box. If you want the polling write to honour a user's typed value, store the resolved write-value on the cheat object when toggling on:

```typescript
// in applyCheat, when willBeActive:
const writeValue = resolveWriteValue(cheat, customValueStr);
setActiveGame(prev => /* ... { ...c, active: willBeActive, activeWriteValue: writeValue } ... */);
```

Then in the poll loop use `cheat.activeWriteValue ?? cheat.onValue`. Skip this for the first cut if you want to keep the diff small — the Carry Weight / Health use case doesn't need it.

### 4. No CSS, no new Rust commands

All changes are in `useTrainer.ts`. The existing `write_int` / `write_float` / `write_double` Tauri commands handle the writes. The existing `onCheatError` plumbing surfaces failures. No new permissions, no new dependencies.

---

## Acceptance Criteria

- Toggling Skyrim's Unlimited Carry Weight to **on** writes 99999 immediately, and every `pollInterval` ms thereafter while it remains on. Equipping armour / levelling / fast travelling no longer "wears off" the cheat — the next poll reasserts.
- Toggling Unlimited Carry Weight to **off** stops the writes. The game's natural carry-weight value returns within one or two polls.
- `Infinite Health` behaves the same way — toggling it on now actually keeps health at 99999 across regen ticks instead of being clobbered.
- Patch toggles (`Instant Shouts`, `One-Hit Kill`) are unaffected — they still apply / restore byte patches exactly once per click.
- Action cheats (`Gold`, `Darwinium`) are unaffected — still one-shot writes.
- Errors from the poll-write (e.g. address became invalid because the player object moved) appear in the cheat row via the existing `cheat-error` UI; they don't flood the row because the error is keyed by cheat id and overwritten in place.
- `cargo build` and `tsc` clean.

---

## Notes

- **Why not just NOP the store instruction?** That's the patch approach used for Instant Shouts. It's more efficient (no continuous CPU work) and more permanent, but each game needs an AOB pattern for the specific store instruction, which is fragile across game versions. Polling-write is the trade: a tiny bit of overhead on a 2-second timer in exchange for working on any version where the value field is at the right offset.
- **Poll cost**: each active toggle adds one extra `WriteProcessMemory` call per tick on top of the existing `ReadProcessMemory`. At the default `pollInterval` of 2000 ms with five active toggles, that's an extra ~2.5 calls/second — negligible.
- **Race with the game's update loop**: if the game writes to the same address between our write and the next user observation, the displayed value might briefly show the game's value before our next tick re-asserts. With a 2-second poll, this is noticeable. If the experience suffers, drop `pollInterval` to 500 ms or expose a per-cheat "reassert rate" — but the default should be fine for stats that recalculate on discrete events (equip / level / travel) rather than every frame.
- **Address cache invalidation**: `addressCache` is keyed by cheat id and reset only on `selectGame`. If the player object relocates mid-session (some games reallocate), the cached address goes stale and the poll-write will silently write to garbage. This is an existing problem, not introduced by this brief — but if it shows up in practice, the fix is to clear `addressCache.current[cheat.id]` whenever `read_*` / `write_*` returns an error.
- **Why skip writing on toggle-off?** Two reasons: (1) the user expects "off" to mean off — having the engine write `onValue` one more time on the off-click would be surprising and could clobber a value the user wanted restored; (2) cleaner semantics — "off" means "stop interfering," which dovetails with the polling model.
