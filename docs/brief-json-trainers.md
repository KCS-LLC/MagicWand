# Brief: JSON/TOML File-Based Trainer Definitions

## Status
Unimplemented — trainers are currently hardcoded in TypeScript.

## Problem
All trainer definitions (game name, executable, cheats, signatures, offsets) live in a large `INITIAL_TRAINERS` constant inside `src/hooks/useTrainer.ts`. Adding or editing a trainer requires modifying source code and rebuilding the app. There is no way for users to add community trainers or for a separate agent to ship new trainer files without touching application logic.

## Goal
Move trainer definitions to external JSON files that are bundled with the app and loaded at runtime. The data shape stays identical to the existing `GameTrainer`/`Cheat` TypeScript interfaces, so no logic changes are needed — only the loading mechanism changes.

## Chosen Format: JSON
JSON is preferred over TOML here because:
- The existing TypeScript interfaces map directly to JSON without any transformation
- Tauri's asset system handles JSON natively via `fetch()`
- Easier for community contributors to write (no TOML knowledge required)

## Relevant Files
- `src/hooks/useTrainer.ts` — `INITIAL_TRAINERS` constant (lines 26–113), `useTrainer()` hook
- New files to create: `public/trainers/*.json` (one file per game)

## File Structure
```
public/
  trainers/
    skyrim-se.json
    dummy-game.json
```

Each file matches the `GameTrainer` interface:
```json
{
  "name": "Skyrim Special Edition",
  "executable": "SkyrimSE.exe",
  "cheats": [
    {
      "id": "skyrim-health-aob",
      "name": "Infinite Health",
      "type": "toggle",
      "valueType": "float",
      "module": "SkyrimSE.exe",
      "signature": "48 8B 05 ?? ?? ?? ?? 48 8B D1 48 8B 00 48 85 C0",
      "offsets": ["0x1B0", "0x0"],
      "onValue": 99999
    }
  ]
}
```

## Implementation Plan

### 1. Create the trainer JSON files
Extract the two entries from `INITIAL_TRAINERS` into:
- `public/trainers/skyrim-se.json`
- `public/trainers/dummy-game.json`

Also create a manifest file `public/trainers/index.json` that lists available trainers:
```json
["skyrim-se.json", "dummy-game.json"]
```

### 2. Add a trainer loader in `useTrainer.ts`
Replace the `const [trainers] = useState<GameTrainer[]>(INITIAL_TRAINERS)` with async loading:

```typescript
const [trainers, setTrainers] = useState<GameTrainer[]>([]);

useEffect(() => {
  async function loadTrainers() {
    try {
      const index = await fetch('/trainers/index.json').then(r => r.json()) as string[];
      const loaded = await Promise.all(
        index.map(f => fetch(`/trainers/${f}`).then(r => r.json()) as Promise<GameTrainer>)
      );
      setTrainers(loaded);
    } catch (err) {
      console.error('Failed to load trainers:', err);
    }
  }
  loadTrainers();
}, []);
```

### 3. Remove `INITIAL_TRAINERS` from `useTrainer.ts`
Delete the constant entirely (lines 26–113).

## Acceptance Criteria
- App loads with the same two trainers as before (Skyrim SE, Dummy Game)
- Adding a new `public/trainers/my-game.json` + adding its filename to `index.json` makes the game appear in the trainer list without any code changes
- No TypeScript errors — loaded JSON must match the `GameTrainer` interface

## Future Extensions (out of scope for this brief)
- User-added trainers stored in Tauri's app data directory (`appLocalDataDir`)
- A "Load Trainer" button in the Settings page that lets users browse for a `.json` file
- Community trainer download from a remote URL

## Notes
- Files in `public/` are served as static assets by Vite and bundled into the Tauri app. `fetch('/trainers/index.json')` works in both dev and production builds.
- The `GameTrainer` and `Cheat` interfaces in `useTrainer.ts` do not need to change — they already represent the correct shape.
