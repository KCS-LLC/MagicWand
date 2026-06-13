# Brief: Custom Value Input for Cheats

## Status
Unimplemented — cheats only write a hardcoded `onValue` from the trainer definition.

## Problem
Every cheat has a fixed `onValue` (e.g. `99999` for health, `50000` for gold). Users cannot choose their own value — if you want 500 gold instead of 50000, there is no way to do that without editing the trainer file. The UI currently shows a live read of the current value but provides no way to write a custom amount.

## Goal
For `toggle` and `action` type cheats with a `valueType` of `int` or `float`, show an editable input field next to the cheat so the user can type a custom value. The typed value replaces `onValue` at write time. Patch-type cheats (NOP sleds) are excluded — they deal in bytes, not numeric values.

## Relevant Files
- `src/hooks/useTrainer.ts` — `toggleCheat()` function, `Cheat` interface
- `src/App.tsx` — cheat item rendering (lines 72–90)
- `src/App.css` — styles

## Implementation Plan

### 1. Track custom values in component state (`App.tsx`)
Add a state map in the `App` component:
```typescript
const [customValues, setCustomValues] = useState<Record<string, string>>({});
```

### 2. Render an input for eligible cheats (`App.tsx`)
Inside the `.cheat-item` div, after `.cheat-info`, add a conditional input:
```tsx
{(cheat.type === 'toggle' || cheat.type === 'action') && cheat.valueType && (
  <input
    className="value-input"
    type="number"
    placeholder={String(cheat.onValue)}
    value={customValues[cheat.id] ?? ''}
    onChange={(e) => setCustomValues(prev => ({ ...prev, [cheat.id]: e.target.value }))}
    disabled={!pid}
  />
)}
```

### 3. Pass the custom value into `toggleCheat`
Modify the `onChange` for the toggle switch to pass the custom value:
```tsx
onChange={() => toggleCheat(cheat, customValues[cheat.id])}
```

Update the `toggleCheat` signature in `useTrainer.ts`:
```typescript
const toggleCheat = async (cheat: Cheat, customValueStr?: string) => {
```

Inside `toggleCheat`, resolve the write value:
```typescript
const writeValue = (() => {
  if (customValueStr !== undefined && customValueStr !== '') {
    const parsed = cheat.valueType === 'float'
      ? parseFloat(customValueStr)
      : parseInt(customValueStr, 10);
    if (!isNaN(parsed)) return parsed;
  }
  return cheat.onValue;
})();
```

Then use `writeValue` instead of `cheat.onValue` in the `invoke` call:
```typescript
await invoke(cmd, { pid, address: hexAddr, value: writeValue });
```

### 4. Add CSS for `.value-input` (`App.css`)
```css
.value-input {
  width: 90px;
  background-color: #0b0e14;
  border: 1px solid #334155;
  border-radius: 0.375rem;
  color: var(--text-color);
  padding: 0.25rem 0.5rem;
  font-size: 0.85rem;
  text-align: right;
  margin-right: 0.75rem;
}

.value-input:focus {
  outline: none;
  border-color: var(--accent-color);
}

.value-input:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
```

### 5. Layout adjustment (`App.css`)
The `.cheat-item` flex row currently has only `.cheat-info` and the toggle switch. With a third element (the input), add a `gap` to space them:
```css
.cheat-item {
  gap: 1rem;
}
```

## Acceptance Criteria
- `toggle` and `action` cheats with `valueType: 'int'` or `'float'` show a number input
- When the input is empty, the cheat uses the default `onValue` from the trainer definition
- When the input has a value, that value is written instead
- Patch-type cheats show no input (bytes, not numbers)
- Input is disabled when the game is not connected (no PID)
- Input does not submit or trigger any action on Enter — only the toggle/button does

## Notes
- `customValues` lives in `App.tsx` state (not in the hook) because it is purely UI-level. The hook only needs the resolved numeric value at write time.
- For `action` cheats there is currently no button in the UI (they share the toggle). That interaction is addressed in `brief-action-off-states.md`.
- Validation only needs to exclude `NaN` — no need for min/max enforcement at this stage.
