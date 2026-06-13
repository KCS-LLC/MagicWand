# Brief: Refresh Game List

## Status
Unimplemented.

## Problem

`scan_games` runs once when Magic Wand launches. If the user starts a game after Magic Wand is already open, it won't appear in the library until the app is restarted. A rescan button fixes this with no backend changes.

---

## Implementation

### 1. Expose rescan — `src/App.tsx`

The existing `fetchGames` function is defined inside a `useEffect` and not reachable from outside it. Lift it to the component scope and expose it as a handler:

```tsx
// Replace the current useEffect with:
const [loading, setLoading] = useState(true);

const fetchGames = async () => {
  setLoading(true);
  try {
    const games = await invoke<DetectedGame[]>("scan_games");
    setDetectedGames(games);
  } catch (error) {
    console.error("APP: scan_games error:", error);
  } finally {
    setLoading(false);
  }
};

useEffect(() => {
  fetchGames();
}, []);
```

### 2. Rescan button — `src/App.tsx`

Add a refresh button to the library header, next to the search bar:

```tsx
<header className="header">
  <h1>My Games</h1>
  <div style={{ display: 'flex', gap: '0.75rem', alignItems: 'center' }}>
    <input
      type="text"
      className="search-bar"
      placeholder="Search library..."
      value={searchQuery}
      onChange={(e) => setSearchQuery(e.target.value)}
    />
    <button
      className="fire-button"
      onClick={fetchGames}
      disabled={loading}
      title="Rescan for running games"
    >
      {loading ? '...' : '↻'}
    </button>
  </div>
</header>
```

No new CSS needed — `.fire-button` already provides the correct styling. The `↻` character renders as a refresh symbol; when loading it shows `...` to indicate activity.

---

## Acceptance Criteria

- A refresh button (↻) appears in the library header next to the search bar
- Clicking it re-runs `scan_games` and updates the game list
- The button shows `...` and is disabled while scanning
- Games launched after Magic Wand opened appear in the list after a rescan
- Games that have since closed are removed from the list after a rescan
- The loading state on initial launch behaves identically to before

## Notes

- **No Rust changes**: `scan_games` already re-enumerates running processes each time it's called — it has no internal cache. Calling it again from the frontend is sufficient.
- **Search query**: the search filter is applied to `detectedGames` state after fetch, so it automatically re-applies to the refreshed list without any extra logic.
- **Button sizing**: `.fire-button` has `white-space: nowrap` and fixed padding. The `↻` glyph fits comfortably. If a text label is preferred, `"Rescan"` works equally well.
