# PMCC Smart Money System -- TODO

## Sprint 1-11: COMPLETE (see git history + previous todo)

## Sprint 12 Phase 1: Binance Futures Integration — COMPLETE

### Step 1: New types in mod.rs
- [x] Add FuturesData, Liquidation structs
- [x] Expand SignalComponents with 3 new fields (funding_signal, oi_delta_signal, liquidation_signal)

### Step 2: BinanceFuturesFeed in feed.rs
- [x] fetch_funding_rate (fapi/v1/premiumIndex)
- [x] fetch_open_interest (fapi/v1/openInterest)
- [x] fetch_liquidations (fapi/v1/allForceOrders)
- [x] fetch_all() convenience method

### Step 3: Enhanced signal in momentum.rs
- [x] compute_signal_enhanced() — 7-component model
- [x] funding_signal: contrarian normalized funding rate
- [x] oi_delta_signal: OI change × price direction
- [x] liquidation_signal: net long vs short liquidation imbalance
- [x] Keep compute_signal() unchanged as fallback

### Step 4: Wire into monitor (smart.rs)
- [x] Add BinanceFuturesFeed alongside BinanceFeed in monitor
- [x] Parallel fetch spot + futures data
- [x] Use compute_signal_enhanced when futures data available
- [x] Graceful fallback to compute_signal if futures fetch fails

### Step 5: Wire into CLI commands
- [x] `crypto signal` — show enhanced signal with futures components
- [x] Futures raw data display (funding rate, mark price, OI, liquidation count)

### Step 6: Verify
- [x] cargo build --release — OK (13 warnings, all pre-existing)
- [x] polymarket smart crypto signal — BTC enhanced signal with futures data
- [x] polymarket smart crypto signal eth — ETH enhanced signal working

## Smart Money Monitor (running)
- Monitor PID running via nohup, 3-min cycles
- 237 wallets, 98 exclude keywords
- Paper trades active with stop-loss + trailing stop
- Dashboard at localhost:3456 (LaunchAgent)

## Next
- Sprint 12 Phase 2: OKX + Hyperliquid cross-exchange aggregation
- Sprint 12 Phase 3: Bybit + weight tuning via backtest grid search
- Fix P0/P1 issues from Night Shift R114 (tasks/issues.md)
