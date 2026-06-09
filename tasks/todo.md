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

- **timestamp**: 2026-06-09T00:00:00+08:00
- **phase**: current paper-trade analysis refreshed
- **last task**: ran current `smart roi`, `smart crypto status`, and `smart reconcile --dry-run`
- **blockers**: none
- **next actions**: accumulate more current sample → update `docs/strategy-result-ledger.md` again → decide whether A.6/B.3 still have enough edge to build

## Current State

- **Branch**: `main`, clean and synced with `origin/main` at `41861c2`
- **Signal**: Sprint 13 rules plus later crypto daily range/strike pivot and strategy sizing/toxicity research
- **Dashboard**: localhost:3456 (LaunchAgent), SM + Crypto split (runtime not re-verified in this task sync)
- **Issues**: 19/19 resolved (14 Night Shift + 4 code review + 1 WONTFIX)
- **SM Monitor**: Sprint 13 rules in code (no whale-exit close, TP/trailing/time-stop, min_wallets=3) plus settled-market reconcile path
- **Crypto Monitor**: tightened sizing/confidence defaults exist in code; live PID/LaunchAgent args need re-check before relying on runtime state
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

## Current Paper Trade Refresh (2026-06-09)

| Category | Current Local Sample | PnL / ROI | Win Rate | Verdict |
|----------|----------------------|-----------|----------|---------|
| Smart Money | 8 closed, 0 open | -$2.9992 / -3.7490% | 50.0% | Too small to justify new entry experiments |
| Crypto | 0 paper trades | N/A | N/A | No current sample; verify/restart only if crypto pivot remains desired |
| Reconcile dry-run | 0 open dry-run positions scanned | $0.00 delta | N/A | No settled zombie cleanup needed in current local store |

## Sprint 13: Strategy Overhaul

### Phase A: Smart Money Exit Logic Overhaul
- [x] A.1 Remove whale-exit auto-close — log only, don't close position
- [x] A.2 Add self-managed exits: TP +20%, trailing +15%/40%, time-stop 7d
- [x] A.3 Fix stop-loss scan interval: 180s → 60s for SM positions (completed via decoupled 60s timer)
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

### Phase B Follow-up (2026-05-05): Real-data Tuning
After 1 month live: SM 66% WR / +$276, Crypto 50% WR / +$22.5 (random region).
- [x] B.7 Lower default amount: $10 → $5 (smart.rs:417)
- [x] B.8 Raise default min_confidence: 0.5 → 0.6 (smart.rs:429)
- [x] B.9 Two-tier sizing: ≥0.65 → 1.5x, ≥0.75 → 2x (smart.rs:5288-5294)
- [x] B.10 Update LaunchAgent plist (amount=5, min-confidence=0.6)
- [x] B.11 cargo build --release
- [x] B.12 launchctl reload com.pmcc.crypto, new PID 47268 with new args
- [x] B.13 Re-run current paper trade analysis, compare WR/ROI after the later crypto pivot and sizing/toxicity changes
- [ ] B.14 Accumulate enough current sample before deciding on CLOB midpoint component or crypto restart/tuning

### Phase C: Backtest & Validation
- [ ] C.1 Export 234 paper trades to CSV with signal components
- [ ] C.2 Backtest Phase A rules against historical SM data
- [ ] C.3 Backtest Phase B rules against historical crypto data
- [ ] C.4 Compare current config results with old Sprint 13 baseline before adding more entry logic

### Strategy Queue (2026-06-07)
- [x] Q.1 Record all implemented, queued, and proposed trading strategies in `docs/new-trading-strategies.md`
- [x] Q.2 Mirror the strategy register in `research/new-trading-strategies.md` (superseded by Q.4; file is now an index/pointer)
- [x] Q.3 Update `docs/smart-money-system.md` to match current Sprint 13 paper-trading rules
- [x] Q.4 Decide canonical strategy-doc location (`docs/` register + ledger; `research/` index/notes only)
- [x] Q.5 Add strategy result ledger with WR/ROI/sample size/disable criteria per strategy (`docs/strategy-result-ledger.md`)
- [x] Q.6 Refresh this queue after Q.5 so A.6/B.3 decisions are evidence-backed, not carried over from stale May assumptions

### Pending from Sprint 12
- [x] Merge `feature/5m-crypto-trade` → main (main now includes Sprint 13 and later follow-up commits)
