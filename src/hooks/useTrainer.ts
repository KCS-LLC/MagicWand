import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface Cheat {
  id: string;
  name: string;
  type: 'toggle' | 'action';
  valueType?: 'int' | 'float';
  module: string;
  base: string;
  offsets: string[];
  onValue: number;
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

  // Polling for live values
  useEffect(() => {
    if (!pid || !activeGame) return;

    const interval = setInterval(async () => {
      const updatedCheats = await Promise.all(activeGame.cheats.map(async (cheat) => {
        try {
          const finalAddr = await invoke<string>('resolve_pointer', {
            pid,
            moduleName: cheat.module,
            baseOffset: cheat.base,
            offsets: cheat.offsets
          });

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

    // Optimistically update UI
    setActiveGame({
      ...activeGame,
      cheats: activeGame.cheats.map(c => 
        c.id === cheat.id ? { ...c, active: !c.active } : c
      )
    });

    try {
      const finalAddr = await invoke<string>('resolve_pointer', {
        pid,
        moduleName: cheat.module,
        baseOffset: cheat.base,
        offsets: cheat.offsets
      });

      const command = cheat.valueType === 'float' ? 'write_float' : 'write_int';
      await invoke(command, {
        pid,
        address: finalAddr,
        value: cheat.onValue
      });

      console.log(`Successfully applied cheat: ${cheat.name}`);
    } catch (err) {
      console.error('CRITICAL: Failed to apply cheat:', err);
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
