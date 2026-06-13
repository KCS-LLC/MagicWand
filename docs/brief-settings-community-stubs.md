# Brief: Implement Settings and Community Pages

## Status
Stub — nav items exist in the sidebar but clicking them does nothing meaningful.

## Problem
The sidebar has three nav items: Library, Community, and Settings. Library works. Community and Settings are rendered as inert `<div>` elements with no click handlers, navigation state, or content. Users who click them see no response.

## Goal
Wire up both nav items so they display a real page with appropriate content. Neither page needs to be deeply functional for v1 — the goal is that clicking them shows a clearly designed page rather than dead UI.

### Settings Page (MVP scope)
A settings panel that lets users manage app-level preferences. For v1, the following settings are sufficient:
- **Poll interval** — how often live values refresh (currently hardcoded to 2000ms in `useTrainer.ts`)
- **Trainer folder path** — show (read-only for now) where custom trainer JSON files should be placed (once `brief-json-trainers.md` is implemented)
- **About section** — app name, version (from `tauri.conf.json`), and a link to the project repo

### Community Page (MVP scope)
A page that explains the community trainer system and shows placeholder content for a future trainer repository. For v1:
- A header explaining what community trainers are
- A card-style list of "coming soon" placeholder entries (2–3 cards with dummy game names)
- A call-to-action pointing users to the docs or GitHub to submit trainers

## Relevant Files
- `src/App.tsx` — nav items (lines 52–57), main content render logic
- `src/App.css` — styles
- `src-tauri/tauri.conf.json` — app version (for About section)

## Current Nav Code (lines 52–57 in App.tsx)
```tsx
<nav>
  <div className={`nav-item ${!activeGame ? 'active' : ''}`} onClick={() => selectGame(null)}>
    Library
  </div>
  <div className="nav-item">Community</div>
  <div className="nav-item">Settings</div>
</nav>
```

## Implementation Plan

### 1. Add page state to `App.tsx`
```typescript
type Page = 'library' | 'community' | 'settings';
const [currentPage, setCurrentPage] = useState<Page>('library');
```

When a game is selected, `currentPage` stays as-is (trainer dashboard overlays regardless). When going back from a trainer, return to whichever page was active.

### 2. Wire nav items
```tsx
<div className={`nav-item ${currentPage === 'library' && !activeGame ? 'active' : ''}`}
  onClick={() => { selectGame(null); setCurrentPage('library'); }}>
  Library
</div>
<div className={`nav-item ${currentPage === 'community' && !activeGame ? 'active' : ''}`}
  onClick={() => { selectGame(null); setCurrentPage('community'); }}>
  Community
</div>
<div className={`nav-item ${currentPage === 'settings' && !activeGame ? 'active' : ''}`}
  onClick={() => { selectGame(null); setCurrentPage('settings'); }}>
  Settings
</div>
```

### 3. Route main content area
Replace the current `!activeGame` branch:
```tsx
{activeGame ? (
  <TrainerDashboard ... />
) : currentPage === 'community' ? (
  <CommunityPage />
) : currentPage === 'settings' ? (
  <SettingsPage pollInterval={pollInterval} onPollIntervalChange={setPollInterval} />
) : (
  <LibraryView ... />
)}
```

### 4. Extract components (keep in `App.tsx` or split into `src/pages/`)
Either inline the JSX or create:
- `src/pages/CommunityPage.tsx`
- `src/pages/SettingsPage.tsx`

#### `CommunityPage` JSX structure
```tsx
<div className="page-view">
  <header className="header"><h1>Community Trainers</h1></header>
  <p className="page-description">
    Community trainers are JSON files anyone can write and share.
    Drop a trainer file into the trainers folder and it will appear in your Library automatically.
  </p>
  <div className="community-grid">
    {/* 2-3 placeholder cards */}
    <div className="community-card coming-soon">
      <span className="game-icon">🎮</span>
      <p>More trainers coming soon</p>
    </div>
  </div>
</div>
```

#### `SettingsPage` JSX structure
```tsx
<div className="page-view">
  <header className="header"><h1>Settings</h1></header>

  <section className="settings-section">
    <h2>Performance</h2>
    <div className="setting-row">
      <label>Value poll interval (ms)</label>
      <input type="number" min={500} max={10000} step={500}
        value={pollInterval} onChange={e => onPollIntervalChange(Number(e.target.value))} />
    </div>
  </section>

  <section className="settings-section">
    <h2>About</h2>
    <p>Magic Wand v1.0</p>
    <p>A game trainer built with Tauri + React</p>
  </section>
</div>
```

### 5. Wire poll interval to `useTrainer`
Export `pollInterval` as a settable value from the hook:
```typescript
// In useTrainer.ts
const [pollInterval, setPollInterval] = useState(2000);
// Change the setInterval call to use pollInterval
// Return pollInterval and setPollInterval
```

In `App.tsx`, pass `pollInterval`/`setPollInterval` from the hook to `SettingsPage`.

### 6. Add CSS
```css
.page-view {
  max-width: 800px;
}

.page-description {
  color: var(--text-muted);
  margin-bottom: 2rem;
  line-height: 1.6;
}

.settings-section {
  background-color: var(--surface-color);
  border: 1px solid #1e293b;
  border-radius: 0.75rem;
  padding: 1.5rem;
  margin-bottom: 1.5rem;
}

.settings-section h2 {
  margin: 0 0 1rem 0;
  font-size: 1rem;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.setting-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.community-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 1.5rem;
}

.community-card {
  background-color: var(--surface-color);
  border: 1px dashed #334155;
  border-radius: 1rem;
  padding: 2rem 1rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.75rem;
  color: var(--text-muted);
}
```

## Acceptance Criteria
- Clicking Community in the sidebar shows the Community page; Library highlights go away
- Clicking Settings shows the Settings page
- Clicking a game from Library (or Community) navigates to the trainer dashboard; back button returns to whichever page was active
- Changing the poll interval in Settings takes effect immediately (no restart needed)
- No TypeScript errors
