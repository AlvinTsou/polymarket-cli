# Polymarket Smart Money Tracking & Copy-Trading System

> 基於 `polymarket-cli` fork 開發的聰明錢跟單系統

## 目標

1. 識別 Polymarket 上持續獲利的「聰明錢」錢包
2. 即時追蹤其持倉變化，產生交易信號
3. 提供跟單建議與半自動/自動下單能力

---

## 系統架構

```
┌─────────────────────────────────────────────────────────┐
│                  Smart Money Pipeline                    │
│                                                          │
│  Phase 1: 數據收集                                       │
│  ┌───────────┐  ┌────────────┐  ┌───────────────┐       │
│  │ 鯨魚錢包   │  │ 評論分析    │  │ 市場賠率變動   │       │
│  │ 追蹤      │  │ (已完成)    │  │ 監控          │       │
│  └─────┬─────┘  └─────┬──────┘  └───────┬───────┘       │
│        │               │                │                │
│  Phase 2: 分析引擎                                       │
│  ┌───────────────────────────────────────────────┐       │
│  │  聰明錢評分 → 信號產生 → 跟單建議               │       │
│  └───────────────────┬───────────────────────────┘       │
│                      │                                   │
│  Phase 3: 執行層                                         │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐            │
│  │ 通知推送   │  │ 自動下單   │  │ Dashboard │            │
│  │ (Telegram) │  │ (CLOB)    │  │ (Worker)  │            │
│  └───────────┘  └───────────┘  └───────────┘            │
└─────────────────────────────────────────────────────────┘
```

---

## Phase 1：聰明錢識別 + 數據收集

### 1.1 聰明錢發現

**資料來源**

| 來源 | CLI 指令 | 說明 |
|------|---------|------|
| PnL 排行榜 | `polymarket -o json data leaderboard --period month --order-by pnl --limit 50` | 月度獲利最高的錢包 |
| 交易量排行 | `polymarket -o json data leaderboard --period month --order-by volume --limit 50` | 大額交易者 |
| Builder 排行 | `polymarket -o json data builder-leaderboard --period week` | 積極參與的造市者 |
| 市場評論 | `polymarket -o json comments list --entity-type event --entity-id <ID>` | 高品質評論者（已有 review 指令） |

**評分模型 (`smart/scorer.rs`)**

```
Smart Score = w1 × 勝率 + w2 × ROI + w3 × 交易頻率 + w4 × 持倉分散度
```

| 指標 | 權重 (建議) | 資料來源 |
|------|-----------|---------|
| 勝率 (Win Rate) | 0.30 | `data trades` → 統計已結算的 PnL |
| ROI (投資報酬率) | 0.35 | `data value` + `data traded` |
| 交易頻率 | 0.15 | `data trades --limit 100` → 計算交易間隔 |
| 持倉分散度 | 0.20 | `data positions` → 不同市場數量 |

**輸出：Smart Wallets List**

```json
{
  "updated_at": "2026-03-07T12:00:00Z",
  "wallets": [
    {
      "address": "0xf5E6...",
      "score": 87.5,
      "win_rate": 0.72,
      "roi": 1.45,
      "total_traded": "$234,500",
      "active_positions": 12,
      "tag": "whale"
    }
  ]
}
```

### 1.2 持倉監控

定時掃描聰明錢的持倉變化，偵測：

| 信號類型 | 偵測方式 |
|---------|---------|
| 新建倉 (New Position) | 之前沒有 → 現在出現 |
| 加倉 (Increase) | 同市場持倉數量增加 |
| 減倉 (Decrease) | 同市場持倉數量減少 |
| 平倉 (Close) | 之前有 → 現在消失 |

**Shell Script 快速驗證版**

