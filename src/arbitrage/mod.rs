use anyhow::Result;
use polymarket_client_sdk::clob;
use polymarket_client_sdk::clob::types::request::OrderBookSummaryRequest;
use polymarket_client_sdk::gamma::{
    self,
    types::{request::MarketsRequest, response::Market},
};
use polymarket_client_sdk::types::{Decimal, U256};
use std::collections::HashMap;

/// Represents a Binary Complement Arbitrage opportunity (YES + NO < $1.00)
#[derive(Clone, Debug, serde::Serialize)]
pub struct ArbOpportunity {
    pub market_id: String,
    pub question: String,
    pub condition_id: String,
    pub yes_token_id: U256,
    pub no_token_id: U256,
    pub yes_ask: Decimal,
    pub no_ask: Decimal,
    pub sum_price: Decimal,
    pub profit_margin_pct: Decimal,
    pub volume: Decimal,
}

/// Represents a Favorite-Longshot Bias opportunity
#[derive(Clone, Debug, serde::Serialize)]
pub struct FLBOpportunity {
    pub market_id: String,
    pub question: String,
    pub token_id: U256,
    pub outcome: String,
    pub price: Decimal,
    pub bias_type: String, // "Favorite" or "Longshot"
    pub volume: Decimal,
}

/// Scans active binary markets for complement arbitrage opportunities.
pub async fn scan_complement_arbitrage(
    gamma_client: &gamma::Client,
    clob_client: &clob::Client,
    limit: i32,
) -> Result<Vec<ArbOpportunity>> {
    // 1. Fetch active markets
    let request = MarketsRequest::builder()
        .limit(limit)
        .maybe_closed(Some(false))
        .maybe_order(Some("volume_num".to_string()))
        .build();

    let markets = gamma_client.markets(&request).await?;
    let mut opportunities = Vec::new();

    // Filter for binary markets (with 2 outcomes and 2 clob_token_ids)
    let binary_markets: Vec<&Market> = markets
        .iter()
        .filter(|m| {
            m.active == Some(true)
                && m.clob_token_ids.as_ref().map_or(false, |ids| ids.len() == 2)
                && m.outcomes.as_ref().map_or(false, |o| o.len() == 2)
        })
        .collect();

    if binary_markets.is_empty() {
        return Ok(opportunities);
    }

    // 2. Prepare batch order book requests for both tokens of all binary markets
    let mut requests = Vec::new();

    for m in &binary_markets {
        if let Some(ids) = &m.clob_token_ids {
            let yes_id = ids[0];
            let no_id = ids[1];

            requests.push(OrderBookSummaryRequest::builder().token_id(yes_id).build());
            requests.push(OrderBookSummaryRequest::builder().token_id(no_id).build());
        }
    }

    // Fetch order books in batch
    let books = clob_client.order_books(&requests).await?;
    let mut asks_map = HashMap::new();

    for book in books {
        let token_id = book.asset_id;
        // Get best ask price (asks are sorted ascending by price)
        if let Some(best_ask) = book.asks.first().map(|a| a.price) {
            asks_map.insert(token_id, best_ask);
        }
    }

    // 3. Calculate sum of YES and NO best asks for each binary market
    for m in &binary_markets {
        if let Some(ids) = &m.clob_token_ids {
            let yes_id = ids[0];
            let no_id = ids[1];

            if let (Some(&yes_ask), Some(&no_ask)) = (asks_map.get(&yes_id), asks_map.get(&no_id)) {
                let sum_price = yes_ask + no_ask;
                let one = Decimal::ONE;

                if sum_price < one {
                    // Risk-free return = (1.00 / sum_price) - 1.00
                    let profit_margin_pct = (one / sum_price - one) * Decimal::from(100);

                    opportunities.push(ArbOpportunity {
                        market_id: m.id.clone(),
                        question: m.question.clone().unwrap_or_default(),
                        condition_id: m.condition_id.map(|c| format!("{c}")).unwrap_or_default(),
                        yes_token_id: yes_id,
                        no_token_id: no_id,
                        yes_ask,
                        no_ask,
                        sum_price,
                        profit_margin_pct,
                        volume: m.volume_num.unwrap_or(Decimal::ZERO),
                    });
                }
            }
        }
    }

    // Sort by profit margin descending
    opportunities.sort_by(|a, b| b.profit_margin_pct.cmp(&a.profit_margin_pct));

    Ok(opportunities)
}

/// Scans active markets for Favorite-Longshot Bias opportunities.
pub async fn scan_favorite_longshot_bias(
    gamma_client: &gamma::Client,
    clob_client: &clob::Client,
    limit: i32,
) -> Result<Vec<FLBOpportunity>> {
    // 1. Fetch active markets
    let request = MarketsRequest::builder()
        .limit(limit)
        .maybe_closed(Some(false))
        .maybe_order(Some("volume_num".to_string()))
        .build();

    let markets = gamma_client.markets(&request).await?;
    let mut opportunities = Vec::new();

    // 2. Filter for markets with CLOB token IDs
    let clob_markets: Vec<&Market> = markets
        .iter()
        .filter(|m| m.active == Some(true) && m.clob_token_ids.is_some() && m.outcomes.is_some())
        .collect();

    if clob_markets.is_empty() {
        return Ok(opportunities);
    }

    // 3. Fetch current prices/orderbooks or midpoints in batch
    let mut requests = Vec::new();
    let mut token_info = HashMap::new();

    for m in &clob_markets {
        if let (Some(ids), Some(outcomes)) = (&m.clob_token_ids, &m.outcomes) {
            for (idx, &token_id) in ids.iter().enumerate() {
                if idx < outcomes.len() {
                    requests.push(OrderBookSummaryRequest::builder().token_id(token_id).build());
                    token_info.insert(token_id, (m.id.clone(), m.question.clone().unwrap_or_default(), outcomes[idx].clone(), m.volume_num.unwrap_or(Decimal::ZERO)));
                }
            }
        }
    }

    let books = clob_client.order_books(&requests).await?;

    // 4. Identify favorites (0.80 - 0.95) and longshots (0.01 - 0.05)
    let min_favorite = Decimal::from_str_radix("0.80", 10).unwrap();
    let max_favorite = Decimal::from_str_radix("0.95", 10).unwrap();
    let min_longshot = Decimal::from_str_radix("0.01", 10).unwrap();
    let max_longshot = Decimal::from_str_radix("0.05", 10).unwrap();

    for book in books {
        let token_id = book.asset_id;
        // Use best ask price (or midpoint if empty asks)
        let price = book.asks.first().map(|a| a.price)
            .or_else(|| book.bids.first().map(|b| b.price))
            .unwrap_or(Decimal::ZERO);

        if price == Decimal::ZERO {
            continue;
        }

        if let Some((market_id, question, outcome, volume)) = token_info.get(&token_id) {
            if price >= min_favorite && price <= max_favorite {
                opportunities.push(FLBOpportunity {
                    market_id: market_id.clone(),
                    question: question.clone(),
                    token_id,
                    outcome: outcome.clone(),
                    price,
                    bias_type: "Favorite".to_string(),
                    volume: *volume,
                });
            } else if price >= min_longshot && price <= max_longshot {
                opportunities.push(FLBOpportunity {
                    market_id: market_id.clone(),
                    question: question.clone(),
                    token_id,
                    outcome: outcome.clone(),
                    price,
                    bias_type: "Longshot".to_string(),
                    volume: *volume,
                });
            }
        }
    }

    // Sort by price descending
    opportunities.sort_by(|a, b| b.price.cmp(&a.price));

    Ok(opportunities)
}
