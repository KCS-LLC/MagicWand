# Magic Wand

Magic Wand is a free, open-source alternative to game trainer platforms like WeMod. It provides a unified dashboard to detect installed games and apply memory-based cheats (trainers) via a lightweight Tauri + Rust architecture.

## Features (Current)

- **Game Detection:** Automatically scans for installed Steam games.
- **Memory Engine:** Low-level Rust backend capable of reading and writing to game process memory.
- **Modern UI:** Responsive dark-themed dashboard built with React and Vanilla CSS.

## Tech Stack

- **Frontend:** React, TypeScript, Vite, Vanilla CSS.
- **Backend:** Rust, Tauri 2.0.
- **System Access:** Windows API (via `windows` crate), `sysinfo`.

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Node.js](https://nodejs.org/) (npm)
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (usually included in Windows 10/11)

### Installation

1. Clone the repository:
   ```bash
   cd magic-wand
   ```
2. Install dependencies:
   ```bash
   npm install
   ```
3. Run the application in development mode:
   ```bash
   npm run tauri dev
   ```

## Project Structure

- `src/`: React frontend source code.
- `src-tauri/`: Rust backend source code.
  - `src/engine.rs`: Process management and memory manipulation logic.
  - `src/scanner.rs`: Game detection logic (Steam/Epic/GOG).
  - `src/lib.rs`: Tauri command registration and app lifecycle.
