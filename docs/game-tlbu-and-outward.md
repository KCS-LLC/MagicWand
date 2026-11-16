# The Land Beneath Us (Unity Mono) and Outward (Unity IL2CPP)

Reference for the two trainers added 2026-10-12/13, the engine pieces they introduced, and how
to re-derive everything after a game update. Trainer data: `public/trainers/the-land-beneath-us.json`
and `public/trainers/outward.json` (both registered in `index.json`).

See also `docs/game-rogue-lords.md` (Unity 2019.4 Mono: `mono_method_ret`, `mono_call_nop`, `dictKey` hops, runtime offset detection).

Conventions these trainers follow (see also the project memory notes):

- **No hotkeys**, ever (CLAUDE.md hard rule).
- **Invincibility patches the damage action** — it never poll-writes the current HP.
- **Locks (food/stamina/...) set the stat, then NOP the code that decreases it.**
- Verify read-only first (the diagnostic bins below), then wire into the JSON and let the user
  toggle it from the UI. Never write to a live game from standalone tooling.

---

## Status (as of 2026-10-13)

### The Land Beneath Us — Mono (`mono-2.0-bdwgc.dll`), exe `The Land Beneath Us.exe`

| Cheat | Type | Status |
|---|---|---|
| Set Gold | `action`, `monoPath` | **Confirmed live** |
| Set Souls (bank) | `action`, `monoPath` | **Confirmed live** (outside runs only; per-run souls were removed) |
| Invincibility | `mono_store_nop` (+ IL pre-patch) | **Confirmed live**, including before the first hit |

### Outward — IL2CPP (`GameAssembly.dll`), process/exe `Outward Definitive Edition.exe`

| Cheat | Type | Status |
|---|---|---|
| Set Silver | `action`, `il2cppPath` | **Confirmed live** |
| Set Food / Drink / Sleep | `action`, `il2cppPath` | **Confirmed live** |
| Invincibility | `il2cpp_method_ret` | Written, **not yet tested** |
| Lock Food / Drink / Sleep at 100% | `il2cpp_decay_nop` | Written, not yet tested |
| Unlimited Stamina | `il2cpp_decay_nop` | Written, not yet tested |
| Unlimited Mana | `il2cpp_decay_nop` | Written, not yet tested — player max mana is 0 until the in-game Mana unlock |
| No Burnt Health/Stamina/Mana | `il2cpp_method_ret` | Written, not yet tested |
| No Skill Cooldowns | `il2cpp_method_ret` | Written, not yet tested |
| Clear Burnt Health/Stamina/Mana | `action` ×3 | Written, not yet tested |
| Set Breakthrough Points Used | `action` | Written, not yet tested |
| Indestructible Equipment, No Weight Penalty | `toggle` (+`offValue`) | Written, not yet tested |

Open ideas not built: mana points (`m_manaPoint`, permanent), corruption/temperature,
ammo/consumable stacks, a "player 2" variant (the trainer always picks the first non-AI Character).

---

## New cheat fields / types

All defined in `src/hooks/useTrainer.ts` (`Cheat` interface); backends in `src-tauri/src/lib.rs`.

### `monoPath` (Mono games) — `resolve_mono_path`
A static singleton plus a chain of reference fields; the last hop's field address is the value.
Works for any cheat type (`action`, `toggle`, `mono_chain`). Never cached (instances move).
```json
"monoClass": "PlayerController", "monoStaticField": "Instance",
"monoPath": [{ "className": "PlayerController", "field": "gold" }]
```
Hops can name a `namespace`. Classes are looked up through the same heuristic Mono struct walker as
`mono_chain` (`mono.rs`).

### `mono_store_nop` — `set_mono_store_nop`
Finds the JIT-compiled body of `monoMethod`, locates every inlined `movss [reg+field], xmm` store to
`monoStoreField` and replaces it with NOPs; the method otherwise keeps running (on-hit triggers,
popups). Original bytes are saved per site and restored on toggle off.

**IL pre-patch** (optional `ilSignature` + `ilPatches`): Mono has no native code for a method until its
first call, so right after a game launch there is nothing to NOP. If the method isn't compiled yet,
the command instead finds the method's IL in process memory by its leading bytes and edits it
(`call set_Hp` -> `pop pop nop nop nop`, 5 bytes, stack-neutral) *before* the game compiles it.
`expect` bytes are verified at every site; if the signature doesn't verify nothing is written.
Catch: a method compiled from edited IL stays edited, so toggle-off needs a game restart. The command
returns `"native" | "il" | "restored" | "restored-baked"` and the UI shows a Note for the last two.

