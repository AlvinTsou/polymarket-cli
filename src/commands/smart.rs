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
    AggregatedSignal, FollowRecord, OddsWatch, Signal, SignalConfidence, SmartScore,
    TelegramConfig, WatchedWallet, odds, scorer, signals, store, tracker,
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

        /// Filter by period: day, week, month, all
        #[arg(long, default_value = "all")]
        period: String,

        /// Filter by status: open, closed, all
        #[arg(long, default_value = "all")]
        status: String,
    },

    /// Show ROI of followed trades
    Roi {
        /// Filter by wallet address
        #[arg(long)]
        wallet: Option<String>,

        /// Filter by market keyword
        #[arg(long)]
        market: Option<String>,

        /// Filter by period: day, week, month, all
        #[arg(long, default_value = "all")]
        period: String,

        /// Filter by status: open, closed, all
        #[arg(long, default_value = "all")]
        status: String,
    },

    /// Backtest: analyze what-if returns for all historical signals
    Backtest {
        /// Simulated USDC per trade
        #[arg(long, default_value = "10")]
        amount: f64,

        /// Minimum confidence to include: low, med, high
        #[arg(long, default_value = "low")]
        min_confidence: String,
    },

    /// Generate an HTML dashboard report
    Report,

    /// Start a live dashboard web server
    Dashboard {
        /// Port to listen on
        #[arg(long, default_value = "3456")]
        port: u16,
    },

    /// Configure Telegram notifications
    Telegram {
        #[command(subcommand)]
        command: TelegramCommand,
    },

    /// Monitor market odds/prices for significant changes
    Odds {
        #[command(subcommand)]
        command: OddsCommand,
    },

    /// Real-time monitor: continuous scan + condition triggers + paper trading
    Monitor {
        /// Scan interval (e.g. "30s", "1m", "3m", "5m")
        #[arg(long, default_value = "5m")]
        interval: String,

        /// Minimum signal confidence: low, med, high
        #[arg(long, default_value = "med")]
        min_confidence: String,

        /// Minimum wallet convergence count to trigger
        #[arg(long, default_value = "1")]
        min_wallets: u32,

        /// Only trigger for markets matching these keywords (comma-separated)
        #[arg(long)]
        market_include: Option<String>,

        /// Skip markets matching these keywords (comma-separated)
        #[arg(long)]
        market_exclude: Option<String>,

        /// Also trigger on odds changes >= this percent (0 = disabled)
        #[arg(long, default_value = "0")]
        odds_threshold: f64,

        /// Auto paper-trade (dry-run) on trigger
        #[arg(long)]
        paper_trade: bool,

        /// USDC amount per paper trade
        #[arg(long, default_value = "10")]
        amount: f64,

        /// Max USDC per day for paper trades
        #[arg(long, default_value = "50")]
        max_per_day: f64,

        /// Send macOS + Telegram notifications on trigger
        #[arg(long)]
        notify: bool,

        /// Save these settings for future --load
        #[arg(long)]
        save: bool,

        /// Load saved settings from monitor.json
        #[arg(long)]
        load: bool,
    },
}

#[derive(Subcommand)]
pub enum OddsCommand {
    /// Watch a market token for price changes
    Watch {
        /// Token ID (numeric string from CLOB)
        token_id: String,

        /// Descriptive label for this market
        #[arg(long)]
        label: Option<String>,

        /// Alert threshold in percent (default: 5.0)
        #[arg(long, default_value = "5.0")]
        threshold: f64,
    },
    /// Stop watching a market token
    Unwatch {
        /// Token ID to remove
        token_id: String,
    },
    /// List all watched markets
    List,
    /// Scan watched markets for price changes
    Scan {
        /// Send notifications (macOS + Telegram)
        #[arg(long)]
        notify: bool,
    },
    /// Show recent odds alerts
    Alerts {
        /// Max alerts to show
        #[arg(long, default_value = "20")]
        limit: usize,
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

        SmartCommand::History {
            limit,
            period,
            status,
        } => cmd_history(limit, &period, &status, &output),

        SmartCommand::Roi {
            wallet,
            market,
            period,
            status,
        } => cmd_roi(wallet.as_deref(), market.as_deref(), &period, &status, &output),

        SmartCommand::Backtest {
            amount,
            min_confidence,
        } => {
            let conf = parse_confidence(&min_confidence)?;
            cmd_backtest(amount, conf, &output)
        }

        SmartCommand::Report => cmd_report(),
        SmartCommand::Dashboard { port } => cmd_dashboard(port).await,

        SmartCommand::Telegram { command } => cmd_telegram(command, &output).await,

        SmartCommand::Odds { command } => cmd_odds(command, &output).await,

        SmartCommand::Monitor {
            interval,
            min_confidence,
            min_wallets,
            market_include,
            market_exclude,
            odds_threshold,
            paper_trade,
            amount,
            max_per_day,
            notify,
            save,
            load,
        } => {
            cmd_monitor(
                client, &interval, &min_confidence, min_wallets,
                market_include.as_deref(), market_exclude.as_deref(),
                odds_threshold, paper_trade, amount, max_per_day,
                notify, save, load, &output,
            )
            .await
        }
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

    // Close follow positions when ClosePosition detected
    for sig in &all_signals {
        if matches!(sig.signal_type, crate::smart::SignalType::ClosePosition) {
            let exit_price: f64 = sig.price.parse().unwrap_or(0.0);
            if exit_price > 0.0 {
                match store::close_follow_position(&sig.condition_id, &sig.outcome, exit_price) {
                    Ok(true) => {
                        eprintln!(
                            "Closed follow position: {} [{}] @ {exit_price}",
                            sig.market_title, sig.outcome
                        );
                    }
                    Ok(false) => {} // no matching open position
                    Err(e) => eprintln!("Error closing position: {e}"),
                }
            }
        }
    }

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

    print_scan_result(&scan_summaries, &all_signals, &aggregated, output)?;

    // Also scan odds watches if any exist
    let odds_watches = store::load_odds_watches().unwrap_or_default();
    if !odds_watches.is_empty() {
        let odds_alerts = odds::scan_odds().await.unwrap_or_else(|e| {
            eprintln!("Odds scan failed: {e}");
            Vec::new()
        });
        if !odds_alerts.is_empty() {
            store::append_odds_alerts(&odds_alerts)?;
            use crate::output::smart::print_odds_alerts;
            println!();
            print_odds_alerts(&odds_alerts, output)?;

            if notify {
                send_odds_macos_notification(&odds_alerts);
                if let Ok(Some(tg_config)) = store::load_telegram_config() {
                    let text = build_odds_telegram_text(&odds_alerts);
                    let _ = send_telegram_message(&tg_config, &text).await;
                }
            }
        }
    }

    Ok(())
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

/// Parse period string to a cutoff datetime.
fn period_cutoff(period: &str) -> Option<chrono::DateTime<Utc>> {
    let now = Utc::now();
    match period.to_lowercase().as_str() {
        "day" | "1d" => Some(now - chrono::Duration::days(1)),
        "week" | "7d" => Some(now - chrono::Duration::days(7)),
        "month" | "30d" => Some(now - chrono::Duration::days(30)),
        _ => None, // "all"
    }
}

/// Filter records by period and status.
fn filter_records(
    records: &[FollowRecord],
    period: &str,
    status_filter: &str,
) -> Vec<FollowRecord> {
    let cutoff = period_cutoff(period);
    records
        .iter()
        .filter(|r| {
            if let Some(c) = cutoff {
                if r.timestamp < c {
                    return false;
                }
            }
            match status_filter.to_lowercase().as_str() {
                "open" => r.is_open(),
                "closed" => !r.is_open(),
                _ => true,
            }
        })
        .cloned()
        .collect()
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

    let pos_id = format!(
        "pos_{}_{}",
        Utc::now().format("%Y%m%d%H%M%S"),
        &signal.condition_id[..8.min(signal.condition_id.len())]
    );
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
        fill_price: if dry_run { None } else { Some(price) },
        status: Some(crate::smart::TradeStatus::Open),
        closed_at: None,
        exit_price: None,
        realized_pnl: None,
        position_id: Some(pos_id),
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

        let pos_id = format!(
            "pos_{}_{}",
            Utc::now().format("%Y%m%d%H%M%S"),
            &sig.condition_id[..8.min(sig.condition_id.len())]
        );
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
            fill_price: if dry_run { None } else { Some(price) },
            status: Some(crate::smart::TradeStatus::Open),
            closed_at: None,
            exit_price: None,
            realized_pnl: None,
            position_id: Some(pos_id),
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

fn cmd_history(limit: usize, period: &str, status_filter: &str, output: &OutputFormat) -> Result<()> {
    let all_records = store::load_follow_records()?;
    let mut records = filter_records(&all_records, period, status_filter);
    records.reverse(); // newest first
    records.truncate(limit);

    match output {
        OutputFormat::Table => {
            if records.is_empty() {
                println!("No follow trades match the filter.");
                return Ok(());
            }
            use tabled::{Table, Tabled, settings::Style};
            #[derive(Tabled)]
            struct HRow {
                #[tabled(rename = "Time")]
                time: String,
                #[tabled(rename = "Mode")]
                mode: String,
                #[tabled(rename = "Status")]
                status: String,
                #[tabled(rename = "Side")]
                side: String,
                #[tabled(rename = "Amount")]
                amount: String,
                #[tabled(rename = "Market")]
                market: String,
                #[tabled(rename = "Outcome")]
                outcome: String,
                #[tabled(rename = "Entry")]
                entry: String,
                #[tabled(rename = "Exit")]
                exit: String,
                #[tabled(rename = "PnL")]
                pnl: String,
            }
            let rows: Vec<HRow> = records
                .iter()
                .map(|r| {
                    let status_str = r.status.as_ref()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "OPEN".to_string());
                    let exit_str = r.exit_price
                        .map(|p| format!("{p:.2}"))
                        .unwrap_or_else(|| "—".to_string());
                    let pnl_str = r.realized_pnl
                        .map(|p| format!("{p:+.2}"))
                        .unwrap_or_else(|| "—".to_string());
                    HRow {
                        time: r.timestamp.format("%m-%d %H:%M").to_string(),
                        mode: if r.dry_run { "DRY" } else { "LIVE" }.to_string(),
                        status: status_str,
                        side: r.side.clone(),
                        amount: format!("${:.2}", r.amount_usdc),
                        market: crate::output::truncate(&r.market_title, 22),
                        outcome: r.outcome.clone(),
                        entry: format!("{:.2}", r.effective_entry()),
                        exit: exit_str,
                        pnl: pnl_str,
                    }
                })
                .collect();
            println!("--- Follow History ({} record(s)) ---", rows.len());
            let table = Table::new(rows).with(Style::rounded()).to_string();
            println!("{table}");
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&records)?);
        }
    }
    Ok(())
}

