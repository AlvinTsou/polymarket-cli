# PMCC Smart Money System — TODO

## Sprint 1-5: COMPLETE (see git history)

## Sprint 6：精確損益追蹤 + 交易歷史視覺化

### Step 1: Types + Serde Compat
- [x] Add `TradeStatus` enum to `mod.rs`
- [x] Extend `FollowRecord` with `fill_price`, `status`, `closed_at`, `exit_price`, `realized_pnl`, `position_id`
- [x] All new fields `Option` + `#[serde(default)]` for backward compat
- [x] Add `effective_entry()` and `is_open()` helper methods

### Step 2: Store Update Function
- [x] Add `save_follow_records()` to `store.rs` — rewrite JSONL
- [x] Add `close_follow_position()` — find + close matching open record

### Step 3: Fill Price Capture
- [x] In `cmd_follow()`: store `fill_price`, `status`, `position_id` in new FollowRecord
- [x] In `cmd_auto_follow()`: same treatment

### Step 4: Position Closure Detection
- [x] In `cmd_scan()`: when ClosePosition detected, call `close_follow_position()`
- [x] Auto-calculate `realized_pnl` on closure

### Step 5: Enhanced `smart roi`
- [x] Add `--wallet`, `--market`, `--period`, `--status` flags
- [x] Split display: realized PnL (closed) vs unrealized PnL (open)
- [x] Win rate calculated from closed trades only
- [x] Summary footer with all metrics

### Step 6: Enhanced `smart history`
- [x] Add `--period`, `--status` flags
- [x] Show status column (OPEN/CLOSED)
- [x] Show exit price + realized PnL for closed trades
- [x] Table format with tabled crate

### Step 7: SVG Chart Helpers
- [x] Equity curve generator (cumulative PnL line chart, inline SVG)
- [x] Trade timeline scatter (entries/exits, win=green, loss=red, size=magnitude)
- [x] Responsive dark-theme styling matching existing dashboard

### Step 8: Dashboard Overhaul
- [x] Add equity curve to report
- [x] Add trade timeline scatter to report
- [x] Add summary cards: realized PnL, unrealized PnL, win rate, best/worst trade
- [x] Add per-market performance breakdown table

### Step 9: Build + Test
- [x] `cargo check` — compile pass (4 warnings, all pre-existing)
- [x] `cargo test` — 108 unit + 49 integration = 157 passed
- [x] Verify old follows.jsonl backward compat (serde(default))
- [x] Release binary built

### Step 10: Manual Verification
- [x] `polymarket smart roi --help` — shows new flags
- [x] `polymarket smart history --help` — shows new flags
- [ ] Live testing with actual follows data (user to verify)
