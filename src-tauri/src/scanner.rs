use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DetectedGame {
    pub name: String,
    pub exe_path: PathBuf,
    pub store: String,
}

pub fn scan_steam() -> Vec<DetectedGame> {
    let mut games = Vec::new();
    let steam_path = PathBuf::from("C:\\Program Files (x86)\\Steam");
    let library_vdf = steam_path.join("steamapps\\libraryfolders.vdf");
    
    if !library_vdf.exists() {
        return games;
    }

    // Basic VDF parsing for paths
    if let Ok(content) = fs::read_to_string(&library_vdf) {
        let mut library_paths = vec![steam_path.clone()];
        
        // Find all "path" "..." lines
        for line in content.lines() {
            if line.contains("\"path\"") {
                let parts: Vec<&str> = line.split('"').collect();
                if parts.len() >= 4 {
                    library_paths.push(PathBuf::from(parts[3]));
                }
            }
        }

        for path in library_paths {
            let common_apps = path.join("steamapps\\common");
            if let Ok(entries) = fs::read_dir(common_apps) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        // Heuristic: Check for .exe in root or 'bin' folder
                        let possible_exes = [
                            entry.path().join(format!("{}.exe", name)),
                            entry.path().join("bin").join(format!("{}.exe", name)),
                        ];
                        
                        for exe in possible_exes {
                            if exe.exists() {
                                games.push(DetectedGame {
                                    name: name.clone(),
                                    exe_path: exe,
                                    store: "Steam".to_string(),
                                });
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    
    games
}

#[derive(Deserialize)]
struct EpicManifest {
    #[serde(rename = "DisplayName")]
    display_name: String,
    #[serde(rename = "InstallLocation")]
    install_location: String,
    #[serde(rename = "LaunchExecutable")]
    launch_executable: String,
}

pub fn scan_epic() -> Vec<DetectedGame> {
    let mut games = Vec::new();
    let manifest_dir = PathBuf::from("C:\\ProgramData\\Epic\\EpicGamesLauncher\\Data\\Manifests");
    
    if let Ok(entries) = fs::read_dir(manifest_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("item") {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Ok(manifest) = serde_json::from_str::<EpicManifest>(&content) {
                        let exe_path = Path::new(&manifest.install_location).join(&manifest.launch_executable);
                        if exe_path.exists() {
                            games.push(DetectedGame {
                                name: manifest.display_name,
                                exe_path,
                                store: "Epic Games".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }
    games
}

pub fn scan_local() -> Vec<DetectedGame> {
    let mut games = Vec::new();
    
    // Look for any dummy-game.exe in common build locations
    let search_patterns = [
        "dummy-game/target/debug/dummy-game.exe",
        "target/debug/dummy-game.exe",
        "../dummy-game/target/debug/dummy-game.exe",
    ];

    for pattern in search_patterns {
        let path = PathBuf::from(pattern);
        if path.exists() {
            games.push(DetectedGame {
                name: "dummy-game".to_string(),
                exe_path: fs::canonicalize(path).unwrap_or_default(),
                store: "Local".to_string(),
            });
            return games;
        }
    }

    // Fallback: manually check project root for the dummy folder
    if let Ok(entries) = fs::read_dir(".") {
        for entry in entries.flatten() {
            if entry.file_name() == "dummy-game" {
                let path = entry.path().join("target/debug/dummy-game.exe");
                if path.exists() {
                    games.push(DetectedGame {
                        name: "dummy-game".to_string(),
                        exe_path: fs::canonicalize(path).unwrap_or_default(),
                        store: "Local".to_string(),
                    });
                    return games;
                }
            }
        }
    }

    games
}

pub fn scan_all() -> Vec<DetectedGame> {
    let mut all_games = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    let mut raw_games = Vec::new();
    raw_games.extend(scan_local());
    raw_games.extend(scan_steam());
    raw_games.extend(scan_epic());

    for game in raw_games {
        let canonical_path = fs::canonicalize(&game.exe_path).unwrap_or(game.exe_path.clone());
        if !seen_paths.contains(&canonical_path) {
            seen_paths.insert(canonical_path);
            all_games.push(game);
        }
    }
    
    all_games
}
