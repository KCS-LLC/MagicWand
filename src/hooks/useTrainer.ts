import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface Cheat {
  id: string;
  name: string;
  type: 'toggle' | 'action' | 'patch';
  valueType?: 'int' | 'float';
  module: string;
  base?: string;
  signature?: string;
  offsets: string[];
  onValue: number;
  onBytes?: number[]; // Bytes to write when active
  offBytes?: number[]; // Original bytes to restore
  active?: boolean;
  currentValue?: string | number;
}

export interface GameTrainer {
  name: string;
  executable: string;
  cheats: Cheat[];
}

export function useTrainer() {
  const [activeGame, setActiveGame] = useState<GameTrainer | null>(null);
  const [pid, setPid] = useState<number | null>(null);
  const [trainers, setTrainers] = useState<GameTrainer[]>([]);

  useEffect(() => {
    fetch('/trainers.json')
      .then((res) => res.json())
      .then((data) => setTrainers(data.games));
  }, []);

  const getModuleBaseRaw = async (moduleName: string): Promise<string> => {
    const res = await invoke<string | null>('get_module_base', { pid, moduleName });
    return res || "0";
  };

  const resolveCheatAddress = async (cheat: Cheat): Promise<string> => {
    if (!pid) throw new Error('Not connected to game');

    let baseAddrStr: string;

    if (cheat.signature) {
      // DYNAMIC: Find address via AOB scan
      const foundAddr = await invoke<string>('aob_scan', {
        pid,
        moduleName: cheat.module,
        pattern: cheat.signature
      });
      // Add optional base offset to the signature result
      baseAddrStr = (BigInt(foundAddr) + BigInt(cheat.base || "0")).toString();
    } else if (cheat.base) {
      // STATIC: Get module base and add offset
      const modBase = await getModuleBaseRaw(cheat.module);
      if (modBase === "0") throw new Error(`Module ${cheat.module} not found`);
      baseAddrStr = (BigInt(modBase) + BigInt(cheat.base)).toString();
    } else {
      throw new Error('Cheat must have a base offset or a signature');
    }

    const modBase = await getModuleBaseRaw(cheat.module);
    const relativeOffset = (BigInt(baseAddrStr) - BigInt(modBase)).toString();

    return await invoke<string>('resolve_pointer', {
      pid,
      moduleName: cheat.module,
      baseOffset: relativeOffset,
      offsets: cheat.offsets
    });
  };

  // Polling for live values
  useEffect(() => {
    if (!pid || !activeGame) return;

    const interval = setInterval(async () => {
      const updatedCheats = await Promise.all(activeGame.cheats.map(async (cheat) => {
        try {
          const finalAddr = await resolveCheatAddress(cheat);
          const command = cheat.valueType === 'float' ? 'read_float' : 'read_int';
          const val = await invoke<number>(command, { pid, address: finalAddr });
          return { ...cheat, currentValue: val };
        } catch (e) {
          return { ...cheat, currentValue: '???' };
        }
      }));

      setActiveGame(prev => prev ? { ...prev, cheats: updatedCheats } : null);
    }, 1000);

    return () => clearInterval(interval);
  }, [pid, activeGame?.name]);

  const selectGame = async (game: GameTrainer | null) => {
    setActiveGame(game);
    if (!game) {
      setPid(null);
      return;
    }
    try {
      const foundPid = await invoke<number | null>('find_game', { name: game.executable });
      setPid(foundPid);
    } catch (err) {
      console.error('Error finding game process:', err);
    }
  };

  const toggleCheat = async (cheat: Cheat) => {
    if (!pid || !activeGame) return;

    setActiveGame({
      ...activeGame,
      cheats: activeGame.cheats.map(c => 
        c.id === cheat.id ? { ...c, active: !c.active } : c
      )
    });

    try {
      const finalAddr = await resolveCheatAddress(cheat);

      if (cheat.type === 'patch') {
        const bytes = !cheat.active ? cheat.onBytes : cheat.offBytes;
        if (!bytes) throw new Error('Missing patch bytes');
        await invoke('patch_bytes', { pid, address: finalAddr, bytes });
      } else {
        const command = cheat.valueType === 'float' ? 'write_float' : 'write_int';
        await invoke(command, {
          pid,
          address: finalAddr,
          value: cheat.onValue
        });
      }

      console.log(`Successfully applied cheat: ${cheat.name}`);
    } catch (err) {
      console.error('CRITICAL: Failed to apply cheat:', err);
      setActiveGame({
        ...activeGame,
        cheats: activeGame.cheats.map(c => 
          c.id === cheat.id ? { ...c, active: c.active } : c
        )
      });
    }
  };

  return { activeGame, trainers, selectGame, toggleCheat, pid };
}
