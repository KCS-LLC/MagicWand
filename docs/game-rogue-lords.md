# Rogue Lords (Unity 2019.4 Mono)

Trainer: `public/trainers/rogue-lords.json` (registered in `index.json`). Process `Rogue Lords.exe`, Steam app 1069690.
Game code is `MainAssembly.dll` (namespace `DarkTale`); shared types (`Value.IntValue`) are in `ToolsAssembly.dll`.
Engine: `mono-2.0-bdwgc.dll` — see `docs/game-tlbu-and-outward.md` for the Mono cheat types.

## Status

| Cheat | Type | Status |
|---|---|---|
| Set Souls | `action`, `monoPath` | **Confirmed live** |
| Set (Max) Diabolic Essence | `action`, `monoPath` + `dictKey`, `writeBias` -50 | **Confirmed live** |
| No Essence Loss | `mono_method_ret` (`DevilStatCollection.ApplyDamage`) | **Confirmed live** |
| No Essence Loss (Devil Mode / Story Challenges) | `mono_method_ret` (`DevilStatCollection.ApplyDevilModeHealthMod`, void, IL `02`→`2a`) | **Confirmed live** 2026-11-03; story-challenge essence costs bypass `ApplyDamage` and go through the Devil Mode transaction panel → `Devil.ApplyDevilModeHealthMod` |
| No Soul Loss | `mono_method_ret` (`Devil.RemoveSouls`) | **Confirmed live**; forge/merchant call `IntValue.Substract` directly and are not specifically covered |
| Unlimited Action Points | `mono_call_nop` | **Confirmed live** |

## Engine changes this game needed (Unity 2019.4 Mono differs from TLBU's build)

- `MonoDomain::domain_assemblies` is at `+0xC8` (not `+0xA0`): `MonoOffsets::detect` finds it by looking for `mscorlib`.
- `image->class_cache` offset is found by scanning the image for a hash-table-shaped region.
- Static-data lookup (`get_static_field_address`) falls back to searching class/vtable offsets and validates against a real object.
- Path hops can find classes in other assemblies; hops can take `dictKey` (Dictionary with 4-byte key / reference value; entry stride 0x18).
- Nested classes have an **empty namespace** in Mono (`<ValidateAction>d__20`).
- New cheat types: `mono_method_ret` (native `xor eax,eax; ret`, or IL edit if not compiled), `mono_call_nop` (NOP the `call r11` of the one `mov r11,<callee>; call r11` inside a compiled caller). `writeBias` is added to every written value.

## Verified facts

- Souls: `DifficultyManager.Instance._devil -> Devil._devilSouls (IntValue) -> _value` (`IntValue` object `+0x54`).
- Essence: `Devil._stats` (DevilStatCollection) `-> _stats` (Dictionary<EDevilStat, Stat>) `-> [0=CurrentHealth, 1=MaxHealth]`.
  A `Stat` stores `_baseValue +0x28`, `_permanentModifierSum +0x2C`; displayed value = base + modifier (base 50 on the tested run).
  Damage lowers `_permanentModifierSum` (50 + -45 = 5). The write path targets the modifier, hence `writeBias: -50`.
- Action points: `PlayerTurn.<ValidateAction>d__20.MoveNext` calls `_actionPointsRemaining.Substract(skill.ActionPointsCost)`.
  The current-AP `IntValue` is a shared asset with no static root (referenced by PlayerTurn, PlayerSelectSkillState, EndPlayerTurn,
  APCristalManager, ...), so "Set AP" isn't possible via `monoPath`; unlimited AP uses the call NOP instead.
- `IntValue.Substract` has four callers: `Devil.RemoveSouls`, `MerchForgeSlot.BuyForge`, `MerchController.<AfterRewardRoutine>`,
  `PlayerTurn.<ValidateAction>` — do not stub the method itself (it would also free skills and purchases).
- Disciple health lives in `CombatStatCollection` (`DepletePhysicalHealth` / `DepleteSpiritualHealth`) — no cheat yet.

## Re-finding after a game update

- Names survive updates; list types/fields with `dnfile` over `MainAssembly.dll` (`pip install dnfile`).
- `ilSignature` values are the first bytes of each method's IL (tiny/fat header aside); regenerate if `ilPatches` stop verifying.
- Diagnostics (read-only, `src-tauri/src/bin/`): `list_mono_assemblies`, `read_mono_path` (`ASM`/`NS` env, `field@key` for dict hops),
  `find_mono_method_jit` (`ASM` env), `find_mono_stat` (find objects of a class holding a value), `find_refs_to`.
