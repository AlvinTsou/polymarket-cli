# Sprint 6: Precise P&L Tracking + Trade History Visualization

## Problem

Current implementation has these gaps:
1. `FollowRecord.price` is signal price, not actual fill price — slippage invisible
2. No realized vs unrealized P&L — all trades treated as open
3. No position closure detection — can't tell when a trade was exited
4. No time-based filtering on `roi`/`history` — everything lumped together
5. Dashboard is static tables only — no charts, no timeline, no visual P&L curve
6. No per-wallet or per-market performance breakdown

## Design

### Step 1: Extend `FollowRecord` (mod.rs + store.rs)

Add fields to `FollowRecord` (backward-compatible via `Option` + `#[serde(default)]`):

```rust
pub struct FollowRecord {
    // ... existing fields ...
    pub fill_price: Option<f64>,       // actual execution price (from order response)
    pub status: Option<TradeStatus>,   // Open | Closed | Expired
    pub closed_at: Option<DateTime<Utc>>,
    pub exit_price: Option<f64>,
    pub realized_pnl: Option<f64>,
    pub position_id: Option<String>,   // groups entry+exit of same position
}

pub enum TradeStatus { Open, Closed, Expired }
```

Old JSONL records deserialize fine — new fields default to `None`.

### Step 2: Capture fill price (commands/smart.rs)

In `cmd_follow()` and `cmd_auto_follow()`, after `place_follow_order()` returns the order response, extract actual fill price and store it in `fill_price`. For dry-run, `fill_price = None`.

### Step 3: Position closure detection (smart/tracker.rs → commands/smart.rs)

During `cmd_scan()`, when a `ClosePosition` or `DecreasePosition` change is detected:
- Find matching open `FollowRecord` by `(condition_id, outcome, side=BUY)`
- Update its `status` to `Closed`, set `closed_at`, `exit_price`, calculate `realized_pnl`
- Store via new `update_follow_record()` in store.rs (rewrite JSONL)

### Step 4: Enhanced `smart roi` command

```
polymarket smart roi [--wallet ADDR] [--market KEYWORD] [--period day|week|month|all] [--status open|closed|all]
```

- Separate realized (closed) vs unrealized (open) P&L in summary
- Group by wallet or market when filtered
- Show win rate for closed trades only (accurate)

### Step 5: Enhanced `smart history` command

```
polymarket smart history [--period day|week|month|all] [--status open|closed] [--export json]
```

### Step 6: Trade History Visualization (HTML dashboard)

Enhance `smart report` to include:

1. **Equity Curve** — cumulative P&L over time (SVG line chart, inline, no JS deps)
2. **Trade Timeline** — scatter plot of entries/exits on time axis, colored by win/loss
3. **Summary Cards** — realized PnL, unrealized PnL, win rate, avg hold time, best/worst trade
4. **Per-Wallet Breakdown** — table with each wallet's follow performance
5. **Per-Market Breakdown** — which markets were most profitable

All rendered as inline SVG in the same dark-theme HTML — no external JS libraries needed.

### Data flow

```
scan → detect ClosePosition → match open FollowRecord → close it (realized PnL)
                                                      ↓
roi/report reads follows.jsonl → splits open/closed → calculates metrics → renders
```

## Implementation Order

1. Types + serde compat (mod.rs) — no breaking changes
2. Store update function (store.rs) — JSONL rewrite for closure updates
3. Fill price capture (commands/smart.rs follow/auto-follow)
4. Closure detection logic (commands/smart.rs scan)
5. Enhanced roi command with filters
6. Enhanced history command with filters
7. SVG chart generation helpers
8. Dashboard overhaul (report command)
9. Compile + test
10. Manual verification

## Non-goals

- Real-time price queries (would need websocket, out of scope)
- Multi-leg position tracking (increase→increase→close as one position) — keep it simple: 1 entry = 1 record
- External charting libraries — all inline SVG
