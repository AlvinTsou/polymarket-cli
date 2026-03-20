# PMCC Smart Money System — TODO

## Sprint 1-7: COMPLETE (see git history)

## Sprint 8：Smart Wallet Intelligence

### Step 1: WatchedWallet extension
- [x] Add `discovery_periods`, `last_seen_at`, `stale` fields (Option + serde(default))
- [x] Add `WalletPnlSnapshot`, `CategoryStat` types

### Step 2: Multi-period discover + auto-renew
- [x] `--auto-renew` flag on `smart discover`
- [x] Query day + week + month leaderboards concurrently
- [x] Merge/dedup wallets, tag with periods found
- [x] Mark stale wallets not seen on any leaderboard
- [x] `update_wallet()` helper in store

### Step 3: `smart wallet-pnl` command
- [x] Fetch open positions via `PositionsRequest` → `cash_pnl`
- [x] Fetch closed positions via `ClosedPositionsRequest` → `realized_pnl`
- [x] Compute open PnL, realized PnL, total PnL, win rate
- [x] Table output: per-position breakdown + summary
- [x] Auto-save PnL snapshot to history

### Step 4: PnL snapshot storage
- [x] `pnl_history/{address}.jsonl` — append per wallet-pnl call
- [x] `append_pnl_snapshot()` / `load_pnl_history()` in store

### Step 5: `smart analyze` command
- [x] Category detection (Politics, Crypto, AI/Tech, Sports, Economy, Geopolitics, Other)
- [x] Trading style profile (avg size, entry price, concentration, direction bias)
- [x] Contrarian score (positions priced < 0.40)
- [x] Conviction assessment
- [x] Recent activity from signal history

### Step 6: Monitor integration
- [ ] Auto-renew watch list every 6h in monitor loop (future enhancement)
- [ ] Record PnL snapshot per wallet per scan cycle (future enhancement)

### Step 7: Report enhancement
- [ ] Per-wallet PnL summary cards (future enhancement)
- [ ] Category distribution SVG chart (future enhancement)

### Step 8: Build + Test
- [x] `cargo check` pass
- [x] `cargo test` — 157 tests passed
- [x] Release binary built
- [x] CLI help verified for all 3 new commands
- [ ] Live test: `smart discover --auto-renew`
- [ ] Live test: `smart wallet-pnl 0x...`
- [ ] Live test: `smart analyze 0x...`
