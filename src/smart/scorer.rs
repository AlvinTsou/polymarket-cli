use anyhow::Result;
use chrono::Utc;
use polymarket_client_sdk::data::{
    self,
    types::request::{PositionsRequest, TradedRequest, ValueRequest},
};
use rust_decimal::prelude::ToPrimitive;

use super::SmartScore;
use crate::commands::parse_address;

/// Build a score by querying positions, value, and market count for a wallet.
pub async fn score_wallet(client: &data::Client, address: &str) -> Result<SmartScore> {
    let addr = parse_address(address)?;

    let positions_req = PositionsRequest::builder().user(addr).limit(100)?.build();
    let value_req = ValueRequest::builder().user(addr).build();
    let traded_req = TradedRequest::builder().user(addr).build();

    let (positions, values, traded) = tokio::join!(
        client.positions(&positions_req),
        client.value(&value_req),
        client.traded(&traded_req),
    );

    let positions = positions?;
    let values = values?;
    let traded = traded?;

    let portfolio_value: f64 = values
        .first()
        .and_then(|v| v.value.to_f64())
        .unwrap_or(0.0);
    let markets_traded = traded.traded as u32;
    let positions_count = positions.len() as u32;

    let total_pnl: f64 = positions
        .iter()
        .filter_map(|p| p.cash_pnl.to_f64())
        .sum();

    let score = compute_score(portfolio_value, markets_traded, total_pnl);

    Ok(SmartScore {
        address: address.to_string(),
        score,
        pnl: format!("{total_pnl:.2}"),
        volume: format!("{portfolio_value:.2}"),
        positions_count,
        markets_traded,
        rank: None,
        name: None,
        updated_at: Utc::now(),
    })
}

/// Lighter scoring based on leaderboard data (no extra API calls).
pub fn score_from_leaderboard(
    proxy_wallet: &str,
    name: Option<&str>,
    pnl: f64,
    volume: f64,
    rank: u64,
) -> SmartScore {
    let rank_score = ((50.0 - rank as f64).max(0.0) / 50.0 * 100.0).min(100.0);
    let pnl_score = if pnl > 0.0 {
        (pnl.log10().max(0.0) / 5.0 * 100.0).min(100.0)
    } else {
        0.0
    };
    let volume_score = if volume > 0.0 {
        (volume.log10().max(0.0) / 7.0 * 100.0).min(100.0)
    } else {
        0.0
    };

    let score = rank_score * 0.30 + pnl_score * 0.40 + volume_score * 0.30;

    SmartScore {
        address: proxy_wallet.to_string(),
        score,
        pnl: format!("{pnl:.2}"),
        volume: format!("{volume:.2}"),
        positions_count: 0,
        markets_traded: 0,
        rank: Some(rank),
        name: name.map(String::from),
        updated_at: Utc::now(),
    }
}

fn compute_score(portfolio_value: f64, markets_traded: u32, total_pnl: f64) -> f64 {
    let value_score = if portfolio_value > 0.0 {
        (portfolio_value.log10().max(0.0) / 6.0 * 100.0).min(100.0)
    } else {
        0.0
    };

    let diversity_score = if markets_traded > 0 {
        ((markets_traded as f64).log2().max(0.0) / 7.0 * 100.0).min(100.0)
    } else {
        0.0
    };

    let pnl_score = if total_pnl > 0.0 {
        (total_pnl.log10().max(0.0) / 5.0 * 100.0).min(100.0)
    } else {
        0.0
    };

    value_score * 0.35 + diversity_score * 0.30 + pnl_score * 0.35
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_score_zero_inputs() {
        assert_eq!(compute_score(0.0, 0, 0.0), 0.0);
    }

    #[test]
    fn compute_score_high_values() {
        let score = compute_score(1_000_000.0, 128, 100_000.0);
        assert!(score > 80.0, "expected high score, got {score}");
    }

    #[test]
    fn compute_score_moderate_values() {
        let score = compute_score(1_000.0, 10, 500.0);
        assert!(score > 20.0 && score < 80.0, "got {score}");
    }

    #[test]
    fn score_from_leaderboard_top_rank() {
        let s = score_from_leaderboard("0x1234", Some("whale"), 50000.0, 1_000_000.0, 1);
        assert!(s.score > 70.0, "got {}", s.score);
        assert_eq!(s.rank, Some(1));
    }

    #[test]
    fn score_from_leaderboard_low_rank() {
        let s = score_from_leaderboard("0xabcd", None, 100.0, 500.0, 49);
        assert!(s.score < 50.0, "got {}", s.score);
    }
}
