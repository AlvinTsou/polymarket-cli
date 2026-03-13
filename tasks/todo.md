# PMCC Smart Money System — TODO

## 環境設定

- [x] 安裝 Rust 工具鏈 (需要 >= 1.88.0) — 已安裝 1.94.0
- [x] `cargo check` 驗證編譯通過
- [x] `cargo test` 跑所有測試 — 148 tests passed

## Sprint 1：核心模組（已完成程式碼，待編譯驗證）

- [x] `src/smart/mod.rs` — 核心型別定義
- [x] `src/smart/store.rs` — 本地 JSON 儲存（wallets, snapshots, signals）
- [x] `src/smart/tracker.rs` — 持倉快照比對（New/Close/Increase/Decrease）
- [x] `src/smart/scorer.rs` — 聰明錢評分模型
- [x] `src/smart/signals.rs` — 持倉變化 → 交易信號
- [x] `src/commands/smart.rs` — CLI 指令群組
- [x] `src/output/smart.rs` — Table + JSON 輸出渲染
- [x] 接入 `main.rs`、`commands/mod.rs`、`output/mod.rs`
- [x] **編譯通過** (`cargo check`) — 修正 1 個型別錯誤 (i32→u64 cast)
- [x] **修正所有編譯錯誤**
- [x] **單元測試通過** (`cargo test`) — 99 unit + 49 integration = 148 passed
- [x] 手動測試 CLI 指令：
  - [x] `polymarket smart discover --period month --limit 10` — 正常顯示排行榜
  - [x] `polymarket smart watch 0x...` — 成功加入追蹤
  - [x] `polymarket smart list` — 正常列出追蹤錢包
  - [x] `polymarket smart scan` — 正常掃描持倉變化
  - [x] `polymarket smart signals` — 正常顯示信號（目前無信號）
  - [x] `polymarket smart profile 0x...` — 正常顯示錢包概況
  - [x] `polymarket smart unwatch 0x...` — 成功移除追蹤
- [x] Git commit — `2a4b692` on `feature/smart-money`

## Sprint 2：跟單信號強化

- [x] 信號聚合：多錢包同方向偵測（2 wallets=MED, 3+=HIGH）
- [x] `polymarket smart scan --notify` macOS 本地通知
- [x] Telegram Bot 推送整合 — setup/test/status + scan --notify 自動推送
- [x] 改善評分模型：加入勝率（closed positions 分析, 25% weight）

## Sprint 3：跟單執行

- [x] 半自動跟單：`polymarket smart follow` 互動選擇信號 + 確認下單
- [x] 自動跟單規則：`polymarket smart auto-follow --max-per-trade --max-per-day --min-confidence`
- [x] Dry-run 模式（`--dry-run`，預設開啟，只記錄不下單）
- [x] 每筆 / 每日上限安全機制（today_spend 追蹤，超限自動停止）
- [x] Follow 歷史記錄：`polymarket smart history`
- [x] PositionSnapshot / Signal 加入 asset (token_id) 用於下單

## Sprint 4：Dashboard + 回測

- [x] `smart roi` — 跟單 ROI 追蹤（entry vs current price, PnL 計算）
- [x] `smart backtest` — 信號回測（模擬不同金額/信心等級的回報）
- [x] `smart report` — HTML Dashboard（dark theme, auto-open in browser）
- [x] 快照價格查詢（current_price_map, load_all_snapshots）

## 現有分支狀態

- `feature/smart-money` — 目前工作分支（未編譯）
- `origin/claude/product-reviews-odds-tracking-J8WvL` — review + generate 指令（可 cherry-pick）

## 相關文件

- `docs/smart-money-system.md` — 完整系統規劃
