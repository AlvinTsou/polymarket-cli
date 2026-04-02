
## 2026-03-26 Round R114 — ALL RESOLVED

- [x] **P0** `bot_token[..10]` panic — `.get(..10).unwrap_or(&bot_token)`
- [x] **P0** `signals[0]` panic — early return + `.first()`
- [x] **P0** osascript injection — `osascript_safe()` strips `"` `\` newlines null
- [x] **P0** Stop-loss ROI division by zero — `if r.amount_usdc > 0.0` guard
- [x] **P1** `addr[..8]` slice panic — length check `>= 12` before slicing
- [x] **P1** `cancelled_keys` unbounded growth — `HashMap<K, DateTime>` + prune > 1h
- [x] **P1** `let _` silent data loss — 10 store ops → `if let Err(e)` with eprintln warning
  - Kept `let _` for notifications (osascript, telegram, browser open) — acceptable to suppress
- [x] **P2** Confirmation delay stale price — use `store::current_price_map()` at execution time
