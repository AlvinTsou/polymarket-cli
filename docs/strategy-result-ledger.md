# PMCC Strategy Result Ledger

Last updated: 2026-06-09

This is the canonical performance ledger for PMCC strategies. It records sample size, WR, PnL/ROI, disable criteria, and the evidence needed before promoting or expanding a strategy.

## Refresh Commands

Run these before changing entry logic or enabling real-money execution:

```bash
polymarket --output json smart roi --period all --status all
polymarket --output json smart crypto status
polymarket --output json smart reconcile --dry-run
```

## Current Ledger

| Strategy | Mode | Latest Evidence | Sample | WR | PnL / ROI | Decision | Disable / Review Criteria |
|----------|------|-----------------|--------|----|-----------|----------|---------------------------|
| Smart Money Multi-Wallet Convergence | Active paper-trade entry | 2026-05-05 follow-up plus 2026-05-25 zombie correction | 632 closed in 2026-05-05 follow-up; 1240 closed dashboard count before zombie correction | 66% in 2026-05-05 follow-up; 60% in 2026-05-25 dashboard snapshot | +$276.13 / +4.4% in 2026-05-05 follow-up; zombie-corrected total +$197.42 / +1.51% on 2026-05-25 | Keep active, but refresh before adding new Smart Money entry experiments | Review if refreshed ROI <= 0%, WR < 52%, or settled-market reconcile finds recurring zombie positions |
| Self-Managed Smart Money Exit Manager | Active exit strategy | Deployed rules plus settled-market reconciliation | Shared with Smart Money paper-trade book | Needs refresh | Needs refresh | Keep active; it fixed a real accounting failure mode | Review if market-closed sweep repeatedly changes reported PnL materially, or if pre-resolution losses remain common |
| Crypto 5m Multi-Exchange Momentum | Active experimental paper-trade strategy | 2026-05-05 follow-up before later crypto pivot and sizing/toxicity work | 204 closed | 50% | +$22.50 / +0.9% | Do not expand until refreshed; current queue should validate before CLOB 8th component work | Disable or redesign if refreshed sample >= 100 closed and WR < 55% or ROI is not positive after realistic fees |
| Crypto daily range/strike pivot | Active experimental paper-trade strategy | Code landed after 2026-05-05 ledger baseline | Needs refresh | Needs refresh | Needs refresh | Measure separately from old 5m market behavior | Require separate sample bucket before using old crypto results as evidence |
| Binary Complement Arbitrage Scanner | Implemented scanner | Scanner implemented, no auto-execution ledger yet | N/A | N/A | N/A | Keep scanner-only until execution model is measured | Do not auto-execute without fee, slippage, stale-book, and capital-lock checks |
| Favorite-Longshot Bias Scanner | Implemented scanner | Scanner implemented, no auto-execution ledger yet | N/A | N/A | N/A | Keep scanner-only until historical candidate outcomes are measured | Do not trade without band-specific WR/ROI and liquidity constraints |
| Odds Momentum Alert | Active alert strategy | Notification-only strategy | N/A | N/A | N/A | Keep as alert-only | Promote only if alert outcomes are logged with later market movement |
| Whale-Exit Fade Experiment | Queued paper-only entry experiment | No result sample | 0 | N/A | N/A | Build only after current Smart Money ledger supports another entry experiment | Disable if first 100 closed paper trades have WR < 55% or ROI <= 0% |
| CLOB Midpoint Crypto Component | Queued signal filter | No result sample | 0 | N/A | N/A | Build only if refreshed crypto ledger shows a weak but salvageable edge | Skip if refreshed crypto strategy is structurally negative after fees |
| Strategy Sizing / Toxicity Research | Research / sizing layer | `41861c2` added sizing and toxicity research core | Needs experiment design | N/A | N/A | Treat as a risk layer, not a standalone alpha source | Require controlled before/after comparison before enabling by default |

## Evidence Backlog

1. Capture fresh `smart roi` output and update Smart Money rows.
2. Capture fresh `smart crypto status` output and split old 5m results from the daily range/strike pivot.
3. Run `smart reconcile --dry-run` before trusting open/closed paper-trade counts.
4. Only after this ledger is refreshed, decide whether `Whale-Exit Fade Experiment` or `CLOB Midpoint Crypto Component` deserves implementation time.
