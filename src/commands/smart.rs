use anyhow::Result;
use chrono::Utc;
use clap::{Args, Subcommand};
use polymarket_client_sdk::data;
use rust_decimal::prelude::ToPrimitive;

use super::data::{OrderBy, TimePeriod};
use crate::output::OutputFormat;
use crate::output::smart::{
    print_discover_results, print_profile, print_scan_result, print_signals, print_wallet_list,
};
use crate::smart::tracker::PositionChange;
use crate::smart::{
    AggregatedSignal, Signal, SmartScore, TelegramConfig, WatchedWallet, scorer, signals, store,
    tracker,
};

#[derive(Args)]
pub struct SmartArgs {
    #[command(subcommand)]
    pub command: SmartCommand,
}

#[derive(Subcommand)]
pub enum SmartCommand {
    /// Discover smart wallets from the leaderboard
    Discover {
        /// Time period: day, week, month, all
        #[arg(long, default_value = "month")]
        period: TimePeriod,

        /// Sort by: pnl or vol
        #[arg(long, default_value = "pnl")]
        order_by: OrderBy,

        /// Max results
        #[arg(long, default_value = "25")]
        limit: i32,

        /// Auto-watch wallets with score above this threshold
        #[arg(long)]
        auto_watch: Option<f64>,
    },

    /// Add a wallet to the watch list
    Watch {
        /// Wallet address (0x...)
        address: String,

        /// Optional tag (e.g. "whale", "builder")
        #[arg(long)]
        tag: Option<String>,
    },

    /// Remove a wallet from the watch list
    Unwatch {
        /// Wallet address (0x...)
        address: String,
    },

    /// List all watched wallets
    List,

    /// Scan watched wallets for position changes and generate signals
    Scan {
        /// Scan only this wallet instead of the full watch list
        #[arg(long)]
        wallet: Option<String>,

        /// Send macOS notification when signals are detected
        #[arg(long)]
        notify: bool,
    },

    /// View recent signals
    Signals {
        /// Max signals to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Show a detailed profile for a wallet
    Profile {
        /// Wallet address (0x...)
        address: String,
    },

    /// Configure Telegram notifications
    Telegram {
        #[command(subcommand)]
        command: TelegramCommand,
    },
}

#[derive(Subcommand)]
pub enum TelegramCommand {
    /// Set up Telegram bot (saves token and auto-detects chat ID)
    Setup {
        /// Bot token from @BotFather
        token: String,
    },
    /// Send a test notification
    Test,
    /// Show current Telegram config status
    Status,
}

pub async fn execute(
    client: &data::Client,
    args: SmartArgs,
    output: OutputFormat,
) -> Result<()> {
    match args.command {
        SmartCommand::Discover {
            period,
            order_by,
            limit,
            auto_watch,
        } => cmd_discover(client, period, order_by, limit, auto_watch, &output).await,

        SmartCommand::Watch { address, tag } => cmd_watch(&address, tag, &output),
        SmartCommand::Unwatch { address } => cmd_unwatch(&address, &output),
        SmartCommand::List => cmd_list(&output),

        SmartCommand::Scan { wallet, notify } => {
            cmd_scan(client, wallet.as_deref(), notify, &output).await
        }

        SmartCommand::Signals { limit } => cmd_signals(limit, &output),

        SmartCommand::Profile { address } => cmd_profile(client, &address, &output).await,

        SmartCommand::Telegram { command } => cmd_telegram(command, &output).await,
    }
}

async fn cmd_discover(
    client: &data::Client,
    period: TimePeriod,
    order_by: OrderBy,
    limit: i32,
    auto_watch: Option<f64>,
    output: &OutputFormat,
) -> Result<()> {
    use polymarket_client_sdk::data::types::request::TraderLeaderboardRequest;

    let request = TraderLeaderboardRequest::builder()
        .time_period(period.into())
        .order_by(order_by.into())
        .limit(limit)?
        .build();

    let entries = client.leaderboard(&request).await?;

    let scores: Vec<SmartScore> = entries
        .iter()
        .map(|e| {
            scorer::score_from_leaderboard(
                &e.proxy_wallet.to_string(),
                e.user_name.as_deref(),
                e.pnl.to_f64().unwrap_or(0.0),
                e.vol.to_f64().unwrap_or(0.0),
                e.rank as u64,
            )
        })
        .collect();

    // Cache scores
    store::save_scores(&scores)?;

    // Auto-watch if threshold provided
    if let Some(threshold) = auto_watch {
        let mut watched = 0u32;
        for s in &scores {
            if s.score >= threshold {
                let wallet = WatchedWallet {
                    address: s.address.clone(),
                    tag: Some("leaderboard".into()),
                    added_at: Utc::now(),
                    score: Some(s.score),
                };
                if store::add_wallet(wallet)? {
                    watched += 1;
                }
            }
        }
        if watched > 0 {
            eprintln!("Auto-watched {watched} wallet(s) with score >= {threshold}");
        }
    }

    print_discover_results(&scores, output)
}

fn cmd_watch(address: &str, tag: Option<String>, output: &OutputFormat) -> Result<()> {
    let wallet = WatchedWallet {
        address: address.to_string(),
        tag,
        added_at: Utc::now(),
        score: None,
    };

    if store::add_wallet(wallet)? {
        match output {
            OutputFormat::Table => println!("Watching {address}"),
            OutputFormat::Json => {
                println!("{}", serde_json::json!({"watched": true, "address": address}));
            }
        }
    } else {
        match output {
            OutputFormat::Table => println!("Already watching {address}"),
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::json!({"watched": false, "reason": "already watching"})
                );
            }
        }
    }
    Ok(())
}

