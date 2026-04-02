use anyhow::{Context, Result};
use reqwest::Client;

use super::{Candle, CryptoAsset, FuturesData, Liquidation, OrderBook, OrderBookLevel, Trade};

const BINANCE_BASE: &str = "https://api.binance.com/api/v3";
const BINANCE_FAPI: &str = "https://fapi.binance.com";

/// Shared HTTP client for Binance API calls.
pub struct BinanceFeed {
    client: Client,
}

impl BinanceFeed {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
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
            client: Client::new(),
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
