import { useState, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { GameTrainer } from '../hooks/useTrainer';
import { parseLePtr64 } from '../utils/memory';

interface DevPanelProps {
  pid: number | null;
  activeGame: GameTrainer;
}

export function DevPanel({ pid, activeGame }: DevPanelProps) {
  const [diffStatus, setDiffStatus] = useState<string>('');
  const [diffResults, setDiffResults] = useState<string[]>([]);
  const [dumpAddr, setDumpAddr] = useState<string>('');
  const [classKeyword, setClassKeyword] = useState<string>('');
  const [snapTarget, setSnapTarget] = useState<string>('');
  const [resultsFilter, setResultsFilter] = useState<string>('');
  const dropRateInterval = useRef<ReturnType<typeof setInterval> | null>(null);
  const [dropRateActive, setDropRateActive] = useState(false);

  const bl4Prop = (className: string) =>
    invoke<string>('resolve_ue5_prop', {
      pid,
      moduleName: 'Borderlands4.exe',
      gobjectsAob: '',
      gnamesAob: '',
      gobjectsOffset: 0x11765A30,
      gnamesOffset: 0x1167FDD0,
      className,
      propertyOffset: 0,
      extraOffsets: null,
    });

  const applyDropRatePatch = async () => {
    if (!pid || !activeGame) return;
    try {
      setDiffStatus('Applying drop rate patch...');
      const baseStr = await invoke<string>('get_module_base', { pid, moduleName: activeGame.executable });
      const base = BigInt(baseStr);
      const patches: [bigint, number[]][] = [
        [base + 0x128E1076n, [0x0D, 0xF6, 0x3E, 0x2B, 0xD5]],
        [base + 0x1F8F83EDn, [0xFC]],
        [base + 0x1F8F83FDn, [0x92, 0xDA]],
        [base + 0x1F902DB0n, [0xBA, 0x03, 0xE4, 0xF5, 0xF8, 0x51]],
      ];
      const log: string[] = [];
      for (const [addr, bytes] of patches) {
        const hex = `0x${addr.toString(16).toUpperCase()}`;
        await invoke('patch_bytes', { pid, address: hex, bytes });
        log.push(`OK ${hex} <- [${bytes.map(b => b.toString(16).padStart(2, '0').toUpperCase()).join(' ')}]`);
      }
      setDiffResults(log);
      setDiffStatus('Drop rate patch applied — test in-game');
    } catch (e) { setDiffStatus(`Drop rate patch FAILED: ${e}`); }
  };

  const applyLegendaryPatch = async () => {
    if (!pid || !activeGame) return;
    try {
      setDiffStatus('Applying legendary patch...');
      const baseStr = await invoke<string>('get_module_base', { pid, moduleName: activeGame.executable });
      const base = BigInt(baseStr);
      const patches: [bigint, number[]][] = [
        [base + 0x128DFFEEn, [0x79]],
        [base + 0x1F8F83A5n, [0x42]],
        [base + 0x1F8F83EDn, [0xC3]],
        [base + 0x1F8F83FDn, [0x1C, 0xB3, 0x88]],
        [base + 0x1F902DB0n, [0xD8, 0xB4, 0xAB, 0x57, 0xF8, 0x51]],
      ];
      const log: string[] = [];
      for (const [addr, bytes] of patches) {
        const hex = `0x${addr.toString(16).toUpperCase()}`;
        await invoke('patch_bytes', { pid, address: hex, bytes });
        log.push(`OK ${hex} <- [${bytes.map(b => b.toString(16).padStart(2, '0').toUpperCase()).join(' ')}]`);
      }
      setDiffResults(log);
      setDiffStatus('Legendary patch applied — test in-game');
    } catch (e) { setDiffStatus(`Legendary patch FAILED: ${e}`); }
  };

  return (
            <div className="cheat-list" style={{ marginTop: '1rem', borderTop: '1px solid #333', paddingTop: '1rem' }}>
              <div className="cheat-item" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: '0.5rem' }}>
                <span className="cheat-name" style={{ fontSize: '0.75rem', color: '#888' }}>PATCH DIFF TOOL</span>
                <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                  <input
                    className="value-input"
                    type="text"
                    placeholder={activeGame?.executable ?? 'module name'}
                    value={snapTarget}
                    onChange={e => setSnapTarget(e.target.value)}
                    style={{ fontFamily: 'monospace', fontSize: '0.75rem' }}
                  />
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!activeGame || !pid) return;
                    const modName = snapTarget.trim() || activeGame.executable;
                    try {
                      setDiffStatus(`Snapshotting exec pages of ${modName}...`);
                      setDiffResults([]);
                      const msg = await invoke<string>('snapshot_by_module_name', { pid, moduleName: modName });
                      setDiffStatus(msg);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Snapshot</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!activeGame || !pid) return;
                    const modName = snapTarget.trim() || activeGame.executable;
                    try {
                      setDiffStatus(`Full snapshot (code+data) of ${modName}...`);
                      setDiffResults([]);
                      const msg = await invoke<string>('snapshot_full', { pid, moduleName: modName });
                      setDiffStatus(msg);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Full Snap</button>
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
                      setDiffStatus('Resolving NexusConfigStoreInventoryRarity...');
                      setDiffResults([]);
                      const addr = await bl4Prop('NexusConfigStoreInventoryRarity');
                      setDiffStatus(`Found at ${addr} — dumping floats...`);
                      const lines = await invoke<string[]>('dump_floats_at', { pid, address: addr, count: 256 });
                      setDiffResults([`Object: ${addr}`, ...lines]);
                      setDiffStatus(`Dumped 256 floats from NexusConfigStoreInventoryRarity`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump RarityConfig</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving NexusConfigStoreInventoryRarity...');
                      setDiffResults([]);
                      const addr = await bl4Prop('NexusConfigStoreInventoryRarity');
                      const objBase = BigInt(addr);
                      const allLines: string[] = [`Object: ${addr}`];
                      // TArray-shaped slots identified from the 1024B dump
                      const slots: [bigint, string][] = [
                        [0x040n, '+0x040 TArray[3]'],
                        [0x168n, '+0x168 TArray[13]'],
                        [0x380n, '+0x380 TArray[1]'],
                        [0x3D0n, '+0x3D0 TArray[2]'],
                      ];
                      for (const [off, label] of slots) {
                        const ptrAddr = `0x${(objBase + off).toString(16).toUpperCase()}`;
                        const rawHex = await invoke<string>('read_raw_bytes', { pid, address: ptrAddr, count: 8 });
                        const dataPtr = parseLePtr64(rawHex.split(' '));
                        const dataPtrStr = `0x${dataPtr.toString(16).toUpperCase()}`;
                        allLines.push(`--- ${label} → ${dataPtrStr} ---`);
                        try {
                          const lines = await invoke<string[]>('dump_floats_at', { pid, address: dataPtrStr, count: 128 });
                          allLines.push(...lines);
                        } catch (e2) { allLines.push(`[read error: ${e2}]`); }
                      }
                      setDiffResults(allLines);
                      setDiffStatus('TArray ptr dump complete');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump Rarity Ptrs</button>
                </div>
                <div style={{ display: 'flex', gap: '0.5rem' }}>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Enumerating modules...');
                      const lines = await invoke<string[]>('list_modules', { pid });
                      setDiffResults(lines);
                      setDiffStatus(`${lines.length} module(s) — sorted by size desc. Copy name into Snapshot input to diff a DLL.`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>List Modules</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Enumerating executable regions...');
                      const lines = await invoke<string[]>('list_exec_regions', { pid });
                      setDiffResults(lines);
                      setDiffStatus(`${lines.length} exec region(s). Large anonymous regions = Denuvo/JIT heap — snapshot those bases.`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>List Exec Regions</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    const modName = snapTarget.trim() || 'Trainer_49051_20e30ce373.dll';
                    const outPath = `C:\\Users\\renga\\OneDrive\\Desktop\\wemod_trainer_dump.dll`;
                    try {
                      setDiffStatus(`Dumping ${modName} to desktop...`);
                      const msg = await invoke<string>('dump_module_to_file', { pid, moduleName: modName, outPath });
                      setDiffStatus(msg + ' — open in Ghidra, Binary Ninja, or CFF Explorer');
                      setDiffResults([]);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump DLL</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    const modName = snapTarget.trim() || 'Trainer_49051_20e30ce373.dll';
                    try {
                      setDiffStatus(`Reading strings from ${modName} (min 10 chars)...`);
                      const lines = await invoke<string[]>('read_module_strings', { pid, moduleName: modName, minLen: 10 });
                      setDiffResults(lines);
                      setDiffStatus(`${lines.length} strings — look for loot/drop/rarity/weight/legendary`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Read Strings</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Reading CE script from Trainer DLL (full script)...');
                      const baseStr = await invoke<string>('get_module_base', { pid, moduleName: 'Trainer_49051_20e30ce373.dll' });
                      const base = BigInt(baseStr);
                      const scriptStart = 0x8B81An;
                      const scriptLen = 0x4000; // 16KB — captures all versions of all cheat scripts
                      const addr = `0x${(base + scriptStart).toString(16).toUpperCase()}`;
                      const hexStr = await invoke<string>('read_raw_bytes', { pid, address: addr, count: scriptLen });
                      const bytes = hexStr.split(' ').map(h => parseInt(h, 16));
                      let text = '';
                      for (const b of bytes) {
                        if (b === 0x0A) text += '\n';
                        else if (b === 0x0D) continue;
                        else if (b === 0x00) text += '\n'; // null separator between lines in CE script
                        else if (b >= 0x20 && b < 0x7F) text += String.fromCharCode(b);
                        else text += ' ';
                      }
                      const lines = text.split('\n').map(l => l.trim()).filter(l => l.length > 0);
                      setDiffResults(lines);
                      setDiffStatus(`CE script — ${lines.length} lines. Filter for jne/jmp/movss to find branch logic.`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Read CE Script</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Scanning for WeMod legendary cave (MOV_7FFE+AND_7FFF+JMP tail) — enable WeMod cheat first...');
                      setDiffResults([]);
                      const lines = await invoke<string[]>('find_outside_jmps', { pid, moduleName: 'Borderlands4.exe' });
                      setDiffResults(lines);
                      setDiffStatus(`${lines.length} cave(s) found — patch_site RVA is what to put in JSON`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Find WeMod JMPs</button>
                </div>
                <div style={{ display: 'flex', gap: '0.5rem' }}>
                  <button
                    className="fire-button"
                    disabled={!pid}
                    style={{ background: dropRateActive ? '#c00' : undefined }}
                    onClick={async () => {
                      if (!pid) return;
                      if (dropRateActive) {
                        if (dropRateInterval.current) { clearInterval(dropRateInterval.current); dropRateInterval.current = null; }
                        setDropRateActive(false);
                        setDiffStatus('Drop rate patch stopped');
                        return;
                      }
                      try {
                        setDiffStatus('Finding WeMod drop rate cave...');
                        const info = await invoke<string[]>('find_wemod_drop_cave', { pid });
                        const [flagAddr, aobAddr, caveBase] = info;
                        // Compute RVA of aobdroprate relative to BL4 base
                        const bl4base = BigInt(await invoke<string>('get_module_base', { pid, moduleName: 'Borderlands4.exe' }));
                        const aobAbs = BigInt(aobAddr);
                        const rva = aobAbs - bl4base;
                        // Write 0F 57 C0 90 90 90 90 at aobdroprate (XORPS XMM0,XMM0 + 4x NOP)
                        // This overrides WeMod's E9 JMP, permanently keeping XMM0=0 (100% drop)
                        const PATCH = [0x0F, 0x57, 0xC0, 0x90, 0x90, 0x90, 0x90];
                        const writeAddr = `0x${aobAbs.toString(16).toUpperCase()}`;
                        const write = () => invoke('patch_bytes', { pid, address: writeAddr, bytes: PATCH }).catch(() => {});
                        write();
                        dropRateInterval.current = setInterval(write, 50);
                        setDropRateActive(true);
                        setDiffResults([
                          `cave base:    ${caveBase}`,
                          `drop_rate flag: ${flagAddr}`,
                          `aobdroprate:  ${aobAddr}  (RVA 0x${rva.toString(16).toUpperCase()})`,
                          `patch bytes:  0F 57 C0 90 90 90 90  (XORPS XMM0,XMM0 + NOP×4)`,
                        ]);
                        setDiffStatus('Drop rate patch ACTIVE (red = click to stop). Test in-game now.');
                      } catch (e) { setDiffStatus(String(e)); }
                    }}
                  >{dropRateActive ? '■ Stop Drop Rate' : '▶ Drop Rate (Cave)'}</button>
                </div>
                <div style={{ display: 'flex', gap: '0.5rem' }}>
                  <button className="fire-button" disabled={!pid} onClick={applyDropRatePatch}>Patch Drop Rate (legacy)</button>
                  <button className="fire-button" disabled={!pid} onClick={applyLegendaryPatch}>Patch Legendary</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid || !activeGame) return;
                    try {
                      const baseStr = await invoke<string>('get_module_base', { pid, moduleName: activeGame.executable });
                      const base = BigInt(baseStr);
                      // Read 512 bytes from the WeMod-confirmed patch site to see full function
                      const startRva = 0x24E1782n;
                      const addr = `0x${(base + startRva).toString(16).toUpperCase()}`;
                      const hexStr = await invoke<string>('read_raw_bytes', { pid, address: addr, count: 512 });
                      const bytes = hexStr.split(' ');
                      const lines: string[] = ['WeMod AoB match @ 0x24E1782 — reading 512 bytes (patch is first 5)'];
                      for (let i = 0; i < bytes.length; i += 16) {
                        const rva = (startRva + BigInt(i)).toString(16).toUpperCase();
                        const chunk = bytes.slice(i, i + 16).join(' ');
                        const marker = i === 0 ? ' ← AND/MOV EAX patch site' : '';
                        lines.push(`${rva}: ${chunk}${marker}`);
                      }
                      setDiffResults(lines);
                      const patchedBytes = bytes.slice(0, 5).join(' ');
                      const isPatched = patchedBytes === 'B8 FE 7F 00 00';
                      setDiffStatus(`Legendary func dump — first 5 bytes: ${patchedBytes} (${isPatched ? 'PATCH ACTIVE' : 'ORIGINAL — patch not applied'})`);
                    } catch (e) { setDiffStatus(`Read failed: ${e}`); }
                  }}>Read Legendary Func</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid || !activeGame) return;
                    try {
                      setDiffStatus('Reading code block 0x128DFF00 + 512 bytes...');
                      const baseStr = await invoke<string>('get_module_base', { pid, moduleName: activeGame.executable });
                      const base = BigInt(baseStr);
                      const startRva = 0x128DFF00n;
                      const addr = `0x${(base + startRva).toString(16).toUpperCase()}`;
                      const hexStr = await invoke<string>('read_raw_bytes', { pid, address: addr, count: 512 });
                      const bytes = hexStr.split(' ');
                      const lines: string[] = [];
                      for (let i = 0; i < bytes.length; i += 16) {
                        const rva = (startRva + BigInt(i)).toString(16).padStart(9, '0').toUpperCase();
                        const chunk = bytes.slice(i, i + 16).join(' ');
                        lines.push(`${rva}: ${chunk}`);
                      }
                      setDiffResults(lines);
                      setDiffStatus('Code block — LG-1 patch @ 0x128DFFEE (offset EE), DR-1 @ 0x128E1076 (offset 176)');
                    } catch (e) { setDiffStatus(`Read failed: ${e}`); }
                  }}>Read Code Block</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid || !activeGame) return;
                    try {
                      setDiffStatus('Reading live bytes at patch RVAs...');
                      const baseStr = await invoke<string>('get_module_base', { pid, moduleName: activeGame.executable });
                      const base = BigInt(baseStr);
                      const targets: [string, bigint, number][] = [
                        ['DBL-A 0x1F8F83E8', base + 0x1F8F83E8n, 8],
                        ['DBL-B 0x1F8F83F0', base + 0x1F8F83F0n, 8],
                        ['DBL-C 0x1F8F83F8', base + 0x1F8F83F8n, 8],
                        ['FLT-D 0x1F902DB0', base + 0x1F902DB0n, 4],
                      ];
                      const lines: string[] = [];
                      for (const [label, addr, count] of targets) {
                        const hex = `0x${addr.toString(16).toUpperCase()}`;
                        const bytes = await invoke<string>('read_raw_bytes', { pid, address: hex, count });
                        lines.push(`${label}: ${bytes}`);
                      }
                      setDiffResults(lines);
                      setDiffStatus('Live bytes — compare WeMod ON vs OFF vs our writes');
                    } catch (e) { setDiffStatus(`Read failed: ${e}`); }
                  }}>Read Live Bytes</button>
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
                  <button className="fire-button" disabled={!pid || !dumpAddr} onClick={async () => {
                    if (!pid || !dumpAddr) return;
                    try {
                      const before = await invoke<number>('read_float', { pid, address: dumpAddr });
                      await invoke<void>('patch_bytes', { pid, address: dumpAddr, bytes: [0x00, 0x00, 0x80, 0x3F] });
                      const after = await invoke<number>('read_float', { pid, address: dumpAddr });
                      setDiffResults([`${dumpAddr}:  ${before.toFixed(6)} → ${after.toFixed(6)}`]);
                      setDiffStatus(`Patched ${dumpAddr} → 1.0`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Write 1.0</button>
                </div>
                <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      const baseStr = await invoke<string>('get_module_base', { pid, moduleName: 'Borderlands4.exe' });
                      const bl4base = BigInt(baseStr);
                      // WeMod's exact aobscanmodule pattern for aobdroprarity (35 bytes, * → ??)
                      const pattern = '25 FF 7F 00 00 F3 0F 2A ?? F3 0F 5E ?? ?? ?? ?? ?? F3 0F 59 07 ?? 8D ?? 18 01 00 00 8B ?? 28 01 00 00 85';
                      setDiffStatus('Scanning full BL4 module for droprarity AoB (60-90s)...');
                      setDiffResults([]);
                      // Fallback patterns: full WeMod → without tail offsets → just AND+CVTSI2SS+DIVSS start
                      const fallbacks: [string, string][] = [
                        ['WeMod full (35B)', pattern],
                        ['no tail (18B)',   '25 FF 7F 00 00 F3 0F 2A ?? F3 0F 5E ?? ?? ?? ?? ?? F3 0F 59'],
                        ['short (13B)',     '25 FF 7F 00 00 F3 0F 2A ?? F3 0F 5E ?? ??'],
                      ];
                      const lines: string[] = [];
                      let found = false;
                      for (const [label, pat] of fallbacks) {
                        setDiffStatus(`Trying ${label}...`);
                        try {
                          const addr = await invoke<string>('aob_scan', { pid, moduleName: 'Borderlands4.exe', pattern: pat });
                          const rva = BigInt(addr) - bl4base;
                          const hex = await invoke<string>('read_raw_bytes', { pid, address: addr, count: 40 });
                          lines.push(`[${label}] RVA 0x${rva.toString(16).toUpperCase()}  abs ${addr}`);
                          lines.push(`  bytes: ${hex}`);
                          found = true;
                          break;
                        } catch {
                          lines.push(`[${label}] NOT FOUND`);
                        }
                        setDiffResults([...lines]);
                      }
                      setDiffResults([...lines]);
                      setDiffStatus(found ? 'Rarity AoB scan complete — verify bytes then update JSON' : 'All rarity patterns failed — address unknown');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Scan Rarity AoB</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Scanning BL4 for ALL AND EAX,0x7FFF occurrences (may take 60-120s)...');
                      setDiffResults([]);
                      const lines = await invoke<string[]>('scan_rarity_candidates', { pid, moduleName: 'Borderlands4.exe' });
                      setDiffResults(lines);
                      setDiffStatus(`${lines[0]} — compare ctx bytes against WeMod AoB to find real patch site`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>All AND EAX</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      const bl4base = BigInt(await invoke<string>('get_module_base', { pid, moduleName: 'Borderlands4.exe' }));
                      // RIP-relative addresses extracted from DIVSS/MULSS at 0x24E1782
                      // DIVSS RIP = 0x24E1793, disp = 0x0C3A6C79 → V1 (divisor)
                      // MULSS RIP = 0x24E179B, disp = 0x0C9D6265 → V2 (multiplier)
                      const v1Abs = `0x${(bl4base + 0x24E1793n + 0x0C3A6C79n).toString(16).toUpperCase()}`;
                      const v2Abs = `0x${(bl4base + 0x24E179Bn + 0x0C9D6265n).toString(16).toUpperCase()}`;
                      const v1 = await invoke<number>('read_float', { pid, address: v1Abs });
                      const v2 = await invoke<number>('read_float', { pid, address: v2Abs });
                      const expectedEAX = Math.trunc(32766.0 / v1 * v2);
                      const lines = [
                        `V1 divisor  @ ${v1Abs}: ${v1.toFixed(6)}`,
                        `V2 mult     @ ${v2Abs}: ${v2.toFixed(6)}`,
                        `int(32766 / ${v1.toFixed(4)} * ${v2.toFixed(4)}) = ${expectedEAX}`,
                        expectedEAX >= 247
                          ? `→ ${expectedEAX} ≥ 247: MOV patch hits threshold ✓`
                          : `→ ${expectedEAX} < 247: MOV patch misses — CMP=0 patch needed ✗`,
                      ];
                      setDiffResults(lines);
                      setDiffStatus('Rarity math complete');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Rarity Math</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      const bl4base = BigInt(await invoke<string>('get_module_base', { pid, moduleName: 'Borderlands4.exe' }));
                      // MULSS RIP = 0x24E179B, disp = 0x0C9D6265 → global V2 multiplier (in .rdata, needs VirtualProtect)
                      const v2Abs = `0x${(bl4base + 0x24E179Bn + 0x0C9D6265n).toString(16).toUpperCase()}`;
                      const before = await invoke<number>('read_float', { pid, address: v2Abs });
                      // 32767.0 as little-endian IEEE 754: 0x46FFFC00
                      await invoke<void>('patch_bytes', { pid, address: v2Abs, bytes: [0x00, 0xFC, 0xFF, 0x46] });
                      const after = await invoke<number>('read_float', { pid, address: v2Abs });
                      setDiffResults([
                        `V2 global @ ${v2Abs}`,
                        `Before: ${before.toFixed(6)}`,
                        `After:  ${after.toFixed(6)}`,
                        `→ kill an enemy now and check drop rarity`,
                      ]);
                      setDiffStatus('V2 forced to 32767 — test drops now');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Force V2=Max</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      const bl4base = BigInt(await invoke<string>('get_module_base', { pid, moduleName: 'Borderlands4.exe' }));
                      const v2Abs = `0x${(bl4base + 0x24E179Bn + 0x0C9D6265n).toString(16).toUpperCase()}`;
                      const before = await invoke<number>('read_float', { pid, address: v2Abs });
                      // 41.0 as little-endian IEEE 754: 0x42240000
                      await invoke<void>('patch_bytes', { pid, address: v2Abs, bytes: [0x00, 0x00, 0x24, 0x42] });
                      const after = await invoke<number>('read_float', { pid, address: v2Abs });
                      setDiffResults([
                        `V2 global @ ${v2Abs}`,
                        `Before: ${before.toFixed(6)}`,
                        `Restored: ${after.toFixed(6)}`,
                      ]);
                      setDiffStatus('V2 restored to 41');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Restore V2</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      const bl4base = BigInt(await invoke<string>('get_module_base', { pid, moduleName: 'Borderlands4.exe' }));
                      // Read 512 bytes centered on the confirmed drop-rate patch at RVA 0x6F66A1A
                      // (256 bytes before + 256 bytes after) to find adjacent rarity logic
                      const startRva = 0x6F66A1An - 0x100n;
                      const startAbs = `0x${(bl4base + startRva).toString(16).toUpperCase()}`;
                      const hex = await invoke<string>('read_raw_bytes', { pid, address: startAbs, count: 512 });
                      const bytes = hex.split(' ').map((h: string) => parseInt(h, 16));
                      const lines: string[] = [`Code dump: RVA 0x${startRva.toString(16).toUpperCase()} + 512 bytes`];
                      // Format as 16 bytes per line with RVA offset
                      for (let i = 0; i < bytes.length; i += 16) {
                        const rva = startRva + BigInt(i);
                        const marker = (rva === 0x6F66A1An) ? ' ← AND EAX,0x7FFF (drop-rate patch)' : '';
                        const row = bytes.slice(i, i + 16).map((b: number) => b.toString(16).padStart(2, '0').toUpperCase()).join(' ');
                        lines.push(`${rva.toString(16).toUpperCase().padStart(9, '0')}: ${row}${marker}`);
                      }
                      setDiffResults(lines);
                      setDiffStatus('Drop func dump complete — look for AND/CMP patterns near patch');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump Drop Func</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    // Scan only 96MB window centred on known rarity address (RVA 0x6F66A1A)
                    // Drop rate and rarity are related loot functions, likely within ±48MB of each other
                    try {
                      const baseStr = await invoke<string>('get_module_base', { pid, moduleName: 'Borderlands4.exe' });
                      const bl4base = BigInt(baseStr);
                      const rarityAbs = bl4base + 0x6F66A1An;
                      const windowSize = 96 * 1024 * 1024; // 96MB
                      const scanBase = rarityAbs - BigInt(windowSize / 2);
                      const scanBaseHex = `0x${scanBase.toString(16).toUpperCase()}`;
                      const patterns: [string, string][] = [
                        ['drop_rate (v1)', '0F 57 C0 F3 0F 2A C5 F3 0F 5E ?? ?? 0F 2E ?? 76 ?? ?? 0F 2E ?? 73'],
                        ['drop_rate (v2)', '0F 57 C0 F3 0F 2A C5 F3 0F 5E ?? ?? 0F 2E ?? 0F 86 ?? ?? ?? ?? ?? 0F 2E'],
                      ];
                      const lines: string[] = [`Scanning 96MB window around rarity (${scanBaseHex} + 96MB)...`];
                      setDiffResults(lines);
                      setDiffStatus('Scanning — this may take 30-60 seconds...');
                      for (const [label, pat] of patterns) {
                        try {
                          const found = await invoke<string>('aob_scan_range', { pid, base: scanBaseHex, size: windowSize, pattern: pat });
                          const rva = BigInt(found) - bl4base;
                          const hex = await invoke<string>('read_raw_bytes', { pid, address: found, count: 32 });
                          lines.push(`${label}: RVA 0x${rva.toString(16).toUpperCase()}  abs ${found}`);
                          lines.push(`  bytes: ${hex}`);
                        } catch {
                          lines.push(`${label}: NOT FOUND in window`);
                        }
                        setDiffResults([...lines]);
                      }
                      setDiffStatus('Targeted AoB scan complete');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Scan Drop AoB</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      const baseStr = await invoke<string>('get_module_base', { pid, moduleName: 'Borderlands4.exe' });
                      const base = BigInt(baseStr);
                      const lines: string[] = [];
                      // Drop rate
                      const drAddr = `0x${(base + 0x96CA774n).toString(16).toUpperCase()}`;
                      const drHex = await invoke<string>('read_raw_bytes', { pid, address: drAddr, count: 8 });
                      const drPatched = drHex.startsWith('0F 57 C0 90');
                      lines.push(`Drop Rate  (RVA 0x96CA774): ${drHex}  ${drPatched ? '✓ PATCH ACTIVE' : '(original)'}`);
                      // Legendary site A — WeMod's original target (MULSS [RDI] form)
                      const lgAddr = `0x${(base + 0x6F66A1An).toString(16).toUpperCase()}`;
                      const lgHex = await invoke<string>('read_raw_bytes', { pid, address: lgAddr, count: 40 });
                      const lgPatched = lgHex.startsWith('B8 FE 7F 00 00');
                      lines.push(`LegendaryA (RVA 0x6F66A1A): ${lgPatched ? '✓ PATCH ACTIVE' : '(original)'}`);
                      lines.push(`  ${lgHex}`);
                      // Legendary site B — AND EAX mask (feeds CVTSI2SS → DIVSS → MULSS → CVTTSS2SI)
                      const lg2Addr = `0x${(base + 0x24E1782n).toString(16).toUpperCase()}`;
                      const lg2Hex = await invoke<string>('read_raw_bytes', { pid, address: lg2Addr, count: 40 });
                      const lg2Patched = lg2Hex.startsWith('B8 FE 7F 00 00');
                      lines.push(`LegendaryB (RVA 0x24E1782): ${lg2Patched ? '✓ PATCH ACTIVE' : '(original)'}`);
                      lines.push(`  ${lg2Hex}`);
                      // Legendary site C — CMP EAX,247 threshold (83 F8 F7 → 83 F8 00)
                      const lg3Addr = `0x${(base + 0x24E179Fn).toString(16).toUpperCase()}`;
                      const lg3Hex = await invoke<string>('read_raw_bytes', { pid, address: lg3Addr, count: 3 });
                      const lg3Patched = lg3Hex === '83 F8 00';
                      lines.push(`LegendaryC (RVA 0x24E179F): ${lg3Hex}  ${lg3Patched ? '✓ CMP=0 ACTIVE' : '(original CMP=247)'}`);
                      setDiffResults(lines);
                      setDiffStatus('Patch verification done');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Verify Patches</button>
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
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Looking up FNames...');
                      const names = await invoke<string[]>('lookup_fnames', {
                        pid, moduleName: 'Borderlands4.exe', gnamesOffset: 0x1167FDD0,
                        indices: [12171, 1471, 78527, 108627, 20100, 1591, 135030, 144917],
                      });
                      const allIdx = [12171,1471,78527,108627,20100,1591,135030,144917];
                      setDiffResults(names.map((n, i) => `${allIdx[i]}: ${n}`));
                      setDiffStatus('FName lookup done');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Lookup FNames</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving NexusConfigStoreItemPool...');
                      setDiffResults([]);
                      const addr = await bl4Prop('NexusConfigStoreItemPool');
                      const lines = await invoke<string[]>('dump_floats_at', { pid, address: addr, count: 128 });
                      setDiffResults([`Object: ${addr}`, ...lines]);
                      setDiffStatus(`Dumped NexusConfigStoreItemPool`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump Item Pool</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving NexusConfigStoreLootConfig...');
                      setDiffResults([]);
                      const addr = await bl4Prop('NexusConfigStoreLootConfig');
                      const lines = await invoke<string[]>('dump_floats_at', { pid, address: addr, count: 256 });
                      setDiffResults([`Object: ${addr}`, ...lines]);
                      setDiffStatus(`Dumped NexusConfigStoreLootConfig`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump LootConfig</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving NexusConfigStore_OakUINameWeightedListDef...');
                      setDiffResults([]);
                      const addr = await bl4Prop('NexusConfigStore_OakUINameWeightedListDef');
                      const lines = await invoke<string[]>('dump_floats_at', { pid, address: addr, count: 256 });
                      setDiffResults([`Object: ${addr}`, ...lines]);
                      setDiffStatus(`Dumped OakUINameWeightedListDef`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump WeightedList</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving NexusConfigStoreItemPoolList...');
                      setDiffResults([]);
                      const addr = await bl4Prop('NexusConfigStoreItemPoolList');
                      const lines = await invoke<string[]>('dump_floats_at', { pid, address: addr, count: 256 });
                      setDiffResults([`Object: ${addr}`, ...lines]);
                      setDiffStatus(`Dumped NexusConfigStoreItemPoolList`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump PoolList</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving NexusConfigStoreLuckCategory...');
                      setDiffResults([]);
                      const addr = await bl4Prop('NexusConfigStoreLuckCategory');
                      const lines = await invoke<string[]>('dump_floats_at', { pid, address: addr, count: 256 });
                      setDiffResults([`Object: ${addr}`, ...lines]);
                      setDiffStatus('Dumped NexusConfigStoreLuckCategory');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump Luck</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving NexusConfigStoreLuckCategory ptrs...');
                      setDiffResults([]);
                      const addr = await bl4Prop('NexusConfigStoreLuckCategory');
                      const objBase = BigInt(addr);
                      const allLines: string[] = [`Object: ${addr}`];
                      // TArray slots identified from flat dump: +0x040 (Num=1), +0x380 (Num=1)
                      const slots: [bigint, string][] = [
                        [0x040n, '+0x040 TArray[1]'],
                        [0x380n, '+0x380 TArray[1]'],
                      ];
                      for (const [off, label] of slots) {
                        const ptrAddr = `0x${(objBase + off).toString(16).toUpperCase()}`;
                        const rawHex = await invoke<string>('read_raw_bytes', { pid, address: ptrAddr, count: 8 });
                        const dataPtr = parseLePtr64(rawHex.split(' '));
                        const dataPtrStr = `0x${dataPtr.toString(16).toUpperCase()}`;
                        allLines.push(`--- ${label} → ${dataPtrStr} ---`);
                        try {
                          const lines = await invoke<string[]>('dump_floats_at', { pid, address: dataPtrStr, count: 128 });
                          allLines.push(...lines);
                        } catch (e2) { allLines.push(`[read error: ${e2}]`); }
                      }
                      setDiffResults(allLines);
                      setDiffStatus('Luck TArray ptr dump complete');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump Luck Ptrs</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving LuckCategory...');
                      const addr = await bl4Prop('NexusConfigStoreLuckCategory');
                      const objBase = BigInt(addr);
                      // Read TArray Data ptr at +0x040 (8 bytes little-endian)
                      const ptrRaw = await invoke<string>('read_raw_bytes', { pid, address: `0x${(objBase + 0x040n).toString(16).toUpperCase()}`, count: 8 });
                      const dataPtr = parseLePtr64(ptrRaw.split(' '));
                      const lines: string[] = [`Object: ${addr}`, `TArray data → 0x${dataPtr.toString(16).toUpperCase()}`];
                      // 100.0 as LE IEEE 754 = [0x00, 0x00, 0xC8, 0x42]
                      const val100 = [0x00, 0x00, 0xC8, 0x42];
                      // Candidate 1.0 multiplier offsets identified from luck ptr dump
                      const offsets = [0x190n, 0x194n, 0x198n, 0x19Cn,  // 4×1.0 block A
                                       0x1CCn, 0x1D0n, 0x1D4n, 0x1D8n,  // 4×1.0 block B
                                       0x1E0n, 0x1E8n, 0x1F0n, 0x1F8n]; // alternating mult slots
                      for (const off of offsets) {
                        const target = `0x${(dataPtr + off).toString(16).toUpperCase()}`;
                        const before = await invoke<number>('read_float', { pid, address: target });
                        await invoke<void>('patch_bytes', { pid, address: target, bytes: val100 });
                        const after = await invoke<number>('read_float', { pid, address: target });
                        lines.push(`+0x${off.toString(16).toUpperCase()}  ${target}  ${before.toFixed(4)} → ${after.toFixed(4)}`);
                      }
                      setDiffResults(lines);
                      setDiffStatus('Luck multipliers boosted to 100 — test drops now');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Boost Luck</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      const addr = await bl4Prop('NexusConfigStoreLuckCategory');
                      const objBase = BigInt(addr);
                      const ptrRaw = await invoke<string>('read_raw_bytes', { pid, address: `0x${(objBase + 0x040n).toString(16).toUpperCase()}`, count: 8 });
                      const dataPtr = parseLePtr64(ptrRaw.split(' '));
                      // 1.0 as LE IEEE 754 = [0x00, 0x00, 0x80, 0x3F]
                      const val1 = [0x00, 0x00, 0x80, 0x3F];
                      const offsets = [0x190n, 0x194n, 0x198n, 0x19Cn,
                                       0x1CCn, 0x1D0n, 0x1D4n, 0x1D8n,
                                       0x1E0n, 0x1E8n, 0x1F0n, 0x1F8n];
                      for (const off of offsets) {
                        const target = `0x${(dataPtr + off).toString(16).toUpperCase()}`;
                        await invoke<void>('patch_bytes', { pid, address: target, bytes: val1 });
                      }
                      setDiffResults([`Restored all luck multipliers to 1.0`]);
                      setDiffStatus('Luck restored');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Restore Luck</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      const bl4base = BigInt(await invoke<string>('get_module_base', { pid, moduleName: 'Borderlands4.exe' }));
                      // Dump 512 bytes centered on the confirmed drop-rate patch at RVA 0x96CA774
                      // (256 before, 256 after) to find adjacent rarity comparison code
                      const startRva = 0x96CA774n - 0x100n;
                      const startAbs = `0x${(bl4base + startRva).toString(16).toUpperCase()}`;
                      const hex = await invoke<string>('read_raw_bytes', { pid, address: startAbs, count: 512 });
                      const bytes = hex.split(' ').map((h: string) => parseInt(h, 16));
                      const lines: string[] = [`Code dump: RVA 0x${startRva.toString(16).toUpperCase()} + 512 bytes`];
                      for (let i = 0; i < bytes.length; i += 16) {
                        const rva = startRva + BigInt(i);
                        const marker = (rva === 0x96CA774n) ? ' ← drop-rate patch (XORPS+CVTSI2SS)' : '';
                        const row = bytes.slice(i, i + 16).map((b: number) => b.toString(16).padStart(2, '0').toUpperCase()).join(' ');
                        lines.push(`${rva.toString(16).toUpperCase().padStart(9, '0')}: ${row}${marker}`);
                      }
                      setDiffResults(lines);
                      setDiffStatus('Drop-rate func dump complete');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump DR Func</button>
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
                <div style={{ display: 'flex', gap: '0.5rem' }}>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving NexusConfigStoreGbxUEDataTableDefs...');
                      setDiffResults([]);
                      const addr = await bl4Prop('NexusConfigStoreGbxUEDataTableDefs');
                      const objBase = BigInt(addr);
                      const lines: string[] = [`Object: ${addr}`];
                      const hexStr = await invoke<string>('read_raw_bytes', { pid, address: addr, count: 2048 });
                      const bytes = hexStr.split(' ').map((h: string) => parseInt(h, 16));
                      // Scan flat for 0.015f = [8F, C2, 75, 3C]
                      for (let i = 0; i <= bytes.length - 4; i++) {
                        if (bytes[i] === 0x8F && bytes[i+1] === 0xC2 && bytes[i+2] === 0x75 && bytes[i+3] === 0x3C) {
                          lines.push(`*** 0.015f at +0x${i.toString(16).toUpperCase()}  addr=0x${(objBase + BigInt(i)).toString(16).toUpperCase()} ***`);
                        }
                      }
                      // Follow 8-byte aligned pointers in first 512 bytes, scan 8KB each
                      for (let off = 0x30; off < 512; off += 8) {
                        if (off + 8 > bytes.length) break;
                        const ptr = parseLePtr64(bytes.slice(off, off + 8));
                        if (ptr >= 0x10000n && ptr < 0x7FFFFFFFFFFFFn) {
                          const ptrStr = `0x${ptr.toString(16).toUpperCase()}`;
                          try {
                            const ph = await invoke<string>('read_raw_bytes', { pid, address: ptrStr, count: 8192 });
                            const pb = ph.split(' ').map((h: string) => parseInt(h, 16));
                            for (let i = 0; i <= pb.length - 4; i++) {
                              if (pb[i] === 0x8F && pb[i+1] === 0xC2 && pb[i+2] === 0x75 && pb[i+3] === 0x3C) {
                                lines.push(`*** 0.015f via [+0x${off.toString(16)}]→${ptrStr}+0x${i.toString(16).toUpperCase()}  addr=0x${(ptr + BigInt(i)).toString(16).toUpperCase()} ***`);
                              }
                            }
                          } catch { /* unreadable */ }
                        }
                      }
                      const floatLines = await invoke<string[]>('dump_floats_at', { pid, address: addr, count: 128 });
                      lines.push('--- flat dump (128 floats) ---', ...floatLines);
                      setDiffResults(lines);
                      setDiffStatus(`Dumped GbxDataTable — look for *** 0.015 markers`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump GbxDataTable</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving GbxMirrorDataTable...');
                      setDiffResults([]);
                      const addr = await bl4Prop('GbxMirrorDataTable');
                      const objBase = BigInt(addr);
                      const lines: string[] = [`Object: ${addr}`];
                      const hexStr = await invoke<string>('read_raw_bytes', { pid, address: addr, count: 1024 });
                      const bytes = hexStr.split(' ').map((h: string) => parseInt(h, 16));
                      for (let i = 0; i <= bytes.length - 4; i++) {
                        if (bytes[i] === 0x8F && bytes[i+1] === 0xC2 && bytes[i+2] === 0x75 && bytes[i+3] === 0x3C) {
                          lines.push(`*** 0.015f at +0x${i.toString(16).toUpperCase()}  addr=0x${(objBase + BigInt(i)).toString(16).toUpperCase()} ***`);
                        }
                      }
                      // Follow 8-byte aligned pointers in first 512 bytes, scan 8KB of each
                      for (let off = 0x30; off < 512; off += 8) {
                        if (off + 8 > bytes.length) break;
                        const ptr = parseLePtr64(bytes.slice(off, off + 8));
                        if (ptr >= 0x10000n && ptr < 0x7FFFFFFFFFFFFn) {
                          const ptrStr = `0x${ptr.toString(16).toUpperCase()}`;
                          try {
                            const ph = await invoke<string>('read_raw_bytes', { pid, address: ptrStr, count: 8192 });
                            const pb = ph.split(' ').map((h: string) => parseInt(h, 16));
                            for (let i = 0; i <= pb.length - 4; i++) {
                              if (pb[i] === 0x8F && pb[i+1] === 0xC2 && pb[i+2] === 0x75 && pb[i+3] === 0x3C) {
                                lines.push(`*** 0.015f via [+0x${off.toString(16)}]→${ptrStr}+0x${i.toString(16).toUpperCase()}  addr=0x${(ptr + BigInt(i)).toString(16).toUpperCase()} ***`);
                              }
                            }
                          } catch { /* unreadable */ }
                        }
                      }
                      const floatLines = await invoke<string[]>('dump_floats_at', { pid, address: addr, count: 128 });
                      lines.push('--- flat dump (128 floats) ---', ...floatLines);
                      setDiffResults(lines);
                      setDiffStatus(`Dumped GbxMirrorDataTable — look for *** 0.015 markers`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Dump MirrorTable</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Scanning all memory for 0.015f — may take 30–90s...');
                      setDiffResults(['Scanning...']);
                      const addresses = await invoke<string[]>('scan_value', { pid, valueType: 'float', value: 0.015 });
                      setDiffResults(addresses.length > 0
                        ? [`Found ${addresses.length} hit(s) — paste one into "Dump at Addr" to inspect context:`, ...addresses.slice(0, 100)]
                        : ['No 0.015f found in process memory']);
                      setDiffStatus(`0.015f scan: ${addresses.length} hit(s)`);
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Scan 0.015f</button>
                  <button className="fire-button" disabled={!pid} onClick={async () => {
                    if (!pid) return;
                    try {
                      setDiffStatus('Resolving GbxMirrorDataTable pairs...');
                      setDiffResults([]);
                      const addr = await bl4Prop('GbxMirrorDataTable');
                      const objBase = BigInt(addr);
                      // TArray at +0x030: pairs ptr (8B), Num (4B at +0x038)
                      const ptrRaw = await invoke<string>('read_raw_bytes', { pid, address: `0x${(objBase + 0x30n).toString(16).toUpperCase()}`, count: 12 });
                      const pb = ptrRaw.split(' ').map((h: string) => parseInt(h, 16));
                      const pairsPtr = parseLePtr64(pb.slice(0, 8));
                      const pairsNum = pb[8] | (pb[9] << 8) | (pb[10] << 16) | (pb[11] << 24);
                      const lines: string[] = [`Pairs @ 0x${pairsPtr.toString(16).toUpperCase()}, Num=${pairsNum}`];
                      setDiffResults([...lines]);
                      // Each entry is 24 bytes: FName(8B) + row_ptr(8B) + hash_next(4B) + extra(4B)
                      const STRIDE = 24;
                      const TARGET = [0x8F, 0xC2, 0x75, 0x3C]; // 0.015f LE bytes
                      for (let i = 0; i < Math.min(pairsNum, 512); i++) {
                        const entryAddr = pairsPtr + BigInt(i * STRIDE);
                        const entryRaw = await invoke<string>('read_raw_bytes', { pid, address: `0x${entryAddr.toString(16).toUpperCase()}`, count: 24 });
                        const eb = entryRaw.split(' ').map((h: string) => parseInt(h, 16));
                        const fnameIdx = eb[0]|(eb[1]<<8)|(eb[2]<<16)|(eb[3]<<24);
                        const rowPtr = parseLePtr64(eb.slice(8, 16));
                        if (rowPtr < 0x10000n) continue;
                        try {
                          const rowRaw = await invoke<string>('read_raw_bytes', { pid, address: `0x${rowPtr.toString(16).toUpperCase()}`, count: 64 });
                          const rb = rowRaw.split(' ').map((h: string) => parseInt(h, 16));
                          for (let j = 0; j <= rb.length - 4; j++) {
                            if (rb[j] === TARGET[0] && rb[j+1] === TARGET[1] && rb[j+2] === TARGET[2] && rb[j+3] === TARGET[3]) {
                              const hitAddr = `0x${(rowPtr + BigInt(j)).toString(16).toUpperCase()}`;
                              lines.push(`*** 0.015f: entry[${i}] FName=${fnameIdx} row=0x${rowPtr.toString(16).toUpperCase()} +0x${j.toString(16)} → ${hitAddr} ***`);
                            }
                          }
                        } catch { /* skip unreadable */ }
                        if (i % 20 === 0) { setDiffResults([...lines]); setDiffStatus(`Scanning row ${i}/${pairsNum}...`); }
                      }
                      setDiffResults([...lines]);
                      setDiffStatus(lines.length > 1 ? `Done — ${lines.filter(l => l.startsWith('***')).length} hit(s)` : 'No 0.015f found in any row struct');
                    } catch (e) { setDiffStatus(String(e)); }
                  }}>Follow Row Ptrs</button>
                </div>
                {diffStatus && <span style={{ fontSize: '0.7rem', color: '#aaa' }}>{diffStatus}</span>}
                {diffResults.length > 0 && (
                  <>
                    <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center', width: '100%' }}>
                      <input
                        className="value-input"
                        type="text"
                        placeholder="filter results..."
                        value={resultsFilter}
                        onChange={e => setResultsFilter(e.target.value)}
                        style={{ fontFamily: 'monospace', fontSize: '0.75rem', flex: 1 }}
                      />
                      <span style={{ fontSize: '0.65rem', color: '#666', whiteSpace: 'nowrap' }}>
                        {resultsFilter
                          ? `${diffResults.filter(l => l.toLowerCase().includes(resultsFilter.toLowerCase())).length} / ${diffResults.length}`
                          : `${diffResults.length} lines`}
                      </span>
                    </div>
                    <pre style={{ fontSize: '0.65rem', color: '#0f0', background: '#111', padding: '0.5rem', borderRadius: '4px', width: '100%', overflowX: 'auto', margin: 0, maxHeight: '400px', overflowY: 'auto' }}>
                      {(resultsFilter
                        ? diffResults.filter(l => l.toLowerCase().includes(resultsFilter.toLowerCase()))
                        : diffResults
                      ).join('\n')}
                    </pre>
                  </>
                )}
              </div>
            </div>
  );
}
