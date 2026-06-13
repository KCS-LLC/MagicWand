# Brief: Scan Mode Settings Toggle

## Status
Unimplemented.

## Problem

`scan`-type cheats require the user to enter the current in-game value, scan memory, and optionally repeat after changing the value. This is useful as a fallback but noisy for games that have a working `mono_chain` or pointer cheat — in that case the scan entry clutters the trainer UI.

The fix is a **Scan Mode** toggle in Settings. When off (the default), scan cheats are hidden. When on, they appear in the trainer alongside other cheats. This lets trainer authors ship both a primary cheat and a scan fallback without burdening the average user.

---

## Behaviour

| Scan Mode | Effect |
|-----------|--------|
| Off (default) | `type: "scan"` cheats are filtered out of the trainer cheat list |
| On | All cheats including scan are shown |

No backend changes. No trainer JSON changes. Purely a frontend display filter.

---

## Implementation

### 1. State — `src/App.tsx`

Add one state variable alongside the existing `customValues` / `scanStates` / `scanInputs` state:

```tsx
const [scanMode, setScanMode] = useState(false);
```

### 2. Cheat list filter — `src/App.tsx`

In the trainer dashboard section, filter the cheat list before rendering:

```tsx
const visibleCheats = activeGame.cheats.filter(c => c.type !== 'scan' || scanMode);
```

Replace the existing `activeGame.cheats.map(...)` with `visibleCheats.map(...)`. No other changes to the render logic.

### 3. Settings toggle — `src/pages/SettingsPage.tsx`

Add `scanMode` and `onScanModeChange` to the props interface and render a toggle row inside the existing **Trainers** section:

```tsx
interface SettingsPageProps {
  pollInterval: number;
  onPollIntervalChange: (value: number) => void;
  scanMode: boolean;
  onScanModeChange: (value: boolean) => void;
}

export function SettingsPage({ pollInterval, onPollIntervalChange, scanMode, onScanModeChange }: SettingsPageProps) {
  return (
    <div className="page-view">
      <header className="header">
        <h1>Settings</h1>
      </header>

      <section className="settings-section">
        <h2>Performance</h2>
        <div className="setting-row">
          <label>Value poll interval (ms)</label>
          <input
            type="number"
            min={500}
            max={10000}
            step={500}
            value={pollInterval}
            onChange={e => onPollIntervalChange(Number(e.target.value))}
          />
        </div>
      </section>

      <section className="settings-section">
        <h2>Trainers</h2>
        <div className="setting-row">
          <label>Scan Mode</label>
          <label className="switch">
            <input
              type="checkbox"
              checked={scanMode}
              onChange={e => onScanModeChange(e.target.checked)}
            />
            <span className="slider"></span>
          </label>
        </div>
        <p className="setting-hint">
          Show manual memory scan cheats. Useful as a fallback when automatic address resolution isn't available for a game.
        </p>
        <div className="setting-row">
          <label>Custom trainer folder</label>
          <span className="setting-value">public/trainers/</span>
        </div>
        <p className="setting-hint">
          Drop <code>.json</code> trainer files here and add them to <code>index.json</code> to load them automatically.
        </p>
      </section>

      <section className="settings-section">
        <h2>About</h2>
        <p>Magic Wand v1.0</p>
        <p>A free, open-source game trainer built with Tauri + React.</p>
      </section>
    </div>
  );
}
```

### 4. Wire up — `src/App.tsx`

Pass the new props to `SettingsPage`:

```tsx
<SettingsPage
  pollInterval={pollInterval}
  onPollIntervalChange={setPollInterval}
  scanMode={scanMode}
  onScanModeChange={setScanMode}
/>
```

No CSS changes — the existing `.switch` / `.slider` styles and `.setting-row` / `.setting-hint` classes already cover this.

---

## Acceptance Criteria

- Scan Mode toggle appears in Settings → Trainers section
- Default state is **off** — scan cheats are hidden when the app launches
- Turning Scan Mode on makes scan cheats immediately visible in any open trainer without navigating away
- Turning Scan Mode off hides them again; any in-progress scan state is cleared (existing `scanStates` reset already happens when the user navigates away or selects a game — no extra logic needed)
- Trainer views with no scan cheats are unaffected
- `cargo build` and TypeScript compile with no errors

## Notes

- **Persistence**: The setting resets to off on app restart. This is intentional — scan mode is a diagnostic tool, not a normal operating mode. Persistence can be added later via Tauri's `tauri-plugin-store` if requested.
- **Scan state cleanup**: When the user turns scan mode off while a scan is in progress, the scan input and result disappear from the UI because the cheat row is filtered out. The `scanStates` and `scanInputs` entries for that cheat remain in memory but are harmless — they'll be cleared on the next game selection or navigation event (existing behaviour).
