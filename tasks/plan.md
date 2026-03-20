# Sprint 8: Smart Wallet Intelligence — Auto-Renew, PnL Tracking, Trade Analysis

## Goals

1. **Auto-renew watch list** — periodically rediscover top wallets across 24h/week/month
2. **Per-wallet PnL tracking** — track open + closed position PnL, historical snapshots
3. **Trade pattern analysis** — understand what and why smart wallets trade

## Feature 1: Auto-Renew Watch List

### `smart discover --auto-renew`

Current `discover` only runs once. New behavior:

```
polymarket smart discover --auto-renew --threshold 90
```

- Runs discover for **all 3 periods** (day, week, month)
- Merges results: wallet appears in multiple periods → higher priority
- Adds new wallets above threshold, **removes stale wallets** that dropped off all leaderboards
- Tags wallets with discovery source: `"leaderboard:day"`, `"leaderboard:week"`, etc.

### Integration with Monitor

Add `--auto-renew` flag to `smart monitor`:
- Every 6 hours (configurable), run auto-renew cycle within the monitor loop
- Keeps watch list fresh without manual intervention

### WatchedWallet Extension

```rust
pub struct WatchedWallet {
    // ...existing fields...
    pub discovery_periods: Option<Vec<String>>,  // ["day","week","month"]
    pub last_seen_at: Option<DateTime<Utc>>,     // last time seen on leaderboard
    pub stale: Option<bool>,                     // dropped off all leaderboards
}
```

## Feature 2: Per-Wallet PnL Tracking

### `smart wallet-pnl [ADDRESS]`

New command that shows detailed PnL for a wallet:

```
polymarket smart wallet-pnl 0x... [--period week]
```

**Data sources:**
1. **Open positions** — `PositionsRequest` → `cash_pnl` per position
2. **Closed positions** — SDK's `ClosedPositionsRequest` → `realized_pnl`
3. **Historical snapshots** — diff stored snapshots to compute PnL deltas over time

**Output:**
```
Wallet: 0xABC...DEF (whale_01)  Score: 98.7

Open Positions (5):
  Market                        Outcome  Size     Entry   Now    PnL
  Will X win election?          Yes      $500     0.45    0.62   +$188.89
  ...

Closed Positions (recent 10):
  Market                        Outcome  Entry    Exit    PnL      ROI
  Bitcoin > 100k by July?       Yes      0.30     0.85    +$183.33 +183%
  ...

Summary:
  Open PnL:     +$342.15 (5 positions)
  Realized PnL: +$1,204.50 (23 closed)
  Total PnL:    +$1,546.65
  Win Rate:     78% (18/23 closed)
```

### PnL Snapshot Storage

```
~/.config/polymarket/smart/pnl_history/{address}.jsonl
```

Each scan cycle appends:
```json
{"timestamp":"...","open_pnl":342.15,"realized_pnl":1204.50,"position_count":5}
```

This builds a time-series for equity curves per wallet.

## Feature 3: Trade Pattern Analysis

### `smart analyze [ADDRESS]`

Understand **what** and **why** smart wallets trade:

```
polymarket smart analyze 0x... [--depth 50]
```

**Analysis dimensions:**

#### A. Market Category Distribution
Group positions by market keywords → detect specialization:
```
Category breakdown:
  Politics/Elections  45% (12 positions, +$800)
  Crypto/Bitcoin      30% (8 positions, +$350)
  AI/Tech             15% (4 positions, +$120)
  Other               10% (3 positions, -$50)
```

#### B. Trading Style Profile
Compute from position data:
```
Trading Style:
  Avg position size:  $150
  Avg entry price:    0.35 (buys low-probability outcomes)
  Hold time:          ~5 days (from snapshot history)
  Concentration:      3 markets = 60% of portfolio (concentrated)
  Direction bias:     72% YES positions
  Conviction:         High (avg size > $100, few positions)
```

#### C. Recent Moves (from signals history)
```
Recent Activity (7 days):
  03-19 14:30  NEW    HIGH  "Will X happen?"         YES  $200 @ 0.35
  03-18 09:15  CLOSE  —     "Bitcoin > 90k March?"   YES  exit @ 0.92 (+$180)
  03-17 22:00  INCREASE MED "AI regulation bill?"    NO   +$50 @ 0.22
```

#### D. Contrarian vs Consensus Indicator
Compare wallet's positions against current market prices:
```
Contrarian Score: 7/10
  - 60% of positions are on outcomes priced < 0.40
  - Betting against consensus on 3 markets
```

### Implementation: Category Detection

Simple keyword-based categorization (no ML needed):
```rust
const CATEGORIES: &[(&str, &[&str])] = &[
    ("Politics", &["election", "president", "congress", "vote", "trump", "biden"]),
    ("Crypto", &["bitcoin", "ethereum", "btc", "eth", "crypto", "defi"]),
    ("AI/Tech", &["ai", "artificial", "openai", "google", "apple", "tech"]),
    ("Sports", &["nba", "nfl", "soccer", "championship", "world cup"]),
    ("Economy", &["gdp", "inflation", "fed", "interest rate", "recession"]),
];
```

### HTML Report Integration

Add to `smart report`:
- Per-wallet P&L cards with mini equity curves
- Category distribution pie chart (inline SVG)
- Top 5 wallets ranked by total PnL

## Implementation Steps

### Step 1: WatchedWallet extension + serde compat
- Add `discovery_periods`, `last_seen_at`, `stale` fields (Option + serde(default))

### Step 2: Multi-period discover
- `cmd_discover` with `--auto-renew` flag
- Query day + week + month leaderboards
- Merge/dedup by address, tag with periods

### Step 3: Stale wallet detection
- If wallet not seen on any leaderboard for 7 days → mark stale
- `smart list` shows stale indicator

### Step 4: PnL snapshot storage
- New `pnl_history/` directory in smart store
- Append PnL snapshot per wallet per scan cycle
- `load_pnl_history(address)` function

### Step 5: `smart wallet-pnl` command
- Fetch open positions + closed positions from API
- Compute summary stats
- Table output

### Step 6: `smart analyze` command
- Category detection from market titles
- Trading style metrics computation
- Recent moves from signal history
- Contrarian score

### Step 7: Monitor integration
- Auto-renew every N hours in monitor loop
- PnL snapshot recording per scan cycle

### Step 8: Report enhancement
- Per-wallet PnL cards
- Category distribution SVG

### Step 9: Build + Test

## SDK API Calls Needed

| Feature | API | Existing? |
|---------|-----|-----------|
| Multi-period leaderboard | `TraderLeaderboardRequest` | Yes, used in discover |
| Open positions + PnL | `PositionsRequest` | Yes, used in scan |
| Closed positions | Needs `ClosedPositionsRequest` | Not yet used |
| Trade history | Needs `TradesRequest` | Not yet used |
| Portfolio value | `ValueRequest` | Yes, used in scorer |

## Non-goals

- ML-based trade prediction
- Real-time position mirroring (copy-trade)
- Cross-chain wallet tracking
