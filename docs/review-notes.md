# Review Notes

Running log of PR review findings that need follow-up. One section per PR. Newest at top.

---

## PR #17 — `refactor: reduce duplication and improve efficiency` — CHANGES REQUESTED 2026-06-14

Did not approve. Two compile errors block; rest of the refactor is strong and should be held.

### Blockers (must fix)

1. **Rust: `engine.rs:79` does not build.**
   `ProcessRefreshKind::new()` does not exist in `sysinfo` 0.33.1. Available constructors are `::nothing()` and `::everything()`. Change to:
   ```rust
   let refresh = ProcessRefreshKind::nothing().with_exe(UpdateKind::Always);
   ```
   Verify with `cargo check` from `src-tauri/` before re-requesting.

2. **TypeScript: `App.tsx:183` does not build.**
   ```typescript
   !(['scan', 'mono', 'mono_chain'] as const).includes(cheat.type)
   ```
   `as const` narrows the tuple's `includes` parameter to `'scan' | 'mono' | 'mono_chain'`, which won't accept `cheat.type`'s wider union. Pick one:
   - Revert to the original `cheat.type !== 'scan' && cheat.type !== 'mono' && cheat.type !== 'mono_chain'` — three items, the rewrite doesn't earn its keep.
   - `(['scan', 'mono', 'mono_chain'] as readonly string[]).includes(cheat.type)`
   - A `Set<string>` lookup.

### Warning (nice to have)

3. **Rust: `engine.rs:96` — unnecessary `unsafe` block in the new `read_memory`.** Both `ProcessHandle::open` and `read_memory_raw` are safe, so the outer `unsafe { ... }` wrapper does nothing. Drop it to silence the warning.

### Hold these — they're real wins, don't lose them in the fix-up

- `ProcessHandle` RAII (closes on early-return paths the original missed)
- `scan_memory_for_bytes` now uses `read_memory_raw` (one `OpenProcess`/`CloseHandle` per scan, not per region — significant on processes with many regions)
- `aob_scan` switched to `windows()` — quietly fixes a one-off bug where the last possible match position was skipped
- `find_process_by_name` targeted refresh (assuming the API fix above): meaningful startup-perf improvement vs. `System::new_all()`
- `get_module_info` hoisted `to_lowercase()` out of the loop
- `read_4bytes` shared by `read_int` and `read_float`
- `memCmd` / `toHexAddr` TS helpers — dedupes four ternary chains and four hex format calls
- Hoisted `getModuleBaseRaw` so the non-signature path makes one IPC call instead of two
- Poll loop `.filter(c => c.valueType != null)` — correctly skips `patch` rows (scan rows still pass through but that's a pre-existing issue, not made worse)
- `setCheatError` memoized via `useCallback` — replaces four inline closures with a stable identity
- `resetCheatState` callback — collapses three call sites
- `handleGameClick` fallback removed (silently loading `trainers[0]` was strictly worse than doing nothing)
- `makeSetter` factory in `useSettings`

### Status
Posted review on GitHub as a comment (cannot self-request-changes on own PR). Author needs to push fixes for items 1 and 2 (and ideally 3), then ping for re-review.
