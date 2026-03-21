# PMCC Smart Money System — TODO

## Sprint 1-8: COMPLETE (see git history)

## Sprint 9：Market-First Smart Money Discovery

### Step 1: Gamma client integration
- [ ] Pass gamma::Client to smart command execute()
- [ ] Import gamma request types (EventsRequest, SearchRequest)

### Step 2: `smart discover-markets`
- [ ] `--tag` flag: browse by category (politics, crypto, economics, ai)
- [ ] `--search` flag: keyword search
- [ ] Display: question, volume, liquidity, prices, condition_id
- [ ] Sort by volume descending

### Step 3: `smart discover-whales`
- [ ] `--tag` flag: scan top markets in category → find top holders
- [ ] `--market` flag: scan specific market condition_id
- [ ] HoldersRequest for each market's condition_id
- [ ] Aggregate wallets across markets, rank by total position size
- [ ] `--min-position` threshold filter
- [ ] `--auto-watch` adds with tag "politics-holder" etc

### Step 4: `smart discover-auto`
- [ ] `--tags` multi-tag scan (politics,crypto,economics)
- [ ] Combines discover-markets + discover-whales
- [ ] `--auto-watch` auto-adds top holders
- [ ] Summary output

### Step 5: Build + Test
- [ ] `cargo check` pass
- [ ] `cargo test` pass
- [ ] Manual test: `smart discover-markets --tag politics`
- [ ] Manual test: `smart discover-whales --tag politics --auto-watch`
- [ ] Manual test: `smart discover-auto --tags "politics,crypto"`
- [ ] Release binary
- [ ] Restart monitor + dashboard
