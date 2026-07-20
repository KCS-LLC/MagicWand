import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface Cheat {
  id: string;
  name: string;
  type: 'toggle' | 'action' | 'patch' | 'scan' | 'mono' | 'mono_chain' | 'ue5_prop' | 'code_patch' | 'code_cave';
  valueType?: 'int' | 'float' | 'double' | 'byte';
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
  onValueFromOffset?: number;
  // ue5_prop-specific fields:
  ue5GObjectsAob?: string;
  ue5GNamesAob?: string;
  ue5GObjectsOffset?: number;
  ue5GNamesOffset?: number;
  ue5ClassName?: string;
  ue5PropertyOffset?: number;
  ue5Offsets?: number[];
  ue5MirrorOffset?: number;
  bitIndex?: number;
  offValue?: number;
  // code_patch-specific fields: each patch resolves via a fixed rva OR a live AOB signature
  patches?: { rva?: string; signature?: string; bytes: number[] }[];
  offPatches?: { rva?: string; signature?: string; bytes: number[] }[];
  // code_cave-specific fields: patchSite (fixed RVA) OR patchSignature (live AOB), one required
  patchSite?: string;
  patchSignature?: string;
  patchLen?: number;
  cavePayload?: number[];
  // farJump: use an unconstrained cave allocation + absolute indirect jumps (both
  // directions) instead of the default rel32 E9 jump. The default requires a free
  // 64KB-aligned slot within 2GB of patchSite, which can be scarce/absent depending on
  // what else the game has mapped nearby at that moment (varies launch to launch).
  // Requires patchLen long enough for a 14-byte `jmp qword ptr [rip+0]` + embedded
  // pointer to land on a real instruction boundary — verify against a live disassembly
  // before setting this, a wrong cut corrupts the instruction stream.
  farJump?: boolean;
  // coreLen: length of the instruction cavePayload's own logic replicates (e.g. 5 for
  // the original AND EAX being neutralized). Only meaningful with farJump — bytes from
  // coreLen up to patchLen are extra instructions the far jump's longer overwrite
  // swallows, which get carried into the cave verbatim (fixed up per ripFixup if set).
  coreLen?: number;
  // ripFixup: describes a RIP-relative operand within the swallowed [coreLen, patchLen)
  // region — its data gets embedded live into the cave and the displacement repointed
  // at that copy, since the original disp32 would resolve relative to wherever the far
  // jump's cave lands, not the original site. Must be the last instruction swallowed.
  ripFixup?: { relDispOffset: number; dataLen: number };
  // code_cave multi-site: when a single toggle needs to patch more than one location
  // (e.g. the same instruction appears at multiple call sites within one function),
  // list them here instead of using the singular fields above. Each site gets its own
  // enable_code_cave/disable_code_cave call, keyed by `${cheat.id}#${index}`.
  caveSites?: {
    patchSite?: string; patchSignature?: string; patchLen?: number; cavePayload: number[];
    farJump?: boolean; coreLen?: number; ripFixup?: { relDispOffset: number; dataLen: number };
  }[];
  active?: boolean;
  currentValue?: string | number;
}

export interface GameTrainer {
  name: string;
  executable: string;
  cheats: Cheat[];
}

function memCmd(op: 'read' | 'write', valueType: 'int' | 'float' | 'double' | 'byte' | undefined): string {
  if (valueType === 'double') return `${op}_double`;
  if (valueType === 'float')  return `${op}_float`;
  if (valueType === 'byte')   return `${op}_byte`;
  return `${op}_int`;
}

function toHexAddr(addr: string | number): string {
  return '0x' + BigInt(addr).toString(16);
}

