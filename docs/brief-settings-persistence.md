# Brief: Settings Persistence

## Status
Unimplemented.

## Problem

`pollInterval` and `scanMode` are React state — they reset to defaults every time Magic Wand restarts. Users who change poll interval to 500ms or enable scan mode have to redo it each session.

`tauri-plugin-store` provides a lightweight JSON key-value store written to the OS app-data directory. It integrates cleanly with the existing settings pattern.

---

## Settings to persist

| Key | Type | Default |
|-----|------|---------|
| `pollInterval` | `number` | `2000` |
| `scanMode` | `boolean` | `false` |
| `alwaysOnTop` | `boolean` | `false` |

`alwaysOnTop` is included so the always-on-top brief (brief-always-on-top.md) can optionally wire into this store without a separate effort.

---

## Implementation

### 1. Rust dependency — `src-tauri/Cargo.toml`

```toml
[dependencies]
tauri-plugin-store = "2"
```

### 2. Register plugin — `src-tauri/src/lib.rs`

```rust
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        // ... existing plugins and invoke_handler
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### 3. Frontend dependency — `package.json`

```json
"@tauri-apps/plugin-store": "^2.0.0"
```

Run `npm install` after adding.

### 4. Settings hook — `src/hooks/useSettings.ts` (new file)

Encapsulate all store access in one hook so `App.tsx` stays clean:

```typescript
import { useEffect, useState } from 'react';
import { load, Store } from '@tauri-apps/plugin-store';

let storePromise: Promise<Store> | null = null;

function getStore(): Promise<Store> {
  if (!storePromise) {
    storePromise = load('settings.json', { autoSave: true });
  }
  return storePromise;
}

export function useSettings() {
  const [pollInterval, setPollIntervalState] = useState(2000);
  const [scanMode, setScanModeState] = useState(false);
  const [alwaysOnTop, setAlwaysOnTopState] = useState(false);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    getStore().then(async store => {
      const pi = await store.get<number>('pollInterval');
      const sm = await store.get<boolean>('scanMode');
      const aot = await store.get<boolean>('alwaysOnTop');
      if (pi != null) setPollIntervalState(pi);
      if (sm != null) setScanModeState(sm);
      if (aot != null) setAlwaysOnTopState(aot);
      setLoaded(true);
    });
  }, []);

  const setPollInterval = async (value: number) => {
    setPollIntervalState(value);
    const store = await getStore();
    await store.set('pollInterval', value);
  };

  const setScanMode = async (value: boolean) => {
    setScanModeState(value);
    const store = await getStore();
    await store.set('scanMode', value);
  };

  const setAlwaysOnTop = async (value: boolean) => {
    setAlwaysOnTopState(value);
    const store = await getStore();
    await store.set('alwaysOnTop', value);
  };

  return { pollInterval, setPollInterval, scanMode, setScanMode, alwaysOnTop, setAlwaysOnTop, loaded };
}
```

The `loaded` flag lets the app defer rendering until stored values are applied, preventing a flash of default values.

### 5. Wire up — `src/App.tsx`

Replace the inline `useState` calls for `pollInterval` and `scanMode` with the hook:

```tsx
import { useSettings } from './hooks/useSettings';

// Remove:
// const [scanMode, setScanMode] = useState(false);
// Replace pollInterval / setPollInterval from useTrainer with settings hook values

const { pollInterval, setPollInterval, scanMode, setScanMode, alwaysOnTop, setAlwaysOnTop, loaded } = useSettings();
const { activeGame, trainers, selectGame, applyCheat, pid } = useTrainer(pollInterval);
```

Update `useTrainer` to accept `pollInterval` as a parameter instead of owning it internally (see section 6).

Optionally gate rendering on `loaded` to avoid a flash:

```tsx
if (!loaded) return null;
```

### 6. Remove `pollInterval` from `useTrainer` — `src/hooks/useTrainer.ts`

`pollInterval` currently lives inside `useTrainer`. Move it out so `useSettings` owns it:

- Remove `const [pollInterval, setPollInterval] = useState(2000);` from `useTrainer`
- Accept `pollInterval` as a parameter: `export function useTrainer(pollInterval: number)`
- Remove `setPollInterval` from the return value
- The `useEffect` poll loop already uses `pollInterval` from closure — no other changes needed inside the hook

Remove from the hook's return:
```typescript
// before:
return { activeGame, trainers, selectGame, applyCheat, pid, pollInterval, setPollInterval };
// after:
return { activeGame, trainers, selectGame, applyCheat, pid };
```

### 7. Pass `alwaysOnTop` initial value — `src/App.tsx`

If brief-always-on-top.md is also implemented, apply the stored value on load:

```tsx
useEffect(() => {
  if (loaded) {
    getCurrentWindow().setAlwaysOnTop(alwaysOnTop);
  }
}, [loaded]);
```

---

## Acceptance Criteria

- Changing poll interval in Settings persists across app restarts
- Enabling scan mode persists across restarts (default off on first launch)
- `alwaysOnTop` persists if that feature is implemented
- No visible flash of default values on startup (gated on `loaded`)
- Settings file written to OS app-data directory (not the project folder)
- `cargo build` and TypeScript compile with no errors
- Existing settings UI and behaviour unchanged

## Notes

- **Store location**: `tauri-plugin-store` writes to `$APPDATA\com.magicwand.app\settings.json` on Windows (or the bundle identifier path). The file is human-readable JSON.
- **`autoSave: true`**: writes are flushed automatically; no explicit `store.save()` call needed.
- **First launch**: if no stored value exists, `store.get()` returns `null` — the `if (x != null)` guards keep the defaults in place.
- **`alwaysOnTop` on load**: the window's always-on-top state is not persisted by the OS, so we must re-apply it via `setAlwaysOnTop` after reading the store value. The `useEffect` in step 7 handles this.
