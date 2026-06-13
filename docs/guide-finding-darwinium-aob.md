# Guide: Finding the Darwinium Pointer Chain with Cheat Engine

Cell to Singularity is a Unity/C# game. Darwinium lives in managed heap memory,
so write instructions land in `mono-2.0-bdwgc.dll` (the Mono GC), not game code.
AOB scanning does not work here. Instead we use a **pointer scan** to find a stable
path from a fixed module address down to the Darwinium value.

This requires **two game sessions** to filter down to a stable chain.

---

## What You Need
- Cell to Singularity running (Steam version)
- Cheat Engine 7.x
- The Darwinium address from Session 1 already found: proceed from Step 2

---

## Session 1 — Find the Address and Run the Pointer Scan

### Step 1 — Find the Darwinium Address (already done if you have it)
1. Attach Cheat Engine to `CellToSingularity.exe`
2. Set Value Type to **Double**, scan for your current Darwinium amount
3. Change it in-game, rescan — repeat until 1 address remains
4. Add it to the address table

### Step 2 — Run the Pointer Scan
1. Right-click the Darwinium address in the address table
2. Select **"Pointer scan for this address"**
3. In the dialog that appears:
   - Check **"Use saved pointer scan results to filter"** — **leave unchecked** for first run
   - Check **"Only find paths with a static base"**
   - Set **Max level** to `6`
   - Set **Max offset** to `2000` (hex)
   - Click **OK**
4. Save the `.ptr` scan file somewhere you can find it (e.g. Desktop, name it `darwinium1.ptr`)
5. Wait — this takes 1–3 minutes. A results window will open with thousands of chains.
6. **Leave everything open. Do not close the game.**

### Step 3 — Change Darwinium and Rescan (same session)
1. Go back to the game and spend or earn some Darwinium so the amount changes
2. Go back to the Cheat Engine pointer scan results window
3. Click **Pointer Scanner → Rescan memory**
4. Enter the **new Darwinium address** (rescan the main CE window to find it after the change, or just re-do the double scan — it may have shifted)
5. Enter the **new value**
6. Click **OK** — results will narrow down significantly

---

## Session 2 — Confirm Stability

1. **Close and relaunch** Cell to Singularity
2. In Cheat Engine, attach to the process again and find the new Darwinium address (re-do the double scan)
3. Go to Pointer Scanner → **"Load pointer scan results"** → open `darwinium1.ptr`
4. Click **"Rescan memory"**, enter the **new address** and **current value**, click OK
5. Results will narrow to chains that survived the restart — these are stable

---

## What to Paste Here

From the final results list, paste **2–3 of the top results**. Each row looks like:

```
CellToSingularity.exe+1A4F20  ->  +2B8  ->  +10  ->  +0
```

The format is: `BaseModule+StaticOffset` then a chain of `+Offset` hops.

That's everything needed to build the trainer. The number of hops and exact offsets
will vary — just paste what Cheat Engine shows.

---

## Tips
- Results with fewer hops (2–3) are generally more stable than long chains (5–6)
- Prefer results where the base is `CellToSingularity.exe` over `mono-2.0-bdwgc.dll`
- If Session 2 still shows hundreds of results, close/reopen the game one more time and rescan again
