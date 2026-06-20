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

/// Tunable phase boundaries. All durations are measured relative to a fixture's
/// `kickoff` (entry) or expected `settlement` (exit / watch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseConfig {
    pub entry_window: Duration,
    pub exit_window: Duration,
    pub settlement_watch: Duration,
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
/// Expected config invariants (held by [`PhaseConfig::default`]; NOT validated on the
/// public fields — a validated constructor is deferred to the wiring slice):
/// `kickoff <= settlement`, all windows `>= 0`, and `settlement_watch <= exit_window`
/// (so the force-exit window is never empty). Out-of-invariant input is classified
/// deterministically but the phase semantics may not match intent.
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