To (re)generate the signature after a game update: read the method's IL from
`<Game>_Data/Managed/Assembly-CSharp.dll` (a .NET metadata reader works; `dnfile` for Python was used),
take its first 64 bytes as `ilSignature`, and list every `call <setter token>` (`28 xx xx xx 06`) offset.

### `il2cppPath` (IL2CPP games) — `resolve_il2cpp_path`
`il2cppClass.il2cppStaticField` is a singleton (static data = `Il2CppClass+0xB8`); `il2cppPath` hops:
`field` (class + field name), `offset` (raw bytes), `listItem` (index into a `List<T>`),
`listFindByte` (first list element whose byte field equals a value — used to pick the first non-AI
`Character`). Module is `GameAssembly.dll`. Not cached at the address level.

### `il2cpp_method_ret` — `set_il2cpp_method_ret`
Writes `C3` (`ret`) at byte 0 of every overload of each method in `il2cppMethods`. Safe only for
**void** methods (nothing has executed yet at byte 0). Saved bytes are keyed `(pid, cheat id)`.
If a byte-0 `C3` is already present the command refuses (stale patch from an earlier session —
restart the game).

### `il2cpp_decay_nop` — `set_il2cpp_decay_nop`
Lock-a-stat toggle. On enable: write the stat (`onValue`, or the value read from `il2cppValuePath`,
e.g. a Stat object's max), then NOP the **first store to `il2cppDecay.field` that follows a `subss`**
in each overload of each `il2cppMethods` entry. Eating/resting restores use other stores and keep
working. Refuses to patch if no decrement store is found (game updated).

### Toggles with `offValue` on `il2cppPath`
The generic `toggle` handler now also writes `offValue` when switching off a cheat that has an
`il2cppPath` (used for the game's own `Character.Cheats` bool flags).

---

## The Land Beneath Us — verified facts

- Mono Unity, `Assembly-CSharp.dll` has ~4,000 types. Field/method names in the lists below are real.
- **Live gold** is `PlayerController.Instance.gold` (int). `PlayerSessionData.CurrentGold` is only a
  save snapshot (`SessionDataManager.ExportPlayerSessionData` writes it, `PlayerController.ImportSessionData`
  reads it on load) — writing it does nothing visible. Run souls: `PlayerController.soul` (not exposed).
- **Banked souls** (outside runs): `PlayerStatusController._instance.<CurrentSoul>k__BackingField` (int);
  Heroic Souls and Soul Shards sit beside it.
- **HP**: `PlayerController.hp` / `maxHp` (floats, offsets 0x14C / 0x150). `set_Hp` is inlined by the JIT.
  `Hurt(float, <enum>, <class>)` (void) contains six `set_Hp` calls: two subtractions, three clamps to 0,
  and the Backup Plan revive (`hp = maxHp`). `GameConfig.EnableInvincibleMode` exists but its branch
  skips the on-hit item triggers, which is why it is not used.
- An early `ret` at `Hurt`'s entry was tried and rejected (it also blocks on-hit abilities).

## Outward — verified facts

- Build: IL2CPP, Unity 2020.3, metadata v27, 13,910 type definitions, ~94k methods. No anti-cheat
  modules seen. The Steam `default-mono` beta would also work with the Mono engine but was not needed.
- Struct layouts were decoded from the game's own `il2cpp_*` exports (e.g. `il2cpp_class_get_name` is
  `mov rax,[rcx+0x10]`): Il2CppClass name `+0x10`, namespace `+0x18`, parent `+0x58`, fields `+0x80`
  (count u16 `+0x120`), methods `+0x98` (count u16 `+0x11C`), static data `+0xB8`; FieldInfo stride
  `0x20`, name `+0`, offset i32 `+0x18`; MethodInfo native pointer `+0`, name `+0x10`.
- The class table global (`s_TypeInfoDefinitionTable`) is decoded at runtime from `il2cpp_image_get_class`
  (`lea rbx,[rdi*8]; mov rax,[rip+disp32]`); RVA `0x285A788` is a hardcoded fallback and was verified equal.
- Player: `CharacterManager.Instance` -> `m_characters` (a `DictionaryExt`; its `List` of values is at
  `+0x18`, keys at `+0x10`) -> first `Character` with `m_isAI == 0`. `m_playerCharacters` maps UID strings
  to strings and is not useful.
- Character: `m_inventory +0x1F8`, `m_characterStats +0x528`, `m_isAI +0x14C`, inline `Cheats` struct at
  `+0x30` (`Invincible 0x30`, `IndestructibleEquipment 0x31`, `NeedsEnabled 0x32`,
  `NotAffectedByWeightPenalties 0x33`).
- Silver: `Character.m_inventory -> CharacterInventory.m_inventoryPouch (+0x58) -> ItemContainer.m_silverCount (+0x480)`.
- CharacterStats: `m_health 0x6C`, `m_mana 0xC0`, `m_stamina 0xF0`, burnt health/mana/stamina `0x5C/0xA8/0xE4`,
  `m_maxStamina 0xC8` / `m_maxManaStat 0x70` (Stat objects, value at `+0x14`).
  PlayerCharacterStats: `m_food 0x2E0`, `m_drink 0x308`, `m_sleep 0x330` (max 1000; UI shows %),
  `m_usedBreakthroughCount 0x420`.
- **Damage path**: `CharacterStats.ReceiveDamage(float)` does `m_health -= dmg` and burnt-health accrual;
  callers `Character.ReceiveDamage`, `Character.SendDamage`, `Skill.ConsumeResources` (health-cost skills
  become free while Invincibility is on). `CharacterStats.AffectHealth` is the heal/effect path and is the
  only place `Character.get_Invincible` is tested.
- **Needs decay**: in `PlayerCharacterStats.UpdateNeeds` (RVA `0x1B682F0` in the verified build) — decay stores at
  `+0x34A` (food), `+0x3D9` (drink), `+0x473` (sleep); later stores at `+0x4A7..0x557` are rest/eating restores.
- **Stamina**: `CharacterStats.UseStamina` (three overloads; the float overload does the subtract at `+0x4F`) and the
  sprint drain/regen merged into one store in `UpdateVitalStats` (`+0x63B`).
- **Mana**: `CharacterStats.UseMana` (only caller `Skill.ConsumeResources`); player max mana reads 0 until the Mana unlock.
- Cooldowns: `Skill.StartCooldown` (void) / `Skill.ResetCoolDown`; burnt stats: `CharacterStats.IncreaseBurnt*` (void).

---

## Pitfalls hit while building this

- Reading a generic `Dictionary`/`DictionaryExt` class's fields shows offset 0 — use the known layout instead.
- `DictionaryExt` keys/values list order differs between fields; verify with the element's class name.
- Mono `Hurt` wasn't compiled right after launch -> "hasn't been compiled yet": that is why the IL pre-patch exists.
- A sync Tauri command runs on the UI thread — whole-process scans (`resolve_mono_method`, store NOPs) are
  `async` + `spawn_blocking`; the first scan version loaded all 2 GB of memory and took 54 s (now streamed, ~5 s).
- IL2CPP class/field lookups re-scanned 14k classes per call (0.7 s each) until they were cached per pid
  (`il2cpp.rs` `CLASS_CACHE`/`FIELD_CACHE`/`TABLE_CACHE`).
- Disassembler helper bug: .NET method bodies have **tiny** (1-byte) and **fat** (12-byte) headers; handle both.
- Running under `tauri dev` reloads Magic Wand on every Rust change but leaves patches in the game and drops the
  saved original bytes — restart the game (save first) before re-testing patch cheats.

## Diagnostic tools (read-only) — `src-tauri/src/bin/`

- `find_mono_method_jit "<process>" <Class> <Method> [ns] [field]` — locate a Mono method's native body, dump
  it to `%TEMP%`, list store sites to a field; `ILSIG="<hex>"` dry-runs the IL pre-patch finder.
- `read_mono_path "<process>" <Class> <staticField> <Class.field>...` — resolve a `monoPath` and print the value.
- `read_il2cpp_player` — Outward: resolve the player's silver/health/needs/stamina/mana/burnt/flags through the Rust walker.

## Re-finding things after a game update

1. **Mono**: class/field names survive updates; re-run `find_mono_method_jit` for the method and field offset. If the
   IL signature stops verifying, regenerate `ilSignature`/`ilPatches` from the new `Assembly-CSharp.dll`.
2. **IL2CPP**: nothing in `outward.json` is an RVA — classes, fields and methods are resolved by name at runtime. If a
   cheat reports "No decrement store found", the compiler layout changed: re-disassemble the method (capstone over the
   live bytes, see `UpdateNeeds`/`UseMana` above) and adjust the rule.
3. Metadata names are greppable offline: `Outward_Defed/.../il2cpp_data/Metadata/global-metadata.dat` (identifier strings).