// ── ROI ─────────────────────────────────────────────────────────

fn cmd_roi(
    wallet_filter: Option<&str>,
    market_filter: Option<&str>,
    period: &str,
    status_filter: &str,
    output: &OutputFormat,
) -> Result<()> {
    let all_records = store::load_follow_records()?;
    if all_records.is_empty() {
        println!("No follow trades yet. Use `polymarket smart follow` or `auto-follow` first.");
        return Ok(());
    }

    let mut records = filter_records(&all_records, period, status_filter);

    // Additional filters
    if let Some(w) = wallet_filter {
        let w_lower = w.to_lowercase();
        // Match signal_id which contains wallet info, or filter via signal store
        // For now, filter by signal_id prefix (best effort)
        records.retain(|r| r.signal_id.to_lowercase().contains(&w_lower));
    }
    if let Some(m) = market_filter {
        let m_lower = m.to_lowercase();
        records.retain(|r| r.market_title.to_lowercase().contains(&m_lower));
    }

    let price_map = store::current_price_map()?;

    let mut realized_pnl_total = 0.0f64;
    let mut unrealized_pnl_total = 0.0f64;
    let mut total_invested = 0.0f64;
    let mut total_current = 0.0f64;
    let mut closed_wins = 0u32;
    let mut closed_total = 0u32;
    let mut rows: Vec<RoiRow> = Vec::new();

    for r in &records {
        let entry_price = r.effective_entry();
        if entry_price <= 0.0 {
            continue;
        }

        let (pnl, current_price, roi_pct);
        if !r.is_open() {
            // Closed trade: use realized PnL
            let rpnl = r.realized_pnl.unwrap_or(0.0);
            let exit = r.exit_price.unwrap_or(entry_price);
            pnl = rpnl;
            current_price = exit;
            roi_pct = if r.amount_usdc > 0.0 { (rpnl / r.amount_usdc) * 100.0 } else { 0.0 };
            realized_pnl_total += rpnl;
            closed_total += 1;
            if rpnl > 0.0 {
                closed_wins += 1;
            }
        } else {
            // Open trade: use snapshot price
            let shares = r.amount_usdc / entry_price;
            current_price = price_map.get(&r.condition_id).copied().unwrap_or(entry_price);
            let current_value = shares * current_price;
            pnl = current_value - r.amount_usdc;
            roi_pct = if r.amount_usdc > 0.0 { (pnl / r.amount_usdc) * 100.0 } else { 0.0 };
            unrealized_pnl_total += pnl;
        }

        total_invested += r.amount_usdc;
        total_current += r.amount_usdc + pnl;

        rows.push(RoiRow {
            time: r.timestamp.format("%m-%d %H:%M").to_string(),
            mode: if r.dry_run { "DRY" } else { "LIVE" }.to_string(),
            market: crate::output::truncate(&r.market_title, 22),
            outcome: r.outcome.clone(),
            side: r.side.clone(),
            invested: r.amount_usdc,
            entry: entry_price,
            current: current_price,
            pnl,
            roi_pct,
            status: r.status.as_ref().map(|s| s.to_string()).unwrap_or_else(|| "OPEN".to_string()),
        });
    }

    let total_pnl = realized_pnl_total + unrealized_pnl_total;
    let total_roi = if total_invested > 0.0 { (total_pnl / total_invested) * 100.0 } else { 0.0 };
    let closed_win_rate = if closed_total > 0 {
        closed_wins as f64 / closed_total as f64 * 100.0
    } else {
        0.0
    };

    match output {
        OutputFormat::Table => {
            use tabled::{Table, Tabled, settings::Style};
            #[derive(Tabled)]
            struct TRow {
                #[tabled(rename = "Time")]
                time: String,
                #[tabled(rename = "Mode")]
                mode: String,
                #[tabled(rename = "Status")]
                status: String,
                #[tabled(rename = "Market")]
                market: String,
                #[tabled(rename = "Side")]
                side: String,
                #[tabled(rename = "Invested")]
                invested: String,
                #[tabled(rename = "Entry")]
                entry: String,
                #[tabled(rename = "Now/Exit")]
                current: String,
                #[tabled(rename = "PnL")]
                pnl: String,
                #[tabled(rename = "ROI")]
                roi: String,
            }
            let trows: Vec<TRow> = rows
                .iter()
                .map(|r| TRow {
                    time: r.time.clone(),
                    mode: r.mode.clone(),
                    status: r.status.clone(),
                    market: r.market.clone(),
                    side: r.side.clone(),
                    invested: format!("${:.2}", r.invested),
                    entry: format!("{:.2}", r.entry),
                    current: format!("{:.2}", r.current),
                    pnl: format!("{:+.2}", r.pnl),
                    roi: format!("{:+.1}%", r.roi_pct),
                })
                .collect();
            println!("--- Follow ROI ({} trade(s)) ---", rows.len());
            let table = Table::new(trows).with(Style::rounded()).to_string();
            println!("{table}");
            println!();
            println!("  Realized PnL:   {realized_pnl_total:+.2} ({closed_total} closed, {closed_win_rate:.0}% win rate)");
            println!("  Unrealized PnL: {unrealized_pnl_total:+.2} ({} open)", rows.len() as u32 - closed_total);
            println!("  Total PnL:      {total_pnl:+.2} ({total_roi:+.1}%)");
        }
        OutputFormat::Json => {
            let data = serde_json::json!({
                "trades": rows.iter().map(|r| serde_json::json!({
                    "time": r.time, "mode": r.mode, "status": r.status,
                    "market": r.market, "outcome": r.outcome, "side": r.side,
                    "invested": r.invested, "entry_price": r.entry,
                    "current_price": r.current, "pnl": r.pnl, "roi_pct": r.roi_pct,
                })).collect::<Vec<_>>(),
                "total_invested": total_invested,
                "total_current": total_current,
                "realized_pnl": realized_pnl_total,
                "unrealized_pnl": unrealized_pnl_total,
                "total_pnl": total_pnl,
                "total_roi_pct": total_roi,
                "closed_trades": closed_total,
                "closed_win_rate": closed_win_rate,
            });
            crate::output::print_json(&data)?;
        }
    }
    Ok(())
}

struct RoiRow {
    time: String,
    mode: String,
    market: String,
    outcome: String,
    side: String,
    invested: f64,
    entry: f64,
    current: f64,
    pnl: f64,
    roi_pct: f64,
    status: String,
}

// ── Backtest ────────────────────────────────────────────────────

