use anyhow::{Context, Result};
use reqwest::Client;

use super::{AggregatedSpot, Candle, CryptoAsset, FuturesData, Liquidation, OrderBook, OrderBookLevel, Trade};

const BINANCE_BASE: &str = "https://api.binance.com/api/v3";
const BINANCE_FAPI: &str = "https://fapi.binance.com";

/// Shared HTTP client for Binance API calls.
pub struct BinanceFeed {
    client: Client,
}

impl BinanceFeed {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetch OHLCV klines (candles).
    ///
    /// `interval`: "1m", "5m", "15m", etc.
    /// `limit`: max 1000
    pub async fn fetch_klines(
        &self,
        asset: CryptoAsset,
        interval: &str,
        limit: u32,
    ) -> Result<Vec<Candle>> {
        let url = format!(
            "{BINANCE_BASE}/klines?symbol={}&interval={interval}&limit={limit}",
            asset.symbol()
        );
        let resp: Vec<Vec<serde_json::Value>> = self
            .client
            .get(&url)
            .send()
            .await
            .context("binance klines request failed")?
            .json()
            .await
            .context("binance klines parse failed")?;

        let candles = resp
            .into_iter()
            .filter_map(|k| {
                if k.len() < 7 {
                    return None;
                }
                Some(Candle {
                    open_time: k[0].as_i64()?,
                    open: k[1].as_str()?.parse().ok()?,
                    high: k[2].as_str()?.parse().ok()?,
                    low: k[3].as_str()?.parse().ok()?,
                    close: k[4].as_str()?.parse().ok()?,
                    volume: k[5].as_str()?.parse().ok()?,
                    close_time: k[6].as_i64()?,
                })
            })
            .collect();
        Ok(candles)
    }

