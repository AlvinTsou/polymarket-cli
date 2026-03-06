use anyhow::Result;
use clap::Args;
use polymarket_client_sdk::gamma::{self, types::request::MarketsRequest};

use crate::output::OutputFormat;
use crate::output::generate::{print_trending_html, print_trending_json};

#[derive(Args)]
pub struct GenerateArgs {
    /// Number of markets to display
    #[arg(long, default_value = "20")]
    pub limit: i32,

    /// Sort field (e.g. volume_num, liquidity_num)
    #[arg(long, default_value = "volume_num")]
    pub order: String,

    /// Output to file instead of stdout
    #[arg(long)]
    pub output_file: Option<String>,

    /// Page title
    #[arg(long, default_value = "Polymarket Trending")]
    pub title: String,

    /// Auto-refresh interval in seconds
    #[arg(long, default_value = "300")]
    pub refresh: u32,
}

pub async fn execute(
    client: &gamma::Client,
    args: GenerateArgs,
    output: OutputFormat,
) -> Result<()> {
    let request = MarketsRequest::builder()
        .closed(false)
        .limit(args.limit)
        .order(args.order)
        .build();

    let markets = client.markets(&request).await?;

    let content = match output {
        OutputFormat::Table => print_trending_html(&markets, &args.title, args.refresh),
        OutputFormat::Json => print_trending_json(&markets)?,
    };

    match &args.output_file {
        Some(path) => {
            std::fs::write(path, &content)?;
            eprintln!("Written to {path}");
        }
        None => print!("{content}"),
    }

    Ok(())
}
