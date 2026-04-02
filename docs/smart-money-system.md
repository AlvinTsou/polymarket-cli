# Polymarket Smart Money Tracking & Crypto Trading System

> Built on `polymarket-cli` — smart wallet tracking, signal aggregation, paper trading, and 5-minute crypto prediction.

## System Overview

```
┌─────────────────────────────────────────────────────────────┐
│                  Smart Money Pipeline                        │
│                                                              │
│  Data Collection                                             │
│  ┌────────────┐  ┌──────────────┐  ┌───────────────┐        │
│  │ Whale       │  │ Market-first │  │ Odds Monitor  │        │
│  │ Discovery   │  │ Discovery    │  │ (conditions)  │        │
│  └──────┬──────┘  └──────┬───────┘  └───────┬───────┘        │
│         │                │                  │                │
│  Analysis                                                    │
│  ┌───────────────────────────────────────────────────┐       │
│  │  Smart Scoring → Signal Aggregation → Paper Trade │       │
│  └──────────────────────┬────────────────────────────┘       │
│                         │                                    │
│  Execution                                                   │
│  ┌───────────┐  ┌──────────────┐  ┌───────────────┐         │
│  │ Telegram   │  │ Paper Trade  │  │ Dashboard     │         │
│  │ + macOS    │  │ (stop-loss)  │  │ (localhost)   │         │
│  └───────────┘  └──────────────┘  └───────────────┘         │
│                                                              │
│  Crypto Module (Sprint 11)                                   │
│  ┌────────────┐  ┌──────────────┐  ┌───────────────┐        │
│  │ Binance    │  │ Momentum     │  │ 5m Market     │        │
│  │ Feed       │  │ Signal       │  │ Paper Trade   │        │
│  └────────────┘  └──────────────┘  └───────────────┘        │
└─────────────────────────────────────────────────────────────┘
```

---

## Modules

### Smart Money (`src/smart/`)

| File | Purpose |
|------|---------|
| `mod.rs` | Types, exports |
| `store.rs` | JSON file storage (`~/.config/polymarket/smart/`) |
| `tracker.rs` | Wallet position snapshot diffing |
| `scorer.rs` | Smart score = win_rate × 0.30 + ROI × 0.35 + frequency × 0.15 + diversity × 0.20 |
| `signals.rs` | Signal detection: NewPosition, Increase, Decrease, Close |
| `odds.rs` | Odds change monitoring with condition triggers |

### Crypto (`src/crypto/`)

| File | Purpose |
|------|---------|
| `mod.rs` | Types (Candle, OrderBook, Trade, MomentumSignal, Direction) |
| `feed.rs` | Binance REST API (klines, depth, trades) |
| `momentum.rs` | Signal: price momentum + order book imbalance + trade flow |
| `market.rs` | Polymarket gamma search for 5-minute BTC/ETH markets |

### Commands & Output

| File | Lines | Purpose |
|------|-------|---------|
| `src/commands/smart.rs` | ~5050 | All smart + crypto CLI commands |
| `src/output/smart.rs` | — | Table/JSON rendering for signals, paper trades, dashboard |

---

## CLI Commands

### Wallet Discovery

```bash
polymarket smart discover --period month --limit 50
polymarket smart discover --min-roi 0.5 --min-trades 20
polymarket smart discover-markets           # find markets, then find wallets in them
polymarket smart discover-whales            # discover from market holders
polymarket smart discover-auto              # combined pipeline
```

### Wallet Tracking

```bash
polymarket smart watch 0xADDRESS
polymarket smart unwatch 0xADDRESS
polymarket smart list
polymarket smart profile 0xADDRESS
```

### Signal Scanning

```bash
polymarket smart scan                       # scan all watched wallets
polymarket smart scan --wallet 0xADDRESS
polymarket smart signals                    # recent signals
polymarket smart signals --market "bitcoin"
```

### Follow Trading (Paper)

```bash
polymarket smart follow 0xADDRESS           # manual follow trade
polymarket smart auto-follow --max-per-trade 50 --max-daily 200
polymarket smart history                    # trade history
polymarket smart roi                        # PnL + ROI summary
polymarket smart backtest                   # historical backtest
```

### Monitor (Real-time)

```bash
polymarket smart monitor \
  --interval 3m --min-confidence med --min-wallets 1 \
  --notify --paper-trade --amount 10 --max-per-day 50 --save

polymarket smart monitor --load             # resume saved config
```

### Dashboard

```bash
polymarket smart report                     # HTML dashboard
# Dashboard auto-served at localhost:3456 via LaunchAgent
```

### Telegram

```bash
polymarket smart telegram                   # configure bot
```

### Crypto (5-Minute Trading)

```bash
polymarket smart crypto feed                # live BTC/ETH from Binance
polymarket smart crypto signal              # current momentum signal
polymarket smart crypto backtest            # 24h historical accuracy
polymarket smart crypto market              # find next 5m Polymarket market
polymarket smart crypto monitor             # auto paper trade loop (60s)
polymarket smart crypto status              # crypto paper trade PnL
```

---

## Paper Trading Features

- **Stop-loss**: -45% (configurable)
- **Trailing stop**: peak +30%, drawdown 50%
- **Entry/exit reason tracking**: per-trade audit trail
- **Confirmation delay**: 10-minute queue for triggers
- **Price filter**: 0.15-0.80 range
- **Anti-hedge**: skip opposing positions
- **Exclude keywords**: 98 noise filters (sports, etc.)
- **30-day horizon filter**: skip near-expiry markets
- **Rate limit**: per-hour and per-day caps

---

## Deployment

Services run via macOS LaunchAgent (see `docs/deployment.md`):

| Service | Label | Port |
|---------|-------|------|
| Dashboard | `com.pmcc.dashboard` | `localhost:3456` |
| Monitor | `com.pmcc.monitor` | — |

---

## Data Storage

```
~/.config/polymarket/smart/
├── wallets.json          # watched wallets
├── follows.jsonl         # paper trade records
├── signals.jsonl         # signal history
├── scores.json           # smart score cache
├── monitor.json          # monitor config (--save/--load)
├── telegram.json         # bot token + chat_id
├── snapshots/            # position snapshots per wallet
├── dashboard.log         # dashboard service log
└── monitor.log           # monitor service log
```

---

## Sprint History

| Sprint | Focus | Key Commits |
|--------|-------|-------------|
| S1 | Smart money wallet tracking | `2a4b692` |
| S2 | Signal aggregation + macOS notify + Telegram | `a79b374`, `71563b7` |
| S3 | Follow trading with safety limits | `03a1458` |
| S4 | ROI tracking, backtesting, HTML dashboard | `cb0182f` |
| S5 | Odds monitoring + worker hardening | `ae5d27f` |
| S6 | Precise P&L tracking, trade history visualization | `15a6fc3` |
| S7 | Real-time monitor with condition triggers + paper trading | `04c09bc` |
| S8 | Wallet intelligence — auto-renew, PnL tracking, trade analysis | `e9162f5` |
| S9 | Market-first whale discovery | `1a286f3` |
| S10 | Exchange-style tabbed paper trade dashboard | `869ff9b` |
| S11 | 5-minute crypto trading (Binance momentum → Polymarket) | Phase A+B complete |

---

## Known Issues

See `tasks/issues.md` for current bug list (Night Shift R114 findings):
- P0: `bot_token[..10]` panic, `signals[0]` panic, osascript injection, division-by-zero
- P1: `let _` silent data loss (14 occurrences), `cancelled_keys` unbounded growth, `addr[..8]` panic
- P2: Confirmation delay uses stale trigger price
