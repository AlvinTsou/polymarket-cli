use anyhow::{Context, Result};
use rust_decimal::prelude::ToPrimitive;

use super::{CryptoAsset, Market5m};

/// Search Polymarket for the next upcoming BTC/ETH "Daily Price" market that fits trading constraints.
///
/// Uses SearchRequest to query active, relevant markets and selects the strike price closest to 0.50.
pub async fn find_next_5m_market(
    gamma_client: &polymarket_client_sdk::gamma::Client,
    asset: CryptoAsset,
) -> Result<Option<Market5m>> {
    use chrono::Utc;
    use polymarket_client_sdk::gamma::types::request::SearchRequest;

    let query = match asset {
        CryptoAsset::BTC => "Bitcoin",
        CryptoAsset::ETH => "Ethereum",
    };

    let request = SearchRequest::builder()
        .q(query.to_string())
        .limit_per_type(30)
        .build();

    let results = gamma_client
        .search(&request)
        .await
        .context("failed to search gamma markets")?;

    let events = results.events.unwrap_or_default();
    let mut best: Option<Market5m> = None;
    let mut best_dist_to_center = 1.0; // We want yes_price closest to 0.50 (center)
    let now = Utc::now();

    for event in &events {
        let mkts = match &event.markets {
            Some(m) => m,
            None => continue,
        };

        for m in mkts {
            // Ensure market is active (not closed)
            if m.closed.unwrap_or(false) {
                continue;
            }

            let question = m.question.as_deref().unwrap_or("");

            // Check if it is a Daily market (e.g. contains "price of Bitcoin be" or "price of Ethereum be")
            let is_btc_price_market = asset == CryptoAsset::BTC
                && (question.contains("price of Bitcoin be")
                    || question.contains("Bitcoin ($BTC) price be"));
            let is_eth_price_market = asset == CryptoAsset::ETH
                && (question.contains("price of Ethereum be")
                    || question.contains("Ethereum ($ETH) price be"));
            if !is_btc_price_market && !is_eth_price_market {
                continue;
            }

            // Must end within 36 hours from now, and not ended yet
            let end_dt = match m.end_date {
                Some(dt) => dt,
                None => continue,
            };

            let ends_in_ms = end_dt.timestamp_millis() - now.timestamp_millis();
            let min_ms = 10 * 60 * 1000; // at least 10 minutes left
            let max_ms = 36 * 3600 * 1000; // at most 36 hours (same-day or next-day daily markets)
            if ends_in_ms < min_ms || ends_in_ms > max_ms {
                continue;
            }

            // Extract outcomes prices for price filtering (0.15 - 0.80)
            let prices = m.outcome_prices.as_ref();
            let yes_price = match prices {
                Some(p) if p.len() >= 2 => p[0].to_f64().unwrap_or(0.0),
                _ => 0.0,
            };

            // Ensure the yes price is in standard tradeable range
            if yes_price < 0.15 || yes_price > 0.80 {
                continue;
            }

            // Extract token IDs and outcomes
            let token_ids = m.clob_token_ids.as_ref();
            let outcomes = m.outcomes.as_ref();
            let (token_up, token_down) = match (token_ids, outcomes) {
                (Some(ids), Some(outs)) if ids.len() >= 2 && outs.len() >= 2 => {
                    let mut up_idx = 0usize;
                    let mut down_idx = 1usize;
                    for (i, out) in outs.iter().enumerate() {
                        let lower = out.to_lowercase();
                        if lower == "yes" {
                            up_idx = i;
                        } else if lower == "no" {
                            down_idx = i;
                        }
                    }
                    (ids[up_idx].to_string(), ids[down_idx].to_string())
                }
                _ => continue,
            };

            let condition_id = m
                .condition_id
                .map(|c| format!("{c:#x}"))
                .unwrap_or_default();

            // start_time can be assumed as 24h before end_time
            let start_time = end_dt.timestamp_millis() - 24 * 3600 * 1000;

            let candidate = Market5m {
                condition_id,
                question: question.to_string(),
                asset,
                start_time,
                end_time: end_dt.timestamp_millis(),
                token_id_up: token_up,
                token_id_down: token_down,
                slug: m.slug.clone().unwrap_or_default(),
            };

            // Pick the market with yes_price closest to 0.50 (most active/traded strike price)
            let dist = (yes_price - 0.50).abs();
            match &best {
                None => {
                    best = Some(candidate);
                    best_dist_to_center = dist;
                }
                Some(_) => {
                    if dist < best_dist_to_center {
                        best = Some(candidate);
                        best_dist_to_center = dist;
                    }
                }
            }
        }
    }

    Ok(best)
}

/// List all active crypto markets for display.
pub async fn list_active_5m_markets(
    gamma_client: &polymarket_client_sdk::gamma::Client,
) -> Result<Vec<Market5m>> {
    let mut all = Vec::new();
    for asset in [CryptoAsset::BTC, CryptoAsset::ETH] {
        if let Some(m) = find_next_5m_market(gamma_client, asset).await? {
            all.push(m);
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    // Tests omitted since we directly use SDK's end_date field instead of manual string parsing
}
