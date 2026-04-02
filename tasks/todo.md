# PMCC Smart Money System — TODO

## Completed Sprints

### Sprint 1-4: Smart Money Pipeline (dba6216)
- Wallet discovery, scoring, tracking, signals, follow trading, Telegram, ROI, backtest, dashboard

### Sprint 5-6: Odds Monitoring + P&L (ae5d27f, 15a6fc3)
- Odds change monitoring, condition triggers, precise P&L tracking, trade history visualization

### Sprint 7: Real-Time Monitor (04c09bc)
- Continuous monitor loop, condition triggers, paper trading with stop-loss

### Sprint 8: Wallet Intelligence (e9162f5)
- Auto-renew wallets, PnL tracking per wallet, trade analysis

### Sprint 9: Market-First Discovery (1a286f3)
- discover-markets, discover-whales, discover-auto pipeline

### Sprint 10: Paper Trade Dashboard (869ff9b)
- Exchange-style tabbed dashboard, sparklines, 24h trend, equity curve

### Sprint 11: 5-Minute Crypto Trading (7b15950)
- Binance feed, momentum signal, Polymarket market discovery, paper trade monitor
- Stop-loss -45%, trailing stop, 98 exclude keywords, anti-hedge, price filter

### Sprint 12 Phase 1: Binance Futures (696ed1c)
- BinanceFuturesFeed: funding rate, OI, liquidations from FAPI
- 7-component enhanced signal (funding_signal, oi_delta_signal, liquidation_signal)
- Dashboard: separate Smart Money vs Crypto 5m sections
- All Night Shift R114 issues resolved (P0 ×4, P1 ×3, P2 ×1)

### Sprint 12 Phase 2: Multi-Exchange (1cc68f2)
- OkxFeed: OB, trades, funding, OI, liquidations
- HyperliquidFeed: L2 book, funding+OI via POST /info
- Aggregator: parallel 3-exchange fetch, volume-weighted OB/trade merge
- Aggregated futures: avg funding, sum OI, combined liquidations

## Current State

- **Branch**: `feature/5m-crypto-trade` (15 commits ahead of main)
- **Binary**: `target/release/polymarket` — multi-exchange enhanced signal
- **Signal**: 7-component, 3 exchanges (Binance + OKX + Hyperliquid)
- **Dashboard**: localhost:3456 (LaunchAgent), SM + Crypto split
- **Monitor**: LaunchAgent, 3-min cycles, enhanced signal active
- **Code**: smart.rs ~5450 lines, crypto/ ~1487 lines (4 files)

## Next

- [ ] Sprint 12 Phase 3: Bybit feed + backtest weight tuning
- [ ] Merge `feature/5m-crypto-trade` → main
- [ ] Run enhanced signal monitor for 24-48h, compare win rate vs old 4-component
