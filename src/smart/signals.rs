use std::collections::HashMap;

use chrono::Utc;

use super::tracker::{ChangeType, PositionChange};
use super::{AggregatedSignal, Signal, SignalConfidence, SignalType, WatchedWallet};

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

/// Aggregate signals: group by (condition_id, outcome, direction).
/// When multiple wallets converge on the same trade, confidence is boosted.
pub fn aggregate_signals(signals: &[Signal]) -> Vec<AggregatedSignal> {
    // Key: (condition_id, outcome, direction)
    let mut groups: HashMap<(String, String, String), Vec<&Signal>> = HashMap::new();

    for sig in signals {
        let dir = sig.signal_type.direction().to_string();
        let key = (sig.condition_id.clone(), sig.outcome.clone(), dir);
        groups.entry(key).or_default().push(sig);
    }

    let mut aggregated: Vec<AggregatedSignal> = groups
        .into_iter()
        .filter(|(_, sigs)| sigs.len() >= 2) // only aggregate 2+ wallets
        .map(|((cid, outcome, _), sigs)| {
            let wallet_count = sigs.len();
            let wallets: Vec<String> = sigs
                .iter()
                .map(|s| {
                    s.wallet_tag
                        .clone()
                        .unwrap_or_else(|| s.wallet.clone())
                })
                .collect();

            let total_size: f64 = sigs
                .iter()
                .map(|s| s.size.parse::<f64>().unwrap_or(0.0))
                .sum();
            let avg_price: f64 = {
                let prices: Vec<f64> = sigs
                    .iter()
                    .filter_map(|s| s.price.parse::<f64>().ok())
                    .collect();
                if prices.is_empty() {
                    0.0
                } else {
                    prices.iter().sum::<f64>() / prices.len() as f64
                }
            };

            let confidence = match wallet_count {
                2 => SignalConfidence::Medium,
                _ => SignalConfidence::High, // 3+
            };

            let direction = sigs[0].signal_type.direction();

            AggregatedSignal {
                condition_id: cid,
                market_title: sigs[0].market_title.clone(),
                outcome,
                direction,
                confidence,
                wallet_count,
                wallets,
                total_size,
                avg_price,
                signals: sigs.into_iter().cloned().collect(),
            }
        })
        .collect();

    // Sort by wallet_count descending
    aggregated.sort_by(|a, b| b.wallet_count.cmp(&a.wallet_count));
    aggregated
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
