# Brief: Steam Path Detection via Windows Registry

## Status
Unimplemented — currently hardcoded.

## Problem
`src-tauri/src/scanner.rs` hardcodes `C:\Program Files (x86)\Steam` as the Steam installation path. Users who installed Steam to a different drive or directory will have zero games detected from Steam.

## Goal
Replace the hardcoded path with a Windows Registry lookup so `scan_steam()` correctly finds Steam regardless of install location.

## Relevant File
- `src-tauri/src/scanner.rs` — `scan_steam()` function, line 14

## Current Code (to replace)
```rust
let steam_path = PathBuf::from("C:\\Program Files (x86)\\Steam");
```

## Implementation Plan

### 1. Add the `winreg` crate
In `src-tauri/Cargo.toml`, add:
```toml
[dependencies]
winreg = "0.52"
```

### 2. Write a helper function in `scanner.rs`
```rust
use winreg::enums::*;
use winreg::RegKey;

fn find_steam_path() -> Option<PathBuf> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // Try 64-bit registry path first, then 32-bit WoW6432Node
    let subkeys = [
        r"SOFTWARE\Valve\Steam",
        r"SOFTWARE\WOW6432Node\Valve\Steam",
    ];

    for subkey in &subkeys {
        if let Ok(key) = hklm.open_subkey(subkey) {
            if let Ok(path) = key.get_value::<String, _>("InstallPath") {
                let pb = PathBuf::from(path);
                if pb.exists() {
                    return Some(pb);
                }
            }
        }
    }

    // Fallback to default path
    let default = PathBuf::from(r"C:\Program Files (x86)\Steam");
    if default.exists() { Some(default) } else { None }
}
```

### 3. Use it in `scan_steam()`
Replace:
```rust
let steam_path = PathBuf::from("C:\\Program Files (x86)\\Steam");
let library_vdf = steam_path.join("steamapps\\libraryfolders.vdf");

if !library_vdf.exists() {
    return games;
}
```
With:
```rust
let steam_path = match find_steam_path() {
    Some(p) => p,
    None => return games,
};
let library_vdf = steam_path.join("steamapps\\libraryfolders.vdf");
```

## Acceptance Criteria
- `scan_steam()` returns games when Steam is installed outside `C:\Program Files (x86)\Steam`
- Falls back gracefully (returns empty vec) when Steam is not installed at all
- No panics if registry keys are missing

## Notes
- The registry key `HKLM\SOFTWARE\Valve\Steam` → `InstallPath` is the canonical location Steam writes on install
- The `WOW6432Node` variant exists because Steam is a 32-bit process writing to 32-bit registry on 64-bit Windows
- `winreg = "0.52"` is the current stable version compatible with the Windows crate already used in this project
