use anyhow::Result;
use clap::{Args, Subcommand};
use polymarket_client_sdk::clob;
use polymarket_client_sdk::gamma;

use crate::arbitrage;
use crate::output::arbitrage::{print_bias_table, print_complement_table};
use crate::output::{OutputFormat, print_json};

#[derive(Args)]
pub struct ArbitrageArgs {
    #[command(subcommand)]
    pub command: ArbitrageCommand,
}

#[derive(Subcommand)]
pub enum ArbitrageCommand {
    /// Scan active binary markets for complement arbitrage (YES + NO < $1.00)
    Complement {
        /// Max active markets to scan (ordered by volume)
        #[arg(long, default_value = "30")]
        limit: i32,
    },
    /// Scan active markets for Favorite-Longshot Bias opportunities
    Bias {
        /// Max active markets to scan (ordered by volume)
        #[arg(long, default_value = "30")]
        limit: i32,
    },
}

pub async fn execute(
    gamma_client: &gamma::Client,
    clob_client: &clob::Client,
    args: ArbitrageArgs,
    output: OutputFormat,
) -> Result<()> {
    match args.command {
        ArbitrageCommand::Complement { limit } => {
            if let OutputFormat::Table = output {
                println!("=== Scanning Active Binary Markets for Complement Arbitrage ===");
                println!("  Limit:  {} markets (by volume)", limit);
                println!("  Target: Sum of YES/NO Best Ask prices < $1.00\n");
            }

            let opportunities = arbitrage::scan_complement_arbitrage(gamma_client, clob_client, limit).await?;

            match output {
                OutputFormat::Table => print_complement_table(&opportunities),
                OutputFormat::Json => print_json(&opportunities)?,
            }
        }

        ArbitrageCommand::Bias { limit } => {
            if let OutputFormat::Table = output {
                println!("=== Scanning Active Markets for Favorite-Longshot Bias ===");
                println!("  Limit:      {} markets (by volume)", limit);
                println!("  Favorites:  $0.80 - $0.95 (systematically undervalued)");
                println!("  Longshots:  $0.01 - $0.05 (systematically overvalued)\n");
            }

            let opportunities = arbitrage::scan_favorite_longshot_bias(gamma_client, clob_client, limit).await?;

            match output {
                OutputFormat::Table => print_bias_table(&opportunities),
                OutputFormat::Json => print_json(&opportunities)?,
            }
        }
    }

    Ok(())
}