fn cmd_unwatch(address: &str, output: &OutputFormat) -> Result<()> {
    if store::remove_wallet(address)? {
        match output {
            OutputFormat::Table => println!("Removed {address} from watch list"),
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::json!({"removed": true, "address": address})
                );
            }
        }
    } else {
        match output {
            OutputFormat::Table => println!("{address} was not in the watch list"),
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::json!({"removed": false, "reason": "not found"})
                );
            }
        }
    }
    Ok(())
}

fn cmd_list(output: &OutputFormat) -> Result<()> {
    let wallets = store::load_wallets()?;
    print_wallet_list(&wallets, output)
}

async fn cmd_scan(
    client: &data::Client,
    single_wallet: Option<&str>,
    notify: bool,
    output: &OutputFormat,
) -> Result<()> {
    let wallets = match single_wallet {
        Some(addr) => {
            // Scan a single wallet (doesn't need to be in watch list)
            vec![WatchedWallet {
                address: addr.to_string(),
                tag: None,
                added_at: Utc::now(),
                score: None,
            }]
        }
        None => {
            let w = store::load_wallets()?;
            if w.is_empty() {
                anyhow::bail!(
                    "No wallets in watch list. Use `polymarket smart watch <address>` or \
                     `polymarket smart discover --auto-watch 60` first."
                );
            }
            w
        }
    };

    let mut all_signals = Vec::new();
    let mut scan_summaries: Vec<ScanSummary> = Vec::new();

    for wallet in &wallets {
        match tracker::scan_wallet(client, &wallet.address).await {
            Ok((changes, snapshot)) => {
                let sigs = signals::generate_signals(wallet, &changes);
                scan_summaries.push(ScanSummary {
                    address: wallet.address.clone(),
                    tag: wallet.tag.clone(),
                    positions: snapshot.positions.len(),
                    changes: changes.len(),
                    signals: sigs.len(),
                    change_details: changes,
                });
                all_signals.extend(sigs);
            }
            Err(e) => {
                eprintln!("Error scanning {}: {e}", wallet.address);
            }
        }
    }

    // Persist signals
    store::append_signals(&all_signals)?;

    // Aggregate: detect multiple wallets converging on same market
    let aggregated = signals::aggregate_signals(&all_signals);

    // Notifications
    if notify && !all_signals.is_empty() {
        send_macos_notification(&all_signals, &aggregated);

        // Telegram (if configured)
        if let Ok(Some(tg_config)) = store::load_telegram_config() {
            let text = build_telegram_text(&all_signals, &aggregated);
            if let Err(e) = send_telegram_message(&tg_config, &text).await {
                eprintln!("Telegram notification failed: {e}");
            }
        }
    }

    print_scan_result(&scan_summaries, &all_signals, &aggregated, output)
}

fn cmd_signals(limit: usize, output: &OutputFormat) -> Result<()> {
    let signals = store::load_signals(limit)?;
    print_signals(&signals, output)
}

async fn cmd_profile(
    client: &data::Client,
    address: &str,
    output: &OutputFormat,
) -> Result<()> {
    let score = scorer::score_wallet(client, address).await?;

    let wallets = store::load_wallets()?;
    let is_watched = wallets
        .iter()
        .any(|w| w.address.to_lowercase() == address.to_lowercase());

    print_profile(&score, is_watched, output)
}

// ── Telegram ────────────────────────────────────────────────────

