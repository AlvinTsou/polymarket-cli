# Sprint 13: Strategy Overhaul — Smart Money Reverse + Crypto Signal Tuning

## Status: PLANNING

## Motivation

Paper trade results (13 days, 234 trades):
- **Smart Money**: 170 closed, -$326.50, 22% win rate — catastrophic
- **Crypto 5m**: 30 closed, -$13.20, 47% win rate — near-random but salvageable
- Root cause: following whales with delay = systematic buy-high sell-low

## Phase A: Smart Money Exit Logic Overhaul

**Goal**: Stop bleeding from whale-follow strategy. Decouple exit from whale behavior.

### Changes

1. **Remove whale-exit auto-close** (`smart.rs:3541-3552`)
   - When whale closes, log it but do NOT close our position
   - Add field `whale_exited_at: Option<DateTime>` to FollowRecord for tracking

2. **Self-managed exit rules** (replace whale-exit):
   - Take-profit: close at +20% ROI
   - Trailing-stop: activate at +15% (lower from +30%), drawdown 40% (tighter from 50%)
   - Stop-loss: keep -45% but check every 60s not every 180s (reduce slippage)
   - Time-stop: close if open > 7 days with < +5% ROI (dead position)

3. **Raise entry bar** (`smart.rs:3324-3429`):
   - `min_wallets`: 2 → 3 (require 3 whale consensus)
   - Price range: 0.15-0.80 (already in place, verify enforced)
   - Market horizon: 30 days → 14 days (shorter = less uncertainty)

4. **Whale-exit-as-entry experiment** (Phase A.2):
   - New trigger type: when whale exits at a loss → we enter same direction
   - Logic: whale panic-sold, market overreacted, we catch the bounce
   - Paper trade only, separate tracking tag `entry_reason: "fade-whale-exit"`

### Files to modify
- `src/commands/smart.rs`: monitor loop exit logic, trigger evaluation
- `src/smart/mod.rs`: FollowRecord struct (add whale_exited_at)
- `src/smart/store.rs`: close_follow_position (add time-stop)

## Phase B: Crypto 5m Signal Tuning

**Goal**: Improve 47% win rate by filtering noise and adding CLOB signal.

### Changes

1. **Raise confidence threshold** (`smart.rs:5157`)
   - Default min_confidence: 0.30 → 0.50
   - Add tiered sizing: conf >= 0.70 → $15, conf >= 0.50 → $10

2. **Add CLOB price as 8th signal component** (`momentum.rs`)
   - Fetch Polymarket CLOB midpoint for Up/Down tokens
   - If CLOB agrees with momentum direction → confidence boost +0.05
   - If CLOB disagrees (e.g., Up token < 0.45 but momentum says UP) → SKIP
   - Weight: 0.10 (redistribute from price_mom_5m 0.10→0.05, oi 0.10→0.05)

3. **Time-of-day filter** (`smart.rs:5122`)
   - Only trade during US+EU overlap: 08:00-20:00 ET
   - Asian session (20:00-08:00 ET) has thinner books, more false signals

4. **Resolution verification** (`smart.rs:5258-5335`)
   - Log actual vs predicted with component breakdown for post-analysis
   - Track which components were correct/wrong per trade

### Files to modify
- `src/crypto/momentum.rs`: add CLOB component, adjust weights
- `src/crypto/market.rs`: fetch CLOB midpoint price
- `src/commands/smart.rs`: confidence threshold, time filter, tiered sizing

## Phase C: Backtest & Validation

1. Export all 234 paper trades to CSV with full signal components
2. Backtest Phase A rules against historical Smart Money trades
3. Backtest Phase B rules against historical Crypto trades
4. Compare projected PnL vs actual
5. Run new config for 48h paper trade before considering real money

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Reverse strategy also loses | Paper trade only, separate tag, easy to disable |
| CLOB data adds latency | Parallel fetch with existing tokio::join! |
| Tighter stops increase trade count | Daily budget cap unchanged ($100 SM, $60 crypto) |
| Reducing trades misses real opportunities | Track skipped-by-filter signals for analysis |

---

# Sprint 12: Multi-Exchange Signal Integration (COMPLETED)

