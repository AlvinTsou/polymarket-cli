use anyhow::Result;
use clap::Args;
use polymarket_client_sdk::clob;
use polymarket_client_sdk::clob::types::{Interval, TimeRange, request::PriceHistoryRequest};
use polymarket_client_sdk::gamma::{
    self,
    types::{
        ParentEntityType,
        request::{CommentsRequest, MarketByIdRequest, MarketBySlugRequest},
        response::Market,
    },
};
use polymarket_client_sdk::types::U256;

use super::clob::CliInterval;
use super::is_numeric_id;
use crate::output::OutputFormat;
use crate::output::review::print_review;

#[derive(Args)]
pub struct ReviewArgs {
    /// Market ID (numeric) or slug
    pub market: String,

    /// Price history interval: 1m, 1h, 6h, 1d, 1w, max
    #[arg(long, default_value = "1d")]
    pub interval: CliInterval,

    /// Max comments to show
    #[arg(long, default_value = "30")]
    pub comments: i32,

    /// Number of price history data points
    #[arg(long)]
    pub fidelity: Option<u32>,
}

pub async fn execute(
    gamma_client: &gamma::Client,
    args: ReviewArgs,
    output: OutputFormat,
) -> Result<()> {
    // 1. Fetch market details
    let market = fetch_market(gamma_client, &args.market).await?;

    // 2. Extract token IDs for price history
    let token_ids: Vec<U256> = market
        .clob_token_ids
        .as_ref()
        .cloned()
        .unwrap_or_default();

    // 3. Fetch comments and price history concurrently
    let comments_request = CommentsRequest::builder()
        .parent_entity_type(ParentEntityType::Market)
        .parent_entity_id(market.id.clone())
        .limit(args.comments)
        .build();

    let interval = Interval::from(args.interval);
    let fidelity = args.fidelity;

    let clob_client = clob::Client::default();

    let (comments_result, price_histories) = tokio::join!(
        gamma_client.comments(&comments_request),
        fetch_price_histories(&clob_client, &token_ids, interval, fidelity),
    );
    let comments = comments_result?;
    let price_histories = price_histories?;

    // 4. Output
    print_review(&market, &comments, &token_ids, &price_histories, &output)
}

async fn fetch_market(client: &gamma::Client, id_or_slug: &str) -> Result<Market> {
    if is_numeric_id(id_or_slug) {
        let req = MarketByIdRequest::builder()
            .id(id_or_slug.to_string())
            .build();
        Ok(client.market_by_id(&req).await?)
    } else {
        let req = MarketBySlugRequest::builder()
            .slug(id_or_slug.to_string())
            .build();
        Ok(client.market_by_slug(&req).await?)
    }
}

async fn fetch_price_histories(
    client: &clob::Client,
    token_ids: &[U256],
    interval: Interval,
    fidelity: Option<u32>,
) -> Result<Vec<polymarket_client_sdk::clob::types::response::PriceHistoryResponse>> {
    let mut results = Vec::with_capacity(token_ids.len());
    for token_id in token_ids {
        let request = PriceHistoryRequest::builder()
            .market(*token_id)
            .time_range(TimeRange::from_interval(interval))
            .maybe_fidelity(fidelity)
            .build();
        results.push(client.price_history(&request).await?);
    }
    Ok(results)
}
