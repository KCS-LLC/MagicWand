import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface Cheat {
  id: string;
  name: string;
  type: 'toggle' | 'action' | 'patch' | 'scan' | 'mono' | 'mono_chain';
  valueType?: 'int' | 'float' | 'double';
  module: string;
  base?: string;
  signature?: string;
  offsets: string[];
  onValue: number;
  onBytes?: number[];
  offBytes?: number[];
  monoAssembly?: string;
  monoNamespace?: string;
  monoClass?: string;
  monoField?: string;
  monoStatic?: boolean;
  // mono_chain-specific fields:
  monoStaticField?: string;
  monoStaticViaParent?: boolean;
  monoInstanceField?: string;
  monoInstanceFieldIsRef?: boolean;
  monoFinalOffset?: number;
  active?: boolean;
  currentValue?: string | number;
}

export interface GameTrainer {
  name: string;
  executable: string;
  cheats: Cheat[];
}

function memCmd(op: 'read' | 'write', valueType: 'int' | 'float' | 'double' | undefined): string {
  if (valueType === 'double') return `${op}_double`;
  if (valueType === 'float')  return `${op}_float`;
  return `${op}_int`;
}

function toHexAddr(addr: string | number): string {
  return '0x' + BigInt(addr).toString(16);
}

export function useTrainer(pollInterval: number = 2000, onCheatError?: (id: string, msg: string) => void) {
  const [activeGame, setActiveGame] = useState<GameTrainer | null>(null);
  const [pid, setPid] = useState<number | null>(null);
  const [trainers, setTrainers] = useState<GameTrainer[]>([]);

  useEffect(() => {
    async function loadTrainers() {
      try {
        const index = await fetch('/trainers/index.json').then(r => r.json()) as string[];
        const loaded = await Promise.all(
          index.map(f => fetch(`/trainers/${f}`).then(r => r.json()) as Promise<GameTrainer>)
        );
        setTrainers(loaded);
      } catch (err) {
        console.error('Failed to load trainers:', err);
      }
    }
    loadTrainers();
  }, []);

  const activeGameRef = useRef<GameTrainer | null>(null);
  const pidRef = useRef<number | null>(null);
  const addressCache = useRef<Record<string, string>>({});

  useEffect(() => { activeGameRef.current = activeGame; }, [activeGame]);
  useEffect(() => { pidRef.current = pid; }, [pid]);

  const getModuleBaseRaw = async (moduleName: string): Promise<string> => {
    try {
      const res = await invoke<string | null>('get_module_base', { pid: pidRef.current, moduleName });
      return res ? '0x' + BigInt(res).toString(16) : '0x0';
    } catch { return '0x0'; }
  };

  const resolveCheatAddress = async (cheat: Cheat): Promise<string> => {
    if (!pidRef.current) throw new Error('Not connected');
    if (cheat.type !== 'mono_chain' && addressCache.current[cheat.id]) return addressCache.current[cheat.id];

    let finalAddr: string;

    if (cheat.type === 'mono') {
      finalAddr = await invoke<string>('resolve_mono_field', {
        pid: pidRef.current,
        moduleName: cheat.module,
        assembly: cheat.monoAssembly ?? 'Assembly-CSharp',
        namespace: cheat.monoNamespace ?? '',
        className: cheat.monoClass ?? '',
        fieldName: cheat.monoField ?? '',
        isStatic: cheat.monoStatic ?? true,
      });
    } else if (cheat.type === 'mono_chain') {
      finalAddr = await invoke<string>('resolve_mono_chain', {
        pid: pidRef.current,
        moduleName: cheat.module,
        assembly: cheat.monoAssembly ?? 'Assembly-CSharp',
        namespace: cheat.monoNamespace ?? '',
        className: cheat.monoClass ?? '',
        staticField: cheat.monoStaticField ?? 'instance',
        viaParent: cheat.monoStaticViaParent ?? false,
        instanceField: cheat.monoInstanceField ?? '',
        finalOffset: cheat.monoFinalOffset ?? 0,
        instanceFieldIsRef: cheat.monoInstanceFieldIsRef ?? false,
      });
    } else {
      const modBase = await getModuleBaseRaw(cheat.module);
      let baseAddrStr: string;
      if (cheat.signature) {
        const found = await invoke<string>('aob_scan', { pid: pidRef.current, moduleName: cheat.module, pattern: cheat.signature });
        baseAddrStr = '0x' + (BigInt(found) + BigInt(cheat.base || '0')).toString(16);
      } else if (cheat.base) {
        baseAddrStr = '0x' + (BigInt(modBase) + BigInt(cheat.base)).toString(16);
      } else {
        throw new Error('Invalid cheat config');
      }

      const relativeOffset = '0x' + (BigInt(baseAddrStr) - BigInt(modBase)).toString(16);
      finalAddr = await invoke<string>('resolve_pointer', {
        pid: pidRef.current,
        moduleName: cheat.module,
        baseOffset: relativeOffset,
        offsets: cheat.offsets,
      });
    }

    addressCache.current[cheat.id] = finalAddr;
    return finalAddr;
  };

  useEffect(() => {
    if (!pid || !activeGame) return;
    const interval = setInterval(async () => {
      const currentActive = activeGameRef.current;
      if (!currentActive || !pidRef.current) return;
      try {
        const results = await Promise.all(
          currentActive.cheats
            .filter(c => c.valueType != null)
            .map(async (cheat) => {
              try {
                const addr = await resolveCheatAddress(cheat);
                const hexAddr = toHexAddr(addr);
                if ((cheat.type === 'toggle' || cheat.type === 'mono_chain') && cheat.active && cheat.valueType) {
                  await invoke(memCmd('write', cheat.valueType), { pid: pidRef.current, address: hexAddr, value: cheat.onValue });
                }
                const val = await invoke<number>(memCmd('read', cheat.valueType), { pid: pidRef.current, address: hexAddr });
                return { id: cheat.id, val };
              } catch (e) {
                const msg = e instanceof Error ? e.message : String(e);
                onCheatError?.(cheat.id, msg);
                return { id: cheat.id, val: '???' };
              }
            })
        );
        setActiveGame(prev => {
          if (!prev) return null;
          return {
            ...prev,
            cheats: prev.cheats.map(c => {
              const r = results.find(res => res.id === c.id);
              return r ? { ...c, currentValue: r.val } : c;
            }),
          };
        });
      } catch (e) { }
    }, pollInterval);
    return () => clearInterval(interval);
  }, [pid, activeGame?.name, pollInterval]);

  const selectGame = useCallback(async (game: GameTrainer | null) => {
    addressCache.current = {};
    setActiveGame(game);
    if (!game) { setPid(null); return; }
    try {
      const foundPid = await invoke<number | null>('find_game', { name: game.executable });
      setPid(foundPid);
    } catch (err) { console.error('HOOK: find_game failed:', err); }
  }, []);

  function resolveWriteValue(cheat: Cheat, customValueStr?: string): number {
    if (customValueStr !== undefined && customValueStr !== '') {
      const parsed = cheat.valueType === 'float'
        ? parseFloat(customValueStr)
        : parseInt(customValueStr, 10);
      if (!isNaN(parsed)) return parsed;
    }
    return cheat.onValue;
  }

  const applyCheat = async (cheat: Cheat, customValueStr?: string, onError?: (id: string, msg: string) => void) => {
    if (!pid || !activeGame) return;

    if (cheat.type === 'action' || cheat.type === 'mono') {
      try {
        const addr = await resolveCheatAddress(cheat);
        await invoke(memCmd('write', cheat.valueType), {
          pid,
          address: toHexAddr(addr),
          value: resolveWriteValue(cheat, customValueStr),
        });
      } catch (err) {
        onError?.(cheat.id, err instanceof Error ? err.message : String(err));
      }
      return;
    }

    const willBeActive = !cheat.active;
    setActiveGame(prev => {
      if (!prev) return null;
      return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: willBeActive } : c) };
    });
    try {
      const addr = await resolveCheatAddress(cheat);
      const hexAddr = toHexAddr(addr);
      if (cheat.type === 'patch') {
        const bytes = willBeActive ? cheat.onBytes : cheat.offBytes;
        await invoke('patch_bytes', { pid, address: hexAddr, bytes });
      } else if (willBeActive) {
        await invoke(memCmd('write', cheat.valueType), {
          pid,
          address: hexAddr,
          value: resolveWriteValue(cheat, customValueStr),
        });
      }
    } catch (err) {
      setActiveGame(prev => {
        if (!prev) return null;
        return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: !willBeActive } : c) };
      });
      onError?.(cheat.id, err instanceof Error ? err.message : String(err));
    }
  };

  return { activeGame, trainers, selectGame, applyCheat, pid };
}
