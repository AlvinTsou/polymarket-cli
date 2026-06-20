//! World Cup fixture lifecycle state machine (pure core).
//!
//! Polymarket World Cup match markets have a *fixed* kickoff schedule, unlike the
//! open-ended political markets the rest of PMCC trades. That lets us drive trading
//! off the fixture clock instead of the brittle title-horizon heuristics, and — most
//! importantly — force a paper exit *before* the settlement window, where atomic
//! settlement has historically jumped a mid-book price straight to 0/1 and handed the
//! monitor a -99% loss it could not react to in time.
//!
//! This module is the deterministic core only: given the clock and a fixture's two
//! anchors (`kickoff`, expected `settlement`), it answers "what phase is this market
//! in, and what is allowed". Schedule ingestion, Polymarket market mapping, and the
//! monitor-cycle wiring are intentionally NOT here — they are side-effectful and are
//! built on top of this once the phase contract is stable.
//!
//! Not yet consumed by a live command (the wiring slice is deferred), so the public
//! items are allowed to be unused for now; they are exercised by this module's tests.
#![allow(dead_code)]

use chrono::{DateTime, Duration, Utc};

/// Start entering up to this many minutes before kickoff.
pub const DEFAULT_ENTRY_WINDOW_MINS: i64 = 60;
/// Begin forcing a paper exit this many minutes before expected settlement, to be
/// flat before the atomic-settlement danger zone.
pub const DEFAULT_EXIT_WINDOW_MINS: i64 = 15;
/// Final window before settlement: assume we are (or should be) flat and just watch
/// for resolution; never open or actively manage here.
pub const DEFAULT_SETTLEMENT_WATCH_MINS: i64 = 5;

/// Invariant violation rejected by the validated constructors. Keeping these typed
/// (rather than `anyhow`) lets callers — and tests — distinguish *which* contract
/// broke before any live wiring relies on the phase classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseError {
    /// One of the phase windows was negative.
    NegativeWindow,
    /// `settlement_watch >= exit_window`: at or above equality the watch window
    /// swallows the whole force-exit window (`watch_open <= exit_open`, and
    /// SettlementWatch is matched first), so `requires_exit()` could never fire.
    SettlementWatchExceedsExit,
    /// A fixture's `settlement` anchor was not strictly after its `kickoff`.
    SettlementNotAfterKickoff,
    /// `exit_window > settlement - kickoff`: `exit_open` would fall before kickoff,
    /// so `ExitWindow` (force-exit) fires before the match even starts. Anchor-
    /// dependent, so checked in [`match_phase_checked`], not [`PhaseConfig::new`].
    ExitWindowBeforeKickoff,
}

impl std::fmt::Display for PhaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            PhaseError::NegativeWindow => "phase window must be non-negative",
            PhaseError::SettlementWatchExceedsExit => "settlement_watch must be < exit_window",
            PhaseError::SettlementNotAfterKickoff => "settlement must be strictly after kickoff",
            PhaseError::ExitWindowBeforeKickoff => "exit_window must be <= settlement - kickoff",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for PhaseError {}

/// Tunable phase boundaries. All durations are measured relative to a fixture's
/// `kickoff` (entry) or expected `settlement` (exit / watch).
///
/// Prefer [`PhaseConfig::new`] over struct literals when the windows are not
/// compile-time constants: it enforces the invariants the phase classification
/// relies on. [`PhaseConfig::default`] is pre-validated by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseConfig {
    pub entry_window: Duration,
    pub exit_window: Duration,
    pub settlement_watch: Duration,
}

impl PhaseConfig {
    /// Validated constructor enforcing: all windows `>= 0`, and (strictly)
    /// `settlement_watch < exit_window` so the force-exit window is non-empty. The
    /// strict bound also implies `exit_window > 0` (since `settlement_watch >= 0`),
    /// ruling out a zero-length, never-matched ExitWindow.
    pub fn new(
        entry_window: Duration,
        exit_window: Duration,
        settlement_watch: Duration,
    ) -> Result<Self, PhaseError> {
        if entry_window < Duration::zero()
            || exit_window < Duration::zero()
            || settlement_watch < Duration::zero()
        {
            return Err(PhaseError::NegativeWindow);
        }
        if settlement_watch >= exit_window {
            return Err(PhaseError::SettlementWatchExceedsExit);
        }
        Ok(Self {
            entry_window,
            exit_window,
            settlement_watch,
        })
    }
}

