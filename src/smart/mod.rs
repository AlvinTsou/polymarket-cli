pub mod odds;
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
    #[serde(default)]
    pub discovery_periods: Option<Vec<String>>,
    #[serde(default)]
    pub last_seen_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub stale: Option<bool>,
}

/// PnL snapshot for a wallet at a point in time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletPnlSnapshot {
    pub timestamp: DateTime<Utc>,
    pub open_pnl: f64,
    pub realized_pnl: f64,
    pub position_count: u32,
}

/// Category breakdown for trade analysis.
#[derive(Clone, Debug)]
pub struct CategoryStat {
    pub name: String,
    pub position_count: u32,
    pub total_pnl: f64,
    pub pct: f64,
}

/// A point-in-time snapshot of a single position.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub condition_id: String,
    pub asset: String, // token_id (U256 as decimal string)
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
    pub asset: String, // token_id for order placement
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

/// Telegram Bot configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: i64,
}

/// Configuration for auto-follow trading.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FollowConfig {
    /// Max USDC per single trade
    pub max_per_trade: f64,
    /// Max USDC total per day
    pub max_per_day: f64,
    /// Minimum signal confidence to follow
    pub min_confidence: SignalConfidence,
    /// Dry-run mode (log only, no real orders)
    pub dry_run: bool,
}

impl Default for FollowConfig {
    fn default() -> Self {
        Self {
            max_per_trade: 10.0,
            max_per_day: 50.0,
            min_confidence: SignalConfidence::Medium,
            dry_run: true, // safe default
        }
    }
}

/// Status of a follow trade.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeStatus {
    Open,
    Closed,
    Expired,
}

impl Default for TradeStatus {
    fn default() -> Self {
        Self::Open
    }
}

impl std::fmt::Display for TradeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "OPEN"),
            Self::Closed => write!(f, "CLOSED"),
            Self::Expired => write!(f, "EXPIRED"),
        }
    }
}

/// Record of a follow trade (executed or dry-run).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FollowRecord {
    pub timestamp: DateTime<Utc>,
    pub signal_id: String,
    pub market_title: String,
    pub condition_id: String,
    pub asset: String,
    pub outcome: String,
    pub side: String,
    pub amount_usdc: f64,
    pub price: f64,
    pub dry_run: bool,
    pub order_id: Option<String>,
    #[serde(default)]
    pub fill_price: Option<f64>,
    #[serde(default)]
    pub status: Option<TradeStatus>,
    #[serde(default)]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub exit_price: Option<f64>,
    #[serde(default)]
    pub realized_pnl: Option<f64>,
    #[serde(default)]
    pub position_id: Option<String>,
    #[serde(default)]
    pub entry_reason: Option<String>,
    #[serde(default)]
    pub exit_reason: Option<String>,
}

impl FollowRecord {
    /// Effective entry price: fill_price if available, otherwise signal price.
    pub fn effective_entry(&self) -> f64 {
        self.fill_price.unwrap_or(self.price)
    }

    /// Whether this trade is considered open.
    pub fn is_open(&self) -> bool {
        !matches!(
            self.status.as_ref(),
            Some(TradeStatus::Closed) | Some(TradeStatus::Expired)
        )
    }
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

// ── Monitor ─────────────────────────────────────────────────────

/// Configuration for the real-time monitor daemon.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MonitorConfig {
    pub interval_secs: u64,
    pub min_confidence: SignalConfidence,
    pub min_wallets: u32,
    pub market_include: Vec<String>,
    pub market_exclude: Vec<String>,
    pub odds_threshold: f64,
    pub paper_trade: bool,
    pub amount: f64,
    pub max_per_day: f64,
    pub notify: bool,
    /// Max open positions per market group (e.g., "win the 2026 masters tournament")
    #[serde(default = "default_max_per_group")]
    pub max_per_group: u32,
}

fn default_max_per_group() -> u32 { 2 }

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300, // 5m
            min_confidence: SignalConfidence::Medium,
            min_wallets: 1,
            market_include: Vec::new(),
            market_exclude: Vec::new(),
            odds_threshold: 0.0,
            paper_trade: false,
            amount: 10.0,
            max_per_day: 50.0,
            notify: false,
            max_per_group: 2,
        }
    }
}

/// What caused a trigger.
#[derive(Clone, Debug)]
pub enum TriggerType {
    Signal,
    Aggregated,
    OddsAlert,
}

impl std::fmt::Display for TriggerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signal => write!(f, "SIGNAL"),
            Self::Aggregated => write!(f, "AGGREGATED"),
            Self::OddsAlert => write!(f, "ODDS"),
        }
    }
}

/// A trigger event that passed all filter conditions.
#[derive(Clone, Debug)]
pub struct TriggerEvent {
    pub trigger_type: TriggerType,
    pub confidence: SignalConfidence,
    pub market_title: String,
    pub outcome: String,
    pub direction: SignalDirection,
    pub price: f64,
    pub wallet_count: u32,
    pub reason: String,
    pub condition_id: String,
    pub asset: String,
    pub signal_id: String,
}

/// Extract a group key from a market title to detect related markets.
/// e.g., "Will Casey Jarvis win the 2026 Masters tournament?" → "win the 2026 masters tournament"
/// e.g., "Will Gavin Newsom win the 2028 Democratic presidential nomination?" → "win the 2028 democratic presidential nomination"
pub fn market_group_key(title: &str) -> String {
    let lower = title.to_lowercase();
    // Strip leading "will <name> " pattern — names are typically 1-4 words before a verb
    let verbs = ["win", "be ", "become", "get ", "receive", "lead ", "finish", "qualify", "advance", "make"];
    for verb in &verbs {
        if let Some(pos) = lower.find(verb) {
            // Only strip if the verb appears within the first ~60 chars (it's the main predicate, not a nested clause)
            if pos < 60 {
                let suffix = &lower[pos..];
                // Remove trailing "?" and whitespace
                return suffix.trim_end_matches('?').trim().to_string();
            }
        }
    }
    // Fallback: return the whole title lowercased, trimmed
    lower.trim_end_matches('?').trim().to_string()
}

// ── Odds Monitoring ─────────────────────────────────────────────

/// A market being monitored for price changes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OddsWatch {
    pub token_id: String,
    pub label: String,
    pub threshold_pct: f64,
    pub baseline_price: f64,
    pub last_price: f64,
    pub added_at: DateTime<Utc>,
    pub last_scanned: Option<DateTime<Utc>>,
}

/// An alert generated when price moves beyond threshold.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OddsAlert {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub token_id: String,
    pub label: String,
    pub baseline_price: f64,
    pub previous_price: f64,
    pub current_price: f64,
    pub change_pct: f64,
    pub threshold_pct: f64,
}

/// Price snapshot for open positions (recorded each monitor cycle).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriceSnapshot {
    pub timestamp: DateTime<Utc>,
    pub prices: std::collections::HashMap<String, f64>, // "condition_id:outcome" -> midpoint
}
