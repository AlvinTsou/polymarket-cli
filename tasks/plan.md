# Sprint 12: Multi-Exchange Signal Integration

## Status: Phase 1+2 COMPLETE, Phase 3 PENDING

## Architecture (Implemented)

```
                    ┌─────────────────────────────────────┐
                    │        Parallel Fetch (~150ms)       │
                    │                                       │
  Spot Data         │  Binance ──┐                          │
  (OB + Trades)     │  OKX ──────┼─► fetch_aggregated_spot  │
                    │  Hyperliquid┘   → merged OB imbalance │
                    │                 → merged trade flow    │
                    │                                       │
  Futures Data      │  Binance FAPI─┐                       │
  (Funding/OI/Liq)  │  OKX Swap ────┼─► fetch_aggregated_futures
                    │  Hyperliquid──┘   → avg funding rate  │
                    │                   → sum OI            │
                    │                   → combined liqs     │
                    │                                       │
  Price Reference   │  Binance ──────► candles (1m × 30)    │
                    └─────────┬───────────────────────────┘
                              │
                              ▼
                    compute_signal_full()
                    ├── price_mom_1m    × 0.15
                    ├── price_mom_5m    × 0.10
                    ├── agg_ob_imbal    × 0.20  (3 exchanges)
                    ├── agg_trade_flow  × 0.20  (2 exchanges)
                    ├── funding_signal  × 0.15  (3 exchanges avg)
                    ├── oi_delta_signal × 0.10
                    └── liquidation_sig × 0.10  (Binance+OKX)
                              │
                              ▼
                    Direction: UP / DOWN / SKIP
                    Confidence: |score| / 0.30, cap 1.0
```

## File Structure (Current)

```
src/crypto/
├── mod.rs        (172 lines)  Types: CryptoAsset, OrderBook, Trade, FuturesData,
│                               Liquidation, AggregatedSpot, MomentumSignal, Market5m
├── feed.rs       (647 lines)  BinanceFeed, BinanceFuturesFeed, OkxFeed,
│                               HyperliquidFeed, fetch_aggregated_spot/futures
├── momentum.rs   (468 lines)  compute_signal, compute_signal_enhanced,
│                               compute_signal_full, backtest_signals
└── market.rs     (200 lines)  find_next_5m_market, parse_5m_time_window
```

## Signal Weights

| Component | Basic (fallback) | Enhanced (Phase 1) | Multi-Exchange (Phase 2) |
|-----------|-----------------|-------------------|------------------------|
| price_mom_1m | 0.30 | 0.15 | 0.15 |
| price_mom_5m | 0.25 | 0.10 | 0.10 |
| ob_imbalance | 0.25 (Binance) | 0.20 (Binance) | 0.20 (3-exchange) |
| trade_flow | 0.20 (Binance) | 0.20 (Binance) | 0.20 (2-exchange) |
| funding | — | 0.15 (Binance) | 0.15 (3-exchange avg) |
| oi_delta | — | 0.10 | 0.10 |
| liquidation | — | 0.10 (Binance) | 0.10 (Binance+OKX) |

## Exchange API Summary

| Exchange | Type | Auth | Spot | Futures | Data Fetched |
|----------|------|------|------|---------|-------------|
| Binance | CEX | No | OB, trades, candles | funding, OI, liquidations | All |
| OKX | CEX | No | OB, trades | funding, OI, liquidations | All |
| Hyperliquid | DEX | No | OB (L2) | funding, OI | No trades/liqs |
| Bybit | CEX | No | — | — | Phase 3 |

## Phase 3 Plan (Next)

1. Add `BybitFeed` (v5 unified API)
   - `GET /v5/market/orderbook?category=spot&symbol=BTCUSDT&limit=200`
   - `GET /v5/market/recent-trade?category=spot&symbol=BTCUSDT&limit=1000`
   - `GET /v5/market/funding/history?category=linear&symbol=BTCUSDT`
   - `GET /v5/market/open-interest?category=linear&symbol=BTCUSDT&intervalTime=5min`
2. Backtest weight tuning — grid search over 7 weights
3. Exchange health monitoring — track latency per feed, auto-disable > 5s
4. WebSocket upgrade consideration — reduce from 60s polling to real-time

## Merge Plan

Before merging `feature/5m-crypto-trade` → main:
1. Run enhanced monitor 24-48h, collect paper trade results
2. Compare win rate: old 4-component vs new 7-component multi-exchange
3. Squash or cherry-pick (15 commits on branch)
4. Delete remote branch after merge