impl Default for PhaseConfig {
    fn default() -> Self {
        Self {
            entry_window: Duration::minutes(DEFAULT_ENTRY_WINDOW_MINS),
            exit_window: Duration::minutes(DEFAULT_EXIT_WINDOW_MINS),
            settlement_watch: Duration::minutes(DEFAULT_SETTLEMENT_WATCH_MINS),
        }
    }
}

/// Lifecycle phase of a single fixture-backed market.
///
/// Time-ordered: `PreMatch → EntryWindow → LockedInPlay → ExitWindow →
/// SettlementWatch → Settled`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchPhase {
    /// Too early to act; before the entry window opens.
    PreMatch,
    /// Open new positions here (up to kickoff).
    EntryWindow,
    /// Match in play: hold existing positions, take no new trades.
    LockedInPlay,
    /// Force a paper exit now, before the settlement danger zone.
    ExitWindow,
    /// Settlement imminent: assume flat, watch only.
    SettlementWatch,
    /// Expected settlement reached/passed; the market resolution sweep owns it now.
    Settled,
}

impl MatchPhase {
    /// Only the entry window admits new positions.
    pub fn allows_new_entry(self) -> bool {
        matches!(self, MatchPhase::EntryWindow)
    }

    /// The exit window is the one phase that actively forces a close.
    pub fn requires_exit(self) -> bool {
        matches!(self, MatchPhase::ExitWindow)
    }
}

/// Pure classification of a fixture-backed market's lifecycle phase.
///
/// `kickoff` and `settlement` are the fixture's two clock anchors (settlement is the
/// *expected* market resolution time, which the schedule provides — for a knockout it
/// includes extra time / penalties).
///
/// Boundaries are checked latest-first, so within a normal timeline an overlap
/// between `kickoff` and `exit_open` resolves in favour of the more advanced phase
/// (exit/watch/settled win over `LockedInPlay`). This is NOT a general "always
/// risk-reducing" guarantee — see the config invariants below; e.g. with
/// `settlement_watch >= exit_window` the `SettlementWatch` window swallows
/// `ExitWindow` and `requires_exit()` never fires.
///
/// Config invariants (`all windows >= 0`, `settlement_watch <= exit_window`) are
/// enforced by [`PhaseConfig::new`] and held by [`PhaseConfig::default`]; the anchor
/// invariant `kickoff < settlement` is enforced by [`match_phase_checked`]. This raw
/// entry point does NOT re-validate: out-of-invariant input is classified
/// deterministically but the phase semantics may not match intent — prefer
/// [`match_phase_checked`] at the wiring boundary.
pub fn match_phase(
    now: DateTime<Utc>,
    kickoff: DateTime<Utc>,
    settlement: DateTime<Utc>,
    cfg: PhaseConfig,
) -> MatchPhase {
    let entry_open = kickoff - cfg.entry_window;
    let exit_open = settlement - cfg.exit_window;
    let watch_open = settlement - cfg.settlement_watch;

    if now >= settlement {
        MatchPhase::Settled
    } else if now >= watch_open {
        MatchPhase::SettlementWatch
    } else if now >= exit_open {
        MatchPhase::ExitWindow
    } else if now >= kickoff {
        MatchPhase::LockedInPlay
    } else if now >= entry_open {
        MatchPhase::EntryWindow
    } else {
        MatchPhase::PreMatch
    }
}

/// Validated wrapper over [`match_phase`] that rejects degenerate fixture anchors up
/// front, so live wiring never feeds the classifier a timeline where the force-exit
/// window opens before the match starts. Two anchor-dependent invariants (which
/// [`PhaseConfig::new`] cannot check, as they need the kickoff↔settlement span):
/// - `settlement > kickoff` (else the whole timeline is inverted), and
/// - `exit_window <= settlement - kickoff` (else `exit_open` precedes kickoff and
///   `ExitWindow` fires before the match is in play).
///
/// With valid anchors it returns exactly what [`match_phase`] would.
pub fn match_phase_checked(
    now: DateTime<Utc>,
    kickoff: DateTime<Utc>,
    settlement: DateTime<Utc>,
    cfg: PhaseConfig,
) -> Result<MatchPhase, PhaseError> {
    if settlement <= kickoff {
        return Err(PhaseError::SettlementNotAfterKickoff);
    }
    if cfg.exit_window > settlement - kickoff {
        return Err(PhaseError::ExitWindowBeforeKickoff);
    }
    Ok(match_phase(now, kickoff, settlement, cfg))
}

