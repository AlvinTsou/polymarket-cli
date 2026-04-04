# PMCC Smart Money System — TODO

## Completed Sprints

### Sprint 1-4: Smart Money Pipeline (dba6216)
### Sprint 5-6: Odds Monitoring + P&L (ae5d27f, 15a6fc3)
### Sprint 7: Real-Time Monitor (04c09bc)
### Sprint 8: Wallet Intelligence (e9162f5)
### Sprint 9: Market-First Discovery (1a286f3)
### Sprint 10: Paper Trade Dashboard (869ff9b)
### Sprint 11: 5-Minute Crypto Trading (7b15950)
### Sprint 12 Phase 1: Binance Futures (696ed1c)
### Sprint 12 Phase 2: OKX + Hyperliquid (1cc68f2)
### Sprint 12 Phase 3: Bybit (48b6120)
### Bug Fixes: Code Review + Night Shift R114 (64530aa, dd789ce)
### Docs: issues.md all resolved (d5a0f22), lessons.md 20 entries (637a476)

## Session State

- **timestamp**: 2026-04-04T18:15:00+08:00
- **phase**: implementing
- **last task**: Sprint 13 Phase A+B implemented, monitors restarted, old paper trades archived
- **blockers**: none
- **next actions**: Wait 48h for new data → run C.4 comparison → consider A.6 fade experiment

## Current State

- **Branch**: `feature/5m-crypto-trade` — uncommitted Sprint 13 changes (not yet pushed)
- **Signal**: 7-component, 4 exchanges (Binance + OKX + Hyperliquid + Bybit)
- **Dashboard**: localhost:3456 (LaunchAgent), SM + Crypto split
- **Issues**: 19/19 resolved (14 Night Shift + 4 code review + 1 WONTFIX)
- **SM Monitor**: PID 88502, Sprint 13 rules (no whale-exit close, TP/trailing/time-stop, min_wallets=3)
- **Crypto Monitor**: PID 88665, BTC, conf 0.50 default, tiered sizing, 08-20 ET only
- **Paper trades**: cleared — fresh start from 2026-04-04 18:13 UTC
- **Backup**: `~/.config/polymarket/smart/follows.jsonl.bak.sprint12-20260404` (236 trades)

## Paper Trade Results (as of 2026-04-04)

| Category | Trades | PnL | Win Rate | Verdict |
|----------|--------|-----|----------|---------|
| Smart Money | 170 closed | -$326.50 | 22% | Strategy broken — whale-follow with delay = buy high sell low |
| Crypto 5m | 30 closed | -$13.20 | 47% | Near-random — signal needs tuning, not fundamentally broken |
| **Total** | **200 closed** | **-$339.70** | **25.5%** | |

Top exit reasons: whale-exit (104), stop-loss (65), trailing-stop (16), 5m-resolved (14)
Stop-loss slippage: set -45% but often triggers at -87%~-99% (3min scan too slow)

## Sprint 13: Strategy Overhaul

### Phase A: Smart Money Exit Logic Overhaul
- [x] A.1 Remove whale-exit auto-close — log only, don't close position
- [x] A.2 Add self-managed exits: TP +20%, trailing +15%/40%, time-stop 7d
- [ ] A.3 Fix stop-loss scan interval: 180s → 60s for SM positions (deferred — needs separate timer)
- [x] A.4 Raise min_wallets: 2 → 3 (monitor.json updated)
- [x] A.5 Reduce market horizon: 30d → 14d
- [ ] A.6 Add whale-exit-as-entry trigger (fade experiment, separate tag)
- [x] A.7 Test: restart SM monitor with new config, verified running (PID 88502)

### Phase B: Crypto 5m Signal Tuning
- [x] B.1 Raise default min_confidence: 0.30 → 0.50
- [x] B.2 Add tiered sizing: conf >= 0.70 → 1.5x, else 1x
- [ ] B.3 Add CLOB midpoint as 8th signal component (weight 0.10) (deferred — needs market.rs change)
- [x] B.4 Add time-of-day filter: only trade 08:00-20:00 ET
- [x] B.5 Log component breakdown on resolution for post-analysis
- [x] B.6 Test: restart crypto monitor with new config (PID 88665, conf 0.50 default)

### Phase C: Backtest & Validation
- [ ] C.1 Export 234 paper trades to CSV with signal components
- [ ] C.2 Backtest Phase A rules against historical SM data
- [ ] C.3 Backtest Phase B rules against historical crypto data
- [ ] C.4 Run new config 48h, compare with old results

### Pending from Sprint 12
- [ ] Merge `feature/5m-crypto-trade` → main (after Sprint 13 Phase A+B stable)
