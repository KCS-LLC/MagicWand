import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTrainer } from "./hooks/useTrainer";
import { useSettings } from "./hooks/useSettings";
import { CommunityPage } from "./pages/CommunityPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TrainerDashboard } from "./components/TrainerDashboard";
import "./App.css";

type Page = 'library' | 'community' | 'settings';

interface DetectedGame {
  name: string;
  exe_path: string;
  store: string;
}

function App() {
  const [detectedGames, setDetectedGames] = useState<DetectedGame[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [cheatErrors, setCheatErrors] = useState<Record<string, string>>({});
  const [currentPage, setCurrentPage] = useState<Page>('library');
  const { pollInterval, setPollInterval, scanMode, setScanMode, alwaysOnTop, setAlwaysOnTop, devMode, setDevMode, loaded } = useSettings();
  const setCheatError = useCallback((id: string, msg: string) =>
    setCheatErrors(prev => ({ ...prev, [id]: msg })), []);

  const { activeGame, trainers, selectGame, applyCheat, pid } = useTrainer(pollInterval, setCheatError);

  const resetCheatState = useCallback(() => {
    setCheatErrors({});
  }, []);

  useEffect(() => {
    if (loaded && alwaysOnTop) {
      getCurrentWindow().setAlwaysOnTop(true);
    }
  }, [loaded]);

  const fetchGames = useCallback(async () => {
    setLoading(true);
    try {
      const games = await invoke<DetectedGame[]>("scan_games");
      setDetectedGames(games);
    } catch (error) {
      console.error("APP: scan_games error:", error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchGames();
  }, [fetchGames]);

  if (!loaded) return null;

  const toggleAlwaysOnTop = async () => {
    const next = !alwaysOnTop;
    await getCurrentWindow().setAlwaysOnTop(next);
    setAlwaysOnTop(next);
  };

  const navTo = (page: Page) => {
    resetCheatState();
    selectGame(null);
    setCurrentPage(page);
  };

  const handleGameClick = (game: DetectedGame) => {
    const trainer = trainers.find(t =>
      t.executable.toLowerCase() === (game.name + ".exe").toLowerCase() ||
      t.executable.toLowerCase() === game.name.toLowerCase() ||
      t.name.toLowerCase() === game.name.toLowerCase()
    );
    resetCheatState();
    if (trainer) selectGame(trainer);
  };

  return (
    <div className="app-container">
      <aside className="sidebar">
        <div className="logo"><span>✨</span> Magic Wand</div>
        <nav>
          <div
            className={`nav-item ${currentPage === 'library' && !activeGame ? 'active' : ''}`}
            onClick={() => navTo('library')}
          >
            Library
          </div>
          <div
            className={`nav-item ${currentPage === 'community' && !activeGame ? 'active' : ''}`}
            onClick={() => navTo('community')}
          >
            Community
          </div>
          <div
            className={`nav-item ${currentPage === 'settings' && !activeGame ? 'active' : ''}`}
            onClick={() => navTo('settings')}
          >
            Settings
          </div>
        </nav>
        <div style={{ marginTop: 'auto' }}>
          <button
            className={`nav-item ${alwaysOnTop ? 'active' : ''}`}
            onClick={toggleAlwaysOnTop}
            title="Keep Magic Wand on top of other windows"
            style={{ width: '100%', background: 'none', border: 'none', cursor: 'pointer', textAlign: 'left' }}
          >
            📌 Always on Top
          </button>
        </div>
      </aside>

      <main className="main-content">
        {activeGame ? (
          <TrainerDashboard
            activeGame={activeGame}
            pid={pid}
            scanMode={scanMode}
            devMode={devMode}
            cheatErrors={cheatErrors}
            setCheatError={setCheatError}
            applyCheat={applyCheat}
            onBack={() => { resetCheatState(); selectGame(null); }}
          />
        ) : currentPage === 'community' ? (
          <CommunityPage />
        ) : currentPage === 'settings' ? (
          <SettingsPage pollInterval={pollInterval} onPollIntervalChange={setPollInterval} scanMode={scanMode} onScanModeChange={setScanMode} devMode={devMode} onDevModeChange={setDevMode} />
        ) : (
          <div className="library-view">
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
                <button className="fire-button" onClick={fetchGames} disabled={loading} title="Rescan for running games">
                  {loading ? '...' : '↻'}
                </button>
              </div>
            </header>

            {loading ? (
              <div className="loading-state">Scanning system for games...</div>
            ) : (
              <div className="game-grid">
                {detectedGames
                  .filter(g => g.name.toLowerCase().includes(searchQuery.toLowerCase()))
                  .map((game, index) => (
                    <div className="game-card" key={index} onClick={() => handleGameClick(game)}>
                      <div className="game-image">🎮</div>
                      <div className="game-info">
                        <p className="game-title">{game.name}</p>
                        <span className="game-store">{game.store}</span>
                      </div>
                    </div>
                  ))
                }
              </div>
            )}
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
