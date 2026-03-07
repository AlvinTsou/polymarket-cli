pub mod scorer;
pub mod signals;
pub mod store;
pub mod tracker;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A wallet we are tracking for smart money signals.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WatchedWallet {
    pub address: String,
    pub tag: Option<String>,
    pub added_at: DateTime<Utc>,
    pub score: Option<f64>,
}

/// A point-in-time snapshot of a single position.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub condition_id: String,
    pub title: String,
    pub slug: String,
    pub outcome: String,
    pub outcome_index: String,
    pub size: String,
    pub avg_price: String,
    pub current_value: String,
    pub cur_price: String,
}

/// All positions for a wallet at a point in time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletSnapshot {
    pub address: String,
    pub timestamp: DateTime<Utc>,
    pub positions: Vec<PositionSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SignalType {
    NewPosition,
    ClosePosition,
    IncreasePosition,
    DecreasePosition,
}

impl std::fmt::Display for SignalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NewPosition => write!(f, "NEW"),
            Self::ClosePosition => write!(f, "CLOSE"),
            Self::IncreasePosition => write!(f, "INCREASE"),
            Self::DecreasePosition => write!(f, "DECREASE"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SignalConfidence {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for SignalConfidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "LOW"),
            Self::Medium => write!(f, "MED"),
            Self::High => write!(f, "HIGH"),
        }
    }
}

/// A trading signal generated from a detected position change.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signal {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub signal_type: SignalType,
    pub confidence: SignalConfidence,
    pub wallet: String,
    pub wallet_tag: Option<String>,
    pub wallet_score: Option<f64>,
    pub market_title: String,
    pub market_slug: String,
    pub condition_id: String,
    pub outcome: String,
    pub price: String,
    pub size: String,
    pub prev_size: Option<String>,
}

/// Direction of a trade signal (for aggregation).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalDirection {
    Buy,  // New or Increase
    Sell, // Close or Decrease
}

impl std::fmt::Display for SignalDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Buy => write!(f, "BUY"),
            Self::Sell => write!(f, "SELL"),
        }
    }
}

impl SignalType {
    pub fn direction(&self) -> SignalDirection {
        match self {
            Self::NewPosition | Self::IncreasePosition => SignalDirection::Buy,
            Self::ClosePosition | Self::DecreasePosition => SignalDirection::Sell,
        }
    }
}

/// Multiple wallets converging on the same market+direction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AggregatedSignal {
    pub condition_id: String,
    pub market_title: String,
    pub outcome: String,
    pub direction: SignalDirection,
    pub confidence: SignalConfidence,
    pub wallet_count: usize,
    pub wallets: Vec<String>,
    pub total_size: f64,
    pub avg_price: f64,
    pub signals: Vec<Signal>,
}

/// Scoring result for a wallet.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SmartScore {
    pub address: String,
    pub score: f64,
    pub pnl: String,
    pub volume: String,
    pub positions_count: u32,
    pub markets_traded: u32,
    pub rank: Option<u64>,
    pub name: Option<String>,
    pub updated_at: DateTime<Utc>,
}
