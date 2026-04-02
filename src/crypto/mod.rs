pub mod feed;
pub mod market;
pub mod momentum;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported crypto assets for 5-minute trading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CryptoAsset {
    BTC,
    ETH,
}

impl CryptoAsset {
    /// Binance trading pair symbol.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::BTC => "BTCUSDT",
            Self::ETH => "ETHUSDT",
        }
    }

    /// Keywords for searching Polymarket titles.
    pub fn market_keywords(self) -> &'static [&'static str] {
        match self {
            Self::BTC => &["Bitcoin", "BTC"],
            Self::ETH => &["Ethereum", "ETH"],
        }
    }
}

impl fmt::Display for CryptoAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BTC => write!(f, "BTC"),
            Self::ETH => write!(f, "ETH"),
        }
    }
}

impl std::str::FromStr for CryptoAsset {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "BTC" | "BITCOIN" => Ok(Self::BTC),
            "ETH" | "ETHEREUM" => Ok(Self::ETH),
            _ => Err(anyhow::anyhow!("unknown asset: {s} (use BTC or ETH)")),
        }
    }
}

/// OHLCV candle from Binance klines.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candle {
    pub open_time: i64,   // ms timestamp
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub close_time: i64,  // ms timestamp
}

/// Single level in the order book.
#[derive(Clone, Debug)]
pub struct OrderBookLevel {
    pub price: f64,
    pub qty: f64,
}

/// Order book snapshot.
#[derive(Clone, Debug)]
pub struct OrderBook {
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub timestamp: i64,
}

/// Single trade from Binance recent trades.
#[derive(Clone, Debug)]
pub struct Trade {
    pub price: f64,
    pub qty: f64,
    pub is_buyer_maker: bool, // true = sell (taker sold), false = buy (taker bought)
    pub time: i64,
}

/// Predicted direction for the next 5-minute window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Up,
    Down,
    Skip,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Up => write!(f, "UP"),
            Self::Down => write!(f, "DOWN"),
            Self::Skip => write!(f, "SKIP"),
        }
    }
}

/// Breakdown of signal components for transparency.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignalComponents {
    pub price_mom_1m: f64,
    pub price_mom_5m: f64,
    pub ob_imbalance: f64,
    pub trade_flow: f64,
    pub volatility: f64,
    // Futures-enhanced components (0.0 when futures data unavailable)
    pub funding_signal: f64,
    pub oi_delta_signal: f64,
    pub liquidation_signal: f64,
    pub raw_score: f64,
}

/// Futures data from Binance FAPI (funding, OI, liquidations).
#[derive(Clone, Debug)]
pub struct FuturesData {
    pub funding_rate: f64,        // current funding rate (8h period)
    pub mark_price: f64,
    pub open_interest_usd: f64,   // total OI in USDT
    pub liquidations: Vec<Liquidation>,
}

/// A single liquidation order.
#[derive(Clone, Debug)]
pub struct Liquidation {
    pub side: String,   // "BUY" (short liq) or "SELL" (long liq)
    pub price: f64,
    pub qty: f64,
    pub time: i64,
}

/// Final momentum signal output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MomentumSignal {
    pub asset: CryptoAsset,
    pub direction: Direction,
    pub confidence: f64,     // 0.0 - 1.0
    pub components: SignalComponents,
    pub price: f64,          // current price at signal time
    pub timestamp: i64,      // ms
}

/// Aggregated order book + trade data from multiple exchanges.
#[derive(Clone, Debug, Default)]
pub struct AggregatedSpot {
    pub orderbooks: Vec<(String, OrderBook)>,   // (exchange_name, book)
    pub trades: Vec<(String, Vec<Trade>)>,      // (exchange_name, trades)
    pub merged_ob_imbalance: f64,               // volume-weighted across exchanges
    pub merged_trade_flow: f64,                 // aggregated buy-sell imbalance
    pub exchange_count: u32,
}

/// A discovered Polymarket 5-minute market.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Market5m {
    pub condition_id: String,
    pub question: String,
    pub asset: CryptoAsset,
    pub start_time: i64,     // ms, ET-based window start
    pub end_time: i64,       // ms, ET-based window end
    pub token_id_up: String,
    pub token_id_down: String,
    pub slug: String,
}
