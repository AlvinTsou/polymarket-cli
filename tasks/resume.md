# PMCC — Session Resume

> Live, verifiable, resumable state only. History lives in git; macOS deploy
> gotcha + long bug narratives live in memory `pmcc-polymarket-cli`.

## Live state (2026-06-20)

- **Branch**: `main`, tracking `origin/main`, **ahead 1** (`bc59345` unpushed).
- **Latest commits**:
  - `bc59345` feat(fixtures): WC fixture lifecycle state machine (pure core) — **unpushed**
  - `3213d9a` fix(smart): PM-P01 peak wiped each cycle for None position_id — **pushed**
- **Green gate**: `cargo test` = **194 passed** (142 bin + 52 integration). `cargo fmt` clean.

### Done this session
- **PM-P01 FIXED + COMMITTED + PUSHED** (`3213d9a`). Trailing-stop `peak_roi` was
  wiped each monitor cycle for `position_id=None` rows (write key vs retain key
  diverged). Fix: single `peak_key_for` → NAMESPACED key `pid:{position_id}` else
  `legacy:{condition_id}:{outcome}`; all get/insert/remove + retain route through it.
  Codex /cso-reviewed (caught + fixed two latent collisions: YES/NO same-market, and
  position_id == another row's condition_id). 4 peak-key tests incl. 2 collision
  regressions.
- **WC fixture lifecycle state machine** (`bc59345`, `src/fixtures/mod.rs`, pure core,
  **unpushed**). `MatchPhase`: PreMatch → EntryWindow → LockedInPlay → ExitWindow →
  SettlementWatch → Settled; `match_phase(now,kickoff,settlement,&PhaseConfig)`,
  boundaries latest-first (degenerate short gap degrades to risk-reducing phase).
  Defaults: enter ≤60m pre-kickoff, force paper-exit 15m pre-settlement, last 5m
  watch-only. Targets the atomic-settlement -99% risk. `#![allow(dead_code)]` until
  wired. 7 unit tests. Built by hand (NOT AgentFlow — see memory
  `feedback-when-to-use-agentflow`).

## Next steps
1. **Push `bc59345`** — gated on a Codex review passing (per push-gate rule).
2. **WC fixture follow-on slices** (deferred; side-effectful, hand-write or separate
   slice): schedule ingestion → Polymarket market mapping → monitor-cycle wiring that
   consumes `match_phase` (entry gate / force-exit). Do AFTER phase contract stable.
   **Must-fix BEFORE live wiring** (Codex /cso): add `PhaseConfig::new(...) -> Result`
   enforcing windows `>= 0`, `settlement_watch <= exit_window` (else SettlementWatch
   swallows the force-exit window), and treat `settlement <= kickoff` as invalid;
   then drop the module-wide `#![allow(dead_code)]`.
3. **PM-P01 live-sample verify**: confirm trailing stop now fires for a real
   `position_id=None` open position. BLOCKED: follows.jsonl currently 0 Open,
   `peak_roi.json={}` — no live sample exists yet.

## Open / unresolved
- **`-45%` vs `-20%` discrepancy (unverified, undecided).** `CLAUDE.md` Money Safety +
  the old config baseline say stop-loss `-45%` / drawdown `50%`, but the live
  hardcoded self-managed exit loop uses `-20%` / `30%` (test-pinned consts). Likely
  two layers (monitor.json risk params vs the exit loop). Decide intent, then fix docs
  OR code — do NOT change exit-loop behaviour blindly.

## Hard boundaries
- **Do NOT** change the self-managed exit-loop behaviour without intent; any behaviour
  change must be verified against a live sample (frozen-behaviour test sprint rule).
- **`cargo test` is the green gate, NOT clippy.** `cargo clippy --all-targets -D
  warnings` has ~44 PRE-EXISTING repo lints (dead_code/style) → do not treat clippy
  red as a regression; use `cargo check` or scoped clippy.
- **Push gated on a passing Codex review** (commit freely).

## Important files
- `src/commands/smart.rs` — monitor loop, `evaluate_exit` + `peak_key_for` (exit/peak rules), ~7300 lines.
- `src/fixtures/mod.rs` — WC fixture lifecycle state machine (pure).
- `tests/fixtures/pmcc-paper/` — deterministic paper-trade JSONL fixtures.
- `docs/paper-validation-harness-report.md` — validation harness report.
- `tasks/issues.md` — issue ledger (PM-P01 row now resolved).

## Prior milestones (one-liners; detail in git)
- 2026-06-13 `ac30569` — deterministic paper-trade validation harness shipped (3 pure
  helpers now on live paths; fixtures + report).
- 2026-05-24/25 — atomic-settlement stop-loss guards (`f9ba1ee`) + zombie settled-
  position sweep (reconcile command + in-cycle market-closed close).
