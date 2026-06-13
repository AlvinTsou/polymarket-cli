# PMCC — Session Resume

> Live state on top; the 2026-05-24 stop-loss-fix session is kept below as history.

## Live state (2026-06-13) — paper-trade validation harness SHIPPED on `main`

Deterministic offline test harness merged + pushed (`ac30569` merge, `4590968`
ticket). 3 pure helpers now used by live paths (`evaluate_exit`,
`classify_direction`, `classify_crypto_stats`); 184 tests green; fixtures in
`tests/fixtures/pmcc-paper/`; full report `docs/paper-validation-harness-report.md`.
Codex-confirmed the exit-loop refactor is behaviour-equivalent. (Built via Path B
after an AgentFlow `source-align` sprint aborted.)

**VERIFIED hardcoded exit constants** (now `const`s in `smart.rs`, test-pinned):
TP `+20%` / SL `-20%` / trailing-activate `+15%` / drawdown `30%` / time-stop
`7d & ROI<+5%`.

### Open TODOs (NOT yet done — both need a live sample, deferred from this session)

1. **Reconcile the `-45%` discrepancy.** `CLAUDE.md` § Money Safety AND this
   resume's history section below both say stop-loss `-45%` / drawdown `50%`, but
   the live hardcoded self-managed exit loop uses `-20%` / `30%` (verified + pinned
   by tests). Likely two layers (monitor.json risk params vs the hardcoded exit
   loop). Decide which is intended, then fix the docs OR the code to match — do NOT
   change the exit-loop behaviour blindly.
2. **Fix PM-P01** (`tasks/issues.md`): trailing-stop peak is wiped each monitor
   cycle when `position_id` is `None` (peak keyed by `condition_id`, but post-loop
   `peak_roi.retain` keys by `position_id`), so trailing never accumulates for
   those positions. This is a BEHAVIOUR change → out of scope for the frozen test
   sprint; verify the fix against a live sample.

---

# PMCC Stop-Loss Fix — Session Resume (history, 2026-05-24)

**Date**: 2026-05-24 ~ 2026-05-25
**Branch**: `main`
**Last commit**: `f9ba1ee feat(monitor): guard against atomic settlement losses`

## Problem

Paper trading 從 5/19 +4.18% ROI 退到 5/24 +2.8% ROI。調查發現多筆 -99% 全虧，根因：
**Polymarket atomic settlement** — price 在結算瞬間從中段直接跳到 0 或 1，monitor 60s cycle 來不及在 -45% 抓到，下一輪檢查時 ROI 已經 -99% 才觸發 stop-loss。

## Diagnosis (verified)

1. **單一 signal 路徑** (`src/commands/smart.rs:4094`) 有呼叫 `market_within_horizon(14)`，但 title parser 太脆弱：
   - 只認月份字串 (january–december) 或 "end of YYYY"
   - "2026 Crunchyroll Anime Awards" 沒有月份 → 預設 `return true`（放行）
2. **Aggregated trigger 路徑** (`src/commands/smart.rs:4023`) **完全沒呼叫** `market_within_horizon`
3. **`end_date_iso` API 從未被查詢** — title heuristic 是唯一保護

## Fixes implemented (commit `f9ba1ee`)

### Code changes (`src/commands/smart.rs`)

1. **Aggregated trigger horizon check** (line ~4051):
   ```rust
   if !market_within_horizon(&agg.market_title, 14) { continue; }
   ```

2. **New async helper `market_resolves_within_hours`** (line ~3976):
   - 用 `gamma_client.markets(condition_ids=[cid])` 查 `end_date` / `end_date_iso`
   - 回傳 `Some(true|false)` 或 `None` (no metadata)

3. **Execution-phase NEAR-RESOLUTION guard** (line ~4625):
   ```rust
   if let Some(true) = market_resolves_within_hours(&trigger.condition_id, 24).await {
       eprintln!("  NEAR-RESOLUTION (skip): {}", trigger.market_title);
       continue;
   }
   ```

### Config changes (`~/.config/polymarket/smart/monitor.json`, not in git)

加入 10 個 exclude keywords:
```
anime awards, anime award, crunchyroll, voice artist,
best drama, best new series, best opening sequence,
best ending sequence, best anime
```

### NOT done (intentional)

- **P2 monitor frequency**: 60s vs 30s 對 atomic settle 都無效（沒中介價），純增加 API 負擔
- **Pre-resolution close**: variance reduction，不是 bug fix；期望值為 0

## Deployment ordeal

1. `cargo build --release` 重建後，`launchctl kickstart -k com.pmcc.monitor` 重啟
2. **新 process 卡在 dyld `__open` 5+ 分鐘**，0% CPU
3. 直接執行 binary 可以跑（<5s 出 version），只有 LaunchAgent 路徑卡死
4. `log show --predicate 'process == "syspolicyd"'` 揭露：
   ```
   GatekeeperPolicyScanError Code=-67018 "Code did not match any currently allowed policy"
   tccd: Service kTCCServiceSystemPolicyAllFiles does not allow prompting; recording denied
   ```
