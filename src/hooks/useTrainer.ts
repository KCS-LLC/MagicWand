import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface Cheat {
  id: string;
  name: string;
  type: 'toggle' | 'action' | 'patch';
  valueType?: 'int' | 'float' | 'double';
  module: string;
  base?: string;
  signature?: string;
  offsets: string[];
  onValue: number;
  onBytes?: number[];
  offBytes?: number[];
  active?: boolean;
  currentValue?: string | number;
}

export interface GameTrainer {
  name: string;
  executable: string;
  cheats: Cheat[];
}

const INITIAL_TRAINERS: GameTrainer[] = [
  {
    "name": "Skyrim Special Edition",
    "executable": "SkyrimSE.exe",
    "cheats": [
      {
        "id": "skyrim-health-aob",
        "name": "Infinite Health",
        "type": "toggle",
        "valueType": "float",
        "module": "SkyrimSE.exe",
        "signature": "48 8B 05 ?? ?? ?? ?? 48 8B D1 48 8B 00 48 85 C0",
        "offsets": ["0x1B0", "0x0"],
        "onValue": 99999
      },
      {
        "id": "skyrim-carryweight-aob",
        "name": "Unlimited Carry Weight",
        "type": "toggle",
        "valueType": "float",
        "module": "SkyrimSE.exe",
        "signature": "48 8B 05 ?? ?? ?? ?? 48 8B D1 48 8B 00 48 85 C0",
        "offsets": ["0x190", "0x28", "0x28"],
        "onValue": 99999
      },
      {
        "id": "skyrim-shout-cooldown",
        "name": "Instant Shouts",
        "type": "patch",
        "module": "SkyrimSE.exe",
        "signature": "F3 0F 11 81 ?? ?? ?? ?? 48 8B 4B ?? 48 85 C9 74 ?? 48 8B 01 FF 90 ?? ?? ?? ?? 48 8B 0B",
        "offsets": [],
        "onValue": 0,
        "onBytes": [0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90],
        "offBytes": [0xF3, 0x0F, 0x11, 0x81, 0x14, 0x01, 0x00, 0x00]
      },
      {
        "id": "skyrim-one-hit-kill",
        "name": "One-Hit Kill",
        "type": "patch",
        "module": "SkyrimSE.exe",
        "signature": "F3 0F 11 81 54 01 00 00",
        "offsets": [],
        "onValue": 0,
        "onBytes": [0x0F, 0x57, 0xC0, 0xF3, 0x0F, 0x11, 0x81, 0x54, 0x01, 0x00, 0x00],
        "offBytes": [0xF3, 0x0F, 0x11, 0x81, 0x54, 0x01, 0x00, 0x00]
      },
      {
        "id": "skyrim-gold-aob",
        "name": "Gold (AOB Scan)",
        "type": "action",
        "valueType": "int",
        "module": "SkyrimSE.exe",
        "signature": "48 8B 05 ?? ?? ?? ?? 48 8B D1 48 8B 00 48 85 C0",
        "offsets": ["0x10"],
        "onValue": 50000
      }
    ]
  },
  {
    "name": "Magic Wand Dummy Game",
    "executable": "dummy-game.exe",
    "cheats": [
      {
        "id": "dummy-health-aob",
        "name": "Health (AOB Scan)",
        "type": "toggle",
        "valueType": "int",
        "module": "dummy-game.exe",
        "signature": "DE AD BE EF 13 37 13 37 42 42 42 42 AA BB CC DD",
        "base": "0x10",
        "offsets": [],
        "onValue": 999
      },
      {
        "id": "dummy-gold-aob",
        "name": "Gold (AOB Scan)",
        "type": "toggle",
        "valueType": "int",
        "module": "dummy-game.exe",
        "signature": "DE AD BE EF 13 37 13 37 42 42 42 42 AA BB CC DD",
        "base": "0x14",
        "offsets": [],
        "onValue": 1337
      }
    ]
  }
];

