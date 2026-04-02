use anyhow::{Context, Result};
use chrono::Datelike;

use super::{CryptoAsset, Market5m};

/// Search Polymarket for the next upcoming 5-minute BTC/ETH "Up or Down" market.
///
/// Uses MarketsRequest with closed=false + startDate desc to find the most recent
/// active markets, then filters by asset keyword and time window.
pub async fn find_next_5m_market(
    gamma_client: &polymarket_client_sdk::gamma::Client,
    asset: CryptoAsset,
) -> Result<Option<Market5m>> {
    use polymarket_client_sdk::gamma::types::request::MarketsRequest;

    // Fetch recent active markets sorted by newest first
    let request = MarketsRequest::builder()
        .limit(50)
        .order("startDate".to_string())
        .ascending(false)
        .closed(false)
        .build();

    let markets = gamma_client
        .markets(&request)
        .await
        .context("failed to fetch gamma markets")?;

    let keyword = match asset {
        CryptoAsset::BTC => "Bitcoin Up or Down",
        CryptoAsset::ETH => "Ethereum Up or Down",
    };

    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut best: Option<Market5m> = None;

    for m in &markets {
        let question = m.question.as_deref().unwrap_or("");
        if !question.contains(keyword) {
            continue;
        }

        // Parse time window from title
        let (start, end) = match parse_5m_time_window(question) {
            Some(t) => t,
            None => continue,
        };

        // Skip markets that already ended
        if end < now_ms {
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
                    if lower.contains("up") {
                        up_idx = i;
                    } else if lower.contains("down") {
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

        let candidate = Market5m {
            condition_id,
            question: question.to_string(),
            asset,
            start_time: start,
            end_time: end,
            token_id_up: token_up,
            token_id_down: token_down,
            slug: m.slug.clone().unwrap_or_default(),
        };

        // Pick the soonest upcoming market (closest to now but not ended)
        match &best {
            None => best = Some(candidate),
            Some(current) => {
                // Prefer market closest to starting (smallest positive secs_until)
                let cur_dist = (current.start_time - now_ms).abs();
                let new_dist = (candidate.start_time - now_ms).abs();
                if new_dist < cur_dist {
                    best = Some(candidate);
                }
            }
        }
    }

    Ok(best)
}

/// List all active 5-minute crypto markets for display.
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

/// Parse a 5-minute time window from a market title.
///
/// Example: "Bitcoin Up or Down - March 29, 2:40AM-2:45AM ET"
/// Returns (start_ms, end_ms) in UTC.
fn parse_5m_time_window(title: &str) -> Option<(i64, i64)> {
    let time_part = title.split(" - ").nth(1)?;
    let time_part = time_part.trim().trim_end_matches(" ET").trim();

    let comma_pos = time_part.find(',')?;
    let date_str = time_part[..comma_pos].trim();
    let time_range = time_part[comma_pos + 1..].trim();

    // "2:40AM-2:45AM" or "2:40AM - 2:45AM"
    let time_range = time_range.replace(' ', "");
    let parts: Vec<&str> = time_range.splitn(2, '-').collect();
    if parts.len() != 2 {
        return None;
    }

    let year = chrono::Utc::now().year();
    let start = parse_et_datetime(date_str, parts[0], year)?;
    let end = parse_et_datetime(date_str, parts[1], year)?;

    Some((start, end))
}

/// Parse a date + time string in ET to UTC milliseconds.
fn parse_et_datetime(date_str: &str, time_str: &str, year: i32) -> Option<i64> {
    use chrono::{NaiveDate, NaiveTime, TimeZone};

    let parts: Vec<&str> = date_str.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let month = match parts[0].to_lowercase().as_str() {
        "january" | "jan" => 1u32,
        "february" | "feb" => 2,
        "march" | "mar" => 3,
        "april" | "apr" => 4,
        "may" => 5,
        "june" | "jun" => 6,
        "july" | "jul" => 7,
        "august" | "aug" => 8,
        "september" | "sep" => 9,
        "october" | "oct" => 10,
        "november" | "nov" => 11,
        "december" | "dec" => 12,
        _ => return None,
    };
    let day: u32 = parts[1].parse().ok()?;

    let time_upper = time_str.to_uppercase();
    let is_pm = time_upper.contains("PM");
    let time_digits = time_upper.trim_end_matches("AM").trim_end_matches("PM");
    let time_parts: Vec<&str> = time_digits.split(':').collect();
    if time_parts.len() != 2 {
        return None;
    }
    let mut hour: u32 = time_parts[0].parse().ok()?;
    let min: u32 = time_parts[1].parse().ok()?;

    if hour == 12 {
        hour = if is_pm { 12 } else { 0 };
    } else if is_pm {
        hour += 12;
    }

    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let time = NaiveTime::from_hms_opt(hour, min, 0)?;
    let naive_dt = date.and_time(time);

    // EDT (Mar-Nov) = UTC-4, EST = UTC-5
    let et_offset = if month >= 3 && month <= 11 {
        chrono::FixedOffset::west_opt(4 * 3600)?
    } else {
        chrono::FixedOffset::west_opt(5 * 3600)?
    };

    let et_dt = et_offset.from_local_datetime(&naive_dt).single()?;
    Some(et_dt.timestamp_millis())
}
