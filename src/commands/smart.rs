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
    AggregatedSignal, FollowRecord, Signal, SignalConfidence, SmartScore, TelegramConfig,
    WatchedWallet, scorer, signals, store, tracker,
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

    /// Interactive follow: pick a signal and place an order
    Follow {
        /// USDC amount per trade
        #[arg(long, default_value = "10")]
        amount: f64,

        /// Dry-run mode (log only, no real orders)
        #[arg(long)]
        dry_run: bool,
    },

    /// Auto-follow signals from scan (runs scan + places orders)
    AutoFollow {
        /// Max USDC per single trade
        #[arg(long, default_value = "10")]
        max_per_trade: f64,

        /// Max USDC total per day
        #[arg(long, default_value = "50")]
        max_per_day: f64,

        /// Minimum confidence: low, med, high
        #[arg(long, default_value = "med")]
        min_confidence: String,

        /// Dry-run mode (log only, no real orders)
        #[arg(long)]
        dry_run: bool,

        /// Also send notifications
        #[arg(long)]
        notify: bool,
    },

    /// Show follow trade history
    History {
        /// Max records to show
        #[arg(long, default_value = "20")]
        limit: usize,
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
    private_key: Option<&str>,
    signature_type: Option<&str>,
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

        SmartCommand::Follow { amount, dry_run } => {
            cmd_follow(amount, dry_run, private_key, signature_type, &output).await
        }

        SmartCommand::AutoFollow {
            max_per_trade,
            max_per_day,
            min_confidence,
            dry_run,
            notify,
        } => {
            let conf = parse_confidence(&min_confidence)?;
            cmd_auto_follow(
                client,
                max_per_trade,
                max_per_day,
                conf,
                dry_run,
                notify,
                private_key,
                signature_type,
                &output,
            )
            .await
        }

        SmartCommand::History { limit } => cmd_history(limit, &output),

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

// ── Follow ──────────────────────────────────────────────────────

fn parse_confidence(s: &str) -> Result<SignalConfidence> {
    match s.to_lowercase().as_str() {
        "low" => Ok(SignalConfidence::Low),
        "med" | "medium" => Ok(SignalConfidence::Medium),
        "high" => Ok(SignalConfidence::High),
        _ => anyhow::bail!("Invalid confidence: {s}. Use: low, med, high"),
    }
}

fn confidence_rank(c: &SignalConfidence) -> u8 {
    match c {
        SignalConfidence::Low => 1,
        SignalConfidence::Medium => 2,
        SignalConfidence::High => 3,
    }
}

async fn cmd_follow(
    amount: f64,
    dry_run: bool,
    private_key: Option<&str>,
    signature_type: Option<&str>,
    output: &OutputFormat,
) -> Result<()> {
    use crate::smart::SignalDirection;

    let recent_signals = store::load_signals(20)?;
    if recent_signals.is_empty() {
        anyhow::bail!("No signals. Run `polymarket smart scan` first.");
    }

    // Show signals for selection
    println!("Recent signals:");
    for (i, sig) in recent_signals.iter().enumerate() {
        let dir = sig.signal_type.direction();
        println!(
            "  [{i}] {} {} {} — {} [{}] @ {} ({})",
            sig.timestamp.format("%m-%d %H:%M"),
            sig.signal_type,
            sig.confidence,
            sig.market_title,
            sig.outcome,
            sig.price,
            if dir == SignalDirection::Buy { "BUY" } else { "SELL" },
        );
    }

    println!("\nEnter signal number to follow (or 'q' to quit):");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input == "q" || input.is_empty() {
        println!("Cancelled.");
        return Ok(());
    }

    let idx: usize = input
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid number"))?;
    if idx >= recent_signals.len() {
        anyhow::bail!("Signal {idx} out of range");
    }

    let signal = &recent_signals[idx];
    let direction = signal.signal_type.direction();
    let price: f64 = signal.price.parse().unwrap_or(0.5);

    println!("\n--- Order Preview ---");
    println!("Market:    {}", signal.market_title);
    println!("Outcome:   {}", signal.outcome);
    println!("Direction: {direction}");
    println!("Amount:    ${amount:.2} USDC");
    println!("Price:     {price}");
    println!("Token ID:  {}", signal.asset);
    if dry_run {
        println!("Mode:      DRY RUN (no real order)");
    }

    println!("\nConfirm? (y/n):");
    let mut confirm = String::new();
    std::io::stdin().read_line(&mut confirm)?;

    if confirm.trim().to_lowercase() != "y" {
        println!("Cancelled.");
        return Ok(());
    }

    let side_str = match direction {
        SignalDirection::Buy => "BUY",
        SignalDirection::Sell => "SELL",
    };

    let order_id = if dry_run {
        println!("[DRY RUN] Would place {side_str} order for ${amount:.2} on {}", signal.market_title);
        None
    } else {
        let result = place_follow_order(
            &signal.asset,
            &direction,
            amount,
            price,
            private_key,
            signature_type,
        )
        .await?;
        println!("Order placed! ID: {result}");
        Some(result)
    };

    let record = FollowRecord {
        timestamp: Utc::now(),
        signal_id: signal.id.clone(),
        market_title: signal.market_title.clone(),
        condition_id: signal.condition_id.clone(),
        asset: signal.asset.clone(),
        outcome: signal.outcome.clone(),
        side: side_str.to_string(),
        amount_usdc: amount,
        price,
        dry_run,
        order_id,
    };
    store::append_follow_record(&record)?;

    match output {
        OutputFormat::Table => {}
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&record)?);
        }
    }
    Ok(())
}

