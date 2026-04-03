
# PMCC Issues — Night Shift R114 + R03:15

## ALL RESOLVED

### Crashes & Security (P0)

| ID | Issue | Fix | Commit |
|----|-------|-----|--------|
| PM-C01 | stop-loss div-by-zero | `if r.amount_usdc > 0.0` guard | `696ed1c` |
| PM-C02 | bot_token[..10] panic | `.get(..10).unwrap_or()` | `696ed1c` |
| PM-C03 | signals[0] panic | early return + `.first()` | `696ed1c` |
| PM-C04 | osascript injection | `osascript_safe()` strips `"` `\` newlines null | `696ed1c` |
| PM-C05 | HTTP no timeout | 5s timeout on all Client builders | `64530aa` |

### Data Integrity (P0/P1)

| ID | Issue | Fix | Commit |
|----|-------|-----|--------|
| PM-D01 | store.rs non-atomic writes | `atomic_write()` — tmp file + rename | `dd789ce` |
| PM-D02 | let _ silent data loss (14x) | 10 store ops → `if let Err(e)` with eprintln | `696ed1c` |
| PM-D03 | cancelled_keys unbounded growth | `HashMap<K, DateTime>` + 1h prune | `696ed1c` |
| PM-D04 | addr[..8] slice panic | `if addr.len() >= 12` guard | `696ed1c` |
| PM-D05 | stale trigger price (10m old) | re-fetch from `store::current_price_map()` | `696ed1c` |

### Reports & Formatting (P1/P2)

| ID | Issue | Fix | Commit |
|----|-------|-----|--------|
| PM-R01 | P&L uses signal price not execution price | Resolved by PM-D05 fix (current price used) | `696ed1c` |
| PM-R02 | Telegram Markdown parse failures | `telegram_safe()` strips `*_\`[]` | `64530aa` |
| PM-R03 | format_money negative numbers | Sign prefix before `$` (-$1.2K not $-1.2K) | `dd789ce` |
| PM-R04 | Dashboard no API routing | **WONTFIX** — single-page dashboard, acceptable | — |

### Operations (P2)

| ID | Issue | Fix | Commit |
|----|-------|-----|--------|
| PM-O01 | Log files unbounded growth | `prune_signals(5000)` + `prune_odds_alerts_log(2000)` every 100 cycles | `dd789ce` |

### Code Review (64530aa)

| Severity | Issue | Fix | Commit |
|----------|-------|-----|--------|
| HIGH | Funding rate zero-dilution on Binance failure | Only average successful exchanges | `64530aa` |
| HIGH | Backtest hours*60 u32 overflow | Widen to u64 before multiply | `64530aa` |
| MEDIUM | OKX OI 100x inflated (wrong contract size) | BTC=0.01, ETH=0.1 per contract | `64530aa` |
| LOW | unused ex_count, dead has_futures branch | Removed | `64530aa` |
