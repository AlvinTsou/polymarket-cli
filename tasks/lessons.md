# PMCC Lessons Learned

## Rust / Code Quality

1. **`let _ =` on store operations = silent data loss.** Always `if let Err(e) = ... { eprintln!(...) }` for any disk I/O. `let _ =` is acceptable ONLY for fire-and-forget notifications (osascript, telegram).

2. **`fs::write()` is not crash-safe.** Use tmp file + `fs::rename()` pattern (`atomic_write`) for any file that gets rewritten (JSON state files, JSONL rewrites). Append-only files (jsonl append) are fine without this.

3. **Slice panics hide in format strings.** `&s[..10]`, `&addr[..8]`, `&token[..N]` — always use `.get(..N).unwrap_or(&s)` or check length first. These pass code review and tests but panic on unexpected short input.

4. **`Client::new()` has no timeout.** Always use `Client::builder().timeout(Duration::from_secs(5)).build()` for external API calls. A single hanging exchange will freeze the entire `tokio::join!`.

5. **u32 overflow in `hours * 60`.** Widen to u64 before multiply, or add CLI `value_parser` range bounds. Release builds silently wrap; debug builds panic.

## Exchange API Integration

6. **OKX contract size is NOT 1:1.** BTC-USDT-SWAP = 0.01 BTC/contract, ETH-USDT-SWAP = 0.1 ETH/contract. Forgetting this inflates OI by 100x (BTC) or 10x (ETH). Always check contract specs per exchange.

7. **Hyperliquid funding is per-hour, not per-8h.** Multiply by 8 before comparing with Binance/OKX/Bybit. HL also uses POST /info for ALL endpoints (not GET).

8. **Binance failure poisons aggregated averages.** If one exchange fails and you `unwrap_or(0.0)` then include it in the average, you dilute toward zero. Only count exchanges that actually returned data.

9. **All CEX/DEX public endpoints are free and keyless.** Binance, OKX, Hyperliquid, Bybit — no API key needed for order books, trades, funding rates, OI, liquidations. Rate limits are generous at 60s polling intervals.

10. **OKX liquidation API uses `bkPx` (bankruptcy price), not standard camelCase.** Need explicit `#[serde(rename = "bkPx")]`. Side semantics also inverted from Binance: OKX `side: "buy"` = long liquidated (forced sell).

## Signal Model

11. **Cross-exchange consensus reduces false signals.** When 4 exchanges agree on direction (OB imbalance + trade flow), the signal is much stronger than single-exchange data.

12. **Funding rate is a contrarian indicator.** Extreme positive funding = longs crowded = fade them. Normalize by ~2 stddev (0.0002 for BTC 8h rate), clamp to [-1, 1].

13. **Entry price at 0.50 for crypto paper trades is a fixed assumption.** P&L numbers are theoretical — actual market prices at entry may differ. Real trading would need CLOB midpoint at entry time.

## Dashboard / UI

14. **CSS radio buttons need unique IDs per section.** When splitting one tabbed section into two (Smart Money + Crypto), all radio `name` and `id` attributes must be prefixed (e.g., `sm-tab-*` vs `c5m-tab-*`) or they interfere globally.

15. **Telegram Markdown v1 breaks on `*_\`[]` in market titles.** Polymarket titles regularly contain these. Strip them via `telegram_safe()` before sending, or the API silently returns 400 and the notification is lost.

## osascript / macOS

16. **`replace('"', "\\\"")` is insufficient for osascript injection.** Backslashes, newlines, and null bytes can also break out of the AppleScript string. Use a whitelist approach: strip all `"`, `\`, `\n`, `\r`, `\0`.

## Monitor Operations

17. **JSONL files grow unbounded.** `signals.jsonl`, `odds_alerts.jsonl`, `price_history.jsonl` are append-only. Add periodic pruning (e.g., keep last 5000/2000 entries, prune every 100 monitor cycles).

18. **`cancelled_keys` HashSet grows forever in long-running monitor.** Convert to `HashMap<K, DateTime>` and prune entries older than 1 hour.

## Process

19. **Night Shift reports find real bugs.** R114 found 14 actionable issues including 5 P0s. Code review agent found 3 additional HIGH issues (timeouts, funding dilution, OKX OI scale). Run both regularly.

20. **Fix P0s before adding features.** Sprint 12 initially mixed feature work (futures/multi-exchange) with bug fixes. Better to fix all P0/P1 first, commit, then add features on a clean base.
