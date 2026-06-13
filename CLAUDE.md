# Magic Wand — Agent Instructions

## Project Overview
Magic Wand is a Tauri v2 (Rust + React/TypeScript) desktop game trainer for Windows. It attaches to running game processes and performs memory read/write/patch operations via a polished UI.

## Hard Rules

### NO KEYBOARD HOTKEYS OR GLOBAL HOTKEYS
**Do not implement hotkey support of any kind.** This includes:
- Global hotkeys (`global-hotkey` crate or any equivalent)
- In-app keyboard shortcuts tied to cheat actions
- Any keybinding system for toggling cheats

**Why:** Magic Wand runs alongside games. Any key bound as a hotkey will also fire inside the game, causing unintended actions (firing weapons, opening menus, etc.) that ruin the experience. The `global-hotkey` crate is already present as an unused dependency in `src-tauri/Cargo.toml` — remove it, do not use it.

Cheats are activated **only** through the Magic Wand UI (toggle switches and Fire buttons).

## Docs Folder
`/docs` contains implementation briefs for planned features. Each brief is self-contained with file paths, code, and acceptance criteria. Work from the briefs; do not invent features not described there.

## Tech Stack
- **Frontend:** React + TypeScript (Vite)
- **Backend:** Rust (Tauri v2)
- **Memory ops:** Windows API via `windows` crate (`ReadProcessMemory`, `WriteProcessMemory`, `VirtualProtectEx`)
- **Game detection:** Steam (VDF + registry), Epic Games (manifest JSON), local builds
- **Trainer definitions:** JSON files in `public/trainers/`, loaded at runtime
