use chrono::Utc;

use super::tracker::{ChangeType, PositionChange};
use super::{Signal, SignalConfidence, SignalType, WatchedWallet};

/// Convert raw position changes into actionable signals.
pub fn generate_signals(wallet: &WatchedWallet, changes: &[PositionChange]) -> Vec<Signal> {
    let now = Utc::now();

    changes
        .iter()
        .map(|change| {
            let signal_type = match change.change_type {
                ChangeType::New => SignalType::NewPosition,
                ChangeType::Closed => SignalType::ClosePosition,
                ChangeType::Increased => SignalType::IncreasePosition,
                ChangeType::Decreased => SignalType::DecreasePosition,
            };

            let confidence = determine_confidence(wallet, change);
            let cid_short = &change.position.condition_id
                [..8.min(change.position.condition_id.len())];
            let id = format!("sig_{}_{cid_short}", now.format("%Y%m%d%H%M%S"));

            Signal {
                id,
                timestamp: now,
                signal_type,
                confidence,
                wallet: wallet.address.clone(),
                wallet_tag: wallet.tag.clone(),
                wallet_score: wallet.score,
                market_title: change.position.title.clone(),
                market_slug: change.position.slug.clone(),
                condition_id: change.position.condition_id.clone(),
                outcome: change.position.outcome.clone(),
                price: change.position.cur_price.clone(),
                size: change.position.size.clone(),
                prev_size: change.previous.as_ref().map(|p| p.size.clone()),
            }
        })
        .collect()
}

fn determine_confidence(wallet: &WatchedWallet, change: &PositionChange) -> SignalConfidence {
    let score = wallet.score.unwrap_or(0.0);
    let size: f64 = change.position.size.parse().unwrap_or(0.0);

    // High: top-scored wallet with a large or new position
    if score >= 80.0 && (size >= 100.0 || matches!(change.change_type, ChangeType::New)) {
        return SignalConfidence::High;
    }

    // Medium: decent score or decent size
    if score >= 50.0 || size >= 50.0 {
        return SignalConfidence::Medium;
    }

    SignalConfidence::Low
}
