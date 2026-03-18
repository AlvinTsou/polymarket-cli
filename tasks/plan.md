# Sprint 7: Real-time Monitor with Condition Triggers + Paper Trading

## Goal

A long-running `smart monitor` command that:
1. Continuously scans wallets + odds at configurable intervals
2. Evaluates signals against user-defined trigger conditions
3. Automatically places paper trades (dry-run) when conditions are met
4. Sends notifications on trigger + paper trade
5. Persists monitor config for easy restart

## CLI Interface

```
polymarket smart monitor \
  --interval 3m \
  --min-confidence med \
  --min-wallets 2 \
  --market-include "election,AI,crypto" \
  --market-exclude "sports" \
  --odds-threshold 5.0 \
  --paper-trade \
  --amount 10 \
  --max-per-day 100 \
  --notify \
  --save
```

### Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--interval` | `5m` | Scan interval (e.g. `1m`, `3m`, `10m`) |
| `--min-confidence` | `med` | Minimum signal confidence to trigger |
| `--min-wallets` | `1` | Minimum wallet convergence count |
| `--market-include` | (none) | Comma-separated keywords, trigger only if market title matches any |
| `--market-exclude` | (none) | Comma-separated keywords, skip if market title matches any |
| `--odds-threshold` | `0` | Also trigger on odds change >= this % (0 = disabled) |
| `--paper-trade` | `false` | Auto-create dry-run FollowRecords on trigger |
| `--amount` | `10` | USDC per paper trade |
| `--max-per-day` | `50` | Daily paper trade spending cap |
| `--notify` | `false` | macOS + Telegram notifications on trigger |
| `--save` | `false` | Persist these settings to monitor.json for next run |
| `--load` | `false` | Load saved settings from monitor.json |

## Architecture

```
cmd_monitor()
  │
  ├─ Load/parse MonitorConfig
  ├─ Print config summary
  │
  └─ Loop (tokio::time::interval)
       │
       ├─ scan_wallets() ──→ Vec<Signal>
       ├─ scan_odds()     ──→ Vec<OddsAlert>
       │
       ├─ evaluate_triggers(signals, alerts, config)
       │    ├─ Filter by min_confidence
       │    ├─ Filter by min_wallets (aggregation check)
       │    ├─ Filter by market_include / market_exclude
       │    ├─ Check odds_threshold alerts
       │    └─ Return Vec<TriggerEvent>
       │
       ├─ If paper_trade && triggers not empty:
       │    ├─ Check daily spend cap
       │    ├─ Create dry-run FollowRecords (reuse existing logic)
       │    └─ Close positions if ClosePosition detected
       │
       ├─ If notify && triggers not empty:
       │    ├─ macOS notification
       │    └─ Telegram with trigger summary + paper trade details
       │
       └─ Print cycle summary to stdout
            "Cycle #N: 3 signals, 1 trigger, 1 paper trade | next scan in 3m"
```

## Data Types

```rust
// ~/.config/polymarket/smart/monitor.json
struct MonitorConfig {
    interval_secs: u64,
    min_confidence: SignalConfidence,
    min_wallets: u32,
    market_include: Vec<String>,
    market_exclude: Vec<String>,
    odds_threshold: f64,
    paper_trade: bool,
    amount: f64,
    max_per_day: f64,
    notify: bool,
}

struct TriggerEvent {
    trigger_type: TriggerType,  // Signal | OddsAlert | Aggregated
    confidence: SignalConfidence,
    market_title: String,
    outcome: String,
    direction: SignalDirection,
    price: f64,
    wallet_count: u32,
    reason: String,  // "HIGH confidence from whale_01" or "3 wallets converge on YES"
}

enum TriggerType {
    Signal,
    Aggregated,
    OddsAlert,
}
```

## Implementation Steps

### Step 1: Add tokio `time` + `net` features to Cargo.toml
- `tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "net"] }`
- `net` already implicitly available from dashboard, but explicit is better

### Step 2: MonitorConfig type + store
- Add `MonitorConfig` to `mod.rs`
- Add `load_monitor_config()` / `save_monitor_config()` to `store.rs`

### Step 3: Parse interval duration
- Helper to parse "1m", "3m", "5m", "30s" etc into Duration

### Step 4: TriggerEvent + evaluate_triggers()
- Core rules engine: filter signals by config conditions
- Check aggregated signals for min_wallets
- Check odds alerts for threshold

### Step 5: CLI subcommand definition
- Add `Monitor { ... }` variant to `SmartCommand`
- Wire up in `execute()`

### Step 6: cmd_monitor() implementation
- The main loop using `tokio::time::interval`
- Reuses `tracker::scan_wallet`, `signals::generate_signals`, `odds::scan_odds`
- Paper trade via existing `store::append_follow_record` with dry_run=true
- Closure detection via `store::close_follow_position`
- Graceful Ctrl+C handling via `tokio::signal::ctrl_c`

### Step 7: Notification formatting
- Build monitor-specific Telegram message with trigger reasons
- macOS notification with summary

### Step 8: Build + Test + launchd plist

## Non-goals

- Web frontend (dashboard already exists)
- VPS deployment scripts (user can tmux/launchd)
- Real order execution (paper-trade only in monitor mode)
- WebSocket streaming (polling is simpler and sufficient for 1-5m intervals)