async fn cmd_auto_follow(
    client: &data::Client,
    max_per_trade: f64,
    max_per_day: f64,
    min_confidence: SignalConfidence,
    dry_run: bool,
    notify: bool,
    private_key: Option<&str>,
    signature_type: Option<&str>,
    output: &OutputFormat,
) -> Result<()> {
    use crate::smart::SignalDirection;

    // Check daily limit
    let spent_today = store::today_spend()?;
    let remaining = max_per_day - spent_today;
    if remaining <= 0.0 && !dry_run {
        anyhow::bail!(
            "Daily limit reached: ${spent_today:.2} / ${max_per_day:.2}. Reset tomorrow."
        );
    }

    // Run scan first
    let wallets = store::load_wallets()?;
    if wallets.is_empty() {
        anyhow::bail!("No wallets in watch list.");
    }

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
            Err(e) => eprintln!("Error scanning {}: {e}", wallet.address),
        }
    }

    store::append_signals(&all_signals)?;
    let aggregated = signals::aggregate_signals(&all_signals);

    // Filter signals by confidence
    let eligible: Vec<&Signal> = all_signals
        .iter()
        .filter(|s| confidence_rank(&s.confidence) >= confidence_rank(&min_confidence))
        .collect();

    println!(
        "Scan: {} signal(s), {} eligible (>= {min_confidence})",
        all_signals.len(),
        eligible.len(),
    );

    // Execute follow trades
    let mut followed = 0u32;
    let mut spent = 0.0f64;

    for sig in &eligible {
        if spent + max_per_trade > remaining && !dry_run {
            eprintln!("Daily limit would be exceeded, stopping.");
            break;
        }

        let direction = sig.signal_type.direction();
        let price: f64 = sig.price.parse().unwrap_or(0.5);
        let side_str = match direction {
            SignalDirection::Buy => "BUY",
            SignalDirection::Sell => "SELL",
        };

        let order_id = if dry_run {
            println!(
                "[DRY RUN] {side_str} ${max_per_trade:.2} — {} [{}] @ {price}",
                sig.market_title, sig.outcome
            );
            None
        } else {
            match place_follow_order(
                &sig.asset,
                &direction,
                max_per_trade,
                price,
                private_key,
                signature_type,
            )
            .await
            {
                Ok(id) => {
                    println!(
                        "Placed {side_str} ${max_per_trade:.2} — {} [{}] -> {id}",
                        sig.market_title, sig.outcome
                    );
                    Some(id)
                }
                Err(e) => {
                    eprintln!("Order failed for {}: {e}", sig.market_title);
                    continue;
                }
            }
        };

        let record = FollowRecord {
            timestamp: Utc::now(),
            signal_id: sig.id.clone(),
            market_title: sig.market_title.clone(),
            condition_id: sig.condition_id.clone(),
            asset: sig.asset.clone(),
            outcome: sig.outcome.clone(),
            side: side_str.to_string(),
            amount_usdc: max_per_trade,
            price,
            dry_run,
            order_id,
        };
        store::append_follow_record(&record)?;
        followed += 1;
        spent += max_per_trade;
    }

    println!(
        "\nFollowed: {followed} trade(s), spent: ${spent:.2}{}",
        if dry_run { " (dry run)" } else { "" }
    );

    // Notifications
    if notify && !all_signals.is_empty() {
        send_macos_notification(&all_signals, &aggregated);
        if let Ok(Some(tg_config)) = store::load_telegram_config() {
            let text = build_telegram_text(&all_signals, &aggregated);
            if let Err(e) = send_telegram_message(&tg_config, &text).await {
                eprintln!("Telegram notification failed: {e}");
            }
        }
    }

    if matches!(output, OutputFormat::Json) {
        let data = serde_json::json!({
            "signals_total": all_signals.len(),
            "signals_eligible": eligible.len(),
            "followed": followed,
            "spent_usdc": spent,
            "dry_run": dry_run,
        });
        crate::output::print_json(&data)?;
    }
    Ok(())
}

