# PMCC Strategy Result Ledger

Last updated: 2026-06-09

This is the canonical performance ledger for PMCC strategies. It records sample size, WR, PnL/ROI, disable criteria, and the evidence needed before promoting or expanding a strategy.

## Refresh Commands

Run these before changing entry logic or enabling real-money execution:

```bash
./target/release/polymarket --output json smart roi --period all --status all
./target/release/polymarket --output json smart crypto status
./target/release/polymarket --output json smart reconcile --dry-run
```

## Latest Refresh: 2026-06-09

| Command | Result |
|---------|--------|
| `smart roi --period all --status all` | 8 closed Smart Money paper trades, 50.0% WR, realized PnL `-$2.9992`, total ROI `-3.7490%`, no unrealized PnL |
| `smart crypto status` | No crypto paper trades in the current local store |
| `smart reconcile --dry-run` | `scanned_open_dry_run=0`, `closed=0`, `errors=0`, `total_pnl=0.0` |

## Current Ledger

| Strategy | Mode | Latest Evidence | Sample | WR | PnL / ROI | Decision | Disable / Review Criteria |
|----------|------|-----------------|--------|----|-----------|----------|---------------------------|
| Smart Money Multi-Wallet Convergence | Active paper-trade entry | 2026-06-09 CLI refresh after 2026-05-05 follow-up and 2026-05-25 zombie correction | Current local store: 8 closed; historical context: 632 closed in 2026-05-05 follow-up | Current: 50.0%; historical: 66% in 2026-05-05 follow-up | Current: `-$2.9992` / `-3.7490%`; historical zombie-corrected total: +$197.42 / +1.51% on 2026-05-25 | Keep active, but do not add new Smart Money entry experiments on an 8-trade current sample | Review if refreshed sample reaches 50+ closed and ROI <= 0%, WR < 52%, or settled-market reconcile finds recurring zombie positions |
| Self-Managed Smart Money Exit Manager | Active exit strategy | 2026-06-09 reconcile dry-run found no open dry-run positions to settle | Current local store: 0 scanned open dry-run positions | N/A | Reconcile delta: $0.00 | Keep active; no zombie-position correction needed in the current local store | Review if market-closed sweep repeatedly changes reported PnL materially, or if pre-resolution losses remain common |
| Crypto 5m Multi-Exchange Momentum | Active experimental paper-trade strategy | 2026-06-09 `smart crypto status` returned no crypto paper trades; historical 2026-05-05 follow-up remains the last measured sample | Current local store: 0; historical: 204 closed | Current: N/A; historical: 50% | Current: N/A; historical: +$22.50 / +0.9% | Do not expand until new crypto trades exist; old 5m results are stale after later pivot and sizing/toxicity work | Disable or redesign if refreshed sample >= 100 closed and WR < 55% or ROI is not positive after realistic fees |
| Crypto daily range/strike pivot | Active experimental paper-trade strategy | 2026-06-09 `smart crypto status` returned no crypto paper trades | 0 | N/A | N/A | Needs live/paper sample before any judgment | Require separate sample bucket before using old crypto results as evidence |
| Binary Complement Arbitrage Scanner | Implemented scanner | Scanner implemented, no auto-execution ledger yet | N/A | N/A | N/A | Keep scanner-only until execution model is measured | Do not auto-execute without fee, slippage, stale-book, and capital-lock checks |
| Favorite-Longshot Bias Scanner | Implemented scanner | Scanner implemented, no auto-execution ledger yet | N/A | N/A | N/A | Keep scanner-only until historical candidate outcomes are measured | Do not trade without band-specific WR/ROI and liquidity constraints |
| Odds Momentum Alert | Active alert strategy | Notification-only strategy | N/A | N/A | N/A | Keep as alert-only | Promote only if alert outcomes are logged with later market movement |
| Whale-Exit Fade Experiment | Queued paper-only entry experiment | No result sample | 0 | N/A | N/A | Build only after current Smart Money ledger supports another entry experiment | Disable if first 100 closed paper trades have WR < 55% or ROI <= 0% |
| CLOB Midpoint Crypto Component | Queued signal filter | No result sample | 0 | N/A | N/A | Build only if refreshed crypto ledger shows a weak but salvageable edge | Skip if refreshed crypto strategy is structurally negative after fees |
| Strategy Sizing / Toxicity Research | Research / sizing layer | `41861c2` added sizing and toxicity research core | Needs experiment design | N/A | N/A | Treat as a risk layer, not a standalone alpha source | Require controlled before/after comparison before enabling by default |

## Evidence Backlog

1. Accumulate a larger current Smart Money sample before deciding on `Whale-Exit Fade Experiment`.
2. Restart or verify crypto paper trading if the daily range/strike pivot is still desired.
3. Re-run the refresh commands after at least 50 Smart Money closed trades or 100 crypto closed trades.
4. Only after the refreshed sample is meaningful, decide whether `Whale-Exit Fade Experiment` or `CLOB Midpoint Crypto Component` deserves implementation time.