5. **根因**: 舊 binary 在「完整磁碟取用權限」白名單；rebuild 後 ad-hoc signature hash 改變，TCC 視為新 binary；背景 session 不能彈窗 → 靜默拒絕 → dyld 卡死
6. **Fix**: System Settings → Privacy & Security → Full Disk Access → 移除舊 entry，加入 `/Users/alvintsou/Documents/Projects/PMCC/target/release/polymarket`
7. `launchctl bootstrap` 後 monitor 正常啟動，banner 顯示新 Exclude 字串

## Verified after deploy

- Monitor 跑滿 133+ cycles，無 crash
- `=== PMCC Monitor ===` banner 出現
- `Exclude:` 行末尾包含新 anime/awards 關鍵字
- TCP 連線正常（與 104.18.34.205:443 ESTABLISHED）
- 12 筆新 paper trades 已產生
- **0 個 NEAR-RESOLUTION skip** — 合理，4.5h 內 confirmed triggers 都不是 24h 內結算市場

## New problem discovered (2026-05-25)

**24 個 zombie open 部位，藏 -$140.59 ghost PnL**

- 用 `https://gamma-api.polymarket.com/markets?condition_ids=<cid>&closed=true` 掃描 76 個 open 部位
- **24 個對應市場已結算**（有的 4 月就結算了），但 `follows.jsonl` 仍標 Open
- 15 個全虧 (-$10/筆)，9 個小賺，淨額 -$140.59

**根因** (`src/commands/smart.rs:4366`):
```rust
if current <= 0.0 { continue; }  // 跳過 price=0 的部位 → 永不平倉
```
- closed market 沒有 order book，clob midpoint 回 0 或失敗
- 此 guard 把這些部位永遠 skip 過

**真實 PnL（修正後）**:
| 報表 | 修正 |
|---|---|
| Total PnL: +$338.01 (+2.6%) | **+$197.42 (+1.51%)** |
| Realized: +$418.68 | Realized: +$278.09 |

## Auto-close mechanism — 三種觸發

| 觸發 | 狀態 | 目的 |
|---|---|---|
| 進場守門 NEAR-RESOLUTION on entry | done: deployed in `f9ba1ee` | 阻擋 24h 內結算市場新進場 |
| 持倉清算 market-closed sweep | done: deployed in `f9d5cbb` | 已結算市場的舊持倉收尾 |
| 預先平倉 pre-resolution close | pending (optional) | 結算前主動平倉避 atomic settle |

## Follow-up status (updated 2026-06-09)

### A. 一次性 reconcile 指令（done）
- `polymarket smart reconcile` 已新增
- 掃所有 dry-run open 部位 → 查 gamma → `closed=true` 則以 `outcomePrices` 平倉
- 已作為 settled-position cleanup 的手動入口

### B. 永久內建在 monitor cycle（done）
- `price <= 0` / missing live price 不再直接讓 closed market 永久 skip
- 若 gamma 顯示 closed → 平倉 with reason `market-closed: settled @ X`
- 使用 per-cycle cache 避免同一輪重複呼叫

### C. (Optional) pre-resolution close（still pending）
- 不建議優先做；先用 current paper-trade analysis 與 strategy result ledger 判斷是否值得新增

## Files / artifacts

- Patch slice tool used: `git diff` → `sed -n '1,60p'` → `git apply --cached` (skip unrelated crypto WIP at `src/commands/smart.rs:6336+` and `src/crypto/market.rs`)
- Stale note cleanup: working tree is now clean on `main` and synced with `origin/main` at `41861c2`
- `tasks/todo.md` refreshed on 2026-06-09 to remove stale branch/runtime assumptions

## Reference data points

- Last memory snapshot (5/19): 1158 trades, 60.8% win, +$462 / +4.18% ROI
- Pre-fix (5/24): 1229 closed, 60% win, +$367.91 / +2.8% ROI
- Post-fix dashboard (5/25): 1240 closed, 60% win, +$338.01 / +2.6% ROI
- Post-fix REAL (zombie-corrected): **+$197.42 / +1.51% ROI**

## macOS gotcha (記得寫進 lessons.md 或 CLAUDE.md)

每次 `cargo build --release` 後重啟 LaunchAgent，需要先到 **System Settings → Privacy & Security → Full Disk Access** 把舊 polymarket 移除、新 binary 加回。否則 TCC 靜默拒絕，dyld 卡 `__open` 無限重啟。

可考慮的根本解法（未實作）：
- 用穩定 codesign identity（需付費 Apple Developer cert）
- 或 LaunchAgent 改成不在 FDA 名單下（移除依賴 — 但 monitor 需讀 `~/.config/polymarket/`，Sandbox 不允許）