/// Why a Polymarket market cannot be classified as a fixture-backed phase. Distinct
/// from [`PhaseError`] (which is about invariant breaches) so the caller can tell
/// "this market simply is not a scheduled fixture" apart from "the fixture's anchors
/// are degenerate".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixtureError {
    /// Market has no `game_start_time`, so it is not a scheduled fixture market and
    /// the fixture clock does not apply (fall back to the normal trading path).
    Unscheduled,
    /// `game_start_time` was present but not an RFC 3339 timestamp.
    UnparseableKickoff,
    /// Market has no `end_date`, so there is no settlement anchor to drive exits.
    MissingSettlement,
    /// The anchors parsed but violate a phase invariant (see [`PhaseError`]).
    Phase(PhaseError),
}

impl std::fmt::Display for FixtureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FixtureError::Unscheduled => f.write_str("market has no game_start_time"),
            FixtureError::UnparseableKickoff => {
                f.write_str("game_start_time is not an RFC 3339 timestamp")
            }
            FixtureError::MissingSettlement => f.write_str("market has no end_date"),
            FixtureError::Phase(e) => write!(f, "fixture anchors invalid: {e}"),
        }
    }
}

impl std::error::Error for FixtureError {}

impl From<PhaseError> for FixtureError {
    fn from(e: PhaseError) -> Self {
        FixtureError::Phase(e)
    }
}