/// Writes `value` at `hexAddr`, and — if `mirrorOffset` is set — writes it again at
/// `hexAddr + mirrorOffset`. Covers UE5 attribute-style fields that store a cached
/// "Value" alongside a "BaseValue" a few bytes later, where a periodic recalculation
/// can stomp a write to only one of the two (e.g. FGbxAttributeFloat.Value/BaseValue).
async function writeWithMirror(
  pid: number | null,
  hexAddr: string,
  valueType: Cheat['valueType'],
  value: number,
  mirrorOffset?: number,
) {
  const cmd = valueType === 'byte' ? 'write_byte' : memCmd('write', valueType);
  await invoke(cmd, { pid, address: hexAddr, value });
  if (mirrorOffset) {
    const mirrorAddr = toHexAddr('0x' + (BigInt(hexAddr) + BigInt(mirrorOffset)).toString(16));
    await invoke(cmd, { pid, address: mirrorAddr, value });
  }
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
  const cheatFailedAt = useRef<Record<string, number>>({});
  const codePatchIntervals = useRef<Map<string, ReturnType<typeof setInterval>>>(new Map());
  // Resolved absolute addresses for each cheat's `patches` entries, keyed by cheat.id.
  // Populated once per enable — signature-based patches are re-applied every 50ms and
  // must not be re-scanned that often (an AOB scan walks the whole module).
  const patchAddrCache = useRef<Record<string, bigint[]>>({});

  const resolvePatchAddresses = async (cheat: Cheat, pid: number): Promise<bigint[]> => {
    const cached = patchAddrCache.current[cheat.id];
    if (cached) return cached;
    const baseStr = await invoke<string>('get_module_base', { pid, moduleName: cheat.module });
    const base = BigInt(baseStr);
    const addrs: bigint[] = [];
    for (const patch of cheat.patches ?? []) {
      if (patch.signature) {
        const found = await invoke<string>('aob_scan', { pid, moduleName: cheat.module, pattern: patch.signature });
        addrs.push(BigInt(found));
      } else {
        addrs.push(base + BigInt(patch.rva ?? '0'));
      }
    }
    patchAddrCache.current[cheat.id] = addrs;
    return addrs;
  };

  const applyCodePatches = async (cheat: Cheat, pid: number) => {
    if (!cheat.patches) return;
    const addrs = await resolvePatchAddresses(cheat, pid);
    for (let i = 0; i < cheat.patches.length; i++) {
      const addr = `0x${addrs[i].toString(16).toUpperCase()}`;
      await invoke('patch_bytes', { pid, address: addr, bytes: cheat.patches[i].bytes });
    }
  };

  // Returns true if the patch resolved and applied at least once. Callers must not
  // flip the cheat's `active` UI state to true until this resolves — a failed AOB scan
  // (stale signature) must not silently retry forever: resolvePatchAddresses only caches
  // on success, so an uncaught failure here would otherwise re-trigger a full, uncached
  // module scan every single tick, indefinitely.
  const startCodePatch = async (cheat: Cheat, pid: number, onError?: (id: string, msg: string) => void): Promise<boolean> => {
    if (!cheat.patches || cheat.patches.length === 0) return false;
    try {
      await applyCodePatches(cheat, pid);
    } catch (err) {
      onError?.(cheat.id, err instanceof Error ? err.message : String(err));
      return false;
    }
    const id = setInterval(() => {
      applyCodePatches(cheat, pid).catch((err) => {
        // Started failing mid-session (process/module gone, or a transient scan failure
        // that will only repeat) — stop retrying instead of re-scanning every 50ms forever.
        const intervalId = codePatchIntervals.current.get(cheat.id);
        if (intervalId !== undefined) { clearInterval(intervalId); codePatchIntervals.current.delete(cheat.id); }
        delete patchAddrCache.current[cheat.id];
        onError?.(cheat.id, err instanceof Error ? err.message : String(err));
        setActiveGame(prev => {
          if (!prev) return null;
          return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: false } : c) };
        });
      });
    }, 50);
    codePatchIntervals.current.set(cheat.id, id);
    return true;
  };

  const stopCodePatch = (cheat: Cheat, pid: number) => {
    const id = codePatchIntervals.current.get(cheat.id);
    if (id !== undefined) { clearInterval(id); codePatchIntervals.current.delete(cheat.id); }
    if (cheat.offPatches && cheat.offPatches.length > 0 && pid) {
      (async () => {
        // offPatches restore the same sites as patches, in the same order — reuse the cache.
        const addrs = await resolvePatchAddresses(cheat, pid);
        for (let i = 0; i < cheat.offPatches!.length; i++) {
          const addr = `0x${addrs[i].toString(16).toUpperCase()}`;
          await invoke('patch_bytes', { pid, address: addr, bytes: cheat.offPatches![i].bytes });
        }
      })().catch(() => {}).finally(() => { delete patchAddrCache.current[cheat.id]; });
    } else {
      delete patchAddrCache.current[cheat.id];
    }
  };

  useEffect(() => {
    if (!pid) {
      codePatchIntervals.current.forEach(id => clearInterval(id));
      codePatchIntervals.current.clear();
      patchAddrCache.current = {};
      // note: offPatches not restored on disconnect — game is closing anyway
    }
  }, [pid]);

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
    } else if (cheat.type === 'ue5_prop') {
      finalAddr = await invoke<string>('resolve_ue5_prop', {
        pid: pidRef.current,
        moduleName: cheat.module,
        gobjectsAob: cheat.ue5GObjectsAob ?? '',
        gnamesAob: cheat.ue5GNamesAob ?? '',
        gobjectsOffset: cheat.ue5GObjectsOffset ?? null,
        gnamesOffset: cheat.ue5GNamesOffset ?? null,
        className: cheat.ue5ClassName ?? '',
        propertyOffset: cheat.ue5PropertyOffset ?? 0,
        extraOffsets: cheat.ue5Offsets ?? null,
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
            .filter(c => c.valueType != null && c.type !== 'code_patch' && c.type !== 'code_cave')
            .map(async (cheat) => {
              try {
                const failedAt = cheatFailedAt.current[cheat.id];
                if (failedAt && Date.now() - failedAt < 30_000) {
                  return { id: cheat.id, val: '???' };
                }
                const addr = await resolveCheatAddress(cheat);
                const hexAddr = toHexAddr(addr);
                if ((cheat.type === 'toggle' || cheat.type === 'mono_chain' || cheat.type === 'ue5_prop') && cheat.active && cheat.valueType) {
                  let writeValue = cheat.onValue;
                  if (cheat.onValueFromOffset != null) {
                    const srcAddr = toHexAddr('0x' + (BigInt(addr) - BigInt(cheat.monoFinalOffset ?? 0) + BigInt(cheat.onValueFromOffset)).toString(16));
                    writeValue = await invoke<number>(memCmd('read', cheat.valueType), { pid: pidRef.current, address: srcAddr });
                  }
                  await writeWithMirror(pidRef.current, hexAddr, cheat.valueType, writeValue, cheat.ue5MirrorOffset);
                }
                const val = cheat.valueType === 'byte'
                  ? await invoke<number>('read_byte', { pid: pidRef.current, address: hexAddr })
                  : await invoke<number>(memCmd('read', cheat.valueType), { pid: pidRef.current, address: hexAddr });
                return { id: cheat.id, val };
              } catch (e) {
                // Only surface this as a user-facing error when the cheat is actually
                // toggled on — this loop also runs for inactive cheats just to display
                // currentValue, and address resolution can legitimately fail there
                // (e.g. player has no valid Pawn yet) before the user has done anything.
                if (cheat.active) {
                  const msg = e instanceof Error ? e.message : String(e);
                  onCheatError?.(cheat.id, msg);
                }
                cheatFailedAt.current[cheat.id] = Date.now();
                delete addressCache.current[cheat.id];
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
    cheatFailedAt.current = {};
    patchAddrCache.current = {};
    codePatchIntervals.current.forEach(id => clearInterval(id));
    codePatchIntervals.current.clear();
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

    if (cheat.type === 'code_patch') {
      const willBeActive = !cheat.active;
      if (willBeActive) {
        // Don't flip to "active" until the patch has actually resolved and applied once.
        const ok = await startCodePatch(cheat, pid, onError);
        if (ok) {
          setActiveGame(prev => {
            if (!prev) return null;
            return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: true } : c) };
          });
        }
      } else {
        stopCodePatch(cheat, pid);
        setActiveGame(prev => {
          if (!prev) return null;
          return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: false } : c) };
        });
      }
      return;
    }

    if (cheat.type === 'code_cave') {
      const sites = cheat.caveSites ?? (cheat.cavePayload
        ? [{
            patchSite: cheat.patchSite, patchSignature: cheat.patchSignature, patchLen: cheat.patchLen,
            cavePayload: cheat.cavePayload, farJump: cheat.farJump, coreLen: cheat.coreLen, ripFixup: cheat.ripFixup,
          }]
        : []);
      if (sites.length === 0) return;
      const willBeActive = !cheat.active;
      setActiveGame(prev => {
        if (!prev) return null;
        return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: willBeActive } : c) };
      });
      if (willBeActive) {
        const enabledIdx: number[] = [];
        try {
          for (let i = 0; i < sites.length; i++) {
            const site = sites[i];
            let siteRva = site.patchSite;
            if (!siteRva && site.patchSignature) {
              const baseStr = await invoke<string>('get_module_base', { pid, moduleName: cheat.module });
              const found = await invoke<string>('aob_scan', { pid, moduleName: cheat.module, pattern: site.patchSignature });
              siteRva = '0x' + (BigInt(found) - BigInt(baseStr)).toString(16);
            }
            await invoke('enable_code_cave', {
              pid,
              cheatId: `${cheat.id}#${i}`,
              moduleName: cheat.module,
              siteRva,
              cavePayload: site.cavePayload,
              siteLen: site.patchLen ?? 5,
              farJump: site.farJump ?? false,
              coreLen: site.coreLen ?? 5,
              ripFixup: site.ripFixup
                ? { relDispOffset: site.ripFixup.relDispOffset, dataLen: site.ripFixup.dataLen }
                : null,
            });
            enabledIdx.push(i);
          }
        } catch (err) {
          // Partial failure — undo whichever sites already installed so we don't leave
          // some of them patched while the UI shows the cheat as off.
          for (const i of enabledIdx) {
            await invoke('disable_code_cave', { pid, cheatId: `${cheat.id}#${i}` }).catch(() => {});
          }
          onError?.(cheat.id, err instanceof Error ? err.message : String(err));
          setActiveGame(prev => {
            if (!prev) return null;
            return { ...prev, cheats: prev.cheats.map(c => c.id === cheat.id ? { ...c, active: false } : c) };
          });
        }
      } else {
        for (let i = 0; i < sites.length; i++) {
          await invoke('disable_code_cave', { pid, cheatId: `${cheat.id}#${i}` }).catch(() => {});
        }
      }
      return;
    }

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
      } else if (cheat.bitIndex !== undefined) {
        const bitSet = willBeActive ? (cheat.onValue !== 0) : ((cheat.offValue ?? 1) !== 0);
        await invoke('toggle_bit_flag', { pid, address: hexAddr, bit: cheat.bitIndex, value: bitSet });
      } else if (cheat.type === 'ue5_prop' && !willBeActive && cheat.offValue !== undefined) {
        await writeWithMirror(pid, hexAddr, cheat.valueType, cheat.offValue, cheat.ue5MirrorOffset);
      } else if (willBeActive) {
        const wv = resolveWriteValue(cheat, customValueStr);
        await writeWithMirror(pid, hexAddr, cheat.valueType, wv, cheat.ue5MirrorOffset);
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
