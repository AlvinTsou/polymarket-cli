# Paper-Trading Rule Validation Harness — Sprint Report

**Branch:** `test/paper-validation-harness`
**Date:** 2026-06-13
**Type:** test-infrastructure / validation sprint (NO strategy or behaviour change)

## Goal

Make the existing Smart Money + Crypto paper-trading rules **deterministically
testable offline**, so a future rule change can be validated by `cargo test`
before it reaches a live monitor sample. Behaviour was treated as **frozen**:
tests assert the *current* logic, they do not change it.

## What shipped

| Area | Change | Files |
|------|--------|-------|
| Exit rules | Extracted pure `evaluate_exit(roi, age_days, prev_peak) -> (ExitDecision, peak)` + 6 threshold `const`s; the live monitor loop now calls it for the exit decision (side effects/logs unchanged) | `src/commands/smart.rs` |
| Crypto signal | Extracted pure `classify_direction(volatility, raw_score) -> (Direction, conf)`; `compute_signal_full` + `compute_signal_from_candles` now call it | `src/crypto/momentum.rs` |
| Crypto status | Extracted pure `classify_crypto_stats(records) -> CryptoStats`; `cmd_crypto_status` derives its counts/win-loss from it | `src/commands/smart.rs` |
| Fixtures | `exit-rules.jsonl` (12 scenarios), `crypto-status.jsonl` (5 records) | `tests/fixtures/pmcc-paper/` |
| Unit tests | 7 crypto-signal + 5 exit/status tests | `src/crypto/momentum.rs`, `src/commands/smart.rs` |
| CLI smoke | 3 integration tests against an isolated empty `$HOME` (tempfile) | `tests/cli_integration.rs` |

`tempfile` added as a **dev-dependency only** (not shipped in the binary).

The three extractions are not parallel copies: the live code paths call the
extracted helpers, so the tests genuinely guard the production logic. Exit
thresholds are now single-sourced as module `const`s shared by the live loop
and the tests — a threshold edit cannot silently diverge.

## What is PROVEN offline (deterministic, no network, no real config)

- **Exit thresholds (AC2)** — take-profit `roi >= +20%`, stop-loss `roi <= -20%`,
  time-stop `age >= 7d && roi < +5%`, trailing-stop arms at peak `>= +15%` and
  fires when `roi < peak * 0.70`. Evaluation order take-profit → stop-loss →
  time-stop → trailing, with boundary cases (`19.9` vs `20.0`, `6d` vs `7d`,
  `roi == 5.0`) and precedence pinned.
- **Crypto signal gate (AC3)** — volatility `> 0.003` skips; `|raw_score| < 0.10`
  skips; otherwise sign of score → `Up`/`Down`; confidence scales by `0.30` and
  clamps to 1.0. Threshold boundaries pinned.
- **Status classification (AC4)** — `Expired` records are counted separately and
  do **not** inflate `Closed` win/loss/PnL; non-`crypto:` records are excluded.
- **Reconcile dry-run no-op (AC5)** — with no open paper positions, `smart
  reconcile --dry-run` is a zero-change no-op and makes no settlement/network call.
- **CLI read-only isolation (AC6)** — `smart roi` / `smart crypto status` /
  `smart reconcile --dry-run` run against an isolated empty `$HOME`, confirming
  they read the (empty) isolated store and not the operator's real config.
- **AC1** — full suite green: 131 unit + 52 integration tests pass.

## What remains LIVE-SAMPLE dependent (NOT validated offline)

- **End-to-end monitor loop behaviour** — `evaluate_exit` is unit-tested, but the
  surrounding async monitor loop (price fetch, settlement reconcile, persistence,
  `peak_roi` file I/O across cycles) is still only exercised live.
- **Signal QUALITY** — tests pin the *gate* (`classify_direction`), not whether
  the upstream `raw_score` / `volatility` math produces *good* signals. Win-rate
  and edge remain a function of live samples.
- **Logic-shape drift gap** — shared `const`s catch threshold changes, but a
  change to the live loop's comparison *shape* (e.g. `>=` → `>`) that bypasses
  `evaluate_exit` would not be caught. The current wiring routes the decision
  through `evaluate_exit`, so this is low-risk but worth noting.
- **U7 (liquidation timing)** — `compute_liquidation_signal` uses wall-clock
  `now_ms`; fixtures here avoid it (zero liquidations), so funding/OI/liquidation
  scoring is not covered offline.
- **NaN / infinity ROI and close-error paths** — `evaluate_exit` is exercised on
  finite ROI inputs only. A NaN ROI currently degrades to `Hold` (all `>=`/`<=`
  comparisons are false by construction), but this is reasoned, not test-pinned.
  The `close_follow_position` error branch (the `eprintln!` warn path) is also
  not covered offline.

## Independent review (Codex)

The exit-loop refactor was diffed pre- vs post-change and confirmed
behaviour-equivalent across all five branches (`peak_roi` map state and
`peak_changed` flag included). Two items it surfaced:

- **Pre-existing peak/retain bug (NOT introduced here, do NOT fix in this frozen
  sprint):** peaks are inserted under `pos_id = position_id.unwrap_or(condition_id)`,
  but the post-loop `peak_roi.retain` keeps only keys derived from `position_id`.
  When `position_id` is `None`, a freshly-inserted peak (keyed by `condition_id`)
  is wiped every cycle, so trailing-stop tracking never accumulates for those
  positions. Matches contract Unknowns U2 / fact F53. **Follow-up ticket; changing
  it is a behaviour change, out of scope for this validation sprint.**
- The "every edge case" phrasing was an overstatement — see the NaN / close-error
  caveat above.

## Findings

- **Doc/code discrepancy:** `CLAUDE.md` § Money Safety states "Stop-loss: -45%;
  trailing stop: peak+30% / drawdown 50%", but the live code (now pinned by
  tests) uses **stop-loss -20%, trailing activate +15%, drawdown 30%**. The tests
  assert the *code* values per AC2. `CLAUDE.md` should be reconciled separately.
- **U1 resolved:** on macOS, `dirs::home_dir()` (dirs v6) honours a child
  process's `$HOME`, so the CLI smoke tests isolate cleanly (the empty-state
  assertions pass).

## Verdict on strategy expansion

Paper-trading exit + signal-gate + status behaviour is now offline-testable, so
rule *threshold* changes can be regression-checked before a live run. Strategy
*expansion* (CLOB midpoint, whale-exit fade, crypto tuning) is still gated on
live sample size for edge/quality validation — that is not something these
deterministic tests can establish.
