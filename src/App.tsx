import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTrainer } from "./hooks/useTrainer";
import "./App.css";

interface DetectedGame {
  name: string;
  exe_path: string;
  store: string;
}

function App() {
  const [detectedGames, setDetectedGames] = useState<DetectedGame[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const { activeGame, trainers, selectGame, toggleCheat, pid } = useTrainer();

  useEffect(() => {
    async function fetchGames() {
      try {
        const games = await invoke<DetectedGame[]>("scan_games");
        setDetectedGames(games);
      } catch (error) {
        console.error("Failed to scan games:", error);
      } finally {
        setLoading(false);
      }
    }
    fetchGames();
  }, []);

  const filteredGames = detectedGames.filter(game => 
    game.name.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const handleGameClick = (game: DetectedGame) => {
    // Find matching trainer config if exists
    const trainer = trainers.find(t => 
      t.executable.toLowerCase() === (game.name + ".exe").toLowerCase() ||
      t.executable.toLowerCase() === game.name.toLowerCase()
    );
    if (trainer) {
      selectGame(trainer);
    } else {
      // For demo, if no trainer matches, let's just use the first dummy one
      if (trainers.length > 0) selectGame(trainers[0]);
    }
  };

  return (
    <div className="app-container">
      <aside className="sidebar">
        <div className="logo">
          <span>✨</span> Magic Wand
        </div>
        <nav>
          <div className="nav-item active">Library</div>
          <div className="nav-item">Community</div>
          <div className="nav-item">Settings</div>
        </nav>
      </aside>

      <main className="main-content">
        {activeGame ? (
          <div>
            <button className="back-button" onClick={() => selectGame(null)}>
              ← Back to Library
            </button>
            <header className="header">
              <div>
                <h1>{activeGame.name}</h1>
                <span className={`status-badge ${pid ? 'status-online' : 'status-offline'}`}>
                  {pid ? `CONNECTED (PID: ${pid})` : 'NOT RUNNING'}
                </span>
              </div>
            </header>
            <div className="cheat-list">
              {activeGame.cheats.map((cheat) => (
                <div className="cheat-item" key={cheat.id}>
                  <div className="cheat-info">
                    <span className="cheat-name">{cheat.name}</span>
                    {cheat.currentValue !== undefined && (
                      <span className="live-value">
                        Value: {typeof cheat.currentValue === 'number' ? cheat.currentValue.toFixed(2) : cheat.currentValue}
                      </span>
                    )}
                  </div>
                  <label className="switch">
                    <input 
                      type="checkbox" 
                      checked={cheat.active || false} 
                      onChange={() => toggleCheat(cheat)}
                      disabled={!pid}
                    />
                    <span className="slider"></span>
                  </label>
                </div>
              ))}
            </div>
          </div>
        ) : (
          <>
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
              <p>Scanning system for games...</p>
            ) : (
              <div className="game-grid">
                {filteredGames.length > 0 ? (
                  filteredGames.map((game, index) => (
                    <div className="game-card" key={index} onClick={() => handleGameClick(game)}>
                      <div className="game-image">🎮</div>
                      <div className="game-info">
                        <p className="game-title">{game.name}</p>
                        <span className="game-store">{game.store}</span>
                      </div>
                    </div>
                  ))
                ) : (
                  <p>No games found matching "{searchQuery}".</p>
                )}
              </div>
            )}
          </>
        )}
      </main>
    </div>
  );
}

export default App;
