# Sprint 10: Paper Trade Dashboard — Exchange-Style Tabs

## Goal

Redesign the Paper Trades section of the live dashboard (`smart dashboard`) to match exchange-style tabbed layout (like Binance Futures), with proper P&L tracking and post-trade review.

## Tabs

### Tab 1: Open Positions (倉位)
- Current open paper trades with **live unrealized P&L**
- Columns: Market, Outcome, Side, Entry Price, Current Price, Size ($), Unrealized PnL, ROI%
- Summary row: total invested, total unrealized PnL

### Tab 2: Trade History (歷史成交)
- All paper trades (open + closed), newest first
- Columns: Time, Side, Market, Outcome, Entry Price, Amount, Status
- Filter controls (CSS-only): All / Today / This Week / This Month

### Tab 3: Position History (倉位歷史)
- Closed/resolved paper trades only
- Columns: Open Time, Close Time, Market, Outcome, Side, Entry, Exit, Realized PnL, ROI%, Hold Time
- Post-trade review: color-coded (green=profit, red=loss)

### Tab 4: Performance (績效)
- Summary cards: Total Trades, Win Rate, Total PnL, Avg PnL, Best Trade, Worst Trade, Avg Hold Time
- Equity curve (cumulative PnL over time, inline SVG — reuse Sprint 6 helper)
- Per-category breakdown (if trades span multiple categories)

## Technical Design

### Tab implementation
- CSS-only tabs using `<input type="radio">` + `:checked` + sibling selectors
- No JavaScript needed — works with auto-refresh
- Remember last selected tab via URL hash (optional, low priority)

### Data flow in `build_live_dashboard()`
```rust
let follows = store::load_follow_records();
let price_map = store::current_price_map();

// Split into open vs closed paper trades
let open_paper: Vec<_> = follows.iter()
    .filter(|r| r.dry_run && r.is_open())
    .collect();
let closed_paper: Vec<_> = follows.iter()
    .filter(|r| r.dry_run && !r.is_open())
    .collect();

// Compute metrics for each
// Reuse calc_open_pnl() for open trades
// Use realized_pnl for closed trades
```

### HTML structure
```html
<div class="tabs">
  <input type="radio" id="tab1" name="ptabs" checked>
  <label for="tab1">Open Positions</label>
  <input type="radio" id="tab2" name="ptabs">
  <label for="tab2">Trade History</label>
  <input type="radio" id="tab3" name="ptabs">
  <label for="tab3">Position History</label>
  <input type="radio" id="tab4" name="ptabs">
  <label for="tab4">Performance</label>

  <div class="tab-content" id="content1">...</div>
  <div class="tab-content" id="content2">...</div>
  <div class="tab-content" id="content3">...</div>
  <div class="tab-content" id="content4">...</div>
</div>
```

### CSS tab pattern
```css
.tabs input[type="radio"] { display: none }
.tabs label { cursor: pointer; padding: 8px 16px; border-bottom: 2px solid transparent }
.tabs input:checked + label { border-color: #4ade80; color: #f8fafc }
.tab-content { display: none }
#tab1:checked ~ #content1,
#tab2:checked ~ #content2,
#tab3:checked ~ #content3,
#tab4:checked ~ #content4 { display: block }
```

## Implementation Steps

### Step 1: Refactor `build_live_dashboard()` data preparation
- Split paper trades into open/closed
- Compute per-trade metrics (unrealized PnL, realized PnL, hold time)
- Build data structs for each tab

### Step 2: Tab 1 — Open Positions
- Table with live unrealized PnL
- Summary footer

### Step 3: Tab 2 — Trade History
- Full trade log, newest first
- Period filter (CSS-only: all/today/week/month using data attributes)

### Step 4: Tab 3 — Position History
- Closed trades with realized PnL, hold time
- Color-coded rows

### Step 5: Tab 4 — Performance
- Summary cards
- Equity curve SVG (reuse `build_equity_curve_svg()`)
- Win rate, avg PnL, best/worst

### Step 6: CSS tabs + styling
- Dark theme consistent with existing dashboard
- Responsive layout

### Step 7: Keep Live Trades section
- Live (non-dry-run) trades keep the current simple table
- Only paper trades get the tabbed layout

### Step 8: Build + Test + Restart

## Files to modify
- `src/commands/smart.rs` — `build_live_dashboard()` + new HTML generation
- No new files needed

## Non-goals
- JavaScript (keep it CSS-only for simplicity + auto-refresh compat)
- Sorting/filtering beyond period tabs
- Editable positions (this is read-only dashboard)
