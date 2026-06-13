# Brief: Cheat Error Feedback

## Status
Unimplemented.

## Problem

When a cheat fails — wrong Mono offset, null singleton pointer, game not fully loaded — the UI shows nothing. The cheat row looks idle, the toggle does nothing, and the user has no way to know why. The error is swallowed in a `catch` block and logged to the devtools console.

This brief surfaces per-cheat errors directly in the trainer UI.

---

## Behaviour

- While a cheat is resolving its address for the first time: show a subtle "Resolving..." label
- If resolution or a write fails: show a small error message below the cheat name
- Error clears when the user navigates away or selects a different game (existing reset behaviour)
- Errors do not block other cheats — each row is independent

---

## Implementation

### 1. Error state — `src/App.tsx`

Add a map of cheat ID → error string alongside the existing state:

```tsx
const [cheatErrors, setCheatErrors] = useState<Record<string, string>>({});
```

Reset it alongside `scanStates` and `scanInputs` wherever those are reset:

```tsx
// In navTo, handleGameClick, and the back button onClick:
setCheatErrors({});
```

### 2. Expose error setter from `useTrainer` — `src/hooks/useTrainer.ts`

`applyCheat` currently catches errors and only logs them. Add an optional `onError` callback parameter:

```typescript
const applyCheat = async (cheat: Cheat, customValueStr?: string, onError?: (id: string, msg: string) => void) => {
  if (!pid || !activeGame) return;

  if (cheat.type === 'action' || cheat.type === 'mono' || cheat.type === 'mono_chain') {
    try {
      const addr = await resolveCheatAddress(cheat);
      const hexAddr = "0x" + BigInt(addr).toString(16);
      const writeValue = resolveWriteValue(cheat, customValueStr);
      const cmd = cheat.valueType === 'double' ? 'write_double' : cheat.valueType === 'float' ? 'write_float' : 'write_int';
      await invoke(cmd, { pid, address: hexAddr, value: writeValue });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      onError?.(cheat.id, msg);
    }
    return;
  }

  // toggle/patch path — same pattern:
  try {
    // ... existing toggle logic
  } catch (err) {
    // revert active state (existing behaviour)
    setActiveGame(prev => { /* ... */ });
    const msg = err instanceof Error ? err.message : String(err);
    onError?.(cheat.id, msg);
  }
};
```

Also surface address resolution errors from the poll loop. In the poll `Promise.all`, the inner catch currently returns `'???'` — additionally fire `onError` if a handler is available. Since the poll loop runs inside `useTrainer` without access to `onError`, the simplest approach is to store errors in a ref inside the hook and expose them:

```typescript
const cheatErrorsRef = useRef<Record<string, string>>({});

// in poll loop inner catch:
catch (e) {
  const msg = e instanceof Error ? e.message : String(e);
  cheatErrorsRef.current[cheat.id] = msg;
  return { id: cheat.id, val: '???' };
}

// expose from hook:
return { activeGame, trainers, selectGame, applyCheat, pid, cheatErrors: cheatErrorsRef.current };
```

In `App.tsx`, merge the hook's `cheatErrors` into the local state on each poll tick by syncing after `setActiveGame` — or simply read from `cheatErrorsRef.current` directly in the render since it's a ref (no re-render triggered). To trigger re-renders on error changes, keep the local `cheatErrors` state in `App.tsx` and pass `setCheatErrors` into `useTrainer` as a callback, similar to `onError` above.

The cleanest approach: keep error state entirely in `App.tsx` and pass a single `onError` callback to both `applyCheat` and the poll loop via the hook:

```typescript
// useTrainer signature change:
export function useTrainer(pollInterval: number, onCheatError?: (id: string, msg: string) => void)

// poll loop inner catch:
catch (e) {
  onCheatError?.(cheat.id, e instanceof Error ? e.message : String(e));
  return { id: cheat.id, val: '???' };
}
```

In `App.tsx`:
```tsx
const { activeGame, trainers, selectGame, applyCheat, pid } = useTrainer(
  pollInterval,
  (id, msg) => setCheatErrors(prev => ({ ...prev, [id]: msg }))
);
```

And pass to `applyCheat` calls:
```tsx
onClick={() => applyCheat(cheat, customValues[cheat.id], (id, msg) => setCheatErrors(prev => ({ ...prev, [id]: msg })))}
```

### 3. UI — `src/App.tsx`

Below the `.cheat-info` div in each cheat row, render the error if present:

```tsx
<div className="cheat-info">
  <div className="cheat-name-row">
    <span className="cheat-name">{cheat.name}</span>
    <span className={`cheat-type-badge cheat-type-${cheat.type}`}>{cheat.type}</span>
  </div>
  {/* existing live-value span */}
  {cheatErrors[cheat.id] && (
    <span className="cheat-error">{cheatErrors[cheat.id]}</span>
  )}
</div>
```

### 4. CSS — `src/App.css`

```css
.cheat-error {
  font-size: 0.75rem;
  color: #f87171;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  opacity: 0.85;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 340px;
}
```

Red monospace, truncated to one line so a long Rust error string doesn't break the layout.

---

## Acceptance Criteria

- When a mono/mono_chain cheat fails to resolve (e.g. "No parent class found for Calculator"), the error string appears below the cheat name in red
- When a write fails (e.g. process access denied), the error appears on the cheat row
- Errors clear when navigating away or selecting a different game
- A cheat with an error does not affect other cheats on the same trainer
- Errors from the poll loop (repeated resolution failures) are shown but don't spam — the same error displayed once per cheat is sufficient (the ref approach naturally handles this since subsequent identical errors overwrite)
- `cargo build` and TypeScript compile with no errors

## Notes

- **Error message source**: Rust errors propagate to the frontend as plain strings via Tauri's `Result<_, String>` return type. They're already human-readable (e.g. "Assembly 'Assembly-CSharp' not found", "Instance pointer is null — singleton not yet initialized").
- **Truncation**: Rust errors can be long. The `max-width` + `text-overflow: ellipsis` on `.cheat-error` keeps the row from expanding.
- **Poll spam**: the poll runs every 2 seconds. Without deduplication, a broken cheat would call `onCheatError` on every tick. Since `setCheatErrors` is `prev => ({ ...prev, [id]: msg })`, writing the same value repeatedly is harmless (React batches identical state updates). No explicit dedup needed.
