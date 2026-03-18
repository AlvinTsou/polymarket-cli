# PMCC Smart Money System — TODO

## Sprint 1-6: COMPLETE (see git history)

## Sprint 7：即時監控 + 條件觸發 + Paper Trading

### Step 1: Cargo.toml — tokio features
- [x] Add `time`, `net`, `signal`, `io-util` to tokio features

### Step 2: MonitorConfig type + store
- [x] `MonitorConfig` struct in `mod.rs` with all fields + Default
- [x] `load_monitor_config()` / `save_monitor_config()` in `store.rs`

### Step 3: Duration parser
- [x] `parse_duration()` — supports `30s`, `3m`, `1h` format

### Step 4: TriggerEvent + evaluate_triggers()
- [x] `TriggerEvent` and `TriggerType` types in `mod.rs`
- [x] `evaluate_triggers()` — filters by confidence, wallets, market keywords, odds threshold
- [x] Aggregated signal support (multi-wallet convergence)
- [x] Dedup: aggregated triggers skip covered individual signals

### Step 5: CLI subcommand
- [x] Add `Monitor { ... }` to `SmartCommand` with 12 flags
- [x] Wire in `execute()` dispatch

### Step 6: cmd_monitor() loop
- [x] tokio::time::interval loop with configurable duration
- [x] Scan wallets each cycle, generate signals, aggregate
- [x] Scan odds if threshold > 0
- [x] Evaluate triggers against config rules
- [x] Paper trade on trigger (dry-run FollowRecord with position_id)
- [x] Close positions on ClosePosition detection
- [x] Daily spend cap enforcement
- [x] Ctrl+C graceful shutdown via tokio::signal
- [x] Cycle summary line to stderr

### Step 7: Notification formatting
- [x] `build_monitor_notification()` for Telegram (trigger type + reason + paper count)
- [x] macOS osascript notification with Glass sound
- [x] `--save` / `--load` config persistence

### Step 8: Build + Test
- [x] `cargo check` pass (5 warnings, all pre-existing or trivial)
- [x] `cargo test` — 108 unit + 49 integration = 157 passed
- [x] Release binary built
- [x] CLI help verified
- [ ] Live test (user to run `smart monitor --interval 30s --notify --paper-trade`)
