# Sprint 12: Multi-Exchange Signal Integration — Architecture Analysis

## Problem

Current 5m crypto signal uses **Binance spot only**. BTC backtest: 55.3% win rate — marginal edge. The signal misses:

1. Leveraged positioning (futures OI, funding rates)
2. Cross-venue order flow consensus (or divergence)
3. Liquidation cascades that drive 5m price action
4. DEX whale activity (Hyperliquid is now top 3 by volume)

## Current Architecture

```
BinanceFeed (REST)
├── fetch_klines("1m", 30)     → 30 candles
├── fetch_depth(20)            → 20-level order book
└── fetch_trades(500)          → 500 recent trades
         │
         ▼
compute_signal()
├── price_mom_1m   × 0.30     (1m return)
├── price_mom_5m   × 0.25     (5m return)
├── ob_imbalance   × 0.25     (bid_vol - ask_vol) / total
└── trade_flow     × 0.20     (buy_vol - sell_vol) / total
         │
         ▼
Direction: UP / DOWN / SKIP
Confidence: |raw_score| / 0.30, capped at 1.0
```

**Gaps:**
- Single exchange = single-point-of-view bias
- Spot only = blind to leveraged sentiment (futures drive BTC 5m moves)
- No funding/OI = can't detect crowded trades or squeeze potential
- No liquidation awareness = misses cascading events

---

## Proposed Architecture

### Layer 1: Exchange Feeds (trait-based)

```
trait ExchangeFeed: Send + Sync {
    fn name(&self) -> &str;
    fn weight(&self) -> f64;  // volume-based importance

    async fn fetch_orderbook(&self, asset: CryptoAsset) -> Result<OrderBook>;
    async fn fetch_trades(&self, asset: CryptoAsset) -> Result<Vec<Trade>>;

    // Optional — not all exchanges have these
    async fn fetch_funding_rate(&self, asset: CryptoAsset) -> Result<Option<f64>>;
    async fn fetch_open_interest(&self, asset: CryptoAsset) -> Result<Option<f64>>;
    async fn fetch_liquidations(&self, asset: CryptoAsset) -> Result<Vec<Liquidation>>;
}
```

### Layer 2: Aggregator (parallel fetch + merge)

```
struct MultiExchangeFeed {
    feeds: Vec<Box<dyn ExchangeFeed>>,
}

impl MultiExchangeFeed {
    async fn fetch_all(&self, asset: CryptoAsset) -> AggregatedData {
        // tokio::join! all feeds in parallel
        // Timeout per exchange: 5s (fail gracefully)
        // Merge results weighted by exchange volume
    }
}
```

```
struct AggregatedData {
    // Existing (now cross-exchange)
    candles: Vec<Candle>,           // Binance only (reference price)
    merged_orderbook: MergedBook,   // volume-weighted bid/ask from all
    merged_trades: Vec<Trade>,      // combined recent trades

    // NEW signals
    funding_rates: Vec<(String, f64)>,   // [(exchange, rate), ...]
    avg_funding_rate: f64,               // volume-weighted average
    total_open_interest: f64,            // sum across exchanges
    oi_delta_5m: f64,                    // OI change over last 5m
    recent_liquidations: Vec<Liquidation>,
    liquidation_imbalance: f64,          // net long vs short liquidations
}
```

### Layer 3: Enhanced Signal Model

```
// Current (4 components, Binance only)
raw_score = 0.30×mom_1m + 0.25×mom_5m + 0.25×ob + 0.20×flow

// Proposed (7 components, multi-exchange)
raw_score =
    0.15 × price_mom_1m              // reduced (less unique with more data)
    0.10 × price_mom_5m              // reduced
    0.20 × agg_ob_imbalance          // cross-exchange order book
    0.20 × agg_trade_flow            // cross-exchange trade flow
    0.15 × funding_signal            // NEW: funding rate sentiment
    0.10 × oi_delta_signal           // NEW: OI momentum
    0.10 × liquidation_signal        // NEW: liquidation cascade
```

**New signal components explained:**

