use std::collections::HashMap;

use anyhow::Result;
use polymarket_client_sdk::data::{self, types::request::PositionsRequest};

use super::{PositionSnapshot, WalletSnapshot, store};
use crate::commands::parse_address;

/// What changed for a single position.
pub struct PositionChange {
    pub change_type: ChangeType,
    pub position: PositionSnapshot,
    pub previous: Option<PositionSnapshot>,
}

pub enum ChangeType {
    New,
    Closed,
    Increased,
    Decreased,
}

/// Fetch current positions, compare with stored snapshot, return changes.
///
/// On the first scan for a wallet (no prior snapshot) an empty change list is
/// returned – the snapshot is saved for next time.
pub async fn scan_wallet(
    client: &data::Client,
    address: &str,
) -> Result<(Vec<PositionChange>, WalletSnapshot)> {
    let request = PositionsRequest::builder()
        .user(parse_address(address)?)
        .limit(100)?
        .build();

    let positions = client.positions(&request).await?;

    let current_positions: Vec<PositionSnapshot> = positions
        .iter()
        .map(|p| PositionSnapshot {
            condition_id: p.condition_id.to_string(),
            asset: p.asset.to_string(),
            title: p.title.clone(),
            slug: p.slug.clone(),
            outcome: p.outcome.clone(),
            outcome_index: p.outcome_index.to_string(),
            size: p.size.to_string(),
            avg_price: p.avg_price.to_string(),
            current_value: p.current_value.to_string(),
            cur_price: p.cur_price.to_string(),
        })
        .collect();

    let new_snapshot = WalletSnapshot {
        address: address.to_string(),
        timestamp: chrono::Utc::now(),
        positions: current_positions,
    };

    let previous = store::load_snapshot(address)?;
    let changes = match previous {
        Some(prev) => compute_changes(&prev.positions, &new_snapshot.positions),
        None => Vec::new(), // first scan – baseline only
    };

    store::save_snapshot(&new_snapshot)?;

    Ok((changes, new_snapshot))
}

fn compute_changes(
    previous: &[PositionSnapshot],
    current: &[PositionSnapshot],
) -> Vec<PositionChange> {
    // Key: (condition_id, outcome_index)
    let prev_map: HashMap<(&str, &str), &PositionSnapshot> = previous
        .iter()
        .map(|p| ((p.condition_id.as_str(), p.outcome_index.as_str()), p))
        .collect();

    let curr_map: HashMap<(&str, &str), &PositionSnapshot> = current
        .iter()
        .map(|p| ((p.condition_id.as_str(), p.outcome_index.as_str()), p))
        .collect();

    let mut changes = Vec::new();

    // New or changed positions
    for (key, curr) in &curr_map {
        match prev_map.get(key) {
            None => {
                changes.push(PositionChange {
                    change_type: ChangeType::New,
                    position: (*curr).clone(),
                    previous: None,
                });
            }
            Some(prev) => {
                let curr_size: f64 = curr.size.parse().unwrap_or(0.0);
                let prev_size: f64 = prev.size.parse().unwrap_or(0.0);
                let diff = (curr_size - prev_size).abs();
                if diff > 0.01 {
                    let change_type = if curr_size > prev_size {
                        ChangeType::Increased
                    } else {
                        ChangeType::Decreased
                    };
                    changes.push(PositionChange {
                        change_type,
                        position: (*curr).clone(),
                        previous: Some((*prev).clone()),
                    });
                }
            }
        }
    }

    // Closed positions (in previous, absent in current)
    for (key, prev) in &prev_map {
        if !curr_map.contains_key(key) {
            changes.push(PositionChange {
                change_type: ChangeType::Closed,
                position: (*prev).clone(),
                previous: Some((*prev).clone()),
            });
        }
    }

    changes
}
