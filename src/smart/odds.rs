use anyhow::Result;
use chrono::Utc;
use polymarket_client_sdk::clob;
use polymarket_client_sdk::clob::types::request::MidpointRequest;
use polymarket_client_sdk::types::U256;
use rust_decimal::prelude::ToPrimitive;

use super::store;
use super::OddsAlert;

/// Scan all watched markets and return alerts for those exceeding threshold.
pub async fn scan_odds() -> Result<Vec<OddsAlert>> {
    let mut watches = store::load_odds_watches()?;
    if watches.is_empty() {
        return Ok(Vec::new());
    }

    let client = clob::Client::default();
    let now = Utc::now();
    let mut alerts = Vec::new();

    for watch in &mut watches {
        let token_id = match watch.token_id.parse::<U256>() {
            Ok(id) => id,
            Err(_) => {
                eprintln!("Invalid token_id: {}, skipping", watch.token_id);
                continue;
            }
        };

        let request = MidpointRequest::builder().token_id(token_id).build();
        match client.midpoint(&request).await {
            Ok(result) => {
                let mid: f64 = result.mid.to_f64().unwrap_or(0.0);
                if mid <= 0.0 {
                    continue;
                }

                // Guard against division by zero (initial watch or corrupted data)
                if watch.baseline_price <= 0.0 {
                    watch.baseline_price = mid;
                }
                if watch.last_price <= 0.0 {
                    watch.last_price = mid;
                }

                let change_from_baseline =
                    ((mid - watch.baseline_price) / watch.baseline_price) * 100.0;
                let change_from_last =
                    ((mid - watch.last_price) / watch.last_price) * 100.0;

                // Alert if change from last scan exceeds threshold
                if change_from_last.abs() >= watch.threshold_pct {
                    let alert = OddsAlert {
                        id: format!(
                            "odds_{}_{:.0}",
                            now.timestamp(),
                            mid * 10000.0
                        ),
                        timestamp: now,
                        token_id: watch.token_id.clone(),
                        label: watch.label.clone(),
                        baseline_price: watch.baseline_price,
                        previous_price: watch.last_price,
                        current_price: mid,
                        change_pct: change_from_last,
                        threshold_pct: watch.threshold_pct,
                    };
                    alerts.push(alert);
                }

                // Update last price and scan time
                watch.last_price = mid;
                watch.last_scanned = Some(now);

                // Also update baseline if this is significantly different
                // (rebase after alert to avoid repeated alerts on same level)
                if change_from_baseline.abs() >= watch.threshold_pct {
                    watch.baseline_price = mid;
                }
            }
            Err(e) => {
                eprintln!("Error fetching midpoint for {}: {e}", watch.label);
            }
        }
    }

    // Persist updated watches (new prices + timestamps)
    store::save_odds_watches(&watches)?;

    Ok(alerts)
}