```bash
#!/bin/bash
# scripts/monitor-smart-wallets.sh

WALLETS_FILE="data/smart_wallets.txt"
SNAPSHOT_DIR="data/snapshots"
SIGNALS_FILE="data/signals.jsonl"

mkdir -p "$SNAPSHOT_DIR"

while IFS= read -r wallet; do
  current=$(polymarket -o json data positions "$wallet" 2>/dev/null)
  snapshot_file="$SNAPSHOT_DIR/$(echo "$wallet" | tr -d '0x').json"

  if [ -f "$snapshot_file" ]; then
    previous=$(cat "$snapshot_file")
    # 比對差異，產生信號
    diff_result=$(echo "$current" | jq --argjson prev "$previous" '
      # 比對邏輯：找出新增、移除、變化的持倉
      . as $curr |
      ($prev | map({(.conditionId): .}) | add // {}) as $prev_map |
      ($curr | map({(.conditionId): .}) | add // {}) as $curr_map |
      {
        new: [.[] | select(.conditionId as $id | $prev_map[$id] == null)],
        closed: [$prev[] | select(.conditionId as $id | $curr_map[$id] == null)]
      }
    ')

    new_count=$(echo "$diff_result" | jq '.new | length')
    closed_count=$(echo "$diff_result" | jq '.closed | length')

    if [ "$new_count" -gt 0 ] || [ "$closed_count" -gt 0 ]; then
      echo "{\"wallet\":\"$wallet\",\"timestamp\":\"$(date -u +%FT%TZ)\",\"new\":$new_count,\"closed\":$closed_count,\"detail\":$diff_result}" \
        >> "$SIGNALS_FILE"
    fi
  fi

  echo "$current" > "$snapshot_file"
done < "$WALLETS_FILE"
```

### 1.3 賠率異動監控

偵測特定市場的價格劇烈變動：

```bash
#!/bin/bash
# scripts/monitor-odds.sh

MARKETS_FILE="data/watch_markets.txt"  # token IDs, one per line
THRESHOLD=0.05  # 5% 價格變動

while IFS= read -r token_id; do
  price=$(polymarket -o json clob midpoint "$token_id" 2>/dev/null | jq -r '.mid')
  # 與上次比對，超過 threshold 就記錄
done < "$MARKETS_FILE"
```

---

## Phase 2：分析引擎

### 2.1 Rust 模組結構

```
src/
├── commands/
│   ├── smart.rs          # CLI 入口：polymarket smart <subcommand>
│   └── ...
├── smart/
│   ├── mod.rs
│   ├── tracker.rs        # 錢包持倉追蹤、快照比對
│   ├── scorer.rs         # 聰明錢評分模型
│   ├── signals.rs        # 信號偵測與產生
│   └── store.rs          # 本地資料儲存 (JSON files / SQLite)
├── output/
│   ├── smart.rs          # 信號輸出渲染
│   └── ...
```

### 2.2 新增 CLI 指令

```bash
# 發現聰明錢
polymarket smart discover --period month --limit 50
polymarket smart discover --min-roi 0.5 --min-trades 20

# 追蹤管理
polymarket smart watch 0xADDRESS          # 加入追蹤清單
polymarket smart unwatch 0xADDRESS        # 移除
polymarket smart list                      # 列出追蹤中的錢包

# 掃描信號
polymarket smart scan                      # 掃描所有追蹤錢包的持倉變化
polymarket smart scan --wallet 0xADDRESS   # 掃描特定錢包

# 查看信號
polymarket smart signals                   # 列出最近的信號
polymarket smart signals --market "bitcoin"

# 錢包分析
polymarket smart profile 0xADDRESS         # 詳細分析某錢包
```

### 2.3 信號格式

```json
{
  "id": "sig_20260307_001",
  "timestamp": "2026-03-07T14:30:00Z",
  "type": "new_position",
  "confidence": "high",
  "wallet": {
    "address": "0xf5E6...",
    "score": 87.5,
    "tag": "whale"
  },
  "market": {
    "question": "Will BTC hit $150k by June 2026?",
    "slug": "btc-150k-june-2026",
    "condition_id": "0xABC..."
  },
  "action": {
    "side": "buy",
    "outcome": "Yes",
    "price": 0.35,
    "size_usd": 5000
  },
  "context": {
    "smart_wallets_same_side": 3,
    "total_smart_money_volume": 15000,
    "market_volume_24h": 250000,
    "odds_change_1h": 0.03
  }
}
```

### 2.4 聚合信號（多個聰明錢同方向）

單一錢包動作可能是噪音，**多錢包同方向** 才是強信號：

| 信號強度 | 條件 |
|---------|------|
| Low | 1 個聰明錢建倉 |
| Medium | 2-3 個聰明錢同方向，24 小時內 |
| High | 4+ 個聰明錢同方向，或 1 個 Top-10 鯨魚大額建倉 (>$10K) |

---

## Phase 3：執行層

### 3.1 通知推送

**Telegram Bot（推薦）**

```bash
# 信號 → Telegram
polymarket smart scan -o json \
  | jq -r '.[] | "[\(.confidence)] \(.wallet.tag) \(.action.side) \(.market.question) @ \(.action.price)"' \
  | while read -r msg; do
      curl -s "https://api.telegram.org/bot$BOT_TOKEN/sendMessage" \
        -d "chat_id=$CHAT_ID" -d "text=$msg"
    done
```