async fn cmd_telegram(command: TelegramCommand, output: &OutputFormat) -> Result<()> {
    match command {
        TelegramCommand::Setup { token } => {
            // Call getUpdates to find the chat_id
            let url = format!("https://api.telegram.org/bot{token}/getUpdates");
            let resp: serde_json::Value = reqwest::get(&url).await?.json().await?;

            if !resp["ok"].as_bool().unwrap_or(false) {
                anyhow::bail!(
                    "Invalid bot token. Please check with @BotFather.\nAPI response: {}",
                    resp
                );
            }

            let results = resp["result"].as_array();
            let chat_id = results
                .and_then(|arr| arr.iter().rev().find_map(|u| u["message"]["chat"]["id"].as_i64()));

            match chat_id {
                Some(id) => {
                    let config = TelegramConfig {
                        bot_token: token,
                        chat_id: id,
                    };
                    store::save_telegram_config(&config)?;
                    match output {
                        OutputFormat::Table => {
                            println!("Telegram configured! chat_id={id}");
                            println!("Run `polymarket smart telegram test` to verify.");
                        }
                        OutputFormat::Json => {
                            println!(
                                "{}",
                                serde_json::json!({"ok": true, "chat_id": id})
                            );
                        }
                    }
                }
                None => {
                    anyhow::bail!(
                        "No messages found. Please send any message to your bot first, then re-run setup."
                    );
                }
            }
            Ok(())
        }
        TelegramCommand::Test => {
            let config = store::load_telegram_config()?
                .ok_or_else(|| anyhow::anyhow!("Telegram not configured. Run `polymarket smart telegram setup <token>` first."))?;

            send_telegram_message(&config, "Polymarket Smart Money — test notification").await?;
            match output {
                OutputFormat::Table => println!("Test message sent!"),
                OutputFormat::Json => {
                    println!("{}", serde_json::json!({"ok": true, "sent": true}));
                }
            }
            Ok(())
        }
        TelegramCommand::Status => {
            match store::load_telegram_config()? {
                Some(config) => {
                    match output {
                        OutputFormat::Table => {
                            println!("Telegram: configured");
                            println!("Chat ID:  {}", config.chat_id);
                            println!("Token:    {}...", &config.bot_token[..10]);
                        }
                        OutputFormat::Json => {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "configured": true,
                                    "chat_id": config.chat_id,
                                })
                            );
                        }
                    }
                }
                None => {
                    match output {
                        OutputFormat::Table => println!("Telegram: not configured"),
                        OutputFormat::Json => {
                            println!("{}", serde_json::json!({"configured": false}));
                        }
                    }
                }
            }
            Ok(())
        }
    }
}

async fn send_telegram_message(config: &TelegramConfig, text: &str) -> Result<()> {
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        config.bot_token
    );
    let body = serde_json::json!({
        "chat_id": config.chat_id,
        "text": text,
        "parse_mode": "Markdown",
    });
    let resp: serde_json::Value = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await?
        .json()
        .await?;
    if !resp["ok"].as_bool().unwrap_or(false) {
        anyhow::bail!("Telegram API error: {}", resp);
    }
    Ok(())
}

fn build_telegram_text(signals: &[Signal], aggregated: &[AggregatedSignal]) -> String {
    let mut lines = vec![format!("*Polymarket: {} signal(s)*", signals.len())];

    // Show aggregated first if any
    for agg in aggregated {
        lines.push(format!(
            "🔥 *{}* {} wallets {} on `{}`\n   Outcome: {} | Size: {:.1} | Price: {:.2}",
            agg.confidence,
            agg.wallet_count,
            agg.direction,
            agg.market_title,
            agg.outcome,
            agg.total_size,
            agg.avg_price,
        ));
    }

    // Individual signals (skip those already in aggregated)
    let remaining: Vec<&Signal> = if aggregated.is_empty() {
        signals.iter().collect()
    } else {
        signals
            .iter()
            .filter(|s| {
                !aggregated.iter().any(|a| {
                    a.signals.iter().any(|as_| as_.id == s.id)
                })
            })
            .collect()
    };

    for sig in remaining.iter().take(5) {
        lines.push(format!(
            "{} {} `{}` [{}] size:{} @{}",
            sig.signal_type,
            sig.confidence,
            sig.market_title,
            sig.outcome,
            sig.size,
            sig.price,
        ));
    }

    if remaining.len() > 5 {
        lines.push(format!("...and {} more", remaining.len() - 5));
    }

    lines.join("\n")
}

// ── Notifications ───────────────────────────────────────────────

fn send_macos_notification(signals: &[Signal], aggregated: &[AggregatedSignal]) {
    let title = format!("Polymarket: {} signal(s) detected", signals.len());
    let body = if aggregated.is_empty() {
        let sig = &signals[0];
        format!(
            "{} {} — {} [{}]",
            sig.signal_type, sig.confidence, sig.market_title, sig.outcome
        )
    } else {
        let agg = &aggregated[0];
        format!(
            "{} wallets {} on {} [{}]",
            agg.wallet_count, agg.direction, agg.market_title, agg.outcome
        )
    };

    // Escape quotes for osascript
    let title = title.replace('"', r#"\""#);
    let body = body.replace('"', r#"\""#);

    let script = format!(
        r#"display notification "{body}" with title "{title}" sound name "Glass""#
    );
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output();
}

/// Summary of a single wallet scan (used for output rendering).
pub struct ScanSummary {
    pub address: String,
    pub tag: Option<String>,
    pub positions: usize,
    pub changes: usize,
    pub signals: usize,
    pub change_details: Vec<PositionChange>,
}
