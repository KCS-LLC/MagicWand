import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTrainer } from "./hooks/useTrainer";
import { CommunityPage } from "./pages/CommunityPage";
import { SettingsPage } from "./pages/SettingsPage";
import "./App.css";

interface DetectedGame {
  name: string;
  exe_path: string;
  store: string;
}

type Page = 'library' | 'community' | 'settings';

function App() {
  const [detectedGames, setDetectedGames] = useState<DetectedGame[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [currentPage, setCurrentPage] = useState<Page>('library');
  const [customValues, setCustomValues] = useState<Record<string, string>>({});
  const { activeGame, trainers, selectGame, applyCheat, pid, pollInterval, setPollInterval } = useTrainer();

  useEffect(() => {
    async function fetchGames() {
      try {
        const games = await invoke<DetectedGame[]>("scan_games");
        setDetectedGames(games);
      } catch (error) {
        console.error("APP: scan_games error:", error);
      } finally {
        setLoading(false);
      }
    }
    fetchGames();
  }, []);

  const handleGameClick = (game: DetectedGame) => {
    const trainer = trainers.find(t =>
      t.executable.toLowerCase() === (game.name + ".exe").toLowerCase() ||
      t.executable.toLowerCase() === game.name.toLowerCase() ||
      t.name.toLowerCase() === game.name.toLowerCase()
    );

    if (trainer) {
      selectGame(trainer);
    } else {
      if (trainers.length > 0) selectGame(trainers[0]);
    }
  };

  const navTo = (page: Page) => {
    selectGame(null);
    setCurrentPage(page);
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
      </aside>

      <main className="main-content">
        {activeGame ? (
          <div className="trainer-dashboard">
            <button className="back-button" onClick={() => selectGame(null)}>← Back</button>
            <div className="trainer-header">
              <h1>{activeGame.name}</h1>
              <span className={`status-badge ${pid ? 'status-online' : 'status-offline'}`}>
                {pid ? `CONNECTED (PID: ${pid})` : 'WAITING FOR GAME...'}
              </span>
            </div>

            <div className="cheat-list">
              {activeGame.cheats.map((cheat) => (
                <div className="cheat-item" key={cheat.id}>
                  <div className="cheat-info">
                    <div className="cheat-name-row">
                      <span className="cheat-name">{cheat.name}</span>
                      <span className={`cheat-type-badge cheat-type-${cheat.type}`}>{cheat.type}</span>
                    </div>
                    <span className="live-value">
                      {cheat.currentValue !== undefined
                        ? `Value: ${typeof cheat.currentValue === 'number' ? cheat.currentValue.toFixed(2) : cheat.currentValue}`
                        : 'Detecting...'}
                    </span>
                  </div>

                  {cheat.type !== 'patch' && cheat.valueType && (
                    <input
                      className="value-input"
                      type="number"
                      placeholder={String(cheat.onValue)}
                      value={customValues[cheat.id] ?? ''}
                      onChange={e => setCustomValues(prev => ({ ...prev, [cheat.id]: e.target.value }))}
                      disabled={!pid}
                    />
                  )}

                  {cheat.type === 'action' ? (
                    <button
                      className="fire-button"
                      onClick={() => applyCheat(cheat, customValues[cheat.id])}
                      disabled={!pid}
                    >
                      Fire
                    </button>
                  ) : (
                    <label className="switch">
                      <input
                        type="checkbox"
                        checked={cheat.active || false}
                        onChange={() => applyCheat(cheat, customValues[cheat.id])}
                        disabled={!pid}
                      />
                      <span className="slider"></span>
                    </label>
                  )}
                </div>
              ))}
            </div>
          </div>
        ) : currentPage === 'community' ? (
          <CommunityPage />
        ) : currentPage === 'settings' ? (
          <SettingsPage pollInterval={pollInterval} onPollIntervalChange={setPollInterval} />
        ) : (
          <div className="library-view">
            <header className="header">
              <h1>My Games</h1>
              <input
                type="text"
                className="search-bar"
                placeholder="Search library..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
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
