# PMCC — Project Context for AI Agents

## What This Is

`polymarket` — a Rust CLI for Polymarket prediction markets.
Supports browsing markets, trading (CLOB), smart money tracking, and 5-minute crypto signal trading.
Binary: `target/release/polymarket`

## Stack

- **Language**: Rust, edition 2024, rust-version 1.88.0
- **Async**: `tokio` (rt-multi-thread)
- **CLI**: `clap` v4 with derive macros
- **SDK**: `polymarket-client-sdk` v0.4.2 — features: gamma, data, bridge, clob, ctf
- **Tables**: `tabled` v0.17 with `Style::rounded()`
- **Errors**: `anyhow` — use `.context()` for context, `.ok_or_else()` for Option conversion (NOT `.context()` on Option)
- **Decimal**: `rust_decimal` (not f64) for money where precision matters
- **Serialization**: `serde` + `serde_json`
- **HTTP**: `reqwest` v0.13 — always set a timeout (5s default)
- **Time**: `chrono` with `DateTime<Utc>`

## Module Structure

```
src/
├── main.rs          — CLI entry, command dispatch via match arms
├── config.rs        — reads ~/.config/polymarket/config.json
├── auth.rs          — private key + signature type resolution
├── shell.rs         — interactive REPL (rustyline)
│
├── commands/        — one file per top-level subcommand
│   ├── mod.rs
│   ├── smart.rs     — LARGEST FILE (~5500 lines): all `polymarket smart *` subcommands
│   │                  discover, watch, list, scan, signals, profile, follow, auto-follow,
│   │                  history, roi, backtest, report, telegram, monitor, crypto (feed/signal/backtest/market/monitor/status)
│   ├── markets.rs, events.rs, tags.rs, series.rs, comments.rs
│   ├── profiles.rs, review.rs, sports.rs, generate.rs
│   ├── approve.rs, clob.rs, ctf.rs, data.rs, bridge.rs
│   ├── wallet.rs, setup.rs, upgrade.rs
│   └── ...
│
├── smart/           — smart money domain logic
│   ├── mod.rs       — all domain structs and enums (WatchedWallet, Signal, AggregatedSignal, etc.)
│   ├── store.rs     — JSON file persistence (~/.config/polymarket/smart/)
│   ├── tracker.rs   — position diffing (detect NEW/CLOSED/INCREASED/DECREASED)
│   ├── signals.rs   — convert position changes → Signal, aggregate signals
│   ├── scorer.rs    — wallet scoring from leaderboard data
│   └── odds.rs      — odds/price monitoring and alerts
│
├── crypto/          — 5-minute crypto trading
│   ├── mod.rs       — CryptoAsset enum (BTC/ETH), Candle struct
│   ├── feed.rs      — Binance klines REST API → Vec<Candle>
│   ├── momentum.rs  — momentum signal computation from candles
│   └── market.rs    — maps crypto assets to Polymarket condition_ids
│
└── output/          — presentation layer (table + JSON for each domain)
    ├── mod.rs       — OutputFormat enum, truncate(), print_json()
    ├── smart.rs     — print_discover_results, print_wallet_list, print_signals, etc.
    └── ... (one file per domain)
```

## Key Patterns

### Command execute() signature
Commands have different signatures depending on what they need:
```rust
// Standard market query (gamma client)
pub async fn execute(client: &gamma::Client, args: XArgs, output: OutputFormat) -> anyhow::Result<()>

// Smart command (needs data + gamma clients, private key, signature type)
pub async fn execute(
    data_client: &data::Client,
    gamma_client: &gamma::Client,
    args: SmartArgs,
    output: OutputFormat,
    private_key: Option<&str>,
    signature_type: Option<&str>,
) -> anyhow::Result<()>
```