## Status: ALL PHASES COMPLETE + ALL ISSUES RESOLVED

## Architecture (Final)

```
                    ┌──────────────────────────────────────────┐
                    │         Parallel Fetch (~150ms)           │
                    │                                            │
  Spot Data         │  Binance ────┐                             │
  (OB + Trades)     │  OKX ────────┤                             │
                    │  Hyperliquid ┼─► fetch_aggregated_spot     │
                    │  Bybit ──────┘   → merged OB (4 exchanges) │
                    │                  → merged trade flow (3)    │
                    │                                            │
  Futures Data      │  Binance FAPI ─┐                           │
  (Funding/OI/Liq)  │  OKX Swap ─────┤                           │
                    │  Hyperliquid ──┼─► fetch_aggregated_futures │
                    │  Bybit Linear ─┘   → avg funding (4)       │
                    │                    → sum OI (4)             │
                    │                    → combined liqs (2)      │
                    │                                            │
  Price Reference   │  Binance ───────► candles (1m × 30)        │
                    └──────────┬─────────────────────────────────┘
                               │
                               ▼
                     compute_signal_full()
                     ├── price_mom_1m    × 0.15
                     ├── price_mom_5m    × 0.10
                     ├── agg_ob_imbal    × 0.20  (4 exchanges)
                     ├── agg_trade_flow  × 0.20  (3 exchanges)
                     ├── funding_signal  × 0.15  (4 exchanges avg)
                     ├── oi_delta_signal × 0.10
                     └── liquidation_sig × 0.10  (Binance+OKX)
                               │
                               ▼
                     Direction: UP / DOWN / SKIP
                     Confidence: |score| / 0.30, cap 1.0
```

## File Structure

```
src/crypto/
├── mod.rs        (~175 lines)  Types: CryptoAsset, OrderBook, Trade, FuturesData,
│                                Liquidation, AggregatedSpot, MomentumSignal, Market5m
├── feed.rs       (~800 lines)  BinanceFeed, BinanceFuturesFeed, OkxFeed,
│                                HyperliquidFeed, BybitFeed,
│                                fetch_aggregated_spot, fetch_aggregated_futures
├── momentum.rs   (~470 lines)  compute_signal, compute_signal_enhanced,
│                                compute_signal_full, backtest_signals
└── market.rs     (~200 lines)  find_next_5m_market, parse_5m_time_window

src/smart/store.rs              atomic_write(), prune_signals(), prune_odds_alerts_log()
```

## Exchange Coverage

| Exchange | Type | OB | Trades | Funding | OI | Liqs | Timeout |
|----------|------|----|----|---------|----|----|---------|
| Binance Spot | CEX | x | x | — | — | — | 5s |
| Binance FAPI | CEX | — | — | x | x | x | 5s |
| OKX | CEX | x | x | x | x (0.01 BTC/ct) | x | 5s |
| Hyperliquid | DEX | x | — | x (×8 norm) | x | — | 5s |
| Bybit | CEX | x | x | x | x | — | 5s |

All public endpoints, no API keys required.

## Commit History (Sprint 12)

```
63cd715 docs: update todo
637a476 docs: add lessons.md — 20 entries
d5a0f22 docs: update issues.md — all 14 resolved
dd789ce fix: atomic writes, format_money negatives, log pruning
64530aa fix: code review — timeouts, funding dilution, OKX OI, Telegram escape
48b6120 feat: add Bybit feed — 4-exchange aggregation (Phase 3)
1cc68f2 feat: multi-exchange signal — OKX + Hyperliquid (Phase 2)
696ed1c feat: Binance Futures, dashboard split, fix R114 (Phase 1)
```

## Merge Plan

Before merging `feature/5m-crypto-trade` → main:
1. Run 4-exchange monitor 24-48h, collect paper trade results
2. Compare win rate: old (trades #1-24) vs new (trades #25+)
3. Squash or cherry-pick (22 commits on branch)
4. Delete remote branch after merge

## Future Considerations (Not Planned)

- Weight tuning via backtest grid search (needs historical multi-exchange data)
- WebSocket upgrade for sub-second latency
- Exchange health monitoring with auto-disable
- dYdX v4 as 5th exchange (diminishing returns)
