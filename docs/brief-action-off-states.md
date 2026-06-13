# Brief: Action Cheat Off States and UI Differentiation

## Status
Unimplemented — `action` cheats render identically to `toggle` cheats but behave differently and have no off state.

## Problem
There are three cheat types in the system (`toggle`, `action`, `patch`) but the UI renders all three identically as a toggle switch. This causes two issues:

1. **`action` cheats have no semantic off state.** An `action` (e.g. "Add 50,000 Gold") is a one-shot write — it makes no sense to "disable" it like a toggle. Flipping the switch off does nothing meaningful, yet the switch stays in the on position after firing, confusing the user.

2. **Visual ambiguity.** Users cannot tell whether a cheat is a persistent modifier (toggle/patch) or a one-shot action just by looking at the UI.

## Goal
- `toggle` and `patch` cheats keep the existing toggle switch UI — they have a genuine on/off state.
- `action` cheats get a **"Fire" button** instead of a toggle switch. Pressing it triggers the write once; there is no on/off state to track.
- Add a subtle type label (badge or icon) so users can distinguish cheat categories at a glance.

## Relevant Files
- `src/App.tsx` — cheat item rendering (lines 72–90)
- `src/hooks/useTrainer.ts` — `toggleCheat()` function
- `src/App.css` — styles

## Implementation Plan

### 1. Rename `toggleCheat` to `applyCheat` in `useTrainer.ts`
The current name implies a binary state; rename for clarity. Update all call sites in `App.tsx`.

### 2. Handle `action` type in `applyCheat`
Currently `toggleCheat` flips `cheat.active` first and then writes. For `action` cheats, skip the active state flip entirely:
```typescript
const applyCheat = async (cheat: Cheat, customValueStr?: string) => {
  if (!pid || !activeGame) return;

  if (cheat.type === 'action') {
    // One-shot write, no state change
    try {
      const addr = await resolveCheatAddress(cheat);
      const hexAddr = "0x" + BigInt(addr).toString(16);
      const writeValue = resolveWriteValue(cheat, customValueStr);
      const cmd = cheat.valueType === 'float' ? 'write_float' : 'write_int';
      await invoke(cmd, { pid, address: hexAddr, value: writeValue });
    } catch (err) {
      console.error('Action cheat failed:', err);
    }
    return;
  }

  // toggle / patch logic unchanged below...
};
```

Extract `resolveWriteValue` as a small helper (or inline it):
```typescript
function resolveWriteValue(cheat: Cheat, customValueStr?: string): number {
  if (customValueStr && customValueStr !== '') {
    const parsed = cheat.valueType === 'float'
      ? parseFloat(customValueStr)
      : parseInt(customValueStr, 10);
    if (!isNaN(parsed)) return parsed;
  }
  return cheat.onValue;
}
```

### 3. Differentiate the UI in `App.tsx`
Replace the single toggle-switch block with a conditional:
```tsx
<div className="cheat-item" key={cheat.id}>
  <div className="cheat-info">
    <div className="cheat-name-row">
      <span className="cheat-name">{cheat.name}</span>
      <span className={`cheat-type-badge cheat-type-${cheat.type}`}>{cheat.type}</span>
    </div>
    <span className="live-value">
      {cheat.currentValue !== undefined
        ? `Value: ${typeof cheat.currentValue === 'number' ? cheat.currentValue.toFixed(2) : cheat.currentValue}`
        : 'Detecting...'}
    </span>
  </div>

  {/* Custom value input (from brief-custom-value-input.md) */}
  {cheat.type !== 'patch' && cheat.valueType && (
    <input className="value-input" type="number"
      placeholder={String(cheat.onValue)}
      value={customValues[cheat.id] ?? ''}
      onChange={e => setCustomValues(prev => ({ ...prev, [cheat.id]: e.target.value }))}
      disabled={!pid} />
  )}

  {/* Action: Fire button */}
  {cheat.type === 'action' ? (
    <button
      className="fire-button"
      onClick={() => applyCheat(cheat, customValues[cheat.id])}
      disabled={!pid}
    >
      Fire
    </button>
  ) : (
    /* Toggle / Patch: switch */
    <label className="switch">
      <input
        type="checkbox"
        checked={cheat.active || false}
        onChange={() => applyCheat(cheat, customValues[cheat.id])}
        disabled={!pid}
      />
      <span className="slider"></span>
    </label>
  )}
</div>
```

### 4. Add CSS (`App.css`)
```css
.cheat-name-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.cheat-type-badge {
  font-size: 0.65rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  padding: 0.1rem 0.35rem;
  border-radius: 0.25rem;
}

.cheat-type-toggle {
  background: #1e3a5f;
  color: #60a5fa;
}

.cheat-type-action {
  background: #3b1f00;
  color: #f97316;
}

.cheat-type-patch {
  background: #1a2e1a;
  color: #4ade80;
}

.fire-button {
  background-color: #7c3aed;
  color: white;
  border: none;
  border-radius: 0.5rem;
  padding: 0.4rem 1rem;
  font-size: 0.85rem;
  font-weight: 600;
  cursor: pointer;
  transition: background-color 0.2s;
  white-space: nowrap;
}

.fire-button:hover:not(:disabled) {
  background-color: var(--accent-color);
}

.fire-button:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
```

## Acceptance Criteria
- `action` cheats display a "Fire" button instead of a toggle switch
- Clicking Fire writes the value once; the button returns to its default state immediately
- `toggle` and `patch` cheats still use the toggle switch with on/off state
- All three cheat types show a small colored type badge (toggle = blue, action = orange, patch = green)
- Disabled state (no PID) disables the Fire button just like the toggle switch

## Notes
- The `active` field on an `action` cheat is never flipped — it stays `false` (or undefined) at all times. Do not accidentally display it as "on" after firing.
- This brief assumes `brief-custom-value-input.md` is implemented concurrently. If not, omit the `value-input` conditional and the `customValues` references — the rest of this brief is independent.