async fn place_follow_order(
    asset: &str,
    direction: &crate::smart::SignalDirection,
    amount_usdc: f64,
    _price: f64,
    private_key: Option<&str>,
    signature_type: Option<&str>,
) -> Result<String> {
    use polymarket_client_sdk::clob::types::{Amount, OrderType, Side};
    use polymarket_client_sdk::types::{Decimal, U256};
    use std::str::FromStr;

    let signer = crate::auth::resolve_signer(private_key)?;
    let clob_client = crate::auth::authenticate_with_signer(&signer, signature_type).await?;

    let token_id = U256::from_str(asset)
        .map_err(|_| anyhow::anyhow!("Invalid token_id: {asset}"))?;

    let side = match direction {
        crate::smart::SignalDirection::Buy => Side::Buy,
        crate::smart::SignalDirection::Sell => Side::Sell,
    };

    let amount = Amount::usdc(
        Decimal::from_str(&format!("{amount_usdc:.2}"))
            .map_err(|_| anyhow::anyhow!("Invalid amount"))?,
    )?;

    let order = clob_client
        .market_order()
        .token_id(token_id)
        .side(side)
        .amount(amount)
        .order_type(OrderType::FOK)
        .build()
        .await?;
    let signed = clob_client.sign(&signer, order).await?;
    let result = clob_client.post_order(signed).await?;

    Ok(result.order_id)
}

fn cmd_history(limit: usize, output: &OutputFormat) -> Result<()> {
    let mut records = store::load_follow_records()?;
    records.reverse(); // newest first
    records.truncate(limit);

    match output {
        OutputFormat::Table => {
            if records.is_empty() {
                println!("No follow trades yet.");
                return Ok(());
            }
            println!("--- Follow History ({} record(s)) ---", records.len());
            for r in &records {
                let mode = if r.dry_run { "[DRY]" } else { "[LIVE]" };
                let oid = r.order_id.as_deref().unwrap_or("—");
                println!(
                    "  {} {} {} ${:.2} — {} [{}] @ {} -> {}",
                    r.timestamp.format("%m-%d %H:%M"),
                    mode,
                    r.side,
                    r.amount_usdc,
                    r.market_title,
                    r.outcome,
                    r.price,
                    oid,
                );
            }
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&records)?);
        }
    }
    Ok(())
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
