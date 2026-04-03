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

## Current State

- **Branch**: `feature/5m-crypto-trade` (17 commits ahead of main)
- **Signal**: 7-component, 4 exchanges (Binance + OKX + Hyperliquid + Bybit)
- **Dashboard**: localhost:3456 (LaunchAgent), SM + Crypto split
- **Code**: smart.rs ~5460 lines, crypto/ ~1640 lines (4 files)

## Next

- [ ] Merge `feature/5m-crypto-trade` → main
- [ ] Run 4-exchange monitor 24-48h, compare win rate
- [ ] Weight tuning via backtest grid search (optional — need historical multi-exchange data)