    /// Fetch order book depth.
    ///
    /// `limit`: 5, 10, 20, 50, 100, 500, 1000
    pub async fn fetch_depth(&self, asset: CryptoAsset, limit: u32) -> Result<OrderBook> {
        let url = format!(
            "{BINANCE_BASE}/depth?symbol={}&limit={limit}",
            asset.symbol()
        );

        #[derive(serde::Deserialize)]
        struct DepthResp {
            bids: Vec<Vec<String>>,
            asks: Vec<Vec<String>>,
        }

        let resp: DepthResp = self
            .client
            .get(&url)
            .send()
            .await
            .context("binance depth request failed")?
            .json()
            .await
            .context("binance depth parse failed")?;

        let parse_levels = |raw: Vec<Vec<String>>| -> Vec<OrderBookLevel> {
            raw.into_iter()
                .filter_map(|l| {
                    if l.len() < 2 {
                        return None;
                    }
                    Some(OrderBookLevel {
                        price: l[0].parse().ok()?,
                        qty: l[1].parse().ok()?,
                    })
                })
                .collect()
        };

        Ok(OrderBook {
            bids: parse_levels(resp.bids),
            asks: parse_levels(resp.asks),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    /// Fetch recent trades.
    ///
    /// `limit`: max 1000
    pub async fn fetch_trades(&self, asset: CryptoAsset, limit: u32) -> Result<Vec<Trade>> {
        let url = format!(
            "{BINANCE_BASE}/trades?symbol={}&limit={limit}",
            asset.symbol()
        );

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct TradeResp {
            price: String,
            qty: String,
            is_buyer_maker: bool,
            time: i64,
        }

        let resp: Vec<TradeResp> = self
            .client
            .get(&url)
            .send()
            .await
            .context("binance trades request failed")?
            .json()
            .await
            .context("binance trades parse failed")?;

        let trades = resp
            .into_iter()
            .filter_map(|t| {
                Some(Trade {
                    price: t.price.parse().ok()?,
                    qty: t.qty.parse().ok()?,
                    is_buyer_maker: t.is_buyer_maker,
                    time: t.time,
                })
            })
            .collect();
        Ok(trades)
    }
}

/// Binance Futures (FAPI) data feed — funding rate, open interest, liquidations.
pub struct BinanceFuturesFeed {
    client: Client,
}

impl BinanceFuturesFeed {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Futures symbol for an asset (e.g. BTCUSDT).
    fn futures_symbol(asset: CryptoAsset) -> &'static str {
        match asset {
            CryptoAsset::BTC => "BTCUSDT",
            CryptoAsset::ETH => "ETHUSDT",
        }
    }

    /// Fetch current funding rate and mark price from premiumIndex.
    pub async fn fetch_funding_rate(&self, asset: CryptoAsset) -> Result<(f64, f64)> {
        let symbol = Self::futures_symbol(asset);
        let url = format!("{BINANCE_FAPI}/fapi/v1/premiumIndex?symbol={symbol}");

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PremiumIndex {
            last_funding_rate: String,
            mark_price: String,
        }

        let resp: PremiumIndex = self
            .client
            .get(&url)
            .send()
            .await
            .context("binance futures premiumIndex request failed")?
            .json()
            .await
            .context("binance futures premiumIndex parse failed")?;

        let rate = resp.last_funding_rate.parse::<f64>().unwrap_or(0.0);
        let mark = resp.mark_price.parse::<f64>().unwrap_or(0.0);
        Ok((rate, mark))
    }

    /// Fetch current open interest in contracts, then convert to USDT.
    pub async fn fetch_open_interest(&self, asset: CryptoAsset) -> Result<f64> {
        let symbol = Self::futures_symbol(asset);
        let url = format!("{BINANCE_FAPI}/fapi/v1/openInterest?symbol={symbol}");

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct OiResp {
            open_interest: String,
        }

        let resp: OiResp = self
            .client
            .get(&url)
            .send()
            .await
            .context("binance futures openInterest request failed")?
            .json()
            .await
            .context("binance futures openInterest parse failed")?;

        let oi = resp.open_interest.parse::<f64>().unwrap_or(0.0);
        Ok(oi)
    }

    /// Fetch recent forced liquidation orders (last ~100).
    pub async fn fetch_liquidations(&self, asset: CryptoAsset) -> Result<Vec<Liquidation>> {
        let symbol = Self::futures_symbol(asset);
        let url = format!(
            "{BINANCE_FAPI}/fapi/v1/allForceOrders?symbol={symbol}&limit=100"
        );

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct LiqOrder {
            side: String,
            price: String,
            original_qty: String,
            time: i64,
        }

        let resp: Vec<LiqOrder> = self
            .client
            .get(&url)
            .send()
            .await
            .context("binance futures allForceOrders request failed")?
            .json()
            .await
            .context("binance futures allForceOrders parse failed")?;

        let liqs = resp
            .into_iter()
            .filter_map(|l| {
                Some(Liquidation {
                    side: l.side,
                    price: l.price.parse().ok()?,
                    qty: l.original_qty.parse().ok()?,
                    time: l.time,
                })
            })
            .collect();
        Ok(liqs)
    }

    /// Fetch all futures data in parallel. Returns None-equivalent FuturesData on error.
    pub async fn fetch_all(&self, asset: CryptoAsset) -> Result<FuturesData> {
        let (funding_res, oi_res, liq_res) = tokio::join!(
            self.fetch_funding_rate(asset),
            self.fetch_open_interest(asset),
            self.fetch_liquidations(asset),
        );

        let (funding_rate, mark_price) = funding_res.unwrap_or((0.0, 0.0));
        let oi_contracts = oi_res.unwrap_or(0.0);
        let liquidations = liq_res.unwrap_or_default();

        // Convert OI from contracts to USDT using mark price
        let oi_usd = if mark_price > 0.0 {
            oi_contracts * mark_price
        } else {
            0.0
        };

        Ok(FuturesData {
            funding_rate,
            mark_price,
            open_interest_usd: oi_usd,
            liquidations,
        })
    }
}

// ── OKX Feed ────────────────────────────────────────────────────

const OKX_BASE: &str = "https://www.okx.com";

/// OKX spot + swap data feed.
pub struct OkxFeed {
    client: Client,
}

impl OkxFeed {
    pub fn new() -> Self {
        Self { client: Client::builder().timeout(std::time::Duration::from_secs(5)).build().unwrap_or_default() }
    }

    fn spot_inst(asset: CryptoAsset) -> &'static str {
        match asset {
            CryptoAsset::BTC => "BTC-USDT",
            CryptoAsset::ETH => "ETH-USDT",
        }
    }

    fn swap_inst(asset: CryptoAsset) -> &'static str {
        match asset {
            CryptoAsset::BTC => "BTC-USDT-SWAP",
            CryptoAsset::ETH => "ETH-USDT-SWAP",
        }
    }

