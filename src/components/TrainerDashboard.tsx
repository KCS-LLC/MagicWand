import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Cheat, GameTrainer } from '../hooks/useTrainer';
import { DevPanel } from './DevPanel';

interface ScanState {
  status: 'idle' | 'scanning' | 'found' | 'multiple' | 'not_found';
  addresses: string[];
  cachedAddress?: string;
}

interface TrainerDashboardProps {
  activeGame: GameTrainer;
  pid: number | null;
  scanMode: boolean;
  devMode: boolean;
  cheatErrors: Record<string, string>;
  setCheatError: (id: string, msg: string) => void;
  applyCheat: (cheat: Cheat, customValueStr?: string, onError?: (id: string, msg: string) => void) => Promise<void>;
  onBack: () => void;
}

export function TrainerDashboard({
  activeGame,
  pid,
  scanMode,
  devMode,
  cheatErrors,
  setCheatError,
  applyCheat,
  onBack,
}: TrainerDashboardProps) {
  const [customValues, setCustomValues] = useState<Record<string, string>>({});
  const [scanStates, setScanStates] = useState<Record<string, ScanState>>({});
  const [scanInputs, setScanInputs] = useState<Record<string, string>>({});

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

  return (
    <div className="trainer-dashboard">
      <button className="back-button" onClick={onBack}>← Back</button>
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
              {cheat.type !== 'scan' && cheat.type !== 'mono' && cheat.type !== 'ue5_prop' && cheat.type !== 'code_patch' && cheat.type !== 'code_cave' && (
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

      {devMode && <DevPanel pid={pid} activeGame={activeGame} />}
    </div>
  );
}