fn cmd_backtest(amount: f64, min_confidence: SignalConfidence, output: &OutputFormat) -> Result<()> {
    let all_signals = store::load_signals(1000)?; // load all
    if all_signals.is_empty() {
        println!("No signals to backtest. Run `polymarket smart scan` to generate signals first.");
        return Ok(());
    }

    let price_map = store::current_price_map()?;

    let eligible: Vec<&Signal> = all_signals
        .iter()
        .filter(|s| confidence_rank(&s.confidence) >= confidence_rank(&min_confidence))
        .collect();

    let mut total_invested = 0.0f64;
    let mut total_current = 0.0f64;
    let mut results: Vec<BacktestResult> = Vec::new();

    for sig in &eligible {
        let entry_price: f64 = sig.price.parse().unwrap_or(0.0);
        if entry_price <= 0.0 {
            continue;
        }

        let current_price = price_map
            .get(&sig.condition_id)
            .copied()
            .unwrap_or(entry_price);

        let is_buy = matches!(
            sig.signal_type,
            crate::smart::SignalType::NewPosition | crate::smart::SignalType::IncreasePosition
        );

        let shares = amount / entry_price;
        let current_value = if is_buy {
            shares * current_price
        } else {
            // For sell signals: profit if price went down
            amount + (entry_price - current_price) * shares
        };
        let pnl = current_value - amount;
        let roi_pct = (pnl / amount) * 100.0;

        total_invested += amount;
        total_current += current_value;

        results.push(BacktestResult {
            time: sig.timestamp.format("%m-%d %H:%M").to_string(),
            confidence: sig.confidence.to_string(),
            direction: sig.signal_type.direction().to_string(),
            market: crate::output::truncate(&sig.market_title, 25),
            outcome: sig.outcome.clone(),
            entry_price,
            current_price,
            pnl,
            roi_pct,
        });
    }

    let total_pnl = total_current - total_invested;
    let total_roi = if total_invested > 0.0 {
        (total_pnl / total_invested) * 100.0
    } else {
        0.0
    };
    let winners = results.iter().filter(|r| r.pnl > 0.0).count();
    let win_rate = if results.is_empty() {
        0.0
    } else {
        winners as f64 / results.len() as f64 * 100.0
    };

    match output {
        OutputFormat::Table => {
            use tabled::{Table, Tabled, settings::Style};
            #[derive(Tabled)]
            struct TRow {
                #[tabled(rename = "Time")]
                time: String,
                #[tabled(rename = "Conf")]
                confidence: String,
                #[tabled(rename = "Dir")]
                direction: String,
                #[tabled(rename = "Market")]
                market: String,
                #[tabled(rename = "Entry")]
                entry: String,
                #[tabled(rename = "Now")]
                current: String,
                #[tabled(rename = "PnL")]
                pnl: String,
                #[tabled(rename = "ROI")]
                roi: String,
            }
            let trows: Vec<TRow> = results
                .iter()
                .map(|r| TRow {
                    time: r.time.clone(),
                    confidence: r.confidence.clone(),
                    direction: r.direction.clone(),
                    market: r.market.clone(),
                    entry: format!("{:.2}", r.entry_price),
                    current: format!("{:.2}", r.current_price),
                    pnl: format!("{:+.2}", r.pnl),
                    roi: format!("{:+.1}%", r.roi_pct),
                })
                .collect();
            println!(
                "--- Backtest: {} signal(s), ${amount:.2}/trade, >= {min_confidence} ---",
                results.len()
            );
            if !trows.is_empty() {
                let table = Table::new(trows).with(Style::rounded()).to_string();
                println!("{table}");
            }
            println!("\nSummary:");
            println!("  Signals:    {}", results.len());
            println!("  Winners:    {winners} ({win_rate:.0}%)");
            println!("  Invested:   ${total_invested:.2}");
            println!("  Current:    ${total_current:.2}");
            println!("  Total PnL:  {total_pnl:+.2} ({total_roi:+.1}%)");
        }
        OutputFormat::Json => {
            let data = serde_json::json!({
                "signals_tested": results.len(),
                "amount_per_trade": amount,
                "winners": winners,
                "win_rate_pct": win_rate,
                "total_invested": total_invested,
                "total_current": total_current,
                "total_pnl": total_pnl,
                "total_roi_pct": total_roi,
                "results": results.iter().map(|r| serde_json::json!({
                    "time": r.time, "confidence": r.confidence,
                    "direction": r.direction, "market": r.market,
                    "outcome": r.outcome,
                    "entry_price": r.entry_price, "current_price": r.current_price,
                    "pnl": r.pnl, "roi_pct": r.roi_pct,
                })).collect::<Vec<_>>(),
            });
            crate::output::print_json(&data)?;
        }
    }
    Ok(())
}

struct BacktestResult {
    time: String,
    confidence: String,
    direction: String,
    market: String,
    outcome: String,
    entry_price: f64,
    current_price: f64,
    pnl: f64,
    roi_pct: f64,
}

// ── Live Dashboard Server ────────────────────────────────────────

async fn cmd_dashboard(port: u16) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).await?;
    println!("Dashboard running at http://localhost:{port}");
    println!("Press Ctrl+C to stop.");

    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            // Read request (we only serve one page, ignore the path)
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;

            let html = build_live_dashboard();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                html.len(),
                html
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        });
    }
}

