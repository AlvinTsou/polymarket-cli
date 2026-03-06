use anyhow::Result;
use polymarket_client_sdk::clob::types::response::PriceHistoryResponse;
use polymarket_client_sdk::gamma::types::response::{Comment, Market};
use polymarket_client_sdk::types::{Decimal, U256};
use serde_json::json;

use super::comments::print_comments_table;
use super::{OutputFormat, format_decimal};

pub fn print_review(
    market: &Market,
    comments: &[Comment],
    token_ids: &[U256],
    price_histories: &[PriceHistoryResponse],
    output: &OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Table => {
            print_market_summary(market);
            println!();

            let comment_count = comments.len();
            println!("--- Comments ({comment_count}) ---");
            print_comments_table(comments);
            println!();

            println!("--- Price History ---");
            print_price_histories_table(market, token_ids, price_histories);
        }
        OutputFormat::Json => {
            let history: Vec<_> = token_ids
                .iter()
                .zip(price_histories.iter())
                .map(|(token_id, ph)| {
                    let points: Vec<_> = ph
                        .history
                        .iter()
                        .map(|p| json!({"timestamp": p.t, "price": p.p.to_string()}))
                        .collect();
                    json!({
                        "token_id": token_id.to_string(),
                        "history": points,
                    })
                })
                .collect();

            let result = json!({
                "market": market,
                "comments": comments,
                "price_history": history,
            });
            super::print_json(&result)?;
        }
    }
    Ok(())
}

fn print_market_summary(m: &Market) {
    let question = m.question.as_deref().unwrap_or("—");
    println!("=== Market: {question} ===");

    let mut parts = Vec::new();

    let status = if m.closed == Some(true) {
        "Closed"
    } else if m.active == Some(true) {
        "Active"
    } else {
        "Inactive"
    };
    parts.push(format!("Status: {status}"));

    if let Some(prices) = &m.outcome_prices {
        let outcomes = m.outcomes.as_deref().unwrap_or(&[]);
        let price_strs: Vec<String> = prices
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let label = outcomes
                    .get(i)
                    .map(|s| s.as_str())
                    .unwrap_or(if i == 0 { "Yes" } else { "No" });
                format!("{label}: {:.2}¢", p * Decimal::from(100))
            })
            .collect();
        parts.push(format!("Price({})", price_strs.join(", ")));
    }

    if let Some(vol) = m.volume_num {
        parts.push(format!("Volume: {}", format_decimal(vol)));
    }

    println!("{}", parts.join(" | "));
}

fn print_price_histories_table(
    market: &Market,
    token_ids: &[U256],
    price_histories: &[PriceHistoryResponse],
) {
    if price_histories.is_empty() {
        println!("No price history available.");
        return;
    }

    let outcomes = market.outcomes.as_deref().unwrap_or(&[]);

    for (i, (token_id, ph)) in token_ids.iter().zip(price_histories.iter()).enumerate() {
        let label = outcomes
            .get(i)
            .map(|s| s.as_str())
            .unwrap_or("Unknown");
        println!("[{label}] Token: {token_id}");

        if ph.history.is_empty() {
            println!("  No data points.");
        } else {
            use tabled::settings::Style;
            use tabled::{Table, Tabled};

            #[derive(Tabled)]
            struct Row {
                #[tabled(rename = "Timestamp")]
                timestamp: String,
                #[tabled(rename = "Price")]
                price: String,
            }

            let rows: Vec<Row> = ph
                .history
                .iter()
                .map(|p| Row {
                    timestamp: chrono::DateTime::from_timestamp(p.t, 0)
                        .map_or(p.t.to_string(), |dt| {
                            dt.format("%Y-%m-%d %H:%M").to_string()
                        }),
                    price: p.p.to_string(),
                })
                .collect();
            let table = Table::new(rows).with(Style::rounded()).to_string();
            println!("{table}");
        }
        println!();
    }
}
