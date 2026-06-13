# Brief: Always-On-Top Toggle

## Status
Unimplemented.

## Problem

Magic Wand needs to remain visible while the user is playing a game. Without always-on-top, switching to the game minimizes or covers the trainer window, forcing the user to alt-tab every time they want to apply a cheat.

Tauri v2 exposes `setAlwaysOnTop` on the window object — this is a one-call frontend change plus a capability declaration.

---

## Implementation

### 1. Tauri capability — `src-tauri/capabilities/default.json`

Add the always-on-top permission to the existing capabilities file:

```json
"core:window:allow-set-always-on-top"
```

Full file context — add to the `"permissions"` array alongside existing entries:

```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-set-always-on-top"
  ]
}
```

### 2. State — `src/App.tsx`

Add one state variable:

```tsx
const [alwaysOnTop, setAlwaysOnTop] = useState(false);
```

### 3. Toggle handler — `src/App.tsx`

```tsx
import { getCurrentWindow } from '@tauri-apps/api/window';

const toggleAlwaysOnTop = async () => {
  const next = !alwaysOnTop;
  await getCurrentWindow().setAlwaysOnTop(next);
  setAlwaysOnTop(next);
};
```

### 4. UI — `src/App.tsx`

Add a pin button to the sidebar below the nav items, or to the top of the main content header. The sidebar placement keeps it always accessible regardless of which page is active:

```tsx
<aside className="sidebar">
  <div className="logo"><span>✨</span> Magic Wand</div>
  <nav>
    {/* existing nav items */}
  </nav>
  <div style={{ marginTop: 'auto' }}>
    <button
      className={`nav-item ${alwaysOnTop ? 'active' : ''}`}
      onClick={toggleAlwaysOnTop}
      title="Keep Magic Wand on top of other windows"
    >
      📌 Always on Top
    </button>
  </div>
</aside>
```

Using `style={{ marginTop: 'auto' }}` pushes the button to the bottom of the sidebar flex column, separating it visually from the navigation links.

No new CSS needed — `.nav-item` and `.nav-item.active` already provide the correct styling for both states.

---

## Acceptance Criteria

- A "Always on Top" button appears at the bottom of the sidebar
- Clicking it pins Magic Wand above all other windows including the game
- The button shows the active (purple) style when pinned, muted style when not
- Clicking again unpins the window
- State defaults to off on launch
- `cargo build` and TypeScript compile with no errors

## Notes

- **No Rust changes needed** — `setAlwaysOnTop` is handled entirely by Tauri's core window plugin; no custom commands required.
- **Persistence**: this brief does not persist the setting across restarts. If persistence is desired, wire it through the settings store (see brief-settings-persistence.md) by saving `alwaysOnTop` alongside `pollInterval`.
- **Capability file location**: in a fresh Tauri v2 project the file is at `src-tauri/capabilities/default.json`. If the project uses a different capabilities file name, add the permission there instead.
