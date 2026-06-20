# PMCC — Session Resume

> Live, verifiable, resumable state only. History lives in git; macOS deploy
> gotcha + long bug narratives live in memory `pmcc-polymarket-cli`.

## Live state (2026-06-20)

- **Branch**: `main`. **3 commits ahead of `origin/main`** (`1cc069f` feat, `14fe684`
  docs, `570c9f1` fix — NOT pushed, Codex round-2 confirm pending per push gate).
  Latest pushed = `d6eaffc`.
- **Green gate**: `cargo test` = **204 passed** (152 bin + 52 integration). `cargo fmt` clean.
  (Note: `cargo test --lib` fails — binary crate has no lib target; use `--bin polymarket`.)

### Done this session (all PUSHED)
- **PM-P01** (`3213d9a`). Trailing-stop `peak_roi` was wiped each monitor cycle for
  `position_id=None` rows (write key vs retain key diverged). Fix: single
  `peak_key_for` → NAMESPACED key `pid:{position_id}` else
  `legacy:{condition_id}:{outcome}`; all get/insert/remove + retain route through it.
  Codex /cso caught + fixed two latent collisions (YES/NO same-market; position_id ==
  another row's condition_id). 4 peak-key tests incl. 2 collision regressions.
- **WC fixture lifecycle state machine** (`bc59345` + `d6eaffc`, `src/fixtures/mod.rs`,
  PURE CORE). `MatchPhase`: PreMatch → EntryWindow → LockedInPlay → ExitWindow →
  SettlementWatch → Settled; `match_phase(now,kickoff,settlement,PhaseConfig)` (by
  value), boundaries latest-first. Defaults: enter ≤60m pre-kickoff, force paper-exit
  15m pre-settlement, last 5m watch-only. Targets the atomic-settlement -99% risk.
  `#![allow(dead_code)]` until wired. 9 unit tests. Codex /cso-reviewed (verdict: safe
  to push as pure core; validated constructor deferred to wiring — see Next step 1).
  Built by hand, NOT AgentFlow (memory `feedback-when-to-use-agentflow`).

## Next steps
0. **Push `1cc069f`** once a Codex /cso review of the diff passes (push gate).
1. **WC fixture follow-on slices** (deferred; side-effectful, hand-write or separate
   slice): schedule ingestion → Polymarket market mapping → monitor-cycle wiring that
   consumes `match_phase` (entry gate / force-exit). Do AFTER phase contract stable.
   **Must-fix prerequisite DONE** (`1cc069f` + Codex round-2 `570c9f1`):
   `PhaseConfig::new` enforces windows `>= 0` and **STRICT** `settlement_watch <
   exit_window` (equality collapses the ExitWindow band → `requires_exit()` never
   fires); `match_phase_checked` rejects `settlement <= kickoff` AND `exit_window >
   settlement - kickoff` (else `exit_open` precedes kickoff). 4 PhaseError variants.
   **Use `match_phase_checked` (not raw `match_phase`) at the wiring boundary.**
   `#![allow(dead_code)]` INTENTIONALLY kept — drop it when wiring consumes these.
2. **PM-P01 live-sample verify**: confirm trailing stop now fires for a real
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
