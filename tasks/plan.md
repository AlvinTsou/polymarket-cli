# Sprint 9: Market-First Smart Money Discovery

## Problem

Current approach (Sprint 1-8): Discover top wallets from leaderboard → watch → hope they trade political/economic markets.

**Result**: 28 watched wallets, 250+ cycles, 0 political/economic triggers. These wallets only trade sports.

## New Strategy: Market-First

Instead of wallet → market, reverse to **market → wallet**:

1. Find active political/economic/AI markets on Polymarket
2. For each market, query top holders (big positions)
3. Watch those wallets — they're proven to trade the markets we care about
4. When they open new positions on similar markets, trigger paper trade

## API Flow

```
EventsRequest (tag_slug="politics")
  → active events with markets
  → for each market condition_id:
      HoldersRequest(markets=[condition_id])
        → top holders with wallet addresses + position sizes
        → filter: position size > threshold
        → add to watch list with tag="politics-holder"

Repeat for tags: "crypto", "ai", "economics", etc.
```

## New Commands

### `smart discover-markets`

Find high-volume active markets by category:

```
polymarket smart discover-markets --tag politics --limit 10
polymarket smart discover-markets --search "trump tariff" --limit 5
```

Output:
```
--- Active Markets (politics) ---
  $2.3M vol  Trump wins 2028?           YES: 0.35  NO: 0.65
  $1.8M vol  Fed rate cut by June?      YES: 0.72  NO: 0.28
  $890K vol  TikTok ban upheld?         YES: 0.45  NO: 0.55
```

### `smart discover-whales`

Find top holders on a specific market or across markets by tag:

```
polymarket smart discover-whales --tag politics --min-position 500 --auto-watch
polymarket smart discover-whales --market 0xABC... --limit 20
```

Flow:
1. Get top markets by tag (or use specific market condition_id)
2. For each market, call HoldersRequest
3. Rank wallets by total position size across markets
4. Auto-watch those above threshold

Output:
```
--- Top Holders (politics markets, 8 markets scanned) ---
  0xABC...DEF  $12,500 across 5 markets  (politics-holder)
  0x123...789  $8,200 across 3 markets   (politics-holder)
  0xFED...321  $5,100 across 2 markets   (politics-holder)

Auto-watched 3 wallet(s) with position >= $500
```

### `smart discover-auto`

All-in-one: discover markets + find whales + watch + configure monitor:

```
polymarket smart discover-auto \
  --tags "politics,crypto,economics" \
  --min-position 500 \
  --markets-per-tag 10 \
  --auto-watch
```

## Implementation Steps

### Step 1: Gamma client integration

Currently `src/commands/smart.rs` only uses `data::Client`. Need to also use `gamma::Client` for market/event search.

- Check how gamma client is initialized in existing code
- Pass gamma client to new smart commands

### Step 2: `smart discover-markets` command

- Use `EventsRequest` with `tag_slug` for category browsing
- Use `SearchRequest` for keyword search
- Display: volume, liquidity, prices, question
- Sort by volume descending

### Step 3: `smart discover-whales` command

- Get top markets by tag (from Step 2)
- For each market, call `HoldersRequest` with condition_id
- Aggregate: group by wallet across markets
- Rank by total position size
- `--auto-watch` adds to watch list with tag like `"politics-holder"`

### Step 4: `smart discover-auto` command

- Combine Steps 2+3: for each tag, find markets → find holders → watch
- Print summary

### Step 5: WatchedWallet tag enhancement

- Tags like `"politics-holder"`, `"crypto-holder"` for market-first wallets
- vs existing `"leaderboard"` for performance-first wallets

### Step 6: Monitor knows both strategies

- Market-first wallets trigger on their specialty markets
- Leaderboard wallets trigger on any matching market
- Both use the same include/exclude filters

### Step 7: Build + Test

## Data Model

```
smart discover-markets --tag politics
  → EventsRequest(tag_slug="politics", limit=10, order=["volume"])
  → for each event.markets:
      display question, volume, prices

smart discover-whales --tag politics --min-position 500
  → same as above to get market condition_ids
  → HoldersRequest(markets=[cid1, cid2, ...], limit=50)
  → aggregate wallets, filter by min-position
  → store::add_wallet() with tag="politics-holder"
```

## Expected Outcome

Instead of 28 sports-focused wallets, we'll have wallets like:
- 5-10 wallets holding large positions on Trump/election markets
- 5-10 wallets active in crypto long-term markets
- 3-5 wallets in economics (Fed, GDP, recession)

When these wallets open NEW positions on similar markets, the monitor triggers — because they're proven to trade exactly the markets we care about.
