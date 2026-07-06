# Dragon Age Trilogy — Engine & Trainer Viability Research

_Researched: 2026-07-06_

---

## Summary

| Game | Year | Engine | Difficulty | WeMod Trainer |
|---|---|---|---|---|
| Dragon Age: Origins | 2009 | BioWare Eclipse | Easy | Yes |
| Dragon Age 2 | 2011 | BioWare Eclipse (updated) | Easy | Yes |
| Dragon Age: Inquisition | 2014 | Frostbite 3 (EA) | Medium | Yes |

WeMod has working trainers for all three — confirming the memory approach is viable for each.

**Note:** In-game console is not a viable alternative — it disables Steam achievements.

---

## Dragon Age: Origins

**Engine:** BioWare Eclipse Engine (proprietary, 32-bit)

One of the most trainer-friendly games available. 32-bit process with stable, non-ASLR addresses that haven't changed in over a decade. No anti-cheat of any kind.

**Cheat approach:** Static pointer chains. Addresses are stable across sessions and patches are rare. No AoB scanning needed — hardcoded RVAs work.

**Typical WeMod cheats available:**
- Infinite Health / Mana
- Infinite Party Health
- Infinite Gold
- Infinite XP / Skill Points
- Infinite Inventory items
- One-Hit Kills

**Nexus Mods:** Enormous community (3,000+ mods). Asset/balance mods are well-supported but irrelevant to Magic Wand's memory approach.

**Notes for implementation:**
- 32-bit process — pointer sizes are 4 bytes, not 8. `read_memory` calls need 4-byte pointer reads
- No ASLR — module base is consistent; static offsets are reliable
- Gibbed's Save Editor exists as an offline alternative for save-state edits (achievements safe)

---

## Dragon Age 2

**Engine:** BioWare Eclipse Engine (updated iteration)

Same family as Origins, same ease of trainer support. Also 32-bit, no ASLR, no anti-cheat.

**Cheat approach:** Same as Origins — static pointer chains, stable addresses.

**Typical WeMod cheats available:**
- Infinite Health / Mana
- Infinite Party Health  
- Infinite Gold
- Infinite XP
- Infinite Ability Points
- One-Hit Kills

**Notes for implementation:**
- Structurally near-identical to Origins — the same implementation approach applies
- DA2 received fewer patches than Origins; addresses are arguably more stable

---

## Dragon Age: Inquisition

**Engine:** Frostbite 3 (EA proprietary)

More complex than the first two. Frostbite has no built-in mod/reflection system and EA never released tooling. The community built Frosty Mod Manager for asset mods, but that's separate from memory training.

**Cheat approach:** AoB scanning. Frostbite addresses shift more between patches than the Eclipse games, so static offsets alone are unreliable. WeMod uses AoB patterns to locate the correct memory regions each session — Magic Wand's AoB scan infrastructure handles this.

**Typical WeMod cheats available:**
- Infinite Health
- Infinite Mana / Stamina
- Infinite Party Health
- Infinite Gold
- Infinite Influence
- Infinite Power
- Infinite Skill Points
- Fast Research
- One-Hit Kills

**No anti-cheat** — EA did not ship any process-detection system with DAI. Frostbite's resistance to modding is structural (no official tooling) rather than active (no scanner).

**Notes for implementation:**
- 64-bit process
- AoB patterns will need to be sourced from FearLess Cheat Engine community tables or by running the Dev Panel scanner against a live DAI session
- Health values are typically `float` at a pointer offset from a character object
- Gold/Influence are likely `int` or `float` at static or near-static offsets from the module base
- Frosty Mod Manager is not relevant to Magic Wand — it handles asset replacement, not memory writes

---

## Implementation Priority

1. **DA:O and DA2 together** — same engine, nearly same offsets, low effort, high confidence. One research session with CE or the Magic Wand dev panel should yield all addresses. Static RVAs expected to be stable.

2. **DAI** — separate session, AoB-based. More work but well-trodden ground given WeMod's existing trainer. FearLess Cheat Engine forum likely has tables with the patterns already documented.

---

## Re-Finding Addresses After a Patch

**DA:O / DA2:** Patches are essentially nonexistent at this point — these games are finished. Addresses found today will work indefinitely.

**DAI:** Receives occasional patches (EA still updates it). If addresses break:
1. Check FearLess Cheat Engine (fearlessrevolution.com) for updated CE tables
2. Use the Magic Wand dev panel value scanner to locate current gold/health values
3. Update AoB patterns in the trainer JSON if the surrounding code changed
