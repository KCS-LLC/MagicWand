# Civilization VII — Engine & Trainer Viability Research

_Researched: 2026-07-06_

---

## Engine

**Proprietary Firaxis engine** — not Unity, not Unreal.

Confirmed from install directory (`Base\Binaries\Win64`):
- `v8.dll`, `v8_libbase.dll`, `v8_libplatform.dll` — Google V8 JavaScript engine embedded for game logic and scripting
- `cohtml.WindowsDesktop.dll`, `RenoirCore.WindowsDesktop.dll` — Coherent Labs HTML/CSS UI renderer (same approach as Civ VI)
- `Civ7_Win64_DX12_FinalRelease.exe` — custom Firaxis executable, no UE/Unity telltales
- No `mono-2.0-bdwgc.dll`, no `GameAssembly.dll`, no Unreal Engine markers

There is no runtime reflection system (no GObjects, no Mono metadata). Game state lives in custom C++ structs with a V8 JavaScript scripting layer on top. Struct layouts are undocumented and change with patches.

---

## Anti-Cheat

2K ships an aggressive anti-cheat with Civ VII that:
- Detects Cheat Engine and WeMod even when they are **installed but not running** (directory scan, not process scan)
- Shuts the game down immediately on detection
- Applies in **single-player** — not just multiplayer
- Tied to 2K account sync / server-side progression, making bypass riskier

This generated significant player backlash on Steam forums. The system is more invasive than typical single-player game protection.

---

## Memory Injection Verdict: Not Viable

Even setting aside anti-cheat, the lack of a reflection system means every cheat would require manual reverse engineering of opaque C++ structs — offsets that change every patch. Combined with active process scanning that terminates the game when external tools are detected, the ReadProcessMemory approach Magic Wand uses is not viable for Civ VII at this time.

---

## What Does Work

The game ships a **first-party mod API** backed by the V8 scripting layer. Mods dropped into:
```
%LOCALAPPDATA%\Firaxis Games\Sid Meier's Civilization VII\Mods\
```
have full access to game state (gold, influence, research, unit stats, etc.) through the official scripting interface. The anti-cheat does not block the mod system.

Working community examples (both already installed):
- **Ea-Nasir's Cheat Tablet** (Nexus Mods #1) — cheat tablet mod
- **Sovereign Cheat Panel** (Nexus Mods #22) — tabbed panel in the sub-system dock, last updated May 2026 for Test of Time expansion

If Magic Wand were to support Civ VII, the right approach would be **mod file generation** — writing a mod into the Mods directory rather than using ReadProcessMemory. This is a different capability from the current engine and is out of scope unless mod generation becomes a supported feature.

---

## If the Anti-Cheat Situation Changes

If a future patch weakens the anti-cheat (has happened with other 2K titles), re-evaluate with:
1. Check if CE can attach without the game shutting down
2. Scan for the V8 heap — game variables stored in JS would be accessible via V8 inspector protocol (same approach as RPG Maker MV/MZ)
3. Alternatively, scan for currency/yield values via Magic Wand's value scanner as a baseline

Until then, the mod API is the correct and stable path.