**macOS 本地通知**

```bash
osascript -e "display notification \"$signal_msg\" with title \"Polymarket Signal\""
```

### 3.2 半自動跟單

收到 High 信號後，CLI 提示是否下單：

```
[HIGH] 3 whales bought "BTC > 150k" YES @ $0.35 (total $45K in 2h)

Current midpoint: 0.37  |  Your balance: $500 USDC

Follow this trade?
  1) Buy $50 (10% of balance)
  2) Buy $100 (20% of balance)
  3) Custom amount
  4) Skip

>
```

### 3.3 自動跟單（進階，需謹慎）

```bash
# 設定跟單規則
polymarket smart auto-follow \
  --max-per-trade 50 \
  --max-daily 200 \
  --min-confidence high \
  --min-score 80
```

安全機制：
- 每筆交易上限
- 每日總額上限
- 只跟 High confidence 信號
- Dry-run 模式（只記錄，不實際下單）

---

## 本地資料儲存

```
~/.config/polymarket/
├── config.json              # 既有：錢包設定
├── smart/
│   ├── wallets.json         # 追蹤中的聰明錢清單
│   ├── snapshots/           # 持倉快照（每次 scan 更新）
│   │   ├── 0xf5E6...json
│   │   └── 0xA1B2...json
│   ├── signals.jsonl        # 信號歷史記錄（append-only）
│   ├── scores.json          # 聰明錢評分快取
│   └── rules.json           # 自動跟單規則
```

---

## 開發順序

### Sprint 1：快速驗證（Shell Pipeline）

- [ ] 從排行榜抓取 Top 50 聰明錢地址
- [ ] 寫 shell script 定時掃描持倉變化
- [ ] 信號輸出到 `signals.jsonl`
- [ ] macOS 本地通知

**預期成果**：能每 5 分鐘掃描一次，有新信號時跳通知

### Sprint 2：Rust 模組化

- [ ] 新增 `smart` 指令群組（discover / watch / scan / signals）
- [ ] 實作 `scorer.rs` 聰明錢評分
- [ ] 實作 `tracker.rs` 快照比對
- [ ] 實作 `signals.rs` 信號產生
- [ ] 本地 JSON 儲存

**預期成果**：`polymarket smart scan` 一行指令完成全部流程

### Sprint 3：跟單執行

- [ ] Telegram Bot 通知
- [ ] 半自動跟單（CLI 互動確認）
- [ ] 信號聚合（多錢包同方向偵測）
- [ ] `polymarket smart profile` 錢包深度分析

### Sprint 4：自動化 + Dashboard

- [ ] 自動跟單模式（含安全限制）
- [ ] Cloudflare Worker Dashboard（擴展已有的 `worker/`）
- [ ] 歷史績效追蹤（跟單 ROI 統計）
- [ ] Dry-run 回測模式

---

## 風險與注意事項

| 風險 | 緩解措施 |
|------|---------|
| API Rate Limit | 控制掃描頻率，批次查詢，加入 backoff |
| 聰明錢反向操作 | 不盲目跟單，設定止損，分散跟單對象 |
| 延遲（信號到下單） | 用 Rust 直接呼叫 SDK 而非 shell pipeline |
| 資金安全 | 設定嚴格上限，先 dry-run 驗證策略 |
| 鏈上數據延遲 | CLOB 數據比鏈上快，優先用 CLOB API |
| Geoblock | 部分地區無法交易，先 `polymarket clob geoblock` 確認 |

---

## 現有可用資源

| 資源 | 狀態 | 說明 |
|------|------|------|
| `polymarket-client-sdk` | 已整合 | Rust SDK，支援 gamma / clob / data / bridge / ctf |
| `review` 指令 | 已完成（branch） | 市場評論 + 價格走勢 |
| `generate` 指令 | 已完成（branch） | 趨勢市場 HTML 產生器 |
| Cloudflare Worker | 已完成（branch） | 可擴展為 Dashboard |
| CLI JSON 輸出 | 已完成 | 所有指令支援 `-o json` |

---

## 技術決策記錄

| 決策 | 選擇 | 原因 |
|------|------|------|
| 資料儲存 | JSON files → 未來可升級 SQLite | 初期簡單，不需額外依賴 |
| 通知方式 | Telegram Bot | 免費、即時、支援手機 |
| 掃描頻率 | 5 分鐘 | 平衡 API 負載與信號即時性 |
| 跟單預設 | 半自動（需確認） | 安全優先 |
| 開發語言 | Rust（擴展現有 CLI） | 效能好、型別安全、共用 SDK |
