# PMCC Smart Money System — TODO

## Sprint 1-9: COMPLETE (see git history)

## Sprint 10：Paper Trade Dashboard — Exchange-Style Tabs — COMPLETE

### Step 1: Refactor data preparation
- [x] Split paper trades into open vs closed in `build_live_dashboard()`
- [x] Compute per-trade: unrealized PnL, realized PnL, hold time, ROI
- [x] Build equity curve points for performance tab

### Step 2: Tab 1 — Open Positions (倉位)
- [x] Table: Market, Outcome, Side, Entry, Current, Size, Unrealized PnL, ROI%
- [x] Summary row: total invested, total unrealized PnL

### Step 3: Tab 2 — Trade History (歷史成交)
- [x] All paper trades newest first
- [x] Columns: Time, Side, Market, Outcome, Entry, Amount, Status
- [x] Period filter (all/today/week/month via CSS data attributes)

### Step 4: Tab 3 — Position History (倉位歷史)
- [x] Closed trades only
- [x] Columns: Open Time, Close Time, Market, Side, Entry, Exit, Realized PnL, ROI%, Hold Time
- [x] Color-coded green/red rows

### Step 5: Tab 4 — Performance (績效)
- [x] Summary cards: Total Trades, Win Rate, Total PnL, Avg PnL, Best/Worst Trade, Avg Hold Time
- [x] Equity curve SVG (reuse `build_equity_curve_svg()`)

### Step 6: CSS tabs + styling
- [x] CSS-only tabs (radio + :checked + sibling selectors)
- [x] Dark theme consistent with existing dashboard
- [x] No JavaScript

### Step 7: Keep Live Trades section unchanged
- [x] Live trades section untouched

### Step 8: Build + Test
- [x] `cargo check` pass
- [x] `cargo test` — skipped (no unit tests for dashboard HTML)
- [x] Release binary built
- [x] Dashboard HTML verified (60KB, all tabs + period filter + perf cards + equity curve)
- [ ] Dashboard LaunchAgent restart (port binding issue — pre-existing, not related to S10)

## Pending items from previous sprints
- Monitor running with 237 wallets (politics/economics/ai holders)
- 4 paper trades open (Pakistan, Exponent, Trump, Atlanta)
- 94 API errors (rate limiting) — 100ms delay added, pending verification
- Dashboard LaunchAgent keeps exiting — needs investigation
