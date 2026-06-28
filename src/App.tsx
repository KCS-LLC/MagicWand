import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from '@tauri-apps/api/window';
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
  const [cheatErrors, setCheatErrors] = useState<Record<string, string>>({});
  const [currentPage, setCurrentPage] = useState<Page>('library');
  const [diffStatus, setDiffStatus] = useState<string>('');
  const [diffResults, setDiffResults] = useState<string[]>([]);
  const [dumpAddr, setDumpAddr] = useState<string>('');
  const [classKeyword, setClassKeyword] = useState<string>('');
  const { pollInterval, setPollInterval, scanMode, setScanMode, alwaysOnTop, setAlwaysOnTop, loaded } = useSettings();
  const setCheatError = useCallback((id: string, msg: string) =>
    setCheatErrors(prev => ({ ...prev, [id]: msg })), []);

  const { activeGame, trainers, selectGame, applyCheat, pid } = useTrainer(pollInterval, setCheatError);

  const resetCheatState = useCallback(() => {
    setScanStates({});
    setScanInputs({});
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
          <div className="trainer-dashboard">
            <button className="back-button" onClick={() => { resetCheatState(); selectGame(null); }}>← Back</button>
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
                    {cheat.type !== 'scan' && cheat.type !== 'mono' && cheat.type !== 'ue5_prop' && (
                      <span className="live-value">
                        {cheat.currentValue !== undefined ? `Value: ${typeof cheat.currentValue === 'number' ? cheat.currentValue.toFixed(2) : cheat.currentValue}` : 'Detecting...'}
                      </span>
                    )}
                    {cheatErrors[cheat.id] && (
                      <span className="cheat-error">{cheatErrors[cheat.id]}</span>
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
                      {cheat.type === 'mono' ? (
                        <button
                          className="fire-button"
                          onClick={() => applyCheat(cheat, undefined, setCheatError)}
                          disabled={!pid}
                        >
                          Set to {cheat.onValue}
                        </button>
                      ) : cheat.type === 'action' ? (
                        <button
                          className="fire-button"
                          onClick={() => applyCheat(cheat, customValues[cheat.id], setCheatError)}
                          disabled={!pid}
                        >
                          Fire
                        </button>
                      ) : (
                        <label className="switch">
                          <input
                            type="checkbox"
                            checked={cheat.active || false}
                            onChange={() => applyCheat(cheat, customValues[cheat.id], setCheatError)}
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

            <div className="cheat-list" style={{ marginTop: '1rem', borderTop: '1px solid #333', paddingTop: '1rem' }}>
              <div className="cheat-item" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: '0.5rem' }}>
                <span className="cheat-name" style={{ fontSize: '0.75rem', color: '#888' }}>PATCH DIFF TOOL</span>
                <div style={{ display: 'flex', gap: '0.5rem' }}>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!activeGame || !pid) return;
                    try {
                      setDiffStatus('Snapshotting...');
                      setDiffResults([]);
                      const msg = await invoke<string>('snapshot_module', { pid, moduleName: activeGame.executable });
                      setDiffStatus(msg);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Snapshot</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Diffing...');
                      const results = await invoke<string[]>('diff_snapshot', { pid });
                      setDiffResults(results);
                      setDiffStatus(`Done — ${results.length} result(s)`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Diff</button>
                  <button className="fire-button" onClick={async () => {
                    try {
                      const r1 = await invoke<string>('read_snapshot_region', { rva: 0x1F8F83A5, size: 90 });
                      const r2 = await invoke<string>('read_snapshot_region', { rva: 0x1F902DB0, size: 4 });
                      setDiffResults([`LOC1: ${r1}`, `LOC2: ${r2}`]);
                      setDiffStatus('Snapshot regions read');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Read Regions</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving NexusConfigStoreLootConfig...');
                      setDiffResults([]);
                      const addr = await invoke<string>('resolve_ue5_prop', {
                        pid,
                        moduleName: 'Borderlands4.exe',
                        gobjectsAob: '',
                        gnamesAob: '',
                        gobjectsOffset: 0x11765A30,
                        gnamesOffset: 0x1167FDD0,
                        className: 'NexusConfigStoreInventoryRarity',
                        propertyOffset: 0,
                        extraOffsets: null,
                      });
                      setDiffStatus(`Found at ${addr} — dumping floats...`);
                      const lines = await invoke<string[]>('dump_floats_at', { pid, address: addr, count: 256 });
                      setDiffResults([`Object: ${addr}`, ...lines]);
                      setDiffStatus(`Dumped 256 floats from NexusConfigStoreInventoryRarity`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump Rarity Obj</button>
                </div>
                <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                  <input
                    className="value-input"
                    type="text"
                    placeholder="0x1EA624380"
                    value={dumpAddr}
                    onChange={e => setDumpAddr(e.target.value)}
                    style={{ fontFamily: 'monospace', fontSize: '0.75rem' }}
                  />
                  <button className="fire-button" disabled={!pid || !dumpAddr} onClick={async () => {
                    if (!pid || !dumpAddr) return;
                    try {
                      setDiffStatus(`Dumping ${dumpAddr}...`);
                      const lines = await invoke<string[]>('dump_floats_at', { pid, address: dumpAddr, count: 128 });
                      setDiffResults([`Dump @ ${dumpAddr}`, ...lines]);
                      setDiffStatus(`Dumped 128 floats from ${dumpAddr}`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump at Addr</button>
                </div>
                <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                  <input
                    className="value-input"
                    type="text"
                    placeholder="loot"
                    value={classKeyword}
                    onChange={e => setClassKeyword(e.target.value)}
                    style={{ fontFamily: 'monospace', fontSize: '0.75rem' }}
                  />
                  <button className="fire-button" disabled={!pid || !classKeyword} onClick={async () => {
                    if (!pid || !classKeyword) return;
                    try {
                      setDiffStatus(`Searching classes for "${classKeyword}"...`);
                      const names = await invoke<string[]>('list_ue5_classes', {
                        pid,
                        moduleName: 'Borderlands4.exe',
                        gobjectsOffset: 0x11765A30,
                        gnamesOffset: 0x1167FDD0,
                        keyword: classKeyword,
                      });
                      setDiffResults(names.length > 0 ? names : ['(no matches)']);
                      setDiffStatus(`Found ${names.length} class(es) containing "${classKeyword}"`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Find Classes</button>
                </div>
                {diffStatus && <span style={{ fontSize: '0.7rem', color: '#aaa' }}>{diffStatus}</span>}
                {diffResults.length > 0 && (
                  <pre style={{ fontSize: '0.65rem', color: '#0f0', background: '#111', padding: '0.5rem', borderRadius: '4px', width: '100%', overflowX: 'auto', margin: 0 }}>
                    {diffResults.join('\n')}
                  </pre>
                )}
              </div>
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