fn build_live_dashboard() -> String {
    let wallets = store::load_wallets().unwrap_or_default();
    let signals = store::load_signals(50).unwrap_or_default();
    let follows = store::load_follow_records().unwrap_or_default();
    let price_map = store::current_price_map().unwrap_or_default();
    let snapshots = store::load_all_snapshots().unwrap_or_default();
    let odds_watches = store::load_odds_watches().unwrap_or_default();
    let odds_alerts = store::load_odds_alerts(30).unwrap_or_default();

    let wallet_positions: std::collections::HashMap<String, usize> = snapshots
        .iter()
        .map(|s| (s.address.clone(), s.positions.len()))
        .collect();

    let now = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();

    // -- Wallets table --
    let wallets_rows: String = wallets
        .iter()
        .map(|w| {
            let score = w.score.unwrap_or(0.0);
            let bar_w = score.clamp(0.0, 100.0);
            let color = if score >= 90.0 { "#4ade80" } else if score >= 70.0 { "#facc15" } else { "#f87171" };
            let positions = wallet_positions.get(&w.address).copied().unwrap_or(0);
            format!(
                "<tr><td class='mono'>{}</td><td>{}</td><td><div class='score-bar' style='--w:{}%;--c:{}'>{:.1}</div></td><td>{}</td></tr>",
                html_escape(&w.address),
                html_escape(w.tag.as_deref().unwrap_or("—")),
                bar_w, color, score, positions
            )
        })
        .collect();

    // -- Signals table --
    let signals_rows: String = signals
        .iter()
        .take(30)
        .map(|s| {
            let conf = s.confidence.to_string();
            let conf_cls = match conf.as_str() { "HIGH" => "badge-high", "MED" => "badge-med", _ => "badge-low" };
            let st = s.signal_type.to_string();
            let type_cls = if st.contains("NEW") || st.contains("INCREASE") { "text-green" } else { "text-red" };
            format!(
                "<tr><td>{}</td><td class='{}'>{}</td><td><span class='badge {}'>{}</span></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                s.timestamp.format("%m-%d %H:%M"),
                type_cls, st, conf_cls, conf,
                html_escape(&s.market_title), html_escape(&s.outcome), s.price, s.size
            )
        })
        .collect();

    // -- Follows table + PnL --
    let mut total_invested = 0.0f64;
    let mut total_pnl = 0.0f64;
    let follows_rows: String = follows
        .iter()
        .map(|r| {
            let entry = r.price;
            let current = price_map.get(&r.condition_id).copied().unwrap_or(entry);
            let shares = if entry > 0.0 { r.amount_usdc / entry } else { 0.0 };
            let pnl = shares * current - r.amount_usdc;
            let roi = if r.amount_usdc > 0.0 { pnl / r.amount_usdc * 100.0 } else { 0.0 };
            total_invested += r.amount_usdc;
            total_pnl += pnl;
            let pnl_cls = if pnl >= 0.0 { "text-green" } else { "text-red" };
            let mode_cls = if r.dry_run { "badge-low" } else { "badge-high" };
            let mode = if r.dry_run { "DRY" } else { "LIVE" };
            format!(
                "<tr><td>{}</td><td><span class='badge {}'>{}</span></td><td>{}</td><td>{}</td><td>${:.2}</td><td>{:.3}</td><td>{:.3}</td><td class='{}'>{:+.2}</td><td class='{}'>{:+.1}%</td></tr>",
                r.timestamp.format("%m-%d %H:%M"),
                mode_cls, mode, html_escape(&r.side),
                html_escape(&r.market_title), r.amount_usdc, entry, current,
                pnl_cls, pnl, pnl_cls, roi
            )
        })
        .collect();

    // -- Odds watches table --
    let odds_rows: String = odds_watches
        .iter()
        .map(|w| {
            let change = if w.baseline_price > 0.0 {
                ((w.last_price - w.baseline_price) / w.baseline_price) * 100.0
            } else { 0.0 };
            let change_cls = if change >= 0.0 { "text-green" } else { "text-red" };
            let dir = if change >= 0.0 { "+" } else { "" };
            format!(
                "<tr><td>{}</td><td class='mono'>{}</td><td>{:.1}%</td><td>{:.4}</td><td>{:.4}</td><td class='{}'>{}{:.1}%</td><td>{}</td></tr>",
                html_escape(&w.label),
                &w.token_id[..w.token_id.len().min(18)],
                w.threshold_pct, w.baseline_price, w.last_price,
                change_cls, dir, change,
                w.last_scanned.map_or("—".into(), |t| t.format("%m-%d %H:%M").to_string())
            )
        })
        .collect();

    // -- Odds alerts table --
    let odds_alerts_rows: String = odds_alerts
        .iter()
        .take(20)
        .map(|a| {
            let dir = if a.change_pct >= 0.0 { "+" } else { "" };
            let cls = if a.change_pct >= 0.0 { "text-green" } else { "text-red" };
            format!(
                "<tr><td>{}</td><td>{}</td><td class='{}'>{}{:.1}%</td><td>{:.4}</td><td>{:.4}</td></tr>",
                a.timestamp.format("%m-%d %H:%M"),
                html_escape(&a.label), cls, dir, a.change_pct,
                a.previous_price, a.current_price
            )
        })
        .collect();

    let total_roi = if total_invested > 0.0 { total_pnl / total_invested * 100.0 } else { 0.0 };
    let pnl_color = if total_pnl >= 0.0 { "#4ade80" } else { "#f87171" };

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="refresh" content="60">
<title>PMCC Live Dashboard</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#0a0e1a;color:#e2e8f0;line-height:1.6;padding:1.5rem 2rem}}
h1{{font-size:1.6rem;color:#f8fafc;display:inline-block}}
.header{{display:flex;justify-content:space-between;align-items:center;margin-bottom:1.5rem;border-bottom:1px solid #1e293b;padding-bottom:1rem}}
.meta{{color:#64748b;font-size:.8rem}}
.live-dot{{display:inline-block;width:8px;height:8px;background:#4ade80;border-radius:50%;margin-right:6px;animation:pulse 2s infinite}}
@keyframes pulse{{0%,100%{{opacity:1}}50%{{opacity:.3}}}}
.cards{{display:grid;grid-template-columns:repeat(auto-fit,minmax(170px,1fr));gap:.8rem;margin-bottom:1.5rem}}
.card{{background:#111827;border-radius:10px;padding:1rem;border:1px solid #1e293b}}
.card .label{{color:#64748b;font-size:.7rem;text-transform:uppercase;letter-spacing:.05em}}
.card .value{{font-size:1.6rem;font-weight:700;margin-top:.2rem}}
h2{{font-size:1.1rem;margin:1.5rem 0 .6rem;color:#94a3b8;display:flex;align-items:center;gap:.5rem}}
h2 .count{{background:#1e293b;color:#64748b;font-size:.75rem;padding:1px 8px;border-radius:10px}}
table{{width:100%;border-collapse:collapse;font-size:.8rem;margin-bottom:.5rem}}
th{{text-align:left;padding:.5rem .6rem;background:#111827;color:#64748b;font-weight:600;text-transform:uppercase;font-size:.7rem;letter-spacing:.05em;border-bottom:2px solid #1e293b;position:sticky;top:0}}
td{{padding:.4rem .6rem;border-bottom:1px solid #111827}}
tr:hover td{{background:#111827aa}}
.mono{{font-family:'SF Mono',Consolas,monospace;font-size:.75rem}}
.text-green{{color:#4ade80}}
.text-red{{color:#f87171}}
.text-yellow{{color:#facc15}}
.badge{{display:inline-block;padding:1px 7px;border-radius:3px;font-size:.7rem;font-weight:600}}
.badge-high{{background:#166534;color:#4ade80}}
.badge-med{{background:#854d0e;color:#facc15}}
.badge-low{{background:#1e293b;color:#94a3b8}}
.score-bar{{position:relative;background:#1e293b;border-radius:3px;padding:1px 6px;font-size:.75rem;font-weight:600}}
.score-bar::before{{content:'';position:absolute;left:0;top:0;bottom:0;width:var(--w);background:var(--c);opacity:.2;border-radius:3px}}
.empty{{color:#475569;text-align:center;padding:1.5rem;font-size:.85rem}}
.grid-2{{display:grid;grid-template-columns:1fr 1fr;gap:1.5rem}}
@media(max-width:900px){{.grid-2{{grid-template-columns:1fr}}}}
.section{{background:#0f1629;border:1px solid #1e293b;border-radius:10px;padding:1rem;overflow-x:auto}}
</style>
</head>
<body>
<div class="header">
  <div><span class="live-dot"></span><h1>PMCC Live Dashboard</h1></div>
  <div class="meta">Auto-refresh 60s &middot; {now}</div>
</div>

<div class="cards">
  <div class="card"><div class="label">Watched Wallets</div><div class="value">{wallet_count}</div></div>
  <div class="card"><div class="label">Signals</div><div class="value">{signal_count}</div></div>
  <div class="card"><div class="label">Odds Watches</div><div class="value">{odds_count}</div></div>
  <div class="card"><div class="label">Follow Trades</div><div class="value">{follow_count}</div></div>
  <div class="card"><div class="label">Total Invested</div><div class="value">${total_invested:.0}</div></div>
  <div class="card"><div class="label">Total PnL</div><div class="value" style="color:{pnl_color}">{total_pnl:+.2}</div></div>
</div>

<div class="grid-2">
<div>
<h2>Odds Monitoring <span class="count">{odds_count}</span></h2>
<div class="section">
{odds_section}
</div>

<h2>Odds Alerts <span class="count">{odds_alert_count}</span></h2>
<div class="section">
{odds_alerts_section}
</div>
</div>

<div>
<h2>Watched Wallets <span class="count">{wallet_count}</span></h2>
<div class="section">
{wallets_section}
</div>
</div>
</div>

<h2>Recent Signals <span class="count">{signal_count}</span></h2>
<div class="section">
{signals_section}
</div>

<h2>Follow Trades <span class="count">{follow_count}</span></h2>
<div class="section">
{follows_section}
</div>

<p class="meta" style="margin-top:2rem;text-align:center">PMCC Smart Money System &mdash; polymarket-cli</p>
</body>
</html>"##,
        now = now,
        wallet_count = wallets.len(),
        signal_count = signals.len(),
        odds_count = odds_watches.len(),
        odds_alert_count = odds_alerts.len(),
        follow_count = follows.len(),
        total_invested = total_invested,
        total_pnl = total_pnl,
        pnl_color = pnl_color,
        odds_section = if odds_rows.is_empty() {
            "<p class='empty'>No markets watched. Use <code>polymarket smart odds watch</code></p>".into()
        } else {
            format!("<table><thead><tr><th>Label</th><th>Token</th><th>Threshold</th><th>Baseline</th><th>Last</th><th>Change</th><th>Scanned</th></tr></thead><tbody>{odds_rows}</tbody></table>")
        },
        odds_alerts_section = if odds_alerts_rows.is_empty() {
            "<p class='empty'>No alerts yet.</p>".into()
        } else {
            format!("<table><thead><tr><th>Time</th><th>Market</th><th>Change</th><th>From</th><th>To</th></tr></thead><tbody>{odds_alerts_rows}</tbody></table>")
        },
        wallets_section = if wallets_rows.is_empty() {
            "<p class='empty'>No wallets being watched.</p>".into()
        } else {
            format!("<table><thead><tr><th>Address</th><th>Tag</th><th>Score</th><th>Positions</th></tr></thead><tbody>{wallets_rows}</tbody></table>")
        },
        signals_section = if signals_rows.is_empty() {
            "<p class='empty'>No signals yet. Run scan to generate.</p>".into()
        } else {
            format!("<table><thead><tr><th>Time</th><th>Type</th><th>Conf</th><th>Market</th><th>Outcome</th><th>Price</th><th>Size</th></tr></thead><tbody>{signals_rows}</tbody></table>")
        },
        follows_section = if follows_rows.is_empty() {
            "<p class='empty'>No follow trades yet.</p>".into()
        } else {
            format!(
                "<table><thead><tr><th>Time</th><th>Mode</th><th>Side</th><th>Market</th><th>Invested</th><th>Entry</th><th>Now</th><th>PnL</th><th>ROI</th></tr></thead><tbody>{follows_rows}</tbody></table>\
                 <p style='margin-top:.5rem;color:#94a3b8;font-size:.8rem'>Total: ${total_invested:.2} | PnL: <span style='color:{pnl_color}'>{total_pnl:+.2}</span> ({total_roi:+.1}%)</p>"
            )
        },
    )
}

// ── Monitor ──────────────────────────────────────────────────────

fn parse_duration(s: &str) -> Result<std::time::Duration> {
    let s = s.trim().to_lowercase();
    if let Some(n) = s.strip_suffix('s') {
        let secs: u64 = n.parse().map_err(|_| anyhow::anyhow!("Invalid duration: {s}"))?;
        return Ok(std::time::Duration::from_secs(secs));
    }
    if let Some(n) = s.strip_suffix('m') {
        let mins: u64 = n.parse().map_err(|_| anyhow::anyhow!("Invalid duration: {s}"))?;
        return Ok(std::time::Duration::from_secs(mins * 60));
    }
    if let Some(n) = s.strip_suffix('h') {
        let hours: u64 = n.parse().map_err(|_| anyhow::anyhow!("Invalid duration: {s}"))?;
        return Ok(std::time::Duration::from_secs(hours * 3600));
    }
    // Fallback: try as seconds
    let secs: u64 = s.parse().map_err(|_| anyhow::anyhow!("Invalid duration: {s}. Use e.g. 30s, 3m, 1h"))?;
    Ok(std::time::Duration::from_secs(secs))
}

fn split_keywords(s: Option<&str>) -> Vec<String> {
    s.map(|v| v.split(',').map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()).collect())
        .unwrap_or_default()
}

fn evaluate_triggers(
    all_signals: &[Signal],
    aggregated: &[AggregatedSignal],
    odds_alerts: &[crate::smart::OddsAlert],
    config: &crate::smart::MonitorConfig,
) -> Vec<crate::smart::TriggerEvent> {
    use crate::smart::{TriggerEvent, TriggerType};

    let market_include = &config.market_include;
    let market_exclude = &config.market_exclude;

    let matches_include = |title: &str| -> bool {
        if market_include.is_empty() { return true; }
        let lower = title.to_lowercase();
        market_include.iter().any(|kw| lower.contains(kw))
    };
    let matches_exclude = |title: &str| -> bool {
        if market_exclude.is_empty() { return false; }
        let lower = title.to_lowercase();
        market_exclude.iter().any(|kw| lower.contains(kw))
    };

    let mut triggers = Vec::new();

    // Check aggregated signals (multi-wallet convergence)
    if config.min_wallets > 1 {
        for agg in aggregated {
            if (agg.wallet_count as u32) < config.min_wallets { continue; }
            if confidence_rank(&agg.confidence) < confidence_rank(&config.min_confidence) { continue; }
            if !matches_include(&agg.market_title) || matches_exclude(&agg.market_title) { continue; }

            let first_sig = agg.signals.first();
            triggers.push(TriggerEvent {
                trigger_type: TriggerType::Aggregated,
                confidence: agg.confidence.clone(),
                market_title: agg.market_title.clone(),
                outcome: agg.outcome.clone(),
                direction: agg.direction.clone(),
                price: agg.avg_price,
                wallet_count: agg.wallet_count as u32,
                reason: format!("{} wallets converge on {} {}", agg.wallet_count, agg.direction, agg.outcome),
                condition_id: agg.condition_id.clone(),
                asset: first_sig.map(|s| s.asset.clone()).unwrap_or_default(),
                signal_id: first_sig.map(|s| s.id.clone()).unwrap_or_default(),
            });
        }
    }

    // Check individual signals (if min_wallets <= 1 or as supplement)
    // Skip signals already covered by aggregated triggers
    let aggregated_conditions: std::collections::HashSet<String> = triggers.iter().map(|t| t.condition_id.clone()).collect();

    for sig in all_signals {
        if aggregated_conditions.contains(&sig.condition_id) { continue; }
        if confidence_rank(&sig.confidence) < confidence_rank(&config.min_confidence) { continue; }
        if !matches_include(&sig.market_title) || matches_exclude(&sig.market_title) { continue; }

        let tag = sig.wallet_tag.as_deref().unwrap_or(&sig.wallet[..8.min(sig.wallet.len())]);
        triggers.push(TriggerEvent {
            trigger_type: TriggerType::Signal,
            confidence: sig.confidence.clone(),
            market_title: sig.market_title.clone(),
            outcome: sig.outcome.clone(),
            direction: sig.signal_type.direction(),
            price: sig.price.parse().unwrap_or(0.0),
            wallet_count: 1,
            reason: format!("{} {} from {}", sig.confidence, sig.signal_type, tag),
            condition_id: sig.condition_id.clone(),
            asset: sig.asset.clone(),
            signal_id: sig.id.clone(),
        });
    }

    // Check odds alerts
    if config.odds_threshold > 0.0 {
        for alert in odds_alerts {
            if alert.change_pct.abs() < config.odds_threshold { continue; }
            if !matches_include(&alert.label) || matches_exclude(&alert.label) { continue; }

            let dir = if alert.change_pct > 0.0 {
                crate::smart::SignalDirection::Buy
            } else {
                crate::smart::SignalDirection::Sell
            };
            triggers.push(TriggerEvent {
                trigger_type: TriggerType::OddsAlert,
                confidence: crate::smart::SignalConfidence::Medium,
                market_title: alert.label.clone(),
                outcome: String::new(),
                direction: dir,
                price: alert.current_price,
                wallet_count: 0,
                reason: format!("Odds moved {:+.1}% ({:.4} -> {:.4})", alert.change_pct, alert.previous_price, alert.current_price),
                condition_id: alert.token_id.clone(),
                asset: alert.token_id.clone(),
                signal_id: alert.id.clone(),
            });
        }
    }

    triggers
}

#[allow(clippy::too_many_arguments)]
async fn cmd_monitor(
    client: &data::Client,
    interval_str: &str,
    min_confidence_str: &str,
    min_wallets: u32,
    market_include: Option<&str>,
    market_exclude: Option<&str>,
    odds_threshold: f64,
    paper_trade: bool,
    amount: f64,
    max_per_day: f64,
    notify: bool,
    save: bool,
    load: bool,
    _output: &OutputFormat,
) -> Result<()> {
    use crate::smart::{MonitorConfig, TradeStatus};

    // Build or load config
    let config = if load {
        store::load_monitor_config()?
            .ok_or_else(|| anyhow::anyhow!("No saved monitor config. Run with flags first, then --save."))?
    } else {
        let interval = parse_duration(interval_str)?;
        MonitorConfig {
            interval_secs: interval.as_secs(),
            min_confidence: parse_confidence(min_confidence_str)?,
            min_wallets,
            market_include: split_keywords(market_include),
            market_exclude: split_keywords(market_exclude),
            odds_threshold,
            paper_trade,
            amount,
            max_per_day,
            notify,
        }
    };

    if save {
        store::save_monitor_config(&config)?;
        println!("Monitor config saved to monitor.json");
    }

    let interval_dur = std::time::Duration::from_secs(config.interval_secs);

    // Print config summary
    println!("=== PMCC Monitor ===");
    println!("  Interval:       {}s", config.interval_secs);
    println!("  Min confidence: {}", config.min_confidence);
    println!("  Min wallets:    {}", config.min_wallets);
    if !config.market_include.is_empty() {
        println!("  Include:        {}", config.market_include.join(", "));
    }
    if !config.market_exclude.is_empty() {
        println!("  Exclude:        {}", config.market_exclude.join(", "));
    }
    if config.odds_threshold > 0.0 {
        println!("  Odds threshold: {:.1}%", config.odds_threshold);
    }
    println!("  Paper trade:    {} (${:.2}/trade, ${:.2}/day max)",
        if config.paper_trade { "ON" } else { "OFF" }, config.amount, config.max_per_day);
    println!("  Notifications:  {}", if config.notify { "ON" } else { "OFF" });
    println!("  Press Ctrl+C to stop.\n");

    let mut cycle = 0u64;
    let mut interval_timer = tokio::time::interval(interval_dur);
    interval_timer.tick().await; // first tick is immediate

    loop {
        tokio::select! {
            _ = interval_timer.tick() => {
                cycle += 1;
                let now = Utc::now().format("%H:%M:%S");
                eprint!("[{now}] Cycle #{cycle}: scanning... ");

                // Load wallets
                let wallets = store::load_wallets().unwrap_or_default();
                if wallets.is_empty() {
                    eprintln!("no wallets to scan");
                    continue;
                }

                // Scan wallets
                let mut all_signals = Vec::new();
                let mut scan_errors = 0u32;
                for wallet in &wallets {
                    match tracker::scan_wallet(client, &wallet.address).await {
                        Ok((changes, _snapshot)) => {
                            let sigs = signals::generate_signals(wallet, &changes);
                            all_signals.extend(sigs);
                        }
                        Err(_) => scan_errors += 1,
                    }
                }

                // Persist signals
                if !all_signals.is_empty() {
                    let _ = store::append_signals(&all_signals);
                }

                // Close follow positions on ClosePosition
                for sig in &all_signals {
                    if matches!(sig.signal_type, crate::smart::SignalType::ClosePosition) {
                        let exit_price: f64 = sig.price.parse().unwrap_or(0.0);
                        if exit_price > 0.0 {
                            let _ = store::close_follow_position(&sig.condition_id, &sig.outcome, exit_price);
                        }
                    }
                }

                // Aggregate
                let aggregated = signals::aggregate_signals(&all_signals);

                // Scan odds
                let odds_alerts = if config.odds_threshold > 0.0 {
                    let alerts = odds::scan_odds().await.unwrap_or_default();
                    if !alerts.is_empty() {
                        let _ = store::append_odds_alerts(&alerts);
                    }
                    alerts
                } else {
                    Vec::new()
                };

                // Evaluate triggers
                let triggers = evaluate_triggers(&all_signals, &aggregated, &odds_alerts, &config);

                // Paper trade
                let mut paper_count = 0u32;
                if config.paper_trade && !triggers.is_empty() {
                    let today_spent = store::today_spend().unwrap_or(0.0);
                    let mut spent = 0.0f64;

                    for trigger in &triggers {
                        if matches!(trigger.trigger_type, crate::smart::TriggerType::OddsAlert) {
                            continue; // don't paper-trade on pure odds alerts
                        }
                        if today_spent + spent + config.amount > config.max_per_day {
                            break;
                        }

                        let side_str = trigger.direction.to_string();
                        let pos_id = format!(
                            "pos_{}_{}",
                            Utc::now().format("%Y%m%d%H%M%S"),
                            &trigger.condition_id[..8.min(trigger.condition_id.len())]
                        );
                        let record = FollowRecord {
                            timestamp: Utc::now(),
                            signal_id: trigger.signal_id.clone(),
                            market_title: trigger.market_title.clone(),
                            condition_id: trigger.condition_id.clone(),
                            asset: trigger.asset.clone(),
                            outcome: trigger.outcome.clone(),
                            side: side_str,
                            amount_usdc: config.amount,
                            price: trigger.price,
                            dry_run: true,
                            order_id: None,
                            fill_price: None,
                            status: Some(TradeStatus::Open),
                            closed_at: None,
                            exit_price: None,
                            realized_pnl: None,
                            position_id: Some(pos_id),
                        };
                        let _ = store::append_follow_record(&record);
                        paper_count += 1;
                        spent += config.amount;
                    }
                }

                // Summary line
                let err_str = if scan_errors > 0 { format!(", {scan_errors} error(s)") } else { String::new() };
                let paper_str = if paper_count > 0 { format!(", {paper_count} paper trade(s)") } else { String::new() };
                eprintln!(
                    "{} signal(s), {} trigger(s){paper_str}{err_str}",
                    all_signals.len(), triggers.len()
                );

                // Notifications
                if config.notify && !triggers.is_empty() {
                    let summary = build_monitor_notification(&triggers, paper_count);

                    // macOS
                    let title = format!("PMCC: {} trigger(s)", triggers.len());
                    let short = triggers.iter().take(3).map(|t| t.reason.as_str()).collect::<Vec<_>>().join("; ");
                    let _ = std::process::Command::new("osascript")
                        .args(["-e", &format!(
                            "display notification \"{}\" with title \"{}\" sound name \"Glass\"",
                            short.replace('"', "\\\""), title
                        )])
                        .output();

                    // Telegram
                    if let Ok(Some(tg_config)) = store::load_telegram_config() {
                        let _ = send_telegram_message(&tg_config, &summary).await;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nMonitor stopped. {} cycles completed.", cycle);
                return Ok(());
            }
        }
    }
}

fn build_monitor_notification(triggers: &[crate::smart::TriggerEvent], paper_count: u32) -> String {
    let mut lines = vec![format!("*PMCC Monitor: {} trigger(s)*", triggers.len())];

    for (i, t) in triggers.iter().enumerate() {
        if i >= 5 { lines.push(format!("_...and {} more_", triggers.len() - 5)); break; }
        let dir = &t.direction;
        lines.push(format!(
            "[{}] {} {} — {} [{}] @ {:.4}",
            t.trigger_type, t.confidence, dir, t.market_title, t.outcome, t.price
        ));
        lines.push(format!("  _{}_", t.reason));
    }

    if paper_count > 0 {
        lines.push(format!("\nPaper trades: {paper_count}"));
    }

    lines.join("\n")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

// ── HTML Report ─────────────────────────────────────────────────

fn cmd_report() -> Result<()> {
    let wallets = store::load_wallets()?;
    let signals = store::load_signals(100)?;
    let follows = store::load_follow_records()?;
    let price_map = store::current_price_map()?;
    let snapshots = store::load_all_snapshots()?;

    let wallet_positions: std::collections::HashMap<String, usize> = snapshots
        .iter()
        .map(|s| (s.address.clone(), s.positions.len()))
        .collect();

    // Compute per-trade data

    let mut realized_pnl = 0.0f64;
    let mut unrealized_pnl = 0.0f64;
    let mut closed_count = 0u32;
    let mut closed_wins = 0u32;
    let mut best_pnl = f64::NEG_INFINITY;
    let mut worst_pnl = f64::INFINITY;
    let mut total_invested = 0.0f64;
    let mut equity_points: Vec<(i64, f64)> = Vec::new(); // (timestamp_ms, cumulative_pnl)
    let mut cumulative_pnl = 0.0f64;
    let mut market_stats: std::collections::HashMap<String, (f64, u32, u32)> = std::collections::HashMap::new(); // market -> (pnl, total, wins)

    let trades: Vec<ReportTradeData> = follows
        .iter()
        .map(|r| {
            let entry = r.effective_entry();
            let status_str = r.status.as_ref().map(|s| s.to_string()).unwrap_or_else(|| "OPEN".to_string());
            let (pnl, current);

            if !r.is_open() {
                let rpnl = r.realized_pnl.unwrap_or(0.0);
                current = r.exit_price.unwrap_or(entry);
                pnl = rpnl;
                realized_pnl += rpnl;
                closed_count += 1;
                if rpnl > 0.0 { closed_wins += 1; }
            } else {
                current = price_map.get(&r.condition_id).copied().unwrap_or(entry);
                let shares = if entry > 0.0 { r.amount_usdc / entry } else { 0.0 };
                pnl = shares * current - r.amount_usdc;
                unrealized_pnl += pnl;
            }

            total_invested += r.amount_usdc;
            cumulative_pnl += pnl;
            equity_points.push((r.timestamp.timestamp_millis(), cumulative_pnl));

            if pnl > best_pnl { best_pnl = pnl; }
            if pnl < worst_pnl { worst_pnl = pnl; }

            let stat = market_stats.entry(r.market_title.clone()).or_insert((0.0, 0, 0));
            stat.0 += pnl;
            stat.1 += 1;
            if pnl > 0.0 { stat.2 += 1; }

            let roi = if r.amount_usdc > 0.0 { pnl / r.amount_usdc * 100.0 } else { 0.0 };

            ReportTradeData {
                time: r.timestamp.format("%Y-%m-%d %H:%M").to_string(),
                timestamp_ms: r.timestamp.timestamp_millis(),
                mode: if r.dry_run { "DRY" } else { "LIVE" }.to_string(),
                status: status_str,
                market: r.market_title.clone(),
                outcome: r.outcome.clone(),
                side: r.side.clone(),
                invested: r.amount_usdc,
                entry,
                current,
                pnl,
                roi,
            }
        })
        .collect();

    let total_pnl = realized_pnl + unrealized_pnl;
    let open_count = trades.len() as u32 - closed_count;
    let win_rate = if closed_count > 0 { closed_wins as f64 / closed_count as f64 * 100.0 } else { 0.0 };
    if best_pnl == f64::NEG_INFINITY { best_pnl = 0.0; }
    if worst_pnl == f64::INFINITY { worst_pnl = 0.0; }

    // Build equity curve SVG
    let equity_svg = build_equity_curve_svg(&equity_points);

    // Build trade scatter SVG
    let scatter_data: Vec<(i64, f64, bool)> = trades.iter().map(|t| (t.timestamp_ms, t.pnl, t.pnl >= 0.0)).collect();
    let scatter_svg = build_trade_scatter_svg(&scatter_data);

    // Per-market breakdown
    let mut market_rows: Vec<(String, f64, u32, u32)> = market_stats.into_iter().map(|(m, (p, t, w))| (m, p, t, w)).collect();
    market_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let html = generate_report_html(
        &wallets, &wallet_positions, &signals, &trades,
        total_invested, total_pnl, realized_pnl, unrealized_pnl,
        open_count, closed_count, win_rate, best_pnl, worst_pnl,
        &equity_svg, &scatter_svg, &market_rows,
    );

    let report_path = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
        .join(".config").join("polymarket").join("smart").join("dashboard.html");
    std::fs::write(&report_path, &html)?;
    println!("Dashboard generated: {}", report_path.display());
    let _ = std::process::Command::new("open").arg(&report_path).output();
    Ok(())
}

// ── SVG Chart Helpers ────────────────────────────────────────────

fn build_equity_curve_svg(points: &[(i64, f64)]) -> String {
    if points.len() < 2 {
        return r##"<svg viewBox="0 0 700 200" xmlns="http://www.w3.org/2000/svg"><text x="350" y="100" fill="#475569" text-anchor="middle" font-size="14">Not enough data for equity curve</text></svg>"##.to_string();
    }

    let w = 700.0f64;
    let h = 200.0f64;
    let pad = 40.0f64;

    let min_t = points.first().unwrap().0 as f64;
    let max_t = points.last().unwrap().0 as f64;
    let t_range = (max_t - min_t).max(1.0);

    let min_v = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).min(0.0);
    let max_v = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max).max(0.0);
    let v_range = (max_v - min_v).max(0.01);

    let x = |t: f64| -> f64 { pad + (t - min_t) / t_range * (w - pad * 2.0) };
    let y = |v: f64| -> f64 { h - pad - (v - min_v) / v_range * (h - pad * 2.0) };

    let mut path = String::new();
    for (i, &(t, v)) in points.iter().enumerate() {
        let cmd = if i == 0 { "M" } else { "L" };
        path.push_str(&format!("{cmd}{:.1},{:.1} ", x(t as f64), y(v)));
    }

    // Fill area
    let last_x = x(points.last().unwrap().0 as f64);
    let first_x = x(points.first().unwrap().0 as f64);
    let zero_y = y(0.0);
    let fill_path = format!("{path}L{last_x:.1},{zero_y:.1} L{first_x:.1},{zero_y:.1} Z");

    let final_pnl = points.last().unwrap().1;
    let line_color = if final_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
    let fill_color = if final_pnl >= 0.0 { "#4ade8015" } else { "#f8717115" };

    format!(
        r##"<svg viewBox="0 0 {w} {h}" xmlns="http://www.w3.org/2000/svg">
<rect width="{w}" height="{h}" fill="#111827" rx="8"/>
<line x1="{pad}" y1="{zy:.1}" x2="{we:.1}" y2="{zy:.1}" stroke="#334155" stroke-width="1" stroke-dasharray="4,4"/>
<text x="{pad}" y="{zy:.1}" dy="-4" fill="#64748b" font-size="10">$0</text>
<path d="{fill_path}" fill="{fill_color}"/>
<path d="{path}" fill="none" stroke="{line_color}" stroke-width="2"/>
<circle cx="{lx:.1}" cy="{ly:.1}" r="4" fill="{line_color}"/>
<text x="{lx:.1}" y="{ly:.1}" dy="-8" fill="{line_color}" font-size="11" text-anchor="end" font-weight="600">${pnl:+.2}</text>
<text x="{pad}" y="16" fill="#94a3b8" font-size="11" font-weight="600">Cumulative P&amp;L</text>
</svg>"##,
        w = w, h = h, pad = pad,
        zy = zero_y,
        we = w - pad,
        lx = x(points.last().unwrap().0 as f64),
        ly = y(final_pnl),
        pnl = final_pnl,
    )
}

fn build_trade_scatter_svg(points: &[(i64, f64, bool)]) -> String {
    if points.is_empty() {
        return r##"<svg viewBox="0 0 700 150" xmlns="http://www.w3.org/2000/svg"><text x="350" y="75" fill="#475569" text-anchor="middle" font-size="14">No trades to display</text></svg>"##.to_string();
    }

    let w = 700.0f64;
    let h = 150.0f64;
    let pad = 40.0f64;

    let min_t = points.iter().map(|p| p.0).min().unwrap() as f64;
    let max_t = points.iter().map(|p| p.0).max().unwrap() as f64;
    let t_range = (max_t - min_t).max(1.0);

    let min_v = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).min(0.0);
    let max_v = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max).max(0.0);
    let v_range = (max_v - min_v).max(0.01);

    let x = |t: f64| -> f64 { pad + (t - min_t) / t_range * (w - pad * 2.0) };
    let y = |v: f64| -> f64 { h - pad - (v - min_v) / v_range * (h - pad * 2.0) };

    let zero_y = y(0.0);
    let mut circles = String::new();
    for &(t, pnl, win) in points {
        let color = if win { "#4ade80" } else { "#f87171" };
        let r = (pnl.abs() / v_range * 20.0).clamp(3.0, 12.0);
        circles.push_str(&format!(
            r#"<circle cx="{:.1}" cy="{:.1}" r="{r:.1}" fill="{color}" opacity="0.7"/>"#,
            x(t as f64), y(pnl)
        ));
    }

    format!(
        r##"<svg viewBox="0 0 {w} {h}" xmlns="http://www.w3.org/2000/svg">
<rect width="{w}" height="{h}" fill="#111827" rx="8"/>
<line x1="{pad}" y1="{zy:.1}" x2="{we:.1}" y2="{zy:.1}" stroke="#334155" stroke-width="1" stroke-dasharray="4,4"/>
{circles}
<text x="{pad}" y="16" fill="#94a3b8" font-size="11" font-weight="600">Trade P&amp;L Scatter</text>
<circle cx="{lx:.0}" cy="16" r="4" fill="#4ade80"/><text x="{lx2:.0}" y="20" fill="#64748b" font-size="9">Win</text>
<circle cx="{rx:.0}" cy="16" r="4" fill="#f87171"/><text x="{rx2:.0}" y="20" fill="#64748b" font-size="9">Loss</text>
</svg>"##,
        w = w, h = h, pad = pad,
        zy = zero_y, we = w - pad,
        lx = w - 100.0, lx2 = w - 92.0,
        rx = w - 60.0, rx2 = w - 52.0,
    )
}

// ── Report HTML ──────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn generate_report_html(
    wallets: &[crate::smart::WatchedWallet],
    wallet_positions: &std::collections::HashMap<String, usize>,
    signals: &[Signal],
    trades: &[ReportTradeData],
    total_invested: f64,
    total_pnl: f64,
    realized_pnl: f64,
    unrealized_pnl: f64,
    open_count: u32,
    closed_count: u32,
    win_rate: f64,
    best_pnl: f64,
    worst_pnl: f64,
    equity_svg: &str,
    scatter_svg: &str,
    market_rows: &[(String, f64, u32, u32)],
) -> String {
    let pnl_color = if total_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
    let realized_color = if realized_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
    let unrealized_color = if unrealized_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
    let total_roi = if total_invested > 0.0 { total_pnl / total_invested * 100.0 } else { 0.0 };

    let wallets_html = wallets_to_html(wallets, wallet_positions);
    let signals_html = signals_to_html(signals);
    let follows_html = trades_to_html(trades, total_invested, total_pnl, total_roi, pnl_color);
    let market_html = market_to_html(market_rows);

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>PMCC Smart Money Report</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#0f172a;color:#e2e8f0;line-height:1.6;padding:2rem;max-width:1200px;margin:0 auto}}
h1{{font-size:1.8rem;margin-bottom:.3rem;color:#f8fafc}}
h2{{font-size:1.2rem;margin:2rem 0 .8rem;color:#94a3b8;border-bottom:1px solid #1e293b;padding-bottom:.5rem}}
.meta{{color:#64748b;font-size:.85rem;margin-bottom:1.5rem}}
.cards{{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:.8rem;margin-bottom:1.5rem}}
.card{{background:#1e293b;border-radius:10px;padding:1rem;border:1px solid #334155}}
.card .label{{color:#94a3b8;font-size:.7rem;text-transform:uppercase;letter-spacing:.05em}}
.card .value{{font-size:1.5rem;font-weight:700;margin-top:.2rem}}
.card .sub{{color:#64748b;font-size:.75rem;margin-top:.1rem}}
.chart-row{{display:grid;grid-template-columns:1fr;gap:1rem;margin:1.5rem 0}}
.chart-row svg{{width:100%;height:auto}}
table{{width:100%;border-collapse:collapse;font-size:.82rem;margin-bottom:.5rem}}
th{{text-align:left;padding:.5rem .6rem;background:#1e293b;color:#94a3b8;font-weight:600;text-transform:uppercase;font-size:.7rem;letter-spacing:.05em;border-bottom:2px solid #334155}}
td{{padding:.4rem .6rem;border-bottom:1px solid #1e293b}}
tr:hover td{{background:#1e293b50}}
.mono{{font-family:'SF Mono',Consolas,monospace;font-size:.75rem}}
.text-green{{color:#4ade80}} .text-red{{color:#f87171}}
.badge{{display:inline-block;padding:1px 7px;border-radius:3px;font-size:.7rem;font-weight:600}}
.badge-high,.badge-live{{background:#166534;color:#4ade80}}
.badge-med{{background:#854d0e;color:#facc15}}
.badge-low,.badge-dry,.badge-open{{background:#1e293b;color:#94a3b8}}
.badge-closed{{background:#1e293b;color:#60a5fa}}
.score-bar{{position:relative;background:#1e293b;border-radius:3px;padding:1px 6px;font-size:.75rem;font-weight:600}}
.score-bar::before{{content:'';position:absolute;left:0;top:0;bottom:0;width:var(--w);background:var(--c);opacity:.2;border-radius:3px}}
.empty{{color:#475569;text-align:center;padding:1.5rem;font-size:.85rem}}
.grid-2{{display:grid;grid-template-columns:1fr 1fr;gap:1.5rem}}
@media(max-width:800px){{.grid-2{{grid-template-columns:1fr}}}}
</style>
</head>
<body>
<h1>PMCC Smart Money Report</h1>
<p class="meta">Generated: {now}</p>

<div class="cards">
<div class="card"><div class="label">Total PnL</div><div class="value" style="color:{pnl_color}">${total_pnl:+.2}</div><div class="sub">{total_roi:+.1}% ROI</div></div>
<div class="card"><div class="label">Realized</div><div class="value" style="color:{realized_color}">${realized_pnl:+.2}</div><div class="sub">{closed_count} closed</div></div>
<div class="card"><div class="label">Unrealized</div><div class="value" style="color:{unrealized_color}">${unrealized_pnl:+.2}</div><div class="sub">{open_count} open</div></div>
<div class="card"><div class="label">Win Rate</div><div class="value">{win_rate:.0}%</div><div class="sub">{closed_count} closed trades</div></div>
<div class="card"><div class="label">Invested</div><div class="value">${total_invested:.0}</div><div class="sub">{trade_count} trades</div></div>
<div class="card"><div class="label">Best Trade</div><div class="value text-green">${best_pnl:+.2}</div></div>
<div class="card"><div class="label">Worst Trade</div><div class="value text-red">${worst_pnl:+.2}</div></div>
</div>

<div class="chart-row">
{equity_svg}
</div>
<div class="chart-row">
{scatter_svg}
</div>

<div class="grid-2">
<div>
<h2>Per-Market Performance</h2>
{market_section}
</div>
<div>
<h2>Watched Wallets ({wallet_count})</h2>
{wallets_section}
</div>
</div>

<h2>Follow Trades ({trade_count})</h2>
{follows_section}

<h2>Recent Signals ({signal_count})</h2>
{signals_section}

<p class="meta" style="margin-top:3rem;text-align:center">PMCC Smart Money System &mdash; polymarket-cli</p>
</body>
</html>"##,
        now = Utc::now().format("%Y-%m-%d %H:%M UTC"),
        total_pnl = total_pnl,
        pnl_color = pnl_color,
        total_roi = total_roi,
        realized_pnl = realized_pnl,
        realized_color = realized_color,
        unrealized_pnl = unrealized_pnl,
        unrealized_color = unrealized_color,
        closed_count = closed_count,
        open_count = open_count,
        win_rate = win_rate,
        total_invested = total_invested,
        trade_count = trades.len(),
        best_pnl = best_pnl,
        worst_pnl = worst_pnl,
        equity_svg = equity_svg,
        scatter_svg = scatter_svg,
        wallet_count = wallets.len(),
        signal_count = signals.len(),
        wallets_section = wallets_html,
        signals_section = signals_html,
        follows_section = follows_html,
        market_section = market_html,
    )
}

struct ReportTradeData {
    time: String,
    #[allow(dead_code)]
    timestamp_ms: i64,
    mode: String,
    status: String,
    market: String,
    outcome: String,
    side: String,
    invested: f64,
    entry: f64,
    current: f64,
    pnl: f64,
    roi: f64,
}

fn wallets_to_html(wallets: &[crate::smart::WatchedWallet], positions: &std::collections::HashMap<String, usize>) -> String {
    if wallets.is_empty() {
        return r#"<p class="empty">No wallets being watched.</p>"#.to_string();
    }
    let rows: String = wallets.iter().map(|w| {
        let score = w.score.unwrap_or(0.0);
        let bar_w = score.clamp(0.0, 100.0);
        let sc = if score >= 90.0 { "#4ade80" } else if score >= 70.0 { "#facc15" } else { "#f87171" };
        format!(
            "<tr><td class='mono'>{}</td><td>{}</td><td><div class='score-bar' style='--w:{}%;--c:{}'>{:.1}</div></td><td>{}</td></tr>",
            html_escape(&w.address), html_escape(w.tag.as_deref().unwrap_or("—")),
            bar_w, sc, score,
            positions.get(&w.address).copied().unwrap_or(0)
        )
    }).collect();
    format!("<table><thead><tr><th>Address</th><th>Tag</th><th>Score</th><th>Positions</th></tr></thead><tbody>{rows}</tbody></table>")
}

fn signals_to_html(signals: &[Signal]) -> String {
    if signals.is_empty() {
        return r#"<p class="empty">No signals yet.</p>"#.to_string();
    }
    let rows: String = signals.iter().take(50).map(|s| {
        let conf = s.confidence.to_string();
        let cc = match conf.as_str() { "HIGH" => "badge-high", "MED" => "badge-med", _ => "badge-low" };
        let tc = if matches!(s.signal_type, crate::smart::SignalType::NewPosition | crate::smart::SignalType::IncreasePosition) { "text-green" } else { "text-red" };
        format!(
            "<tr><td>{}</td><td class='{tc}'>{}</td><td><span class='badge {cc}'>{conf}</span></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            s.timestamp.format("%m-%d %H:%M"), s.signal_type,
            html_escape(&s.market_title), html_escape(&s.outcome), s.price
        )
    }).collect();
    format!("<table><thead><tr><th>Time</th><th>Type</th><th>Conf</th><th>Market</th><th>Outcome</th><th>Price</th></tr></thead><tbody>{rows}</tbody></table>")
}

fn trades_to_html(trades: &[ReportTradeData], total_invested: f64, total_pnl: f64, total_roi: f64, pnl_color: &str) -> String {
    if trades.is_empty() {
        return r#"<p class="empty">No follow trades yet.</p>"#.to_string();
    }
    let rows: String = trades.iter().map(|t| {
        let pc = if t.pnl >= 0.0 { "text-green" } else { "text-red" };
        let sc = match t.status.as_str() { "CLOSED" => "badge-closed", "OPEN" => "badge-open", _ => "badge-low" };
        format!(
            "<tr><td>{}</td><td><span class='badge badge-{}'>{}</span></td><td><span class='badge {sc}'>{}</span></td><td>{}</td><td>{}</td><td>${:.2}</td><td>{:.2}</td><td>{:.2}</td><td class='{pc}'>{:+.2}</td><td class='{pc}'>{:+.1}%</td></tr>",
            t.time, t.mode.to_lowercase(), t.mode, t.status,
            t.side, html_escape(&t.market), t.invested, t.entry, t.current, t.pnl, t.roi
        )
    }).collect();
    format!(
        "<table><thead><tr><th>Time</th><th>Mode</th><th>Status</th><th>Side</th><th>Market</th><th>Invested</th><th>Entry</th><th>Now/Exit</th><th>PnL</th><th>ROI</th></tr></thead><tbody>{rows}</tbody></table>\
         <p style='margin-top:.5rem;color:#94a3b8;font-size:.8rem'>Total: ${total_invested:.2} | PnL: <span style='color:{pnl_color}'>{total_pnl:+.2}</span> ({total_roi:+.1}%)</p>"
    )
}

fn market_to_html(market_rows: &[(String, f64, u32, u32)]) -> String {
    if market_rows.is_empty() {
        return r#"<p class="empty">No market data yet.</p>"#.to_string();
    }
    let rows: String = market_rows.iter().map(|(market, pnl, total, wins)| {
        let pc = if *pnl >= 0.0 { "text-green" } else { "text-red" };
        let wr = if *total > 0 { *wins as f64 / *total as f64 * 100.0 } else { 0.0 };
        format!(
            "<tr><td>{}</td><td>{total}</td><td>{wr:.0}%</td><td class='{pc}'>{pnl:+.2}</td></tr>",
            html_escape(&crate::output::truncate(market, 30))
        )
    }).collect();
    format!("<table><thead><tr><th>Market</th><th>Trades</th><th>Win Rate</th><th>PnL</th></tr></thead><tbody>{rows}</tbody></table>")
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

// ── Odds monitoring ─────────────────────────────────────────────

async fn cmd_odds(command: OddsCommand, output: &OutputFormat) -> Result<()> {
    use crate::output::smart::{print_odds_alerts, print_odds_list};
    use polymarket_client_sdk::clob;
    use polymarket_client_sdk::clob::types::request::MidpointRequest;
    use polymarket_client_sdk::types::U256;

    match command {
        OddsCommand::Watch {
            token_id,
            label,
            threshold,
        } => {
            // Fetch current midpoint to set baseline
            let tid: U256 = token_id
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid token ID: {token_id}"))?;
            let client = clob::Client::default();
            let request = MidpointRequest::builder().token_id(tid).build();
            let result = client.midpoint(&request).await?;
            let mid: f64 = result.mid.to_f64().unwrap_or(0.0);
            if mid <= 0.0 {
                anyhow::bail!("Could not fetch midpoint for token {token_id} (got {mid})");
            }

            let label = label.unwrap_or_else(|| format!("token:{}", &token_id[..token_id.len().min(12)]));
            let watch = OddsWatch {
                token_id: token_id.clone(),
                label: label.clone(),
                threshold_pct: threshold,
                baseline_price: mid,
                last_price: mid,
                added_at: Utc::now(),
                last_scanned: None,
            };

            if store::add_odds_watch(watch)? {
                println!("Watching \"{label}\" (threshold: {threshold}%, baseline: {mid:.4})");
            } else {
                println!("Already watching token {token_id}");
            }
        }

        OddsCommand::Unwatch { token_id } => {
            if store::remove_odds_watch(&token_id)? {
                println!("Removed odds watch for {token_id}");
            } else {
                println!("Token {token_id} not in odds watch list");
            }
        }

        OddsCommand::List => {
            let watches = store::load_odds_watches()?;
            print_odds_list(&watches, output)?;
        }

        OddsCommand::Scan { notify } => {
            let alerts = odds::scan_odds().await?;
            store::append_odds_alerts(&alerts)?;

            if alerts.is_empty() {
                println!("No odds alerts. All watched markets within threshold.");
            } else {
                print_odds_alerts(&alerts, output)?;

                if notify {
                    send_odds_macos_notification(&alerts);

                    if let Ok(Some(tg_config)) = store::load_telegram_config() {
                        let text = build_odds_telegram_text(&alerts);
                        if let Err(e) = send_telegram_message(&tg_config, &text).await {
                            eprintln!("Telegram notification failed: {e}");
                        }
                    }
                }
            }
        }

        OddsCommand::Alerts { limit } => {
            let alerts = store::load_odds_alerts(limit)?;
            print_odds_alerts(&alerts, output)?;
        }
    }

    Ok(())
}

fn send_odds_macos_notification(alerts: &[super::super::smart::OddsAlert]) {
    let title = format!("Polymarket: {} odds alert(s)", alerts.len());
    let body = if let Some(a) = alerts.first() {
        let dir = if a.change_pct > 0.0 { "↑" } else { "↓" };
        format!(
            "{} {dir}{:.1}% ({:.2} → {:.2})",
            a.label, a.change_pct.abs(), a.previous_price, a.current_price
        )
    } else {
        return;
    };

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

fn build_odds_telegram_text(alerts: &[super::super::smart::OddsAlert]) -> String {
    let mut lines = vec![format!("*Polymarket: {} odds alert(s)*", alerts.len())];
    for alert in alerts.iter().take(10) {
        let dir = if alert.change_pct > 0.0 { "📈" } else { "📉" };
        lines.push(format!(
            "{dir} `{}` {:.1}% ({:.4} → {:.4})",
            alert.label,
            alert.change_pct,
            alert.previous_price,
            alert.current_price,
        ));
    }
    if alerts.len() > 10 {
        lines.push(format!("...and {} more", alerts.len() - 10));
    }
    lines.join("\n")
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