    /// Fetch order book (up to 400 levels).
    pub async fn fetch_orderbook(&self, asset: CryptoAsset) -> Result<OrderBook> {
        let inst = Self::spot_inst(asset);
        let url = format!("{OKX_BASE}/api/v5/market/books?instId={inst}&sz=20");

        #[derive(serde::Deserialize)]
        struct Resp { data: Vec<BookData> }
        #[derive(serde::Deserialize)]
        struct BookData { bids: Vec<Vec<String>>, asks: Vec<Vec<String>> }

        let resp: Resp = self.client.get(&url).send().await
            .context("okx books request failed")?
            .json().await
            .context("okx books parse failed")?;

        let book = resp.data.into_iter().next().context("okx books empty")?;
        let parse = |raw: Vec<Vec<String>>| -> Vec<OrderBookLevel> {
            raw.into_iter().filter_map(|l| {
                Some(OrderBookLevel { price: l.first()?.parse().ok()?, qty: l.get(1)?.parse().ok()? })
            }).collect()
        };

        Ok(OrderBook {
            bids: parse(book.bids),
            asks: parse(book.asks),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    /// Fetch recent trades (up to 500).
    pub async fn fetch_trades(&self, asset: CryptoAsset) -> Result<Vec<Trade>> {
        let inst = Self::spot_inst(asset);
        let url = format!("{OKX_BASE}/api/v5/market/trades?instId={inst}&limit=500");

        #[derive(serde::Deserialize)]
        struct Resp { data: Vec<TradeData> }
        #[derive(serde::Deserialize)]
        struct TradeData { px: String, sz: String, side: String, ts: String }

        let resp: Resp = self.client.get(&url).send().await
            .context("okx trades request failed")?
            .json().await
            .context("okx trades parse failed")?;

        let trades = resp.data.into_iter().filter_map(|t| {
            Some(Trade {
                price: t.px.parse().ok()?,
                qty: t.sz.parse().ok()?,
                is_buyer_maker: t.side == "sell", // OKX: side=sell means taker sold
                time: t.ts.parse().ok()?,
            })
        }).collect();
        Ok(trades)
    }

    /// Fetch current funding rate for the perpetual swap.
    pub async fn fetch_funding_rate(&self, asset: CryptoAsset) -> Result<f64> {
        let inst = Self::swap_inst(asset);
        let url = format!("{OKX_BASE}/api/v5/public/funding-rate?instId={inst}");

        #[derive(serde::Deserialize)]
        struct Resp { data: Vec<FrData> }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct FrData { funding_rate: String }

        let resp: Resp = self.client.get(&url).send().await
            .context("okx funding-rate request failed")?
            .json().await
            .context("okx funding-rate parse failed")?;

        let rate = resp.data.first()
            .and_then(|d| d.funding_rate.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(rate)
    }

    /// Fetch open interest for the perpetual swap (in contracts → convert to USD).
    pub async fn fetch_open_interest(&self, asset: CryptoAsset) -> Result<f64> {
        let inst = Self::swap_inst(asset);
        let url = format!("{OKX_BASE}/api/v5/public/open-interest?instType=SWAP&instId={inst}");

        #[derive(serde::Deserialize)]
        struct Resp { data: Vec<OiData> }
        #[derive(serde::Deserialize)]
        struct OiData { oi: String }

        let resp: Resp = self.client.get(&url).send().await
            .context("okx open-interest request failed")?
            .json().await
            .context("okx open-interest parse failed")?;

        // OI is in contracts; for USDT-margined, 1 contract = 1 unit of base asset
        // We return raw value — caller can multiply by price if needed
        let oi = resp.data.first()
            .and_then(|d| d.oi.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(oi)
    }

    /// Fetch recent liquidation orders.
    pub async fn fetch_liquidations(&self, asset: CryptoAsset) -> Result<Vec<Liquidation>> {
        let inst = Self::swap_inst(asset);
        let url = format!("{OKX_BASE}/api/v5/public/liquidation-orders?instType=SWAP&instId={inst}&limit=100&state=filled");

        #[derive(serde::Deserialize)]
        struct Resp { data: Vec<LiqWrapper> }
        #[derive(serde::Deserialize)]
        struct LiqWrapper { details: Vec<LiqDetail> }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct LiqDetail { side: String, bkPx: String, sz: String, ts: String }

        let resp: Resp = self.client.get(&url).send().await
            .context("okx liquidation-orders request failed")?
            .json().await
            .context("okx liquidation-orders parse failed")?;

        let mut liqs = Vec::new();
        for wrapper in resp.data {
            for d in wrapper.details {
                if let (Ok(price), Ok(qty), Ok(time)) = (d.bkPx.parse::<f64>(), d.sz.parse::<f64>(), d.ts.parse::<i64>()) {
                    // OKX side: "buy" = long liquidated (forced sell), "sell" = short liquidated (forced buy)
                    let normalized_side = if d.side == "buy" { "SELL".to_string() } else { "BUY".to_string() };
                    liqs.push(Liquidation { side: normalized_side, price, qty, time });
                }
            }
        }
        Ok(liqs)
    }
}

// ── Hyperliquid Feed ────────────────────────────────────────────

const HL_BASE: &str = "https://api.hyperliquid.xyz";

/// Hyperliquid perpetual DEX data feed. All endpoints use POST /info.
pub struct HyperliquidFeed {
    client: Client,
}

impl HyperliquidFeed {
    pub fn new() -> Self {
        Self { client: Client::builder().timeout(std::time::Duration::from_secs(5)).build().unwrap_or_default() }
    }

    fn coin(asset: CryptoAsset) -> &'static str {
        match asset {
            CryptoAsset::BTC => "BTC",
            CryptoAsset::ETH => "ETH",
        }
    }

    /// Fetch L2 order book.
    pub async fn fetch_orderbook(&self, asset: CryptoAsset) -> Result<OrderBook> {
        let body = serde_json::json!({ "type": "l2Book", "coin": Self::coin(asset) });
        let resp: serde_json::Value = self.client
            .post(&format!("{HL_BASE}/info"))
            .json(&body)
            .send().await.context("hl l2Book request failed")?
            .json().await.context("hl l2Book parse failed")?;

        let levels = resp.get("levels").and_then(|l| l.as_array());
        let parse_side = |idx: usize| -> Vec<OrderBookLevel> {
            levels.and_then(|l| l.get(idx)).and_then(|s| s.as_array())
                .map(|arr| arr.iter().filter_map(|entry| {
                    Some(OrderBookLevel {
                        price: entry.get("px")?.as_str()?.parse().ok()?,
                        qty: entry.get("sz")?.as_str()?.parse().ok()?,
                    })
                }).collect())
                .unwrap_or_default()
        };

        Ok(OrderBook {
            bids: parse_side(0),
            asks: parse_side(1),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    /// Fetch funding rate and open interest from metaAndAssetCtxs.
    pub async fn fetch_meta(&self, asset: CryptoAsset) -> Result<(f64, f64)> {
        let body = serde_json::json!({ "type": "metaAndAssetCtxs" });
        let resp: serde_json::Value = self.client
            .post(&format!("{HL_BASE}/info"))
            .json(&body)
            .send().await.context("hl meta request failed")?
            .json().await.context("hl meta parse failed")?;

        // Response is [meta, [assetCtx, ...]] — find coin by index matching meta.universe
        let coin = Self::coin(asset);
        let meta = resp.as_array().and_then(|a| a.first());
        let ctxs = resp.as_array().and_then(|a| a.get(1)).and_then(|a| a.as_array());

        let idx = meta
            .and_then(|m| m.get("universe")).and_then(|u| u.as_array())
            .and_then(|universe| universe.iter().position(|item|
                item.get("name").and_then(|n| n.as_str()) == Some(coin)
            ));

        let (mut funding, mut oi) = (0.0, 0.0);
        if let (Some(i), Some(ctxs)) = (idx, ctxs) {
            if let Some(ctx) = ctxs.get(i) {
                funding = ctx.get("funding").and_then(|f| f.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                oi = ctx.get("openInterest").and_then(|o| o.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            }
        }

        Ok((funding, oi))
    }
}

// ── Bybit Feed ──────────────────────────────────────────────────

const BYBIT_BASE: &str = "https://api.bybit.com";

/// Bybit v5 unified API data feed.
pub struct BybitFeed {
    client: Client,
}

impl BybitFeed {
    pub fn new() -> Self {
        Self { client: Client::builder().timeout(std::time::Duration::from_secs(5)).build().unwrap_or_default() }
    }

    fn symbol(asset: CryptoAsset) -> &'static str {
        match asset {
            CryptoAsset::BTC => "BTCUSDT",
            CryptoAsset::ETH => "ETHUSDT",
        }
    }

    /// Fetch order book (spot, up to 200 levels).
    pub async fn fetch_orderbook(&self, asset: CryptoAsset) -> Result<OrderBook> {
        let symbol = Self::symbol(asset);
        let url = format!("{BYBIT_BASE}/v5/market/orderbook?category=spot&symbol={symbol}&limit=50");

        #[derive(serde::Deserialize)]
        struct Resp { result: BookResult }
        #[derive(serde::Deserialize)]
        struct BookResult { b: Vec<Vec<String>>, a: Vec<Vec<String>> }

        let resp: Resp = self.client.get(&url).send().await
            .context("bybit orderbook request failed")?
            .json().await
            .context("bybit orderbook parse failed")?;

        let parse = |raw: Vec<Vec<String>>| -> Vec<OrderBookLevel> {
            raw.into_iter().filter_map(|l| {
                Some(OrderBookLevel { price: l.first()?.parse().ok()?, qty: l.get(1)?.parse().ok()? })
            }).collect()
        };

        Ok(OrderBook {
            bids: parse(resp.result.b),
            asks: parse(resp.result.a),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    /// Fetch recent trades (spot, up to 1000).
    pub async fn fetch_trades(&self, asset: CryptoAsset) -> Result<Vec<Trade>> {
        let symbol = Self::symbol(asset);
        let url = format!("{BYBIT_BASE}/v5/market/recent-trade?category=spot&symbol={symbol}&limit=500");

        #[derive(serde::Deserialize)]
        struct Resp { result: TradeResult }
        #[derive(serde::Deserialize)]
        struct TradeResult { list: Vec<TradeData> }
        #[derive(serde::Deserialize)]
        struct TradeData {
            #[serde(rename = "p")] price: String,
            #[serde(rename = "v")] qty: String,
            #[serde(rename = "S")] side: String,
            #[serde(rename = "T")] time: i64,
        }

        let resp: Resp = self.client.get(&url).send().await
            .context("bybit trades request failed")?
            .json().await
            .context("bybit trades parse failed")?;

        let trades = resp.result.list.into_iter().filter_map(|t| {
            Some(Trade {
                price: t.price.parse().ok()?,
                qty: t.qty.parse().ok()?,
                is_buyer_maker: t.side == "Sell", // Bybit: "Sell" = taker sold
                time: t.time,
            })
        }).collect();
        Ok(trades)
    }

    /// Fetch latest funding rate (linear perpetual).
    pub async fn fetch_funding_rate(&self, asset: CryptoAsset) -> Result<f64> {
        let symbol = Self::symbol(asset);
        let url = format!("{BYBIT_BASE}/v5/market/funding/history?category=linear&symbol={symbol}&limit=1");

        #[derive(serde::Deserialize)]
        struct Resp { result: FrResult }
        #[derive(serde::Deserialize)]
        struct FrResult { list: Vec<FrData> }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct FrData { funding_rate: String }

        let resp: Resp = self.client.get(&url).send().await
            .context("bybit funding request failed")?
            .json().await
            .context("bybit funding parse failed")?;

        let rate = resp.result.list.first()
            .and_then(|d| d.funding_rate.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(rate)
    }

    /// Fetch open interest (linear perpetual).
    pub async fn fetch_open_interest(&self, asset: CryptoAsset) -> Result<f64> {
        let symbol = Self::symbol(asset);
        let url = format!("{BYBIT_BASE}/v5/market/open-interest?category=linear&symbol={symbol}&intervalTime=5min&limit=1");

        #[derive(serde::Deserialize)]
        struct Resp { result: OiResult }
        #[derive(serde::Deserialize)]
        struct OiResult { list: Vec<OiData> }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct OiData { open_interest: String }

        let resp: Resp = self.client.get(&url).send().await
            .context("bybit OI request failed")?
            .json().await
            .context("bybit OI parse failed")?;

        let oi = resp.result.list.first()
            .and_then(|d| d.open_interest.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(oi)
    }
}

// ── Multi-Exchange Aggregator ───────────────────────────────────

/// Fetch order books and trades from Binance + OKX + Hyperliquid in parallel,
/// then compute aggregated imbalance metrics.
pub async fn fetch_aggregated_spot(
    binance: &BinanceFeed,
    okx: &OkxFeed,
    hl: &HyperliquidFeed,
    bybit: &BybitFeed,
    asset: CryptoAsset,
) -> AggregatedSpot {
    let (b_ob, b_tr, o_ob, o_tr, h_ob, by_ob, by_tr) = tokio::join!(
        binance.fetch_depth(asset, 20),
        binance.fetch_trades(asset, 500),
        okx.fetch_orderbook(asset),
        okx.fetch_trades(asset),
        hl.fetch_orderbook(asset),
        bybit.fetch_orderbook(asset),
        bybit.fetch_trades(asset),
    );

    let mut agg = AggregatedSpot::default();

    // Collect successful order books
    if let Ok(ob) = b_ob { agg.orderbooks.push(("binance".into(), ob)); }
    if let Ok(ob) = o_ob { agg.orderbooks.push(("okx".into(), ob)); }
    if let Ok(ob) = h_ob { agg.orderbooks.push(("hyperliquid".into(), ob)); }
    if let Ok(ob) = by_ob { agg.orderbooks.push(("bybit".into(), ob)); }

    // Collect successful trades
    if let Ok(tr) = b_tr { agg.trades.push(("binance".into(), tr)); }
    if let Ok(tr) = o_tr { agg.trades.push(("okx".into(), tr)); }
    if let Ok(tr) = by_tr { agg.trades.push(("bybit".into(), tr)); }

    agg.exchange_count = agg.orderbooks.len() as u32;

    // Compute merged OB imbalance: sum bid/ask volumes across all exchanges
    let (mut total_bid, mut total_ask) = (0.0f64, 0.0f64);
    for (_, ob) in &agg.orderbooks {
        total_bid += ob.bids.iter().map(|l| l.qty).sum::<f64>();
        total_ask += ob.asks.iter().map(|l| l.qty).sum::<f64>();
    }
    let total_ob = total_bid + total_ask;
    agg.merged_ob_imbalance = if total_ob > 0.0 { (total_bid - total_ask) / total_ob } else { 0.0 };

    // Compute merged trade flow: aggregate buy/sell volume across all exchanges
    let (mut total_buy, mut total_sell) = (0.0f64, 0.0f64);
    for (_, trades) in &agg.trades {
        for t in trades {
            let notional = t.price * t.qty;
            if t.is_buyer_maker { total_sell += notional; } else { total_buy += notional; }
        }
    }
    let total_flow = total_buy + total_sell;
    agg.merged_trade_flow = if total_flow > 0.0 { (total_buy - total_sell) / total_flow } else { 0.0 };

    agg
}

/// Fetch futures-grade data from Binance FAPI + OKX + Hyperliquid in parallel.
/// Merges funding rates (average) and OI (sum), combines liquidations.
pub async fn fetch_aggregated_futures(
    binance_fut: &BinanceFuturesFeed,
    okx: &OkxFeed,
    hl: &HyperliquidFeed,
    bybit: &BybitFeed,
    asset: CryptoAsset,
) -> FuturesData {
    let (b_all, o_fr, o_oi, o_liq, h_meta, by_fr, by_oi) = tokio::join!(
        binance_fut.fetch_all(asset),
        okx.fetch_funding_rate(asset),
        okx.fetch_open_interest(asset),
        okx.fetch_liquidations(asset),
        hl.fetch_meta(asset),
        bybit.fetch_funding_rate(asset),
        bybit.fetch_open_interest(asset),
    );

    let binance_ok = b_all.is_ok();
    let binance = b_all.unwrap_or(FuturesData {
        funding_rate: 0.0, mark_price: 0.0, open_interest_usd: 0.0, liquidations: vec![],
    });

    // Collect funding rates for averaging — only include exchanges that succeeded
    let mut funding_rates: Vec<f64> = Vec::new();
    let mut funding_count = 0u32;
    if binance_ok {
        funding_rates.push(binance.funding_rate);
        funding_count += 1;
    }
    if let Ok(fr) = o_fr {
        funding_rates.push(fr);
        funding_count += 1;
    }
    if let Ok((fr, _)) = h_meta {
        // Hyperliquid funding is per-hour, normalize to 8h for comparison
        funding_rates.push(fr * 8.0);
        funding_count += 1;
    }
    if let Ok(fr) = by_fr {
        funding_rates.push(fr);
        funding_count += 1;
    }

    let avg_funding = if funding_count > 0 {
        funding_rates.iter().sum::<f64>() / funding_count as f64
    } else {
        0.0
    };

    // Sum OI across exchanges
    let mut total_oi = binance.open_interest_usd;
    if let Ok(oi) = o_oi {
        // OKX BTC-USDT-SWAP: 1 contract = 0.01 BTC; ETH-USDT-SWAP: 1 contract = 0.1 ETH
        let contract_size = match asset {
            CryptoAsset::BTC => 0.01,
            CryptoAsset::ETH => 0.1,
        };
        total_oi += oi * contract_size * binance.mark_price;
    }
    if let Ok((_, oi)) = h_meta {
        total_oi += oi * binance.mark_price;
    }
    if let Ok(oi) = by_oi {
        // Bybit OI is in base asset units
        total_oi += oi * binance.mark_price;
    }

    // Combine liquidations
    let mut all_liqs = binance.liquidations;
    if let Ok(mut okx_liqs) = o_liq {
        all_liqs.append(&mut okx_liqs);
    }

    FuturesData {
        funding_rate: avg_funding,
        mark_price: binance.mark_price,
        open_interest_usd: total_oi,
        liquidations: all_liqs,
    }
}
