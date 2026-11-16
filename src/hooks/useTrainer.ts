import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface CodePatch {
  rva?: string;
  signature?: string;
  monoMethod?: { assembly?: string; namespace?: string; className: string; method: string };
  bytes: number[];
}

export interface Cheat {
  id: string;
  name: string;
  type:
    | 'toggle'
    | 'action'
    | 'patch'
    | 'scan'
    | 'mono'
    | 'mono_chain'
    | 'ue5_prop'
    | 'code_patch'
    | 'mono_store_nop'
    | 'mono_method_ret'
    | 'mono_call_nop'
    | 'il2cpp_method_ret'
    | 'il2cpp_decay_nop'
    | 'code_cave';
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
  // Multi-hop alternative to monoInstanceField: monoClass.monoStaticField is the singleton,
  // each hop derefs a reference field, and the last hop's field is the value.
  // Added to every value written (typed or onValue) — for stats stored as base + modifier, where the
  // field we write is the modifier and the number the user sees is base + modifier.
  writeBias?: number;
  monoPath?: { namespace?: string; className: string; field: string; dictKey?: number }[];
  // IL2CPP games (module GameAssembly.dll): il2cppClass.il2cppStaticField is a singleton and
  // each hop follows a field / raw offset / List element; the last hop is the value itself.
  il2cppClass?: string;
  il2cppNamespace?: string;
  il2cppStaticField?: string;
  il2cppPath?: {
    kind: 'field' | 'offset' | 'listItem' | 'listFindByte';
    namespace?: string;
    className?: string;
    field?: string;
    offset?: number;
    index?: number;
    equals?: number;
  }[];
  onValueFromOffset?: number;
  // Godot 4 / GDScript games: the value is member `index` of a GDScript instance's member
  // array, located per launch by scanning for the GDScriptInstance vtable (module RVA),
  // then matching the member count + Variant type-tag fingerprint (-1 = any). The resolved
  // address is the Variant's payload; valueType is 'double' for GDScript floats. See godot.rs.
  godot?: {
    vtableRva: string;
    memberCount: number;
    tags: number[];
    index: number;
  };
  // ue5_prop-specific fields:
  ue5GObjectsAob?: string;
  ue5GNamesAob?: string;
  ue5GObjectsOffset?: number;
  ue5GNamesOffset?: number;
  ue5ClassName?: string;
  ue5PropertyOffset?: number;
  ue5Offsets?: number[];
  ue5MirrorOffset?: number;
  // Set true when the resolved chain passes through a pointer that can legitimately
  // change to a DIFFERENT LIVE (non-null) object during normal play — e.g. an
  // equipped-weapon slot the player can swap at will. A stale cached address in that
  // case doesn't error (the old object is still valid memory), so the usual
  // cache-invalidate-on-error path never fires and every write silently keeps targeting
  // the object that was equipped when the cheat was first toggled on. Same underlying
  // reasoning as mono_chain/ue5ArrayMatchClass already skipping the cache, just for a
  // ue5_prop chain instead.
  ue5NoCache?: boolean;
  // Present only for UE5 games whose internal GObjects/GNames struct layout differs from
  // Magic Wand's chunked-array default (calibrated against Borderlands 4) — e.g. a flat,
  // non-chunked TUObjectArray. When set, routes through resolve_ue5_prop_custom with this
  // exact layout instead of the hardcoded default. Field names are snake_case to match the
  // Rust Ue5Offsets struct's own (unrenamed) serde field names.
  ue5CustomLayout?: {
    fuobjectitem_size: number;
    fuobjectitem_object: number;
    uobject_class: number;
    uobject_name: number;
    gobjects_chunk_size: number;
    gobjects_objects: number;
    gobjects_num_elements: number;
    fname_stride: number;
    fname_entry_header: number;
    gnames_chunk_size: number;
    gobjects_chunked: boolean;
  };
  // ue5 array-item fields: resolves a value inside a dynamic TArray element (e.g. a
  // currency stack in an inventory) by walking ue5PropertyOffset/ue5Offsets down to a
  // container object, then linear-scanning its array for the first element whose
  // sub-object class matches ue5ArrayMatchClass. Presence of ue5ArrayMatchClass alone
  // (independent of `type`) routes address resolution through resolve_ue5_array_item —
  // pair it with type: "action" for a one-shot "Fire" write, since the matched array
  // slot isn't stable enough to poll/toggle continuously.
  ue5ArrayOffset?: number;
  ue5ArrayItemStride?: number;
  ue5ArrayItemPtrOffset?: number;
  ue5ArrayMatchClass?: string;
  ue5ArrayValueOffset?: number;
  bitIndex?: number;
  offValue?: number;
  // code_patch-specific fields: each patch resolves via a fixed rva OR a live AOB signature
  // A patch may instead target a managed method's JIT entry via `monoMethod` (Unity Mono
  // games; `cheat.module` must then be mono-2.0-bdwgc.dll).
  patches?: CodePatch[];
  offPatches?: CodePatch[];
  // mono_store_nop-specific fields: NOPs every inlined store to `monoStoreField` (declared on
  // monoMethod.className) inside monoMethod's JIT body, leaving the rest of the method running.
  // Use for invincibility that must still fire on-hit triggers (see feedback_invincibility_nop_damage).
  monoMethod?: { assembly?: string; namespace?: string; className: string; method: string };
  monoStoreField?: string;
  // Optional: if the method hasn't been JIT-compiled yet, edit its IL in memory instead (so no
  // first hit is needed). ilSignature = space-separated hex of the IL's first bytes;
  // ilPatches = byte edits (offset from the signature start, expected bytes, replacement).
  // mono_call_nop: monoMethod is the caller; monoCallee is the method whose single call inside it
  // is skipped while on (both must already be compiled).
  monoCallee?: { assembly?: string; namespace?: string; className: string; method: string };
  // mono_method_ret also uses monoMethod/ilSignature/ilPatches: while on, the method returns 0 at
  // its first instruction (native `xor eax,eax; ret`, or an IL edit if it hasn't compiled yet).
  ilSignature?: string;
  ilPatches?: { offset: number; expect: string; replace: string }[];
  // il2cpp_method_ret-specific: makes a void IL2CPP method return at its first byte while on
  // (e.g. CharacterStats.ReceiveDamage for invincibility). Original byte is restored on off.
  il2cppMethod?: { namespace?: string; className: string; method: string };
  // Several methods (every overload of each is patched) — takes precedence over il2cppMethod.
  il2cppMethods?: { namespace?: string; className: string; method: string }[];
  // il2cpp_decay_nop: read the value to set from this path instead of using onValue
  // (e.g. a stat's current max), using the same il2cppClass/il2cppStaticField root.
  il2cppValuePath?: {
    kind: 'field' | 'offset' | 'listItem' | 'listFindByte';
    namespace?: string;
    className?: string;
    field?: string;
    offset?: number;
    index?: number;
    equals?: number;
  }[];
  // il2cpp_decay_nop-specific: while on, NOPs the per-tick *decrement* store to
  // il2cppDecay.field inside il2cppMethod (eating/resting can still raise it). On enable the
  // stat is first set to onValue via il2cppPath.
  il2cppDecay?: { className: string; field: string };
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
    patchSite?: string;
    patchSignature?: string;
    patchLen?: number;
    cavePayload: number[];
    farJump?: boolean;
    coreLen?: number;
    ripFixup?: { relDispOffset: number; dataLen: number };
    // Byte offset within this site's own cavePayload/cave allocation holding an 8-byte
    // scratch pointer the cave's own logic compares against (e.g. "is the calling object
    // owned by the live player pawn"). When set, the poll loop re-resolves the cheat's
    // ue5* fields (class/property chain, extraOffsets left empty so resolution stops at
    // the field's own address rather than dereferencing into it) each tick, reads the
    // live pointer there, and writes those exact bytes into cave_addr + this offset via
    // patch_bytes — keeping the comparison value current across respawns without needing
    // any shared memory between multiple cave sites.
    playerPtrScratchOffset?: number;
    // Byte offset within this site's cave allocation holding a 4-byte float the cave's
    // own logic reads as a plain constant (e.g. a multiplier) — unlike
    // playerPtrScratchOffset, this is never refreshed by the poll loop on its own; it's
    // written once whenever the user picks a new value via `sliderOptions` (see below),
    // either right after install or, if the cave is already active, directly into the
    // already-running cave without reinstalling it.
    valueScratchOffset?: number;
  }[];
  // Renders a discrete stepped slider instead of a toggle switch + free-value box. The
  // first entry is the "off" position (selecting it disables the cave/cheat entirely,
  // not just writes a value of 1) — every other entry both installs the cave (if not
  // already active) and writes that value into every caveSite's valueScratchOffset.
  // Only meaningful on a `code_cave` cheat whose caveSites all set valueScratchOffset.
  sliderOptions?: number[];
  active?: boolean;
  currentValue?: string | number;
}