### Error handling
```rust
// Use context() for Results:
let data = fs::read_to_string(&path).context("Failed to read wallets")?;

// Use ok_or_else() for Options (NOT .context()):
let val = opt_val.ok_or_else(|| anyhow::anyhow!("value missing"))?;

// Never silently discard errors — log them:
if let Err(e) = store::save_wallets(&wallets) {
    eprintln!("Warning: failed to save wallets: {e}");
}
```

### SDK usage
```rust
// Clients are constructed as default():
let gamma = polymarket_client_sdk::gamma::Client::default();
let data  = polymarket_client_sdk::data::Client::default();

// CLOB orders use OrderType::FOK (not Fok):
use polymarket_client_sdk::clob::OrderType;
OrderType::FOK
```

### Data storage (smart module)
All smart money data lives in `~/.config/polymarket/smart/`:
- `wallets.json` — watched wallets list
- `signals.json` — historical signals
- `snapshots/{address}.json` — per-wallet position snapshots
- `follow_records.json` — paper trade follow log
- `monitor.json` — monitor loop config
- `telegram.json` — Telegram bot config (bot_token, chat_id)

Store functions use **atomic writes** (write to `.tmp` then rename):
```rust
fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
```

### Output pattern (table + JSON)
```rust
pub fn print_something(items: &[MyType], output: &OutputFormat) -> anyhow::Result<()> {
    match output {
        OutputFormat::Table => {
            #[derive(Tabled)]
            struct Row { /* ... */ }
            let rows: Vec<Row> = items.iter().map(|i| Row { /* ... */ }).collect();
            let table = Table::new(rows).with(Style::rounded()).to_string();
            println!("{table}");
        }
        OutputFormat::Json => super::print_json(&items)?,
    }
    Ok(())
}
```

### macOS notifications
Use `osascript_safe()` which strips `"`, `\`, newlines, and null bytes to prevent injection:
```rust
fn osascript_safe(s: &str) -> String {
    s.chars().filter(|c| *c != '"' && *c != '\\' && *c != '\n' && *c != '\0').collect()
}
// Then:
let _ = std::process::Command::new("osascript")
    .args(["-e", &format!(r#"display notification "{}" with title "polymarket""#, osascript_safe(&msg))])
    .spawn();
```

### HTTP clients — always set timeout
```rust
let client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(5))
    .build()?;
```

### Telegram safety
Strip Markdown special chars before sending:
```rust
fn telegram_safe(s: &str) -> String {
    s.chars().filter(|c| !"*_`[]".contains(*c)).collect()
}
```

## Domain Structs (smart/mod.rs)

```rust
WatchedWallet   — tracked wallet (address, tag, score, stale, disabled)
Signal          — single wallet position change event
AggregatedSignal — multiple wallets converging on same trade
SignalConfidence — High / Medium / Low (based on score + size)
SignalType      — NewPosition / ClosePosition / IncreasePosition / DecreasePosition
SmartScore      — leaderboard entry (rank, address, pnl, volume, score)
FollowRecord    — paper trade record (entry price, stop-loss, status)
MonitorConfig   — monitor loop settings (interval, min_wallets, thresholds)
OddsWatch       — market odds monitoring subscription
TelegramConfig  — { bot_token: String, chat_id: String }
```

## Current Sprint Status

- Sprint 1-10: COMPLETE (smart money tracking)
- Sprint 11 Phase A+B: COMPLETE (5m crypto signals — BTC/ETH via Binance)
- Sprint 11 Phase C: NEXT (signal tuning, OKX/DEX feeds, WebSocket)
- Branch: `feature/5m-crypto-trade` (current)

## Known Constraints

- `smart.rs` in commands/ is ~5500 lines — new subcommands go here, match on `SmartCommand` enum
- Paper trading only — no real order execution in `auto-follow` (uses `FollowRecord` with `TradeStatus`)
- Binance klines are REST (no WebSocket yet) — Phase C adds WebSocket
- `polymarket smart monitor` is a long-running loop — use `tokio::time::sleep` between cycles
- Prices/sizes are stored as `String` (from SDK) — parse with `.parse::<f64>()` when computing
