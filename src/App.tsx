import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTrainer, Cheat } from "./hooks/useTrainer";
import { useSettings } from "./hooks/useSettings";
import { CommunityPage } from "./pages/CommunityPage";
import { SettingsPage } from "./pages/SettingsPage";
import "./App.css";

type Page = 'library' | 'community' | 'settings';

interface DetectedGame {
  name: string;
  exe_path: string;
  store: string;
}

interface ScanState {
  status: 'idle' | 'scanning' | 'found' | 'multiple' | 'not_found';
  addresses: string[];
  cachedAddress?: string;
}

function App() {
  const [detectedGames, setDetectedGames] = useState<DetectedGame[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [customValues, setCustomValues] = useState<Record<string, string>>({});
  const [scanStates, setScanStates] = useState<Record<string, ScanState>>({});
  const [scanInputs, setScanInputs] = useState<Record<string, string>>({});
  const [currentPage, setCurrentPage] = useState<Page>('library');
  const { pollInterval, setPollInterval, scanMode, setScanMode, alwaysOnTop: storedAlwaysOnTop, loaded } = useSettings();
  const { activeGame, trainers, selectGame, applyCheat, pid } = useTrainer(pollInterval);

  useEffect(() => {
    if (loaded && storedAlwaysOnTop) {
      getCurrentWindow().setAlwaysOnTop(true);
    }
  }, [loaded]);

  if (!loaded) return null;

  const navTo = (page: Page) => {
    setScanStates({});
    setScanInputs({});
    selectGame(null);
    setCurrentPage(page);
  };

  const handleScan = async (cheat: Cheat) => {
    if (!pid) return;
    const currentValueStr = scanInputs[cheat.id];
    if (!currentValueStr) return;
    const value = cheat.valueType === 'int'
      ? parseInt(currentValueStr, 10)
      : parseFloat(currentValueStr);
    if (isNaN(value)) return;
    setScanStates(prev => ({ ...prev, [cheat.id]: { status: 'scanning', addresses: [] } }));
    try {
      const addresses = await invoke<string[]>('scan_value', {
        pid,
        valueType: cheat.valueType ?? 'int',
        value,
      });
      if (addresses.length === 0) {
        setScanStates(prev => ({ ...prev, [cheat.id]: { status: 'not_found', addresses: [] } }));
      } else if (addresses.length === 1) {
        setScanStates(prev => ({ ...prev, [cheat.id]: { status: 'found', addresses, cachedAddress: addresses[0] } }));
      } else {
        setScanStates(prev => ({ ...prev, [cheat.id]: { status: 'multiple', addresses } }));
      }
    } catch {
      setScanStates(prev => ({ ...prev, [cheat.id]: { status: 'not_found', addresses: [] } }));
    }
  };

  const handleScanWrite = async (cheat: Cheat) => {
    if (!pid) return;
    const state = scanStates[cheat.id];
    if (!state?.cachedAddress) return;
    const cmd = cheat.valueType === 'double' ? 'write_double'
              : cheat.valueType === 'float'  ? 'write_float'
              : 'write_int';
    await invoke(cmd, { pid, address: state.cachedAddress, value: cheat.onValue });
  };

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
    setScanStates({});
    setScanInputs({});
    if (trainer) {
      selectGame(trainer);
    } else {
      if (trainers.length > 0) selectGame(trainers[0]);
    }
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
            <button className="back-button" onClick={() => { setScanStates({}); setScanInputs({}); selectGame(null); }}>← Back</button>
            <div className="trainer-header">
              <h1>{activeGame.name}</h1>
              <span className={`status-badge ${pid ? 'status-online' : 'status-offline'}`}>
                {pid ? `CONNECTED (PID: ${pid})` : 'WAITING FOR GAME...'}
              </span>
            </div>

            <div className="cheat-list">
              {activeGame.cheats.filter(c => c.type !== 'scan' || scanMode).map((cheat) => (
                <div className="cheat-item" key={cheat.id}>
                  <div className="cheat-info">
                    <div className="cheat-name-row">
                      <span className="cheat-name">{cheat.name}</span>
                      <span className={`cheat-type-badge cheat-type-${cheat.type}`}>{cheat.type}</span>
                    </div>
                    {cheat.type !== 'scan' && cheat.type !== 'mono' && cheat.type !== 'mono_chain' && (
                      <span className="live-value">
                        {cheat.currentValue !== undefined ? `Value: ${typeof cheat.currentValue === 'number' ? cheat.currentValue.toFixed(2) : cheat.currentValue}` : 'Detecting...'}
                      </span>
                    )}
                  </div>
                  {cheat.type === 'scan' ? (
                    <div className="scan-cheat">
                      <div className="scan-row">
                        <input
                          className="value-input"
                          type="number"
                          placeholder="Current value in game"
                          value={scanInputs[cheat.id] ?? ''}
                          onChange={e => setScanInputs(prev => ({ ...prev, [cheat.id]: e.target.value }))}
                          disabled={!pid || scanStates[cheat.id]?.status === 'scanning'}
                        />
                        <button
                          className="fire-button"
                          onClick={() => handleScan(cheat)}
                          disabled={!pid || !scanInputs[cheat.id]}
                        >
                          {scanStates[cheat.id]?.status === 'scanning' ? 'Scanning...' : 'Scan'}
                        </button>
                      </div>
                      {scanStates[cheat.id]?.status === 'found' && (
                        <div className="scan-result found">
                          ✓ Found.
                          <button className="fire-button" onClick={() => handleScanWrite(cheat)}>
                            Set to {cheat.onValue}
                          </button>
                        </div>
                      )}
                      {scanStates[cheat.id]?.status === 'multiple' && (
                        <div className="scan-result multiple">
                          {scanStates[cheat.id].addresses.length} matches — change the value in-game then scan again
                        </div>
                      )}
                      {scanStates[cheat.id]?.status === 'not_found' && (
                        <div className="scan-result not-found">Not found — check the value and try again</div>
                      )}
                    </div>
                  ) : (
                    <>
                      {(cheat.type === 'toggle' || cheat.type === 'action') && cheat.valueType && (
                        <input
                          className="value-input"
                          type="number"
                          placeholder={String(cheat.onValue)}
                          value={customValues[cheat.id] ?? ''}
                          onChange={e => setCustomValues(prev => ({ ...prev, [cheat.id]: e.target.value }))}
                          disabled={!pid}
                        />
                      )}
                      {cheat.type === 'mono' || cheat.type === 'mono_chain' ? (
                        <button
                          className="fire-button"
                          onClick={() => applyCheat(cheat)}
                          disabled={!pid}
                        >
                          Set to {cheat.onValue}
                        </button>
                      ) : cheat.type === 'action' ? (
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
                    </>
                  )}
                </div>
              ))}
            </div>
          </div>
        ) : currentPage === 'community' ? (
          <CommunityPage />
        ) : currentPage === 'settings' ? (
          <SettingsPage pollInterval={pollInterval} onPollIntervalChange={setPollInterval} scanMode={scanMode} onScanModeChange={setScanMode} />
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