| Component | Calculation | Meaning |
|-----------|-------------|---------|
| `agg_ob_imbalance` | Volume-weighted avg of per-exchange OB imbalance | Cross-venue buy/sell pressure consensus |
| `agg_trade_flow` | Sum(buy_vol) - Sum(sell_vol) across all exchanges / total | Net aggressive buying direction |
| `funding_signal` | -1 × normalized(avg_funding_rate) | Contrarian: extreme +funding → short squeeze → DOWN signal. Extreme -funding → long squeeze → UP signal |
| `oi_delta_signal` | sign(price_mom) × normalized(oi_change) | OI increasing with price = momentum continuation. OI decreasing = exhaustion |
| `liquidation_signal` | (long_liq_vol - short_liq_vol) / total_liq_vol | Net short liquidations = price going up (forced buying). Net long liquidations = price going down |

---

## Exchange Selection & Priority

### Tier 1 — Must Have (highest impact for 5m)

| Exchange | Type | Why | Data |
|----------|------|-----|------|
| **Binance Spot** | CEX | Already integrated, reference price | Candles, OB, trades |
| **Binance Futures** | CEX | #1 futures volume, funding/OI/liquidations | Funding, OI, liquidations |
| **OKX** | CEX | #2 by BTC volume, different user base (Asia-heavy) | OB, trades, funding, OI, liquidations |
| **Hyperliquid** | DEX | #1 perpetual DEX, whale-heavy, on-chain transparent | OB, trades, funding, OI |

### Tier 2 — Good to Have

| Exchange | Type | Why | Data |
|----------|------|-----|------|
| **Bybit** | CEX | #3 by volume, retail-heavy | OB, trades, funding, OI |
| **dYdX v4** | DEX | Decentralized order book, different liquidity profile | OB, trades, funding |

### Tier 3 — Optional (diminishing returns)

- Coinbase (US-centric, premium indicator)
- Kraken, Bitfinex (lower volume)

**Recommendation: Start with Tier 1 (4 feeds), add Tier 2 later.**

---

## API Details & Constraints

### No API Key Required (all public endpoints)

| Exchange | Rate Limit | Latency | Notes |
|----------|-----------|---------|-------|
| Binance Spot | 2400 wt/min | ~50ms | Already integrated |
| Binance Futures | 2400 wt/min | ~50ms | Same IP pool as spot (shared limit) |
| OKX | 20 req/2s | ~100ms | All REST, strings |
| Hyperliquid | 1200 req/min | ~150ms | **POST /info** (all endpoints), JSON body |
| Bybit | 10-50 req/s | ~80ms | Unified v5 API |
| dYdX v4 | 100 req/10s | ~200ms | Cosmos indexer, ticker=BTC-USD |

### Per-Cycle Budget (60s interval)

Each cycle needs per exchange: OB(1) + trades(1) + funding(1) + OI(1) = 4 requests
Per asset (BTC only initially): 4 × 4 exchanges = 16 requests
With BTC+ETH: 32 requests total

At 60s interval, well within all rate limits.

### Parallel Fetch Strategy

```
// All exchanges fetched concurrently
let (binance, binance_f, okx, hl) = tokio::join!(
    binance_spot.fetch_all(asset),      // ~50ms
    binance_futures.fetch_all(asset),   // ~50ms
    okx.fetch_all(asset),              // ~100ms
    hyperliquid.fetch_all(asset),      // ~150ms
);
// Total latency: ~150ms (bounded by slowest)
```

---

## File Structure

```
src/crypto/
├── mod.rs              # Types: CryptoAsset, Direction, MomentumSignal, +new types
├── feed.rs             # trait ExchangeFeed + BinanceFeed (existing, refactored)
├── feeds/
│   ├── mod.rs          # re-exports
│   ├── binance.rs      # BinanceFeed (spot) — extracted from feed.rs
│   ├── binance_fut.rs  # BinanceFuturesFeed (fapi)
│   ├── okx.rs          # OkxFeed
│   └── hyperliquid.rs  # HyperliquidFeed
├── aggregator.rs       # MultiExchangeFeed — parallel fetch, merge, weighted combine
├── momentum.rs         # compute_signal() — updated with 7-component model
├── market.rs           # find_next_5m_market() (unchanged)
└── types.rs            # New types: FundingRate, OpenInterest, Liquidation, AggregatedData, MergedBook
```