/// Bridge from a Polymarket gamma market to the fixture phase machine: derive the
/// `kickoff` anchor from the market's `game_start_time` (RFC 3339) and the
/// `settlement` anchor from its `end_date`, then classify via [`match_phase_checked`].
///
/// This is the seam the live monitor uses to decide whether a fixture market admits a
/// new entry ([`MatchPhase::allows_new_entry`]) or must be force-exited
/// ([`MatchPhase::requires_exit`]); it does not itself touch any positions.
pub fn market_match_phase(
    now: DateTime<Utc>,
    game_start_time: Option<&str>,
    end_date: Option<DateTime<Utc>>,
    cfg: PhaseConfig,
) -> Result<MatchPhase, FixtureError> {
    let raw = game_start_time.ok_or(FixtureError::Unscheduled)?;
    let kickoff = DateTime::parse_from_rfc3339(raw)
        .map_err(|_| FixtureError::UnparseableKickoff)?
        .with_timezone(&Utc);
    let settlement = end_date.ok_or(FixtureError::MissingSettlement)?;
    Ok(match_phase_checked(now, kickoff, settlement, cfg)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // A regulation fixture: kickoff 18:00 UTC, expected settlement 19:50 UTC
    // (~110 min: 90 + stoppage + resolution lag).
    fn kickoff() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 20, 18, 0, 0).unwrap()
    }
    fn settlement() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 20, 19, 50, 0).unwrap()
    }
    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 20, h, m, 0).unwrap()
    }

    fn phase(now: DateTime<Utc>) -> MatchPhase {
        match_phase(now, kickoff(), settlement(), PhaseConfig::default())
    }

    #[test]
    fn pre_match_before_entry_window_opens() {
        // entry window opens 60m before kickoff (17:00); 16:59 is still PreMatch.
        assert_eq!(phase(at(16, 59)), MatchPhase::PreMatch);
    }

    #[test]
    fn entry_window_inclusive_lower_bound_and_up_to_kickoff() {
        // Opens exactly at kickoff-60m, runs up to (not including) kickoff.
        assert_eq!(phase(at(17, 0)), MatchPhase::EntryWindow);
        assert_eq!(phase(at(17, 59)), MatchPhase::EntryWindow);
    }

    #[test]
    fn locked_in_play_from_kickoff_until_exit_window() {
        // Kickoff is inclusive; exit window opens at settlement-15m (19:35).
        assert_eq!(phase(at(18, 0)), MatchPhase::LockedInPlay);
        assert_eq!(phase(at(19, 34)), MatchPhase::LockedInPlay);
    }

    #[test]
    fn exit_window_until_settlement_watch() {
        // settlement-15m (19:35) .. settlement-5m (19:45).
        assert_eq!(phase(at(19, 35)), MatchPhase::ExitWindow);
        assert_eq!(phase(at(19, 44)), MatchPhase::ExitWindow);
    }

    #[test]
    fn settlement_watch_then_settled() {
        // settlement-5m (19:45) .. settlement (19:50), then Settled.
        assert_eq!(phase(at(19, 45)), MatchPhase::SettlementWatch);
        assert_eq!(phase(at(19, 49)), MatchPhase::SettlementWatch);
        assert_eq!(phase(at(19, 50)), MatchPhase::Settled);
        assert_eq!(phase(at(20, 30)), MatchPhase::Settled);
    }

    #[test]
    fn action_helpers_track_phases() {
        assert!(MatchPhase::EntryWindow.allows_new_entry());
        assert!(!MatchPhase::LockedInPlay.allows_new_entry());
        assert!(MatchPhase::ExitWindow.requires_exit());
        assert!(!MatchPhase::SettlementWatch.requires_exit());
        // No phase both admits entry and forces exit.
        for p in [
            MatchPhase::PreMatch,
            MatchPhase::EntryWindow,
            MatchPhase::LockedInPlay,
            MatchPhase::ExitWindow,
            MatchPhase::SettlementWatch,
            MatchPhase::Settled,
        ] {
            assert!(!(p.allows_new_entry() && p.requires_exit()));
        }
    }

    #[test]
    fn default_config_has_a_nonempty_force_exit_window() {
        // Core motivation guard: with default config (settlement_watch 5m <
        // exit_window 15m) the pre-settlement period MUST contain an ExitWindow that
        // fires requires_exit(), i.e. SettlementWatch must NOT swallow the whole
        // force-exit window. (With settlement_watch >= exit_window it would — that is
        // the invariant a validated constructor will enforce before live wiring.)
        let cfg = PhaseConfig::default();
        assert!(cfg.settlement_watch <= cfg.exit_window);
        let p = match_phase(at(19, 35), kickoff(), settlement(), cfg);
        assert_eq!(p, MatchPhase::ExitWindow);
        assert!(p.requires_exit());
    }

    #[test]
    fn settlement_at_or_before_kickoff_classifies_settled() {
        // Documented degradation for invalid/degenerate anchors: once now >= settlement
        // the market is Settled regardless of kickoff (the resolution sweep owns it).
        let ko = kickoff();
        assert_eq!(
            match_phase(ko, ko, ko, PhaseConfig::default()),
            MatchPhase::Settled
        );
    }

    #[test]
    fn phase_config_new_accepts_valid_defaults() {
        let cfg = PhaseConfig::new(
            Duration::minutes(DEFAULT_ENTRY_WINDOW_MINS),
            Duration::minutes(DEFAULT_EXIT_WINDOW_MINS),
            Duration::minutes(DEFAULT_SETTLEMENT_WATCH_MINS),
        )
        .unwrap();
        assert_eq!(cfg, PhaseConfig::default());
    }

    #[test]
    fn phase_config_new_rejects_negative_window() {
        for (e, x, w) in [
            (
                Duration::minutes(-1),
                Duration::minutes(15),
                Duration::minutes(5),
            ),
            (
                Duration::minutes(60),
                Duration::minutes(-1),
                Duration::minutes(5),
            ),
            (
                Duration::minutes(60),
                Duration::minutes(15),
                Duration::minutes(-1),
            ),
        ] {
            assert_eq!(PhaseConfig::new(e, x, w), Err(PhaseError::NegativeWindow));
        }
    }

    #[test]
    fn phase_config_new_rejects_watch_at_or_above_exit() {
        // The invariant is STRICT (settlement_watch < exit_window): at equality
        // watch_open == exit_open and, since SettlementWatch is checked before
        // ExitWindow, the ExitWindow band is empty -> requires_exit() never fires,
        // which defeats the whole module. So both `>` and `==` are rejected.
        for w in [Duration::minutes(16), Duration::minutes(15)] {
            assert_eq!(
                PhaseConfig::new(Duration::minutes(60), Duration::minutes(15), w),
                Err(PhaseError::SettlementWatchExceedsExit)
            );
        }
    }

    #[test]
    fn phase_config_new_allows_watch_just_below_exit() {
        // Boundary: 14m < 15m is the largest valid watch under a 15m exit window.
        assert!(
            PhaseConfig::new(
                Duration::minutes(60),
                Duration::minutes(15),
                Duration::minutes(14),
            )
            .is_ok()
        );
    }

    #[test]
    fn match_phase_checked_rejects_exit_window_exceeding_match_span() {
        // exit_window (61m) > match span (60m) would put exit_open before kickoff, so
        // ExitWindow fires before the match starts. Rejected at the anchor boundary.
        let ko = kickoff();
        let st = ko + Duration::minutes(60);
        let cfg = PhaseConfig::new(
            Duration::minutes(60),
            Duration::minutes(61),
            Duration::minutes(5),
        )
        .unwrap();
        assert_eq!(
            match_phase_checked(ko, ko, st, cfg),
            Err(PhaseError::ExitWindowBeforeKickoff)
        );
    }

    #[test]
    fn match_phase_checked_allows_exit_window_equal_to_match_span() {
        // exit_window == match span: exit_open == kickoff. EntryWindow stays intact
        // (it runs entirely before kickoff); LockedInPlay collapses to empty, which
        // is the safe direction (force-exit for the whole in-play period).
        let ko = kickoff();
        let st = ko + Duration::minutes(60);
        let cfg = PhaseConfig::new(
            Duration::minutes(60),
            Duration::minutes(60),
            Duration::minutes(5),
        )
        .unwrap();
        assert!(match_phase_checked(ko - Duration::minutes(1), ko, st, cfg).is_ok());
    }

    #[test]
    fn match_phase_checked_rejects_settlement_at_or_before_kickoff() {
        let ko = kickoff();
        let cfg = PhaseConfig::default();
        assert_eq!(
            match_phase_checked(ko, ko, ko, cfg),
            Err(PhaseError::SettlementNotAfterKickoff)
        );
        assert_eq!(
            match_phase_checked(ko, ko, ko - Duration::minutes(1), cfg),
            Err(PhaseError::SettlementNotAfterKickoff)
        );
    }

    #[test]
    fn match_phase_checked_agrees_with_match_phase_for_valid_input() {
        let now = at(19, 35);
        let cfg = PhaseConfig::default();
        assert_eq!(
            match_phase_checked(now, kickoff(), settlement(), cfg),
            Ok(match_phase(now, kickoff(), settlement(), cfg))
        );
    }

    #[test]
    fn market_match_phase_rejects_unscheduled_market() {
        // No game_start_time => not a fixture-backed (scheduled) market.
        assert_eq!(
            market_match_phase(at(18, 0), None, Some(settlement()), PhaseConfig::default()),
            Err(FixtureError::Unscheduled)
        );
    }

    #[test]
    fn market_match_phase_rejects_unparseable_kickoff() {
        assert_eq!(
            market_match_phase(
                at(18, 0),
                Some("not-a-timestamp"),
                Some(settlement()),
                PhaseConfig::default(),
            ),
            Err(FixtureError::UnparseableKickoff)
        );
    }

    #[test]
    fn market_match_phase_rejects_missing_settlement() {
        assert_eq!(
            market_match_phase(
                at(18, 0),
                Some("2026-06-20T18:00:00Z"),
                None,
                PhaseConfig::default(),
            ),
            Err(FixtureError::MissingSettlement)
        );
    }

    #[test]
    fn market_match_phase_parses_rfc3339_and_matches_checked() {
        // Kickoff 18:00Z, settlement 19:50Z, now 19:35 -> ExitWindow.
        let got = market_match_phase(
            at(19, 35),
            Some("2026-06-20T18:00:00Z"),
            Some(settlement()),
            PhaseConfig::default(),
        );
        assert_eq!(
            got,
            Ok(
                match_phase_checked(at(19, 35), kickoff(), settlement(), PhaseConfig::default())
                    .unwrap()
            )
        );
        assert_eq!(got, Ok(MatchPhase::ExitWindow));
    }

    #[test]
    fn market_match_phase_accepts_offset_rfc3339() {
        // Same instant as 18:00Z expressed with a +02:00 offset must parse equal.
        let got = market_match_phase(
            at(19, 35),
            Some("2026-06-20T20:00:00+02:00"),
            Some(settlement()),
            PhaseConfig::default(),
        );
        assert_eq!(got, Ok(MatchPhase::ExitWindow));
    }

    #[test]
    fn market_match_phase_propagates_phase_errors() {
        // settlement <= kickoff must surface as the underlying PhaseError.
        assert_eq!(
            market_match_phase(
                at(18, 0),
                Some("2026-06-20T18:00:00Z"),
                Some(kickoff()),
                PhaseConfig::default(),
            ),
            Err(FixtureError::Phase(PhaseError::SettlementNotAfterKickoff))
        );
    }

    #[test]
    fn degenerate_short_gap_prefers_risk_reducing_phase() {
        // Settlement only 10m after kickoff: the 15m exit window opens BEFORE
        // kickoff. Latest-first checks must still never report LockedInPlay past
        // the exit point — at kickoff we are already in the exit/watch zone.
        let ko = kickoff();
        let st = ko + Duration::minutes(10);
        let cfg = PhaseConfig::default();
        // 1m after kickoff: settlement-5m (watch) = ko+5m, settlement-15m (exit) =
        // ko-5m. now=ko+1m is >= exit_open and < watch_open -> ExitWindow, not Locked.
        assert_eq!(
            match_phase(ko + Duration::minutes(1), ko, st, cfg),
            MatchPhase::ExitWindow
        );
        // ko+6m is within settlement-5m -> SettlementWatch.
        assert_eq!(
            match_phase(ko + Duration::minutes(6), ko, st, cfg),
            MatchPhase::SettlementWatch
        );
    }
}