export function useTrainer() {
  const [activeGame, setActiveGame] = useState<GameTrainer | null>(null);
  const [pid, setPid] = useState<number | null>(null);
  const [trainers] = useState<GameTrainer[]>(INITIAL_TRAINERS);
  const activeGameRef = useRef<GameTrainer | null>(null);
  const pidRef = useRef<number | null>(null);
  const addressCache = useRef<Record<string, string>>({});

  useEffect(() => { activeGameRef.current = activeGame; }, [activeGame]);
  useEffect(() => { pidRef.current = pid; }, [pid]);

  const getModuleBaseRaw = async (moduleName: string): Promise<string> => {
    try {
      const res = await invoke<string | null>('get_module_base', { pid: pidRef.current, moduleName });
      // Always ensure we return a hex string for consistency
      return res ? "0x" + BigInt(res).toString(16) : "0x0";
    } catch { return "0x0"; }
  };

  const resolveCheatAddress = async (cheat: Cheat): Promise<string> => {
    if (!pidRef.current) throw new Error('Not connected');
    if (addressCache.current[cheat.id]) return addressCache.current[cheat.id];

    let baseAddrStr: string;
    if (cheat.signature) {
      const found = await invoke<string>('aob_scan', { pid: pidRef.current, moduleName: cheat.module, pattern: cheat.signature });
      baseAddrStr = "0x" + (BigInt(found) + BigInt(cheat.base || "0")).toString(16);
    } else if (cheat.base) {
      const modBase = await getModuleBaseRaw(cheat.module);
      baseAddrStr = "0x" + (BigInt(modBase) + BigInt(cheat.base)).toString(16);
    } else { throw new Error('Invalid config'); }

    const modBase = await getModuleBaseRaw(cheat.module);
    const relativeOffset = "0x" + (BigInt(baseAddrStr) - BigInt(modBase)).toString(16);

    const finalAddr = await invoke<string>('resolve_pointer', {
      pid: pidRef.current,
      moduleName: cheat.module,
      baseOffset: relativeOffset,
      offsets: cheat.offsets
    });

    addressCache.current[cheat.id] = finalAddr;
    return finalAddr;
  };

  useEffect(() => {
    if (!pid || !activeGame) return;
    const interval = setInterval(async () => {
      const currentActive = activeGameRef.current;
      if (!currentActive || !pidRef.current) return;
      try {
        const results = await Promise.all(currentActive.cheats.map(async (cheat) => {
          try {
            const addr = await resolveCheatAddress(cheat);
            const cmd = cheat.valueType === 'double' ? 'read_double' : cheat.valueType === 'float' ? 'read_float' : 'read_int';
            // Final address is decimal from Rust, convert to 0xHex for safety if needed
            const hexAddr = "0x" + BigInt(addr).toString(16);
            const val = await invoke<number>(cmd, { pid: pidRef.current, address: hexAddr });
            return { id: cheat.id, val };
          } catch { return { id: cheat.id, val: '???' }; }
        }));
        setActiveGame(prev => {
          if (!prev) return null;
          return { ...prev, cheats: prev.cheats.map(c => {
            const r = results.find(res => res.id === c.id);
            return r ? { ...c, currentValue: r.val } : c;
          })};
        });
      } catch (e) { }
    }, 2000);
    return () => clearInterval(interval);
  }, [pid, activeGame?.name]);

  const selectGame = useCallback(async (game: GameTrainer | null) => {
    addressCache.current = {};
    setActiveGame(game);
    if (!game) { setPid(null); return; }
    try {
      const foundPid = await invoke<number | null>('find_game', { name: game.executable });
      setPid(foundPid);
    } catch (err) { console.error('HOOK: find_game failed:', err); }
  }, []);

  const toggleCheat = async (cheat: Cheat) => {
    if (!pid || !activeGame) return;
    setActiveGame(prev => {
      if (!prev) return null;
      return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: !c.active } : c) };
    });
    try {
      const addr = await resolveCheatAddress(cheat);
      const hexAddr = "0x" + BigInt(addr).toString(16);
      if (cheat.type === 'patch') {
        const bytes = !cheat.active ? cheat.onBytes : cheat.offBytes;
        await invoke('patch_bytes', { pid, address: hexAddr, bytes });
      } else {
        const cmd = cheat.valueType === 'double' ? 'write_double' : cheat.valueType === 'float' ? 'write_float' : 'write_int';
        await invoke(cmd, { pid, address: hexAddr, value: cheat.onValue });
      }
    } catch (err) {
      setActiveGame(prev => {
        if (!prev) return null;
        return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: c.active } : c) };
      });
    }
  };

  return { activeGame, trainers, selectGame, toggleCheat, pid };
}