## New Types

```rust
struct FundingRate {
    exchange: String,
    rate: f64,           // per-period rate (normalize to 8h)
    next_time: Option<i64>,
}

struct OpenInterest {
    exchange: String,
    oi_usd: f64,         // total OI in USD
    timestamp: i64,
}

struct Liquidation {
    exchange: String,
    side: String,        // "LONG" or "SHORT"
    size_usd: f64,
    price: f64,
    timestamp: i64,
}

struct MergedBook {
    bid_volume: f64,     // total bid volume across exchanges
    ask_volume: f64,     // total ask volume across exchanges
    imbalance: f64,      // (bid - ask) / total, volume-weighted
    spread_bps: f64,     // average spread in basis points
}

struct AggregatedData {
    candles: Vec<Candle>,
    merged_book: MergedBook,
    all_trades: Vec<Trade>,
    funding_rates: Vec<FundingRate>,
    open_interests: Vec<OpenInterest>,
    liquidations: Vec<Liquidation>,
    exchange_count: u32,       // how many succeeded
    failed_exchanges: Vec<String>,
}

struct EnhancedSignalComponents {
    // Existing
    price_mom_1m: f64,
    price_mom_5m: f64,
    volatility: f64,
    // Enhanced (cross-exchange)
    agg_ob_imbalance: f64,
    agg_trade_flow: f64,
    // New
    funding_signal: f64,
    oi_delta_signal: f64,
    liquidation_signal: f64,
    // Metadata
    exchanges_used: u32,
    raw_score: f64,
}
```

---

## Implementation Phases

### Phase 1: Trait + Binance Futures (lowest effort, highest impact)

Futures data (funding, OI, liquidations) adds the most new information vs. just adding more spot order books.

1. Define `ExchangeFeed` trait
2. Refactor existing `BinanceFeed` to implement trait
3. Add `BinanceFuturesFeed` (fapi endpoints)
4. Update `compute_signal()` with funding + OI + liquidation components
5. Backtest: compare 4-component vs 7-component win rate

**Estimated new code:** ~300 lines
**Expected improvement:** +3-5% win rate (funding/OI are strong 5m predictors)

### Phase 2: OKX + Hyperliquid

Cross-exchange consensus amplifies weak signals and reduces false positives.

1. Add `OkxFeed` (spot + futures, REST)
2. Add `HyperliquidFeed` (POST /info pattern)
3. Build `MultiExchangeFeed` aggregator
4. Volume-weighted merge of order books and trade flow
5. Backtest: compare single-exchange vs multi-exchange

**Estimated new code:** ~500 lines
**Expected improvement:** +2-3% win rate (consensus filtering)

### Phase 3: Bybit + Tuning

1. Add `BybitFeed`
2. Tune signal weights via backtest grid search
3. Add exchange health monitoring (latency tracking, auto-disable slow feeds)
4. Consider WebSocket upgrade for lower latency

**Estimated new code:** ~300 lines
**Expected improvement:** +1-2% (diminishing returns, but better robustness)

---

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Exchange API down | Missing data for one source | Graceful fallback: if < 2 exchanges respond, use Binance-only (current behavior) |
| Rate limiting | 429 errors | Per-exchange backoff, request budget tracking |
| Data inconsistency | Price discrepancy between exchanges | Use Binance as reference price, normalize others as delta |
| Overfitting weights | Good backtest, bad live | Walk-forward optimization, out-of-sample testing window |
| Funding rate noise | 8h cycle irrelevant for 5m | Use as contrarian filter only when extreme (> 2 stddev) |
| Latency regression | Slower cycle if one exchange is slow | Per-exchange timeout (5s), don't wait for all |

---

## Decision: Phase 1 First

**Binance Futures** integration gives the best signal improvement per effort:
- Same API style as existing Binance spot (minimal new code)
- Shared IP rate limit pool (already within budget)
- Funding + OI + Liquidations = 3 entirely new signal dimensions
- No API key needed

After Phase 1 backtest validates the expanded model, proceed to Phase 2 (OKX + Hyperliquid) for cross-exchange consensus.