export interface GameTrainer {
  name: string;
  executable: string;
  cheats: Cheat[];
}

function memCmd(
  op: 'read' | 'write',
  valueType: 'int' | 'float' | 'double' | 'byte' | undefined,
): string {
  if (valueType === 'double') return `${op}_double`;
  if (valueType === 'float') return `${op}_float`;
  if (valueType === 'byte') return `${op}_byte`;
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

export function useTrainer(
  pollInterval: number = 2000,
  onCheatError?: (id: string, msg: string) => void,
) {
  const [activeGame, setActiveGame] = useState<GameTrainer | null>(null);
  const [pid, setPid] = useState<number | null>(null);
  const [trainers, setTrainers] = useState<GameTrainer[]>([]);

  useEffect(() => {
    async function loadTrainers() {
      try {
        const index = (await fetch('/trainers/index.json').then((r) => r.json())) as string[];
        const loaded = await Promise.all(
          index.map((f) => fetch(`/trainers/${f}`).then((r) => r.json()) as Promise<GameTrainer>),
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
  const godotInstances = useRef<Record<string, string>>({});
  // For ue5NoCache cheats: caches just the expensive part (the class-object pointer from
  // find_object_by_class's GObjects scan), separately from the full resolved address. The
  // class object itself (e.g. the player controller) is stable for the whole session, so
  // there's no need to re-scan 500k+ objects every poll tick just because a downstream hop
  // in the chain (e.g. equipped weapon) can legitimately change — only the cheap tail of
  // the chain gets re-walked every tick via resolve_pointer_chain_from.
  const classObjCache = useRef<Record<string, string>>({});
  const cheatFailedAt = useRef<Record<string, number>>({});
  // Active code_cave sites with a playerPtrScratchOffset (see the Cheat.caveSites field
  // comment) — keyed by `${cheat.id}#${i}`, refreshed by the main poll loop below.
  const caveScratchSites = useRef<Record<string, { cheat: Cheat; scratchAddr: string }>>({});
  // Active code_cave sites with a valueScratchOffset (see the Cheat.caveSites field
  // comment) — keyed by `${cheat.id}#${i}`, lets a slider cheat rewrite its live value
  // without reinstalling the cave. Not poll-refreshed like caveScratchSites; written once
  // per user selection in setSliderCheat.
  const valueScratchSites = useRef<Record<string, string>>({});
  // setInterval does not wait for a previous async tick to finish before firing the next
  // one. A UE5 GObjects scan's cost scales with the live object count, which only grows
  // as a session progresses (observed climbing well past 500k on a long TOW2 session) — if
  // a scan ever takes longer than pollInterval, ticks start overlapping, each one queuing
  // its own scan on top of the last, compounding until the app becomes unresponsive. This
  // guard skips starting a new tick while one is still in flight.
  const pollInFlight = useRef(false);
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
      if (patch.monoMethod) {
        const found = await invoke<string>('resolve_mono_method', {
          pid,
          moduleName: cheat.module,
          assembly: patch.monoMethod.assembly ?? 'Assembly-CSharp',
          namespace: patch.monoMethod.namespace ?? '',
          className: patch.monoMethod.className,
          method: patch.monoMethod.method,
        });
        addrs.push(BigInt(found));
      } else if (patch.signature) {
        const found = await invoke<string>('aob_scan', {
          pid,
          moduleName: cheat.module,
          pattern: patch.signature,
        });
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
  const startCodePatch = async (
    cheat: Cheat,
    pid: number,
    onError?: (id: string, msg: string) => void,
  ): Promise<boolean> => {
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
        if (intervalId !== undefined) {
          clearInterval(intervalId);
          codePatchIntervals.current.delete(cheat.id);
        }
        delete patchAddrCache.current[cheat.id];
        onError?.(cheat.id, err instanceof Error ? err.message : String(err));
        setActiveGame((prev) => {
          if (!prev) return null;
          return {
            ...prev,
            cheats: prev.cheats.map((c) => (c.id === cheat.id ? { ...c, active: false } : c)),
          };
        });
      });
    }, 50);
    codePatchIntervals.current.set(cheat.id, id);
    return true;
  };

  const stopCodePatch = (cheat: Cheat, pid: number) => {
    const id = codePatchIntervals.current.get(cheat.id);
    if (id !== undefined) {
      clearInterval(id);
      codePatchIntervals.current.delete(cheat.id);
    }
    if (cheat.offPatches && cheat.offPatches.length > 0 && pid) {
      (async () => {
        // offPatches restore the same sites as patches, in the same order — reuse the cache.
        const addrs = await resolvePatchAddresses(cheat, pid);
        for (let i = 0; i < cheat.offPatches!.length; i++) {
          const addr = `0x${addrs[i].toString(16).toUpperCase()}`;
          await invoke('patch_bytes', { pid, address: addr, bytes: cheat.offPatches![i].bytes });
        }
      })()
        .catch(() => {})
        .finally(() => {
          delete patchAddrCache.current[cheat.id];
        });
    } else {
      delete patchAddrCache.current[cheat.id];
    }
  };

  useEffect(() => {
    if (!pid) {
      codePatchIntervals.current.forEach((id) => clearInterval(id));
      codePatchIntervals.current.clear();
      patchAddrCache.current = {};
      // note: offPatches not restored on disconnect — game is closing anyway
    }
  }, [pid]);

  useEffect(() => {
    activeGameRef.current = activeGame;
  }, [activeGame]);
  useEffect(() => {
    pidRef.current = pid;
  }, [pid]);

  const getModuleBaseRaw = async (moduleName: string): Promise<string> => {
    try {
      const res = await invoke<string | null>('get_module_base', {
        pid: pidRef.current,
        moduleName,
      });
      return res ? '0x' + BigInt(res).toString(16) : '0x0';
    } catch {
      return '0x0';
    }
  };

  const resolveCheatAddress = async (cheat: Cheat): Promise<string> => {
    if (!pidRef.current) throw new Error('Not connected');
    // Array-item cheats resolve to a slot inside a dynamic array (e.g. an inventory stack)
    // that can move whenever the array is resized — never trust a cached address for these,
    // same reasoning as mono_chain's ever-changing instance pointer.
    if (
      cheat.type !== 'mono_chain' &&
      !cheat.godot &&
      !cheat.monoPath &&
      !cheat.il2cppPath &&
      !cheat.ue5ArrayMatchClass &&
      !cheat.ue5NoCache &&
      addressCache.current[cheat.id]
    )
      return addressCache.current[cheat.id];

    let finalAddr: string;

    if (cheat.godot) {
      // Cheap on every call after the first: the cached instance is just revalidated.
      const [instance, addr] = await invoke<string[]>('resolve_godot_member', {
        pid: pidRef.current,
        moduleName: cheat.module,
        vtableRva: cheat.godot.vtableRva,
        memberCount: cheat.godot.memberCount,
        tags: cheat.godot.tags,
        index: cheat.godot.index,
        hint: godotInstances.current[cheat.id] ?? null,
      });
      godotInstances.current[cheat.id] = instance;
      return addr;
    } else if (cheat.ue5ArrayMatchClass) {
      finalAddr = await invoke<string>('resolve_ue5_array_item', {
        pid: pidRef.current,
        moduleName: cheat.module,
        gobjectsOffset: cheat.ue5GObjectsOffset ?? 0,
        gnamesOffset: cheat.ue5GNamesOffset ?? 0,
        className: cheat.ue5ClassName ?? '',
        propertyOffset: cheat.ue5PropertyOffset ?? 0,
        extraOffsets: cheat.ue5Offsets ?? null,
        arrayOffset: cheat.ue5ArrayOffset ?? 0,
        itemStride: cheat.ue5ArrayItemStride ?? 0,
        itemPtrOffset: cheat.ue5ArrayItemPtrOffset ?? 0,
        matchClass: cheat.ue5ArrayMatchClass,
        valueOffset: cheat.ue5ArrayValueOffset ?? 0,
        offsets: cheat.ue5CustomLayout ?? null,
      });
    } else if (cheat.type === 'mono') {
      finalAddr = await invoke<string>('resolve_mono_field', {
        pid: pidRef.current,
        moduleName: cheat.module,
        assembly: cheat.monoAssembly ?? 'Assembly-CSharp',
        namespace: cheat.monoNamespace ?? '',
        className: cheat.monoClass ?? '',
        fieldName: cheat.monoField ?? '',
        isStatic: cheat.monoStatic ?? true,
      });
    } else if (cheat.il2cppPath) {
      finalAddr = await invoke<string>('resolve_il2cpp_path', {
        pid: pidRef.current,
        moduleName: cheat.module,
        namespace: cheat.il2cppNamespace ?? '',
        className: cheat.il2cppClass ?? '',
        staticField: cheat.il2cppStaticField ?? 'Instance',
        hops: cheat.il2cppPath,
      });
    } else if (cheat.monoPath) {
      finalAddr = await invoke<string>('resolve_mono_path', {
        pid: pidRef.current,
        moduleName: cheat.module,
        assembly: cheat.monoAssembly ?? 'Assembly-CSharp',
        namespace: cheat.monoNamespace ?? '',
        className: cheat.monoClass ?? '',
        staticField: cheat.monoStaticField ?? '_instance',
        hops: cheat.monoPath.map((h) => ({
          namespace: h.namespace ?? '',
          className: h.className,
          field: h.field,
          dictKey: h.dictKey,
        })),
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
    } else if (cheat.type === 'ue5_prop' && cheat.ue5CustomLayout && cheat.ue5NoCache) {
      // Cheap path: reuse the cached class-object pointer (only re-scan GObjects if we've
      // never resolved it, or a previous tick invalidated it) and re-walk just the tail of
      // the chain every call — see classObjCache's comment for why.
      let objAddr = classObjCache.current[cheat.id];
      if (objAddr) {
        // A level transition (fast travel, save reload) can recreate the controller
        // itself while leaving the old one's memory committed — a write through the
        // stale cached pointer can keep "succeeding" (no read/write error) while doing
        // nothing, since nothing ever throws to trigger the catch-block invalidation
        // below. This cheap per-tick check (a couple of reads, not a GObjects scan)
        // catches that case proactively instead of relying on an actual fault.
        const stillValid = await invoke<boolean>('verify_object_class', {
          pid: pidRef.current,
          moduleName: cheat.module,
          gnamesOffset: cheat.ue5GNamesOffset ?? 0,
          objAddr,
          className: cheat.ue5ClassName ?? '',
          offsets: cheat.ue5CustomLayout,
        }).catch(() => false);
        if (!stillValid) {
          delete classObjCache.current[cheat.id];
          objAddr = '';
        }
      }
      if (!objAddr) {
        objAddr = await invoke<string>('resolve_ue5_prop_custom', {
          pid: pidRef.current,
          moduleName: cheat.module,
          gobjectsOffset: cheat.ue5GObjectsOffset ?? 0,
          gnamesOffset: cheat.ue5GNamesOffset ?? 0,
          className: cheat.ue5ClassName ?? '',
          propertyOffset: 0,
          extraOffsets: null,
          offsets: cheat.ue5CustomLayout,
        });
        classObjCache.current[cheat.id] = objAddr;
      }
      try {
        finalAddr = await invoke<string>('resolve_pointer_chain_from', {
          pid: pidRef.current,
          objAddr,
          propertyOffset: cheat.ue5PropertyOffset ?? 0,
          extraOffsets: cheat.ue5Offsets ?? [],
        });
      } catch (e) {
        // The cached class object pointer itself may have gone stale (e.g. respawn) —
        // drop it so the next attempt pays for a fresh scan, then surface the error as
        // usual this tick.
        delete classObjCache.current[cheat.id];
        throw e;
      }
    } else if (cheat.type === 'ue5_prop' && cheat.ue5CustomLayout) {
      finalAddr = await invoke<string>('resolve_ue5_prop_custom', {
        pid: pidRef.current,
        moduleName: cheat.module,
        gobjectsOffset: cheat.ue5GObjectsOffset ?? 0,
        gnamesOffset: cheat.ue5GNamesOffset ?? 0,
        className: cheat.ue5ClassName ?? '',
        propertyOffset: cheat.ue5PropertyOffset ?? 0,
        extraOffsets: cheat.ue5Offsets ?? null,
        offsets: cheat.ue5CustomLayout,
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
        const found = await invoke<string>('aob_scan', {
          pid: pidRef.current,
          moduleName: cheat.module,
          pattern: cheat.signature,
        });
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
      if (pollInFlight.current) return;
      const currentActive = activeGameRef.current;
      if (!currentActive || !pidRef.current) return;
      pollInFlight.current = true;
      try {
        const results = await Promise.all(
          currentActive.cheats
            .filter(
              (c) =>
                c.valueType != null &&
                c.type !== 'code_patch' &&
                c.type !== 'code_cave' &&
                // Array-item cheats (e.g. a currency stack inside a dynamic inventory
                // array) intentionally never cache their resolved address — polling one
                // every tick would re-walk the entire GObjects table (500k+ entries on
                // some UE5 games) just to refresh a number nobody asked to watch live.
                // These are one-shot "Fire" actions; only resolve on demand.
                !c.ue5ArrayMatchClass,
            )
            .map(async (cheat) => {
              try {
                const failedAt = cheatFailedAt.current[cheat.id];
                if (failedAt && Date.now() - failedAt < 30_000) {
                  return { id: cheat.id, val: '???' };
                }
                const addr = await resolveCheatAddress(cheat);
                const hexAddr = toHexAddr(addr);
                if (
                  (cheat.type === 'toggle' ||
                    cheat.type === 'mono_chain' ||
                    cheat.type === 'ue5_prop') &&
                  cheat.active &&
                  cheat.valueType
                ) {
                  let writeValue = cheat.onValue;
                  if (cheat.onValueFromOffset != null) {
                    const srcAddr = toHexAddr(
                      '0x' +
                        (
                          BigInt(addr) -
                          BigInt(cheat.monoFinalOffset ?? 0) +
                          BigInt(cheat.onValueFromOffset)
                        ).toString(16),
                    );
                    writeValue = await invoke<number>(memCmd('read', cheat.valueType), {
                      pid: pidRef.current,
                      address: srcAddr,
                    });
                  }
                  await writeWithMirror(
                    pidRef.current,
                    hexAddr,
                    cheat.valueType,
                    writeValue,
                    cheat.ue5MirrorOffset,
                  );
                }
                const val =
                  cheat.valueType === 'byte'
                    ? await invoke<number>('read_byte', { pid: pidRef.current, address: hexAddr })
                    : await invoke<number>(memCmd('read', cheat.valueType), {
                        pid: pidRef.current,
                        address: hexAddr,
                      });
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
            }),
        );
        setActiveGame((prev) => {
          if (!prev) return null;
          return {
            ...prev,
            cheats: prev.cheats.map((c) => {
              const r = results.find((res) => res.id === c.id);
              return r ? { ...c, currentValue: r.val } : c;
            }),
          };
        });

        // Refresh any active code_cave sites' playerPtrScratchOffset with the current
        // live player pointer, so a cave's own comparison logic (e.g. "is the object
        // that just called GetBaseDamage owned by the player") stays correct across
        // respawns/pawn changes without needing shared memory between cave sites.
        await Promise.all(
          Object.entries(caveScratchSites.current).map(async ([key, { cheat, scratchAddr }]) => {
            // No `cheat.active` guard here on purpose: `cheat` is a snapshot captured at
            // install time (in installCaveSites), so its `.active` field is permanently
            // stale (still false, from just before the toggle-on state flip) and would
            // make this bail out on every tick forever. An entry's mere presence in this
            // map already means its cave is currently installed — added in
            // installCaveSites, removed in uninstallCaveSites/on failure — so that's the
            // real "should refresh" signal.
            try {
              if (cheat.godot) {
                await refreshGodotScratch(cheat, scratchAddr);
                return;
              }
              const fieldAddr = await invoke<string>('resolve_ue5_prop_custom', {
                pid: pidRef.current,
                moduleName: cheat.module,
                gobjectsOffset: cheat.ue5GObjectsOffset ?? 0,
                gnamesOffset: cheat.ue5GNamesOffset ?? 0,
                className: cheat.ue5ClassName ?? '',
                propertyOffset: cheat.ue5PropertyOffset ?? 0,
                extraOffsets: null,
                offsets: cheat.ue5CustomLayout,
              });
              const hex = await invoke<string>('read_raw_bytes', {
                pid: pidRef.current,
                address: fieldAddr,
                count: 8,
              });
              const bytes = hex.split(' ').map((b) => parseInt(b, 16));
              await invoke('patch_bytes', { pid: pidRef.current, address: scratchAddr, bytes });
            } catch (e) {
              onCheatError?.(cheat.id, e instanceof Error ? e.message : String(e));
              delete caveScratchSites.current[key];
            }
          }),
        );
      } catch {
        /* empty */
      } finally {
        pollInFlight.current = false;
      }
    }, pollInterval);
    return () => {
      clearInterval(interval);
      pollInFlight.current = false;
    };
  }, [pid, activeGame?.name, pollInterval]);

  const selectGame = useCallback(async (game: GameTrainer | null) => {
    addressCache.current = {};
    godotInstances.current = {};
    classObjCache.current = {};
    cheatFailedAt.current = {};
    patchAddrCache.current = {};
    caveScratchSites.current = {};
    codePatchIntervals.current.forEach((id) => clearInterval(id));
    codePatchIntervals.current.clear();
    setActiveGame(game);
    if (!game) {
      setPid(null);
      return;
    }
    try {
      const foundPid = await invoke<number | null>('find_game', { name: game.executable });
      setPid(foundPid);
    } catch (err) {
      console.error('HOOK: find_game failed:', err);
    }
  }, []);

  function resolveWriteValue(cheat: Cheat, customValueStr?: string): number {
    if (customValueStr !== undefined && customValueStr !== '') {
      const parsed =
        cheat.valueType === 'float' || cheat.valueType === 'double'
          ? parseFloat(customValueStr)
          : parseInt(customValueStr, 10);
      if (!isNaN(parsed)) return parsed + (cheat.writeBias ?? 0);
    }
    return cheat.onValue + (cheat.writeBias ?? 0);
  }

  function getCaveSites(cheat: Cheat) {
    return (
      cheat.caveSites ??
      (cheat.cavePayload
        ? [
            {
              patchSite: cheat.patchSite,
              patchSignature: cheat.patchSignature,
              patchLen: cheat.patchLen,
              cavePayload: cheat.cavePayload,
              farJump: cheat.farJump,
              coreLen: cheat.coreLen,
              ripFixup: cheat.ripFixup,
            },
          ]
        : [])
    );
  }

  // Godot code_cave: the cave compares a store destination against a scratch qword holding the
  // live address of the target member (see the `godot` cheat field); keep it current.
  async function refreshGodotScratch(cheat: Cheat, scratchAddr: string) {
    const addr = BigInt(await resolveCheatAddress(cheat));
    const bytes = Array.from({ length: 8 }, (_, i) => Number((addr >> BigInt(8 * i)) & 0xffn));
    await invoke('patch_bytes', { pid: pidRef.current, address: scratchAddr, bytes });
  }

  async function installCaveSites(
    cheat: Cheat,
    sites: ReturnType<typeof getCaveSites>,
    onError?: (id: string, msg: string) => void,
  ): Promise<boolean> {
    const enabledIdx: number[] = [];
    try {
      for (let i = 0; i < sites.length; i++) {
        const site = sites[i];
        let siteRva = site.patchSite;
        if (!siteRva && site.patchSignature) {
          const baseStr = await invoke<string>('get_module_base', { pid, moduleName: cheat.module });
          const found = await invoke<string>('aob_scan', {
            pid,
            moduleName: cheat.module,
            pattern: site.patchSignature,
          });
          siteRva = '0x' + (BigInt(found) - BigInt(baseStr)).toString(16);
        }
        const caveAddr = await invoke<string>('enable_code_cave', {
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
        if (site.playerPtrScratchOffset != null) {
          const scratchAddr =
            '0x' + (BigInt(caveAddr) + BigInt(site.playerPtrScratchOffset)).toString(16);
          caveScratchSites.current[`${cheat.id}#${i}`] = { cheat, scratchAddr };
          // Fill it right away rather than waiting a poll tick (until then the cave is inert).
          if (cheat.godot) await refreshGodotScratch(cheat, scratchAddr);
        }
        if (site.valueScratchOffset != null) {
          const valueAddr = '0x' + (BigInt(caveAddr) + BigInt(site.valueScratchOffset)).toString(16);
          valueScratchSites.current[`${cheat.id}#${i}`] = valueAddr;
        }
      }
      return true;
    } catch (err) {
      // Partial failure — undo whichever sites already installed so we don't leave some
      // of them patched while the UI shows the cheat as off.
      for (const i of enabledIdx) {
        await invoke('disable_code_cave', { pid, cheatId: `${cheat.id}#${i}` }).catch(() => {});
        delete caveScratchSites.current[`${cheat.id}#${i}`];
        delete valueScratchSites.current[`${cheat.id}#${i}`];
      }
      onError?.(cheat.id, err instanceof Error ? err.message : String(err));
      return false;
    }
  }

  async function uninstallCaveSites(cheat: Cheat, sites: ReturnType<typeof getCaveSites>) {
    for (let i = 0; i < sites.length; i++) {
      await invoke('disable_code_cave', { pid, cheatId: `${cheat.id}#${i}` }).catch(() => {});
      delete caveScratchSites.current[`${cheat.id}#${i}`];
      delete valueScratchSites.current[`${cheat.id}#${i}`];
    }
  }

  const applyCheat = async (
    cheat: Cheat,
    customValueStr?: string,
    onError?: (id: string, msg: string) => void,
  ) => {
    if (!pid || !activeGame) return;

    if (cheat.type === 'il2cpp_decay_nop') {
      const willBeActive = !cheat.active;
      try {
        if (willBeActive && cheat.il2cppPath && cheat.valueType) {
          let value = cheat.onValue;
          if (cheat.il2cppValuePath) {
            const srcAddr = await invoke<string>('resolve_il2cpp_path', {
              pid,
              moduleName: cheat.module,
              namespace: cheat.il2cppNamespace ?? '',
              className: cheat.il2cppClass ?? '',
              staticField: cheat.il2cppStaticField ?? 'Instance',
              hops: cheat.il2cppValuePath,
            });
            value = await invoke<number>(memCmd('read', cheat.valueType), {
              pid,
              address: srcAddr,
            });
          }
          const addr = await resolveCheatAddress(cheat);
          await invoke(memCmd('write', cheat.valueType), {
            pid,
            address: toHexAddr(addr),
            value,
          });
        }
        await invoke('set_il2cpp_decay_nop', {
          pid,
          moduleName: cheat.module,
          key: cheat.id,
          methods: cheat.il2cppMethods ?? (cheat.il2cppMethod ? [cheat.il2cppMethod] : []),
          fieldClass: cheat.il2cppDecay?.className ?? '',
          field: cheat.il2cppDecay?.field ?? '',
          enable: willBeActive,
        });
        setActiveGame((prev) => {
          if (!prev) return null;
          return {
            ...prev,
            cheats: prev.cheats.map((c) =>
              c.id === cheat.id ? { ...c, active: willBeActive } : c,
            ),
          };
        });
      } catch (err) {
        onError?.(cheat.id, err instanceof Error ? err.message : String(err));
      }
      return;
    }

    if (cheat.type === 'il2cpp_method_ret') {
      const willBeActive = !cheat.active;
      try {
        await invoke('set_il2cpp_method_ret', {
          pid,
          moduleName: cheat.module,
          key: cheat.id,
          methods: cheat.il2cppMethods ?? (cheat.il2cppMethod ? [cheat.il2cppMethod] : []),
          enable: willBeActive,
        });
        setActiveGame((prev) => {
          if (!prev) return null;
          return {
            ...prev,
            cheats: prev.cheats.map((c) =>
              c.id === cheat.id ? { ...c, active: willBeActive } : c,
            ),
          };
        });
      } catch (err) {
        onError?.(cheat.id, err instanceof Error ? err.message : String(err));
      }
      return;
    }

    if (cheat.type === 'mono_call_nop') {
      const willBeActive = !cheat.active;
      try {
        const m = cheat.monoMethod;
        const c = cheat.monoCallee;
        await invoke<string>('set_mono_call_nop', {
          pid,
          cheatId: cheat.id,
          moduleName: cheat.module,
          caller: { assembly: m?.assembly ?? 'Assembly-CSharp', namespace: m?.namespace ?? '', className: m?.className ?? '', method: m?.method ?? '' },
          callee: { assembly: c?.assembly ?? 'Assembly-CSharp', namespace: c?.namespace ?? '', className: c?.className ?? '', method: c?.method ?? '' },
          enable: willBeActive,
        });
        setActiveGame((prev) => {
          if (!prev) return null;
          return {
            ...prev,
            cheats: prev.cheats.map((x) => (x.id === cheat.id ? { ...x, active: willBeActive } : x)),
          };
        });
      } catch (err) {
        onError?.(cheat.id, err instanceof Error ? err.message : String(err));
      }
      return;
    }

    if (cheat.type === 'mono_store_nop' || cheat.type === 'mono_method_ret') {
      const willBeActive = !cheat.active;
      try {
        const retOnly = cheat.type === 'mono_method_ret';
        const status = await invoke<string>(retOnly ? 'set_mono_method_ret' : 'set_mono_store_nop', {
          pid,
          ...(retOnly ? { cheatId: cheat.id } : { field: cheat.monoStoreField ?? '' }),
          moduleName: cheat.module,
          assembly: cheat.monoMethod?.assembly ?? 'Assembly-CSharp',
          namespace: cheat.monoMethod?.namespace ?? '',
          className: cheat.monoMethod?.className ?? '',
          method: cheat.monoMethod?.method ?? '',
          ilSignature: cheat.ilSignature ?? null,
          ilPatches: cheat.ilPatches ?? null,
          enable: willBeActive,
        });
        // Informational notices (shown in the cheat's message line, not failures).
        if (status === 'il') {
          onError?.(
            cheat.id,
            'Note: applied before the game compiled this method. Turning it off will only take effect after a game restart.',
          );
        } else if (status === 'restored-baked') {
          onError?.(
            cheat.id,
            'Note: the game compiled this method while it was on, so the effect stays until you restart the game.',
          );
        }
        setActiveGame((prev) => {
          if (!prev) return null;
          return {
            ...prev,
            cheats: prev.cheats.map((c) =>
              c.id === cheat.id ? { ...c, active: willBeActive } : c,
            ),
          };
        });
      } catch (err) {
        onError?.(cheat.id, err instanceof Error ? err.message : String(err));
      }
      return;
    }

    if (cheat.type === 'code_patch') {
      const willBeActive = !cheat.active;
      if (willBeActive) {
        // Don't flip to "active" until the patch has actually resolved and applied once.
        const ok = await startCodePatch(cheat, pid, onError);
        if (ok) {
          setActiveGame((prev) => {
            if (!prev) return null;
            return {
              ...prev,
              cheats: prev.cheats.map((c) => (c.id === cheat.id ? { ...c, active: true } : c)),
            };
          });
        }
      } else {
        stopCodePatch(cheat, pid);
        setActiveGame((prev) => {
          if (!prev) return null;
          return {
            ...prev,
            cheats: prev.cheats.map((c) => (c.id === cheat.id ? { ...c, active: false } : c)),
          };
        });
      }
      return;
    }

    if (cheat.type === 'code_cave') {
      const sites = getCaveSites(cheat);
      if (sites.length === 0) return;
      const willBeActive = !cheat.active;
      setActiveGame((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          cheats: prev.cheats.map((c) => (c.id === cheat.id ? { ...c, active: willBeActive } : c)),
        };
      });
      if (willBeActive) {
        const ok = await installCaveSites(cheat, sites, onError);
        if (!ok) {
          setActiveGame((prev) => {
            if (!prev) return null;
            return {
              ...prev,
              cheats: prev.cheats.map((c) => (c.id === cheat.id ? { ...c, active: false } : c)),
            };
          });
        }
      } else {
        await uninstallCaveSites(cheat, sites);
      }
      return;
    }

    if (cheat.type === 'action' || cheat.type === 'mono') {
      try {
        const addr = await resolveCheatAddress(cheat);
        await writeWithMirror(
          pid,
          toHexAddr(addr),
          cheat.valueType,
          resolveWriteValue(cheat, customValueStr),
          cheat.ue5MirrorOffset,
        );
      } catch (err) {
        onError?.(cheat.id, err instanceof Error ? err.message : String(err));
      }
      return;
    }

    const willBeActive = !cheat.active;
    setActiveGame((prev) => {
      if (!prev) return null;
      return {
        ...prev,
        cheats: prev.cheats.map((c) => (c.id === cheat.id ? { ...c, active: willBeActive } : c)),
      };
    });
    try {
      const addr = await resolveCheatAddress(cheat);
      const hexAddr = toHexAddr(addr);
      if (cheat.type === 'patch') {
        const bytes = willBeActive ? cheat.onBytes : cheat.offBytes;
        await invoke('patch_bytes', { pid, address: hexAddr, bytes });
      } else if (cheat.bitIndex !== undefined) {
        const bitSet = willBeActive ? cheat.onValue !== 0 : (cheat.offValue ?? 1) !== 0;
        await invoke('toggle_bit_flag', {
          pid,
          address: hexAddr,
          bit: cheat.bitIndex,
          value: bitSet,
        });
      } else if (
        (cheat.type === 'ue5_prop' || cheat.il2cppPath) &&
        !willBeActive &&
        cheat.offValue !== undefined
      ) {
        await writeWithMirror(pid, hexAddr, cheat.valueType, cheat.offValue, cheat.ue5MirrorOffset);
      } else if (willBeActive) {
        const wv = resolveWriteValue(cheat, customValueStr);
        await writeWithMirror(pid, hexAddr, cheat.valueType, wv, cheat.ue5MirrorOffset);
      }
    } catch (err) {
      setActiveGame((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          cheats: prev.cheats.map((c) => (c.id === cheat.id ? { ...c, active: !willBeActive } : c)),
        };
      });
      onError?.(cheat.id, err instanceof Error ? err.message : String(err));
    }
  };

  // Discrete stepped-slider cheats (see Cheat.sliderOptions): the first option is "off"
  // (uninstalls the cave entirely, not just a value of that magnitude); any other option
  // installs the cave if it isn't already active, then writes the chosen value into every
  // site's valueScratchOffset — rewriting an already-active cave's value in place rather
  // than reinstalling it, since the cave logic itself doesn't change between values.
  const setSliderCheat = async (
    cheat: Cheat,
    value: number,
    onError?: (id: string, msg: string) => void,
  ) => {
    if (!pid || !activeGame || !cheat.sliderOptions) return;
    const sites = getCaveSites(cheat);
    if (sites.length === 0) return;
    const offValue = cheat.sliderOptions[0];

    if (value === offValue) {
      if (!cheat.active) return;
      await uninstallCaveSites(cheat, sites);
      setActiveGame((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          cheats: prev.cheats.map((c) =>
            c.id === cheat.id ? { ...c, active: false, currentValue: value } : c,
          ),
        };
      });
      return;
    }

    if (!cheat.active) {
      const ok = await installCaveSites(cheat, sites, onError);
      if (!ok) return;
      setActiveGame((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          cheats: prev.cheats.map((c) => (c.id === cheat.id ? { ...c, active: true } : c)),
        };
      });
    }

    try {
      for (let i = 0; i < sites.length; i++) {
        const addr = valueScratchSites.current[`${cheat.id}#${i}`];
        if (addr) {
          await invoke('write_float', { pid, address: addr, value });
        }
      }
      setActiveGame((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          cheats: prev.cheats.map((c) => (c.id === cheat.id ? { ...c, currentValue: value } : c)),
        };
      });
    } catch (err) {
      onError?.(cheat.id, err instanceof Error ? err.message : String(err));
    }
  };

  return { activeGame, trainers, selectGame, applyCheat, setSliderCheat, pid };
}
