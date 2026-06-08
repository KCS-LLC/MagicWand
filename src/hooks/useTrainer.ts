import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface Cheat {
  id: string;
  name: string;
  type: 'toggle' | 'action';
  module: string;
  base: string;
  offsets: string[];
  onValue: number;
  active?: boolean;
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

    // Optimistically update UI
    setActiveGame({
      ...activeGame,
      cheats: activeGame.cheats.map(c => 
        c.id === cheat.id ? { ...c, active: !c.active } : c
      )
    });

    try {
      const baseAddr = await invoke<number | null>('get_module_base', { 
        pid, 
        moduleName: cheat.module 
      });

      if (!baseAddr) {
        throw new Error(`Module ${cheat.module} not found`);
      }

      const finalAddr = await invoke<number>('resolve_pointer', {
        pid,
        baseAddress: baseAddr + parseInt(cheat.base, 16),
        offsets: cheat.offsets.map(o => parseInt(o, 16))
      });

      await invoke('write_int', {
        pid,
        address: finalAddr,
        value: cheat.onValue
      });

      console.log(`Successfully applied cheat: ${cheat.name}`);
    } catch (err) {
      console.error('Failed to apply cheat:', err);
      // Revert UI on failure
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
