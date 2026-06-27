# BL4 Drop Rate Cheats — Implementation Brief

## Goal
Add two working drop rate cheats to `public/trainers/borderlands-4.json` matching WeMod's:
1. **100% Drop Rate** — every enemy drops loot
2. **Max Legendary Drop Rate** — maximizes legendary item drop chance

Both cheats already exist as `scan`-type skeletons in the JSON but have no addresses.
They need to become AOB-based (`signature` field) or `ue5_prop`-based once the addresses are pinned.

---

## What We Know From the SDK (Dumper-7 CppSDK)

Relevant structs in `CppSDK/SDK/OakGame_structs.hpp`:

```cpp
// ScriptStruct OakGame.RarityWeightData  (Data Table row)
struct FRarityWeightData : public FTableRowBase {
    float BaseWeight;      // 0x0008
    float GrowthExponent;  // 0x000C
};

// ScriptStruct OakGame.LocalRarityModiferData  (Data Table row)
struct FLocalRarityModiferData : public FTableRowBase {
    float CommonModifier;    // 0x0008
    float UncommonModifier;  // 0x000C
    float RareModifier;      // 0x0010
    float VeryRareModifier;  // 0x0014
    float LegendaryModifier; // 0x0018
    float PearlModifier;     // 0x001C
};
```

Live runtime object of interest:
- `NexusConfigStoreLootConfig` — one live instance seen in the GObjects dump
- Class path: `GbxGame.NexusConfigStoreLootConfig`
- Runtime address during Dumper-7 session: `{0x5b5b5890}` (changes each run, use class lookup)

GObjects and GNames offsets (BL4 5.5.4, confirmed by Dumper-7):
- `GObjects  = module_base + 0x11765A30`
- `GNames    = module_base + 0x1167FDC0`  (FNamePool struct)
- `FNamePool.Blocks = GNames + 0x10`  → use `0x1167FDD0` as `ue5GNamesOffset`

---

## What's Needed: CE Scanning Session

The drop rate floats live inside Data Table asset objects loaded from Pak.
There is no known static offset — they must be found by live scanning.

### Step 1 — Scan for world drop rate

Target: the float controlling whether a killed enemy drops *any* loot.
In BL3 this was ~`0.09` (9%) for regular enemies; expect similar in BL4.

CE procedure:
1. Attach CE to `Borderlands4.exe`, select a `float` scan type
2. Kill an enemy, note whether it dropped anything
3. Scan for a small float around `0.05`–`0.15`
4. Kill more enemies, refine by "unchanged value" or by changing areas
5. Once candidates are narrow: set one to `1.0` and kill something — confirm all enemies drop

### Step 2 — Scan for legendary weight

Target: the float controlling legendary rarity probability.
It should be a very small number (`0.001`–`0.05`) relative to common.

CE procedure:
1. After finding drop rate (Step 1), look at neighboring floats
2. `FLocalRarityModiferData.LegendaryModifier` is at `+0x18` from the struct base
3. Scan for small positive floats that increase legendary frequency when raised
4. Set to `100.0` or `1.0` and verify orange drops become common

### Step 3 — Build AOB

Once both addresses are confirmed:
1. In CE: right-click address → "Find out what accesses this address"
2. Trigger a drop, note which instruction reads the float
3. Copy the surrounding bytes (12–20 bytes) → build `? ?` wildcard pattern
4. Test the pattern with CE's AOB scanner to verify single unique hit

---

## Cheat JSON Template (fill in after scanning)

```json
{
  "id": "bl4-drop-rate",
  "name": "100% Drop Rate",
  "type": "toggle",
  "module": "Borderlands4.exe",
  "valueType": "float",
  "signature": "?? ?? ?? ?? ?? ?? ?? ??",   // AOB from Step 3
  "base": "0x0",                              // byte offset from AOB hit to float
  "offsets": [],
  "onValue": 1.0,
  "offsets": []
},
{
  "id": "bl4-legendary-drop-rate",
  "name": "Max Legendary Drop Rate",
  "type": "toggle",
  "module": "Borderlands4.exe",
  "valueType": "float",
  "signature": "?? ?? ?? ?? ?? ?? ?? ??",   // AOB from Step 3 (neighboring float)
  "base": "0x0",
  "offsets": [],
  "onValue": 100.0,
  "offsets": []
}
```

---

## Alternative: ue5_prop Approach

If `NexusConfigStoreLootConfig` holds the relevant floats, we can skip AOB and use:

```json
{
  "type": "ue5_prop",
  "ue5GObjectsOffset": "0x11765A30",
  "ue5GNamesOffset":   "0x1167FDD0",
  "ue5ClassName": "NexusConfigStoreLootConfig",
  "ue5PropertyOffset": ???    // need to grep SDK for the relevant property
}
```

Check `GbxGame_classes.hpp` for `UNexusConfigStoreLootConfig` property list.
The class was nearly empty in the dump — it may inherit its config from a nested struct.

---

## Files to Edit When Ready

| File | Change |
|------|--------|
| `public/trainers/borderlands-4.json` | Replace scan-type drop rate entries with AOB or ue5_prop |
| (no Rust/TS changes needed — infrastructure already supports both types) |

---

## Current State of Other BL4 Cheats

- **Invincibility** (`bl4-godmode`): implemented via `ue5_prop`, NOT yet tested
  - GObjects: `0x11765A30`, GNames.Blocks: `0x1167FDD0`
  - Class: `OakCharacter`, prop offset: `0x62` (byte), ON=`0x01`, OFF=`0x05`
  - First item needed: live test to confirm toggle works and doesn't error
- **All scan cheats** (health, shield, ammo, cash, etc.): working skeletons, need manual scan
