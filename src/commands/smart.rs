use anyhow::Result;
use chrono::Utc;
use clap::{Args, Subcommand};
use polymarket_client_sdk::data;
use rust_decimal::prelude::ToPrimitive;

use super::data::{OrderBy, TimePeriod};
use super::parse_address;
use crate::crypto;
use crate::output::OutputFormat;
use crate::output::smart::{
    print_discover_results, print_profile, print_scan_result, print_signals, print_wallet_list,
};
use crate::smart::tracker::PositionChange;
use crate::smart::{
    AggregatedSignal, FollowRecord, OddsWatch, PriceSnapshot, Signal, SignalConfidence,
    SmartScore, TelegramConfig, WatchedWallet, odds, scorer, signals, store, tracker,
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

        /// Auto-renew: scan day+week+month, add new high-scorers, mark stale
        #[arg(long)]
        auto_renew: bool,
    },

    /// Show detailed PnL for a specific wallet
    WalletPnl {
        /// Wallet address (0x...)
        address: String,
    },

    /// Analyze a wallet's trading patterns and style
    Analyze {
        /// Wallet address (0x...)
        address: String,

        /// Max positions/trades to analyze
        #[arg(long, default_value = "50")]
        depth: i32,
    },

    /// Discover active markets by category or keyword
    DiscoverMarkets {
        /// Browse by tag: politics, crypto, economics, ai, etc.
        #[arg(long)]
        tag: Option<String>,

        /// Search by keyword
        #[arg(long)]
        search: Option<String>,

        /// Max results
        #[arg(long, default_value = "10")]
        limit: i32,
    },

    /// Find top holders (whales) on markets by category
    DiscoverWhales {
        /// Find whales on markets with this tag
        #[arg(long)]
        tag: Option<String>,

        /// Find whales on a specific market (condition_id)
        #[arg(long)]
        market: Option<String>,

        /// Minimum position size (USD) to include
        #[arg(long, default_value = "500")]
        min_position: f64,

        /// Markets to scan per tag
        #[arg(long, default_value = "10")]
        limit: i32,

        /// Auto-watch discovered whales
        #[arg(long)]
        auto_watch: bool,
    },

    /// All-in-one: discover markets + find whales + watch
    DiscoverAuto {
        /// Comma-separated tags to scan (e.g. "politics,crypto,economics")
        #[arg(long, default_value = "politics,crypto")]
        tags: String,

        /// Minimum position size (USD) to include
        #[arg(long, default_value = "500")]
        min_position: f64,

        /// Markets to scan per tag
        #[arg(long, default_value = "10")]
        limit: i32,

        /// Auto-watch discovered whales
        #[arg(long)]
        auto_watch: bool,
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

    /// 5-minute crypto Up/Down trading (Binance data + momentum signals)
    Crypto {
        #[command(subcommand)]
        command: CryptoCommand,
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

#[derive(Subcommand)]
pub enum CryptoCommand {
    /// Fetch and display live BTC/ETH exchange data
    Feed {
        /// Asset: btc or eth
        #[arg(default_value = "btc")]
        asset: String,
    },
    /// Compute and display current momentum signal
    Signal {
        /// Asset: btc or eth
        #[arg(default_value = "btc")]
        asset: String,
    },
    /// Find next upcoming 5-minute market on Polymarket
    Market {
        /// Asset: btc or eth
        #[arg(default_value = "btc")]
        asset: String,
    },
    /// Backtest momentum signal against historical price data
    Backtest {
        /// Asset: btc or eth
        #[arg(default_value = "btc")]
        asset: String,

        /// Number of hours of historical data
        #[arg(long, default_value = "24")]
        hours: u32,
    },
    /// Run live paper trading loop (fetch data -> signal -> trade)
    Monitor {
        /// Asset: btc, eth, or all
        #[arg(default_value = "btc")]
        asset: String,

        /// USDC amount per paper trade
        #[arg(long, default_value = "10")]
        amount: f64,

        /// Max trades per hour
        #[arg(long, default_value = "6")]
        max_per_hour: u32,

        /// Max USDC per day
        #[arg(long, default_value = "60")]
        max_per_day: f64,

        /// Minimum signal confidence (0.0-1.0)
        #[arg(long, default_value = "0.3")]
        min_confidence: f64,

        /// Send notifications (macOS + Telegram)
        #[arg(long)]
        notify: bool,
    },
    /// Show crypto paper trade status and PnL
    Status,
}

pub async fn execute(
    client: &data::Client,
    gamma_client: &polymarket_client_sdk::gamma::Client,
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
            auto_renew,
        } => {
            if auto_renew {
                cmd_auto_renew(client, limit, auto_watch.unwrap_or(85.0), &output).await
            } else {
                cmd_discover(client, period, order_by, limit, auto_watch, &output).await
            }
        }

        SmartCommand::WalletPnl { address } => cmd_wallet_pnl(client, &address, &output).await,
        SmartCommand::Analyze { address, depth } => cmd_analyze(client, &address, depth, &output).await,

        SmartCommand::DiscoverMarkets { tag, search, limit } => {
            cmd_discover_markets(gamma_client, tag.as_deref(), search.as_deref(), limit, &output).await
        }
        SmartCommand::DiscoverWhales { tag, market, min_position, limit, auto_watch } => {
            cmd_discover_whales(client, gamma_client, tag.as_deref(), market.as_deref(), min_position, limit, auto_watch, &output).await
        }
        SmartCommand::DiscoverAuto { tags, min_position, limit, auto_watch } => {
            cmd_discover_auto(client, gamma_client, &tags, min_position, limit, auto_watch, &output).await
        }

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

        SmartCommand::Crypto { command } => cmd_crypto(gamma_client, command, &output).await,

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
                    discovery_periods: None,
                    last_seen_at: None,
                    stale: None,
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

// ── Auto-Renew ──────────────────────────────────────────────────

async fn cmd_auto_renew(
    client: &data::Client,
    limit: i32,
    threshold: f64,
    output: &OutputFormat,
) -> Result<()> {
    use polymarket_client_sdk::data::types::request::TraderLeaderboardRequest;

    let periods = [
        ("day", polymarket_client_sdk::data::types::TimePeriod::Day),
        ("week", polymarket_client_sdk::data::types::TimePeriod::Week),
        ("month", polymarket_client_sdk::data::types::TimePeriod::Month),
    ];

    // Collect wallets across all periods
    let mut seen: std::collections::HashMap<String, (f64, Vec<String>)> = std::collections::HashMap::new();

    for (period_name, period_val) in &periods {
        let request = TraderLeaderboardRequest::builder()
            .time_period(*period_val)
            .order_by(polymarket_client_sdk::data::types::LeaderboardOrderBy::Pnl)
            .limit(limit)?
            .build();

        match client.leaderboard(&request).await {
            Ok(entries) => {
                for e in &entries {
                    let addr = e.proxy_wallet.to_string().to_lowercase();
                    let score = scorer::score_from_leaderboard(
                        &addr,
                        e.user_name.as_deref(),
                        e.pnl.to_f64().unwrap_or(0.0),
                        e.vol.to_f64().unwrap_or(0.0),
                        e.rank as u64,
                    ).score;
                    let entry = seen.entry(addr).or_insert((0.0, Vec::new()));
                    if score > entry.0 { entry.0 = score; }
                    if !entry.1.contains(&period_name.to_string()) {
                        entry.1.push(period_name.to_string());
                    }
                }
                eprintln!("  {period_name}: {} wallets", entries.len());
            }
            Err(e) => eprintln!("  {period_name}: error — {e}"),
        }
    }

    // Add/update wallets above threshold
    let mut added = 0u32;
    let mut updated = 0u32;
    let now = Utc::now();

    for (addr, (score, periods_found)) in &seen {
        if *score < threshold { continue; }

        let existing = store::load_wallets()?;
        let already = existing.iter().any(|w| w.address.to_lowercase() == *addr);

        if already {
            store::update_wallet(addr, |w| {
                w.score = Some(*score);
                w.discovery_periods = Some(periods_found.clone());
                w.last_seen_at = Some(now);
                w.stale = Some(false);
            })?;
            updated += 1;
        } else {
            let wallet = WatchedWallet {
                address: addr.clone(),
                tag: Some("leaderboard".into()),
                added_at: now,
                score: Some(*score),
                discovery_periods: Some(periods_found.clone()),
                last_seen_at: Some(now),
                stale: Some(false),
            };
            if store::add_wallet(wallet)? { added += 1; }
        }
    }

    // Mark stale: wallets not seen in any period
    let seen_addrs: std::collections::HashSet<String> = seen.keys().cloned().collect();
    let all_wallets = store::load_wallets()?;
    let mut stale_count = 0u32;
    for w in &all_wallets {
        if w.tag.as_deref() == Some("leaderboard") && !seen_addrs.contains(&w.address.to_lowercase()) {
            store::update_wallet(&w.address, |w| { w.stale = Some(true); })?;
            stale_count += 1;
        }
    }

    match output {
        OutputFormat::Table => {
            println!("Auto-renew complete:");
            println!("  Scanned:  {} unique wallets across 3 periods", seen.len());
            println!("  Added:    {added} new (score >= {threshold})");
            println!("  Updated:  {updated} existing");
            println!("  Stale:    {stale_count} marked stale");
        }
        OutputFormat::Json => {
            let data = serde_json::json!({
                "scanned": seen.len(), "added": added, "updated": updated, "stale": stale_count,
            });
            crate::output::print_json(&data)?;
        }
    }
    Ok(())
}

// ── Wallet PnL ──────────────────────────────────────────────────

async fn cmd_wallet_pnl(
    client: &data::Client,
    address: &str,
    output: &OutputFormat,
) -> Result<()> {
    use polymarket_client_sdk::data::types::request::{ClosedPositionsRequest, PositionsRequest};

    let addr = parse_address(address)?;

    // Fetch open positions
    let open_req = PositionsRequest::builder().user(addr).limit(100)?.build();
    let open_positions = client.positions(&open_req).await?;

    // Fetch closed positions
    let closed_req = ClosedPositionsRequest::builder().user(addr).limit(100)?.build();
    let closed_positions = client.closed_positions(&closed_req).await?;

    // Compute open PnL
    let mut open_pnl = 0.0f64;
    for p in &open_positions {
        open_pnl += p.cash_pnl.to_f64().unwrap_or(0.0);
    }

    // Compute realized PnL + win rate
    let mut realized_pnl = 0.0f64;
    let mut closed_wins = 0u32;
    for p in &closed_positions {
        let rpnl = p.realized_pnl.to_f64().unwrap_or(0.0);
        realized_pnl += rpnl;
        if rpnl > 0.0 { closed_wins += 1; }
    }

    let total_pnl = open_pnl + realized_pnl;
    let win_rate = if !closed_positions.is_empty() {
        closed_wins as f64 / closed_positions.len() as f64 * 100.0
    } else { 0.0 };

    // Check if watched
    let wallets = store::load_wallets()?;
    let watched = wallets.iter().find(|w| w.address.to_lowercase() == address.to_lowercase());
    let tag = watched.and_then(|w| w.tag.as_deref()).unwrap_or("—");
    let score = watched.and_then(|w| w.score).unwrap_or(0.0);

    match output {
        OutputFormat::Table => {
            let short_addr = if address.len() > 14 {
                format!("{}...{}", &address[..8], &address[address.len()-6..])
            } else { address.to_string() };

            println!("=== Wallet PnL: {} ({}) ===", short_addr, tag);
            if score > 0.0 { println!("  Score: {score:.1}"); }
            println!();

            // Open positions table
            if !open_positions.is_empty() {
                use tabled::{Table, Tabled, settings::Style};
                #[derive(Tabled)]
                struct ORow {
                    #[tabled(rename = "Market")]
                    market: String,
                    #[tabled(rename = "Outcome")]
                    outcome: String,
                    #[tabled(rename = "Size")]
                    size: String,
                    #[tabled(rename = "Entry")]
                    entry: String,
                    #[tabled(rename = "Now")]
                    now: String,
                    #[tabled(rename = "PnL")]
                    pnl: String,
                }
                let rows: Vec<ORow> = open_positions.iter().map(|p| {
                    let pnl = p.cash_pnl.to_f64().unwrap_or(0.0);
                    ORow {
                        market: crate::output::truncate(&p.title, 30),
                        outcome: p.outcome.clone(),
                        size: format!("{}", p.size),
                        entry: format!("{:.3}", p.avg_price),
                        now: format!("{:.3}", p.cur_price),
                        pnl: format!("{pnl:+.2}"),
                    }
                }).collect();
                println!("Open Positions ({}):", open_positions.len());
                println!("{}", Table::new(rows).with(Style::rounded()));
            }

            // Closed positions table
            if !closed_positions.is_empty() {
                use tabled::{Table, Tabled, settings::Style};
                #[derive(Tabled)]
                struct CRow {
                    #[tabled(rename = "Market")]
                    market: String,
                    #[tabled(rename = "Outcome")]
                    outcome: String,
                    #[tabled(rename = "Avg Price")]
                    avg_price: String,
                    #[tabled(rename = "Realized PnL")]
                    pnl: String,
                }
                let rows: Vec<CRow> = closed_positions.iter().take(20).map(|p| {
                    let pnl = p.realized_pnl.to_f64().unwrap_or(0.0);
                    CRow {
                        market: crate::output::truncate(&p.title, 30),
                        outcome: p.outcome.clone(),
                        avg_price: format!("{:.3}", p.avg_price),
                        pnl: format!("{pnl:+.2}"),
                    }
                }).collect();
                println!("\nClosed Positions (showing {}/{}):", rows.len(), closed_positions.len());
                println!("{}", Table::new(rows).with(Style::rounded()));
            }

            println!();
            let pc = |v: f64| if v >= 0.0 { "+" } else { "" };
            println!("  Open PnL:     {}{:.2} ({} positions)", pc(open_pnl), open_pnl, open_positions.len());
            println!("  Realized PnL: {}{:.2} ({} closed)", pc(realized_pnl), realized_pnl, closed_positions.len());
            println!("  Total PnL:    {}{:.2}", pc(total_pnl), total_pnl);
            println!("  Win Rate:     {win_rate:.0}% ({closed_wins}/{})", closed_positions.len());
        }
        OutputFormat::Json => {
            let data = serde_json::json!({
                "address": address,
                "open_pnl": open_pnl,
                "realized_pnl": realized_pnl,
                "total_pnl": total_pnl,
                "open_positions": open_positions.len(),
                "closed_positions": closed_positions.len(),
                "win_rate": win_rate,
            });
            crate::output::print_json(&data)?;
        }
    }

    // Store PnL snapshot
    let snapshot = crate::smart::WalletPnlSnapshot {
        timestamp: Utc::now(),
        open_pnl,
        realized_pnl,
        position_count: open_positions.len() as u32,
    };
    store::append_pnl_snapshot(address, &snapshot)?;

    Ok(())
}

// ── Analyze ─────────────────────────────────────────────────────

const CATEGORIES: &[(&str, &[&str])] = &[
    ("Politics", &["election", "president", "congress", "vote", "trump", "biden", "governor", "senate", "party"]),
    ("Crypto", &["bitcoin", "ethereum", "btc", "eth", "crypto", "defi", "solana", "token"]),
    ("AI/Tech", &["ai", "artificial", "openai", "google", "apple", "tech", "gpt", "model"]),
    ("Sports", &["nba", "nfl", "soccer", "championship", "world cup", "game", "match", "league"]),
    ("Economy", &["gdp", "inflation", "fed", "interest rate", "recession", "tariff", "unemployment"]),
    ("Geopolitics", &["war", "russia", "china", "ukraine", "nato", "sanction", "missile"]),
];

fn categorize_market(title: &str) -> &'static str {
    let lower = title.to_lowercase();
    for (cat, keywords) in CATEGORIES {
        if keywords.iter().any(|kw| lower.contains(kw)) {
            return cat;
        }
    }
    "Other"
}

async fn cmd_analyze(
    client: &data::Client,
    address: &str,
    depth: i32,
    output: &OutputFormat,
) -> Result<()> {
    use polymarket_client_sdk::data::types::request::{ClosedPositionsRequest, PositionsRequest};

    let addr = parse_address(address)?;

    // Fetch positions
    let open_req = PositionsRequest::builder().user(addr).limit(depth)?.build();
    let open_positions = client.positions(&open_req).await?;

    let closed_req = ClosedPositionsRequest::builder().user(addr).limit(depth)?.build();
    let closed_positions = client.closed_positions(&closed_req).await?;

    // Category distribution
    let mut cat_stats: std::collections::HashMap<&str, (u32, f64)> = std::collections::HashMap::new();
    let total_positions = open_positions.len() + closed_positions.len();

    for p in &open_positions {
        let cat = categorize_market(&p.title);
        let e = cat_stats.entry(cat).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += p.cash_pnl.to_f64().unwrap_or(0.0);
    }
    for p in &closed_positions {
        let cat = categorize_market(&p.title);
        let e = cat_stats.entry(cat).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += p.realized_pnl.to_f64().unwrap_or(0.0);
    }

    let mut categories: Vec<crate::smart::CategoryStat> = cat_stats.iter().map(|(name, (count, pnl))| {
        crate::smart::CategoryStat {
            name: name.to_string(),
            position_count: *count,
            total_pnl: *pnl,
            pct: if total_positions > 0 { *count as f64 / total_positions as f64 * 100.0 } else { 0.0 },
        }
    }).collect();
    categories.sort_by(|a, b| b.pct.partial_cmp(&a.pct).unwrap_or(std::cmp::Ordering::Equal));

    // Trading style metrics
    let mut total_size = 0.0f64;
    let mut total_entry_price = 0.0f64;
    let mut yes_count = 0u32;
    let mut position_count = 0u32;
    let mut top_3_size = 0.0f64;

    let mut sizes: Vec<f64> = Vec::new();
    for p in &open_positions {
        let size = p.size.to_f64().unwrap_or(0.0);
        let entry = p.avg_price.to_f64().unwrap_or(0.0);
        total_size += size;
        total_entry_price += entry;
        position_count += 1;
        if p.outcome.to_lowercase().contains("yes") { yes_count += 1; }
        sizes.push(size);
    }
    sizes.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    for s in sizes.iter().take(3) { top_3_size += s; }

    let avg_size = if position_count > 0 { total_size / position_count as f64 } else { 0.0 };
    let avg_entry = if position_count > 0 { total_entry_price / position_count as f64 } else { 0.0 };
    let yes_pct = if position_count > 0 { yes_count as f64 / position_count as f64 * 100.0 } else { 0.0 };
    let concentration = if total_size > 0.0 { top_3_size / total_size * 100.0 } else { 0.0 };

    // Contrarian score: how many positions are on outcomes priced < 0.40
    let contrarian_count = open_positions.iter().filter(|p| p.cur_price.to_f64().unwrap_or(0.5) < 0.40).count();
    let contrarian_score = if !open_positions.is_empty() {
        (contrarian_count as f64 / open_positions.len() as f64 * 10.0).round() as u32
    } else { 0 };

    // Recent signals for this wallet
    let all_signals = store::load_signals(200)?;
    let addr_lower = address.to_lowercase();
    let recent_signals: Vec<&Signal> = all_signals.iter()
        .filter(|s| s.wallet.to_lowercase() == addr_lower)
        .take(10)
        .collect();

    match output {
        OutputFormat::Table => {
            let short_addr = if address.len() > 14 {
                format!("{}...{}", &address[..8], &address[address.len()-6..])
            } else { address.to_string() };

            println!("=== Trade Analysis: {} ===\n", short_addr);

            // Category breakdown
            println!("Category Breakdown:");
            for c in &categories {
                let pc = if c.total_pnl >= 0.0 { "+" } else { "" };
                let bar = "#".repeat((c.pct / 5.0).round() as usize);
                println!("  {:<12} {:>4.0}%  ({} pos, {}{:.2} PnL)  {}", c.name, c.pct, c.position_count, pc, c.total_pnl, bar);
            }

            // Trading style
            println!("\nTrading Style:");
            println!("  Avg position size:  {avg_size:.1}");
            println!("  Avg entry price:    {avg_entry:.3} {}", if avg_entry < 0.40 { "(buys low-probability)" } else if avg_entry > 0.60 { "(buys high-probability)" } else { "(balanced)" });
            println!("  Direction bias:     {yes_pct:.0}% YES positions");
            println!("  Concentration:      top 3 = {concentration:.0}% of portfolio");
            println!("  Contrarian score:   {contrarian_score}/10 ({contrarian_count}/{} positions priced < 0.40)", open_positions.len());

            // Conviction label
            let conviction = if avg_size > 100.0 && position_count < 10 { "High (large bets, few positions)" }
                else if avg_size < 20.0 && position_count > 20 { "Low (small bets, many positions)" }
                else { "Medium" };
            println!("  Conviction:         {conviction}");

            // Recent moves
            if !recent_signals.is_empty() {
                println!("\nRecent Activity ({} signals):", recent_signals.len());
                for s in &recent_signals {
                    println!("  {} {:<8} {:<4} {} [{}] @ {}",
                        s.timestamp.format("%m-%d %H:%M"),
                        s.signal_type.to_string(),
                        s.confidence.to_string(),
                        crate::output::truncate(&s.market_title, 30),
                        s.outcome,
                        s.price,
                    );
                }
            }
        }
        OutputFormat::Json => {
            let data = serde_json::json!({
                "address": address,
                "categories": categories.iter().map(|c| serde_json::json!({
                    "name": c.name, "pct": c.pct, "positions": c.position_count, "pnl": c.total_pnl,
                })).collect::<Vec<_>>(),
                "style": {
                    "avg_size": avg_size, "avg_entry_price": avg_entry,
                    "yes_pct": yes_pct, "concentration_top3": concentration,
                    "contrarian_score": contrarian_score,
                },
                "open_positions": open_positions.len(),
                "closed_positions": closed_positions.len(),
                "recent_signals": recent_signals.len(),
            });
            crate::output::print_json(&data)?;
        }
    }
    Ok(())
}

// ── Market-First Discovery ───────────────────────────────────────

async fn cmd_discover_markets(
    gamma_client: &polymarket_client_sdk::gamma::Client,
    tag: Option<&str>,
    search: Option<&str>,
    limit: i32,
    output: &OutputFormat,
) -> Result<()> {
    use polymarket_client_sdk::gamma::types::request::{EventsRequest, SearchRequest};

    if let Some(query) = search {
        // Keyword search
        let request = SearchRequest::builder()
            .q(query.to_string())
            .limit_per_type(limit)
            .build();
        let results = gamma_client.search(&request).await?;

        let events = results.events.unwrap_or_default();
        let mut markets: Vec<(&str, Option<f64>, Option<f64>, String)> = Vec::new();
        for event in &events {
            if let Some(mkts) = &event.markets {
                for m in mkts {
                    let question = m.question.as_deref().unwrap_or("?");
                    let vol = m.volume_num.and_then(|v| v.to_f64());
                    let liq = m.liquidity_num.and_then(|v| v.to_f64());
                    let prices = m.outcome_prices.as_ref()
                        .map(|p| p.iter().map(|v| format!("{v:.2}")).collect::<Vec<_>>().join("/"))
                        .unwrap_or_default();
                    markets.push((question, vol, liq, prices));
                }
            }
        }
        markets.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        match output {
            OutputFormat::Table => {
                println!("--- Markets matching \"{}\" ({}) ---", query, markets.len());
                for (q, vol, liq, prices) in &markets {
                    let vol_str = vol.map(|v| crate::output::format_decimal(rust_decimal::Decimal::from_f64_retain(v).unwrap_or_default())).unwrap_or_else(|| "—".into());
                    let liq_str = liq.map(|v| crate::output::format_decimal(rust_decimal::Decimal::from_f64_retain(v).unwrap_or_default())).unwrap_or_else(|| "—".into());
                    println!("  ${:<8} liq ${:<8} {}  {}", vol_str, liq_str, prices, crate::output::truncate(q, 50));
                }
            }
            OutputFormat::Json => {
                let data: Vec<_> = markets.iter().map(|(q, vol, liq, prices)| {
                    serde_json::json!({"question": q, "volume": vol, "liquidity": liq, "prices": prices})
                }).collect();
                crate::output::print_json(&data)?;
            }
        }
        return Ok(());
    }

    // Tag-based browse
    let tag_slug = tag.unwrap_or("politics");
    let request = EventsRequest::builder()
        .limit(limit)
        .maybe_closed(Some(false))
        .maybe_tag_slug(Some(tag_slug.to_string()))
        .order(vec!["volume".to_string()])
        .build();

    let events = gamma_client.events(&request).await?;

    match output {
        OutputFormat::Table => {
            println!("--- Active Markets [{}] ({} events) ---\n", tag_slug, events.len());
            for e in &events {
                let title = e.title.as_deref().unwrap_or("?");
                let vol = e.volume.and_then(|v| v.to_f64()).unwrap_or(0.0);
                let vol_str = crate::output::format_decimal(rust_decimal::Decimal::from_f64_retain(vol).unwrap_or_default());
                println!("  ${:<8}  {}", vol_str, crate::output::truncate(title, 55));

                if let Some(mkts) = &e.markets {
                    for m in mkts.iter().take(3) {
                        let q = m.question.as_deref().unwrap_or("?");
                        let prices = m.outcome_prices.as_ref()
                            .map(|p| p.iter().map(|v| format!("{v:.2}")).collect::<Vec<_>>().join("/"))
                            .unwrap_or_default();
                        let cid = m.condition_id.map(|c| format!("{c}")).unwrap_or_default();
                        println!("    {} [{}]  cid: {}...", crate::output::truncate(q, 40), prices, &cid[..18.min(cid.len())]);
                    }
                }
                println!();
            }
        }
        OutputFormat::Json => {
            crate::output::print_json(&events)?;
        }
    }
    Ok(())
}

async fn cmd_discover_whales(
    client: &data::Client,
    gamma_client: &polymarket_client_sdk::gamma::Client,
    tag: Option<&str>,
    market_cid: Option<&str>,
    min_position: f64,
    limit: i32,
    auto_watch: bool,
    output: &OutputFormat,
) -> Result<()> {
    use polymarket_client_sdk::data::types::request::HoldersRequest;
    use polymarket_client_sdk::gamma::types::request::EventsRequest;

    let mut condition_ids: Vec<alloy::primitives::B256> = Vec::new();
    let mut market_names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let tag_label;

    if let Some(cid_str) = market_cid {
        // Single market
        let cid = super::parse_condition_id(cid_str)?;
        condition_ids.push(cid);
        tag_label = "market".to_string();
    } else {
        // Tag-based: get top markets
        let tag_slug = tag.unwrap_or("politics");
        tag_label = tag_slug.to_string();

        let request = EventsRequest::builder()
            .limit(limit)
            .maybe_closed(Some(false))
            .maybe_tag_slug(Some(tag_slug.to_string()))
            .order(vec!["volume".to_string()])
            .build();

        let events = gamma_client.events(&request).await?;
        for e in &events {
            if let Some(mkts) = &e.markets {
                for m in mkts {
                    if let Some(cid) = m.condition_id {
                        let q = m.question.as_deref().unwrap_or("?").to_string();
                        market_names.insert(format!("{cid}"), q);
                        condition_ids.push(cid);
                    }
                }
            }
        }
        eprintln!("Scanning {} markets in [{}]...", condition_ids.len(), tag_slug);
    }

    if condition_ids.is_empty() {
        println!("No markets found.");
        return Ok(());
    }

    // Query holders for each market (batch in chunks to avoid API limits)
    let mut whale_map: std::collections::HashMap<String, (f64, u32, Option<String>)> = std::collections::HashMap::new();

    for chunk in condition_ids.chunks(5) {
        let request = HoldersRequest::builder()
            .markets(chunk.to_vec())
            .limit(20)?
            .build();

        match client.holders(&request).await {
            Ok(meta_holders) => {
                for mh in &meta_holders {
                    for h in &mh.holders {
                        let addr = h.proxy_wallet.to_string().to_lowercase();
                        let amount = h.amount.to_f64().unwrap_or(0.0);
                        let entry = whale_map.entry(addr).or_insert((0.0, 0, h.name.clone()));
                        entry.0 += amount;
                        entry.1 += 1;
                    }
                }
            }
            Err(e) => eprintln!("  holders error: {e}"),
        }
    }

    // Filter and sort by total position
    let mut whales: Vec<(String, f64, u32, Option<String>)> = whale_map
        .into_iter()
        .filter(|(_, (amount, _, _))| *amount >= min_position)
        .map(|(addr, (amount, count, name))| (addr, amount, count, name))
        .collect();
    whales.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    match output {
        OutputFormat::Table => {
            println!("--- Top Holders [{}] ({} whales, min ${:.0}) ---\n", tag_label, whales.len(), min_position);
            for (addr, amount, count, name) in whales.iter().take(20) {
                let short = if addr.len() >= 12 {
                    format!("{}...{}", &addr[..8], &addr[addr.len()-4..])
                } else {
                    addr.clone()
                };
                let name_str = name.as_deref().unwrap_or("");
                println!("  ${:<10.0} across {:<2} markets  {}  {}", amount, count, short, name_str);
            }
        }
        OutputFormat::Json => {
            let data: Vec<_> = whales.iter().map(|(addr, amount, count, name)| {
                serde_json::json!({"address": addr, "total_position": amount, "markets": count, "name": name})
            }).collect();
            crate::output::print_json(&data)?;
        }
    }

    // Auto-watch
    if auto_watch {
        let mut added = 0u32;
        let now = Utc::now();
        for (addr, _amount, _count, name) in &whales {
            let wallet = WatchedWallet {
                address: addr.clone(),
                tag: Some(format!("{tag_label}-holder")),
                added_at: now,
                score: None,
                discovery_periods: None,
                last_seen_at: Some(now),
                stale: Some(false),
            };
            if store::add_wallet(wallet)? { added += 1; }
        }
        println!("\nAuto-watched {added} new wallet(s) tagged \"{tag_label}-holder\"");
    }

    Ok(())
}

async fn cmd_discover_auto(
    client: &data::Client,
    gamma_client: &polymarket_client_sdk::gamma::Client,
    tags: &str,
    min_position: f64,
    limit: i32,
    auto_watch: bool,
    output: &OutputFormat,
) -> Result<()> {
    let tag_list: Vec<&str> = tags.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()).collect();

    println!("=== Discover Auto: {} tag(s) ===\n", tag_list.len());

    for tag in &tag_list {
        println!("--- [{tag}] ---");
        cmd_discover_whales(client, gamma_client, Some(tag), None, min_position, limit, auto_watch, output).await?;
        println!();
    }

    let total = store::load_wallets()?.len();
    println!("Total watched wallets: {total}");

    Ok(())
}

fn cmd_watch(address: &str, tag: Option<String>, output: &OutputFormat) -> Result<()> {
    let wallet = WatchedWallet {
        address: address.to_string(),
        tag,
        added_at: Utc::now(),
        score: None,
        discovery_periods: None,
        last_seen_at: None,
        stale: None,
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
                discovery_periods: None,
                last_seen_at: None,
                stale: None,
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
                match store::close_follow_position(&sig.condition_id, &sig.outcome, exit_price, "scan: whale ClosePosition") {
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

/// Calculate PnL accounting for BUY vs SELL direction.
fn calc_open_pnl(side: &str, amount_usdc: f64, entry_price: f64, current_price: f64) -> f64 {
    if entry_price <= 0.0 { return 0.0; }
    let shares = amount_usdc / entry_price;
    if side == "SELL" {
        // SELL profits when price drops
        amount_usdc + (entry_price - current_price) * shares - amount_usdc
    } else {
        // BUY profits when price rises
        shares * current_price - amount_usdc
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
        entry_reason: Some(format!("manual follow: {} {}", signal.confidence, signal.signal_type)),
        exit_reason: None,
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
            entry_reason: Some(format!("auto-follow: {} {}", sig.confidence, sig.signal_type)),
            exit_reason: None,
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
            // Open trade: use snapshot price, account for BUY/SELL direction
            current_price = price_map.get(&(r.condition_id.clone(), r.outcome.clone())).copied().unwrap_or(entry_price);
            pnl = calc_open_pnl(&r.side, r.amount_usdc, entry_price, current_price);
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
            .get(&(sig.condition_id.clone(), sig.outcome.clone()))
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
    let price_history = store::load_price_history(Utc::now() - chrono::Duration::hours(24)).unwrap_or_default();

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

    // -- Follows: split paper vs live --
    let mut paper_invested = 0.0f64;
    let mut paper_pnl = 0.0f64;
    let mut paper_count = 0u32;
    let mut paper_wins = 0u32;
    let mut live_invested = 0.0f64;
    let mut live_pnl = 0.0f64;

    let build_follow_row = |r: &FollowRecord, price_map: &std::collections::HashMap<(String, String), f64>| -> (String, f64) {
        let entry = r.effective_entry();
        let (pnl, current);
        if !r.is_open() {
            pnl = r.realized_pnl.unwrap_or(0.0);
            current = r.exit_price.unwrap_or(entry);
        } else {
            current = price_map.get(&(r.condition_id.clone(), r.outcome.clone())).copied().unwrap_or(entry);
            pnl = calc_open_pnl(&r.side, r.amount_usdc, entry, current);
        }
        let roi = if r.amount_usdc > 0.0 { pnl / r.amount_usdc * 100.0 } else { 0.0 };
        let pnl_cls = if pnl >= 0.0 { "text-green" } else { "text-red" };
        let status = r.status.as_ref().map(|s| s.to_string()).unwrap_or_else(|| "OPEN".to_string());
        let status_cls = match status.as_str() { "CLOSED" => "color:#60a5fa", _ => "color:#94a3b8" };
        let row = format!(
            "<tr><td>{}</td><td style='{}'>{}</td><td>{}</td><td>{}</td><td>${:.2}</td><td>{:.3}</td><td>{:.3}</td><td class='{}'>{:+.2}</td><td class='{}'>{:+.1}%</td></tr>",
            r.timestamp.format("%m-%d %H:%M"),
            status_cls, status, html_escape(&r.side),
            html_escape(&r.market_title), r.amount_usdc, entry, current,
            pnl_cls, pnl, pnl_cls, roi
        );
        (row, pnl)
    };

    let mut live_rows = String::new();
    let mut open_rows = String::new();
    let mut closed_rows = String::new();
    let mut history_rows_vec: Vec<(chrono::DateTime<Utc>, String)> = Vec::new();
    let mut equity_points: Vec<(i64, f64)> = Vec::new();
    let mut paper_open_invested = 0.0f64;
    let mut paper_open_pnl = 0.0f64;
    let mut paper_closed_count = 0u32;
    let mut paper_closed_wins = 0u32;
    let mut paper_closed_pnl_values: Vec<f64> = Vec::new();
    let mut paper_closed_hold_hours: Vec<f64> = Vec::new();
    let utc_now = Utc::now();

    for r in &follows {
        if !r.dry_run {
            let (row, pnl) = build_follow_row(r, &price_map);
            live_rows.push_str(&row);
            live_invested += r.amount_usdc;
            live_pnl += pnl;
            continue;
        }

        // Skip crypto trades — rendered in separate section
        if r.entry_reason.as_deref().map(|e| e.starts_with("crypto:")).unwrap_or(false) {
            continue;
        }

        let entry = r.effective_entry();
        paper_invested += r.amount_usdc;
        paper_count += 1;

        // Period classification for trade history filter
        let age_hours = (utc_now - r.timestamp).num_hours();
        let mut periods = Vec::new();
        if age_hours < 24 { periods.push("today"); }
        if age_hours < 168 { periods.push("week"); }
        if age_hours < 720 { periods.push("month"); }
        let p_attr = periods.join(" ");

        if r.is_open() {
            let current = price_map.get(&(r.condition_id.clone(), r.outcome.clone())).copied().unwrap_or(entry);
            let pnl = calc_open_pnl(&r.side, r.amount_usdc, entry, current);
            let roi = if r.amount_usdc > 0.0 { pnl / r.amount_usdc * 100.0 } else { 0.0 };
            let pnl_cls = if pnl >= 0.0 { "text-green" } else { "text-red" };
            paper_pnl += pnl;
            if pnl > 0.0 { paper_wins += 1; }
            paper_open_invested += r.amount_usdc;
            paper_open_pnl += pnl;

            // Tab 1: Open Positions — build sparkline + 24h change
            let price_key = format!("{}:{}", r.condition_id, r.outcome);
            let hist_prices: Vec<f64> = price_history.iter()
                .filter_map(|s| s.prices.get(&price_key).copied())
                .collect();
            let sparkline = build_mini_sparkline(&hist_prices, current);
            let change_24h = if let Some(&first) = hist_prices.first() {
                if first > 0.0 { ((current - first) / first) * 100.0 } else { 0.0 }
            } else { 0.0 };
            let ch_cls = if change_24h >= 0.0 { "text-green" } else { "text-red" };
            let ch_str = if hist_prices.is_empty() { "—".to_string() } else { format!("{:+.1}%", change_24h) };

            let entry_reason = r.entry_reason.as_deref().unwrap_or("—");
            open_rows.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td><td>{:.3}</td><td>${:.2}</td><td class='{}'>{:+.2}</td><td class='{}'>{:+.1}%</td><td class='{}'>{}</td><td>{}</td><td>{}</td></tr>",
                r.timestamp.format("%m-%d %H:%M"),
                html_escape(&r.market_title), html_escape(&r.outcome), html_escape(&r.side),
                entry, current, r.amount_usdc, pnl_cls, pnl, pnl_cls, roi,
                ch_cls, ch_str, sparkline, html_escape(entry_reason)
            ));

            // Tab 2: Trade History
            history_rows_vec.push((r.timestamp, format!(
                "<tr data-p='{}'><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td><td>${:.2}</td><td style='color:#94a3b8'>OPEN</td></tr>",
                p_attr, r.timestamp.format("%m-%d %H:%M"), html_escape(&r.side),
                html_escape(&r.market_title), html_escape(&r.outcome), entry, r.amount_usdc
            )));
        } else {
            let pnl = r.realized_pnl.unwrap_or(0.0);
            let exit = r.exit_price.unwrap_or(entry);
            let roi = if r.amount_usdc > 0.0 { pnl / r.amount_usdc * 100.0 } else { 0.0 };
            let pnl_cls = if pnl >= 0.0 { "text-green" } else { "text-red" };
            let row_cls = if pnl >= 0.0 { "row-win" } else { "row-loss" };
            paper_pnl += pnl;
            if pnl > 0.0 { paper_wins += 1; }
            paper_closed_count += 1;
            if pnl > 0.0 { paper_closed_wins += 1; }
            paper_closed_pnl_values.push(pnl);

            let close_time = r.closed_at.unwrap_or(utc_now);
            let hold_h = (close_time - r.timestamp).num_hours() as f64;
            paper_closed_hold_hours.push(hold_h);
            let hold_str = if hold_h < 1.0 {
                format!("{}m", (close_time - r.timestamp).num_minutes())
            } else if hold_h < 24.0 {
                format!("{:.0}h", hold_h)
            } else {
                format!("{:.0}d {:.0}h", (hold_h / 24.0).floor(), hold_h % 24.0)
            };

            // Tab 3: Position History
            let exit_reason = r.exit_reason.as_deref().unwrap_or("—");
            closed_rows.push_str(&format!(
                "<tr class='{}'><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td><td>{:.3}</td><td class='{}'>{:+.2}</td><td class='{}'>{:+.1}%</td><td>{}</td><td>{}</td></tr>",
                row_cls, r.timestamp.format("%m-%d %H:%M"),
                r.closed_at.map_or("—".into(), |t| t.format("%m-%d %H:%M").to_string()),
                html_escape(&r.market_title), html_escape(&r.side),
                entry, exit, pnl_cls, pnl, pnl_cls, roi, hold_str,
                html_escape(exit_reason)
            ));

            // Tab 2: Trade History
            let status_str = r.status.as_ref().map(|s| s.to_string()).unwrap_or_else(|| "CLOSED".to_string());
            history_rows_vec.push((r.timestamp, format!(
                "<tr data-p='{}'><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td><td>${:.2}</td><td style='color:#60a5fa'>{}</td></tr>",
                p_attr, r.timestamp.format("%m-%d %H:%M"), html_escape(&r.side),
                html_escape(&r.market_title), html_escape(&r.outcome), entry, r.amount_usdc, status_str
            )));

            // Equity curve point
            equity_points.push((close_time.timestamp_millis(), pnl));
        }
    }

    // Sort trade history newest first
    history_rows_vec.sort_by(|a, b| b.0.cmp(&a.0));
    let history_rows: String = history_rows_vec.into_iter().map(|(_, r)| r).collect();

    // Build equity curve (cumulative PnL)
    equity_points.sort_by_key(|p| p.0);
    let mut cumulative = 0.0f64;
    let eq_cumulative: Vec<(i64, f64)> = equity_points.iter().map(|&(t, pnl)| {
        cumulative += pnl;
        (t, cumulative)
    }).collect();

    // Build crypto section (separate from smart money paper trades)
    let (crypto_section, crypto_stats) = build_crypto_paper_section(&follows, &price_map, &price_history);

    let total_invested = paper_invested + live_invested + crypto_stats.2;
    let total_pnl = paper_pnl + live_pnl + crypto_stats.1;
    let paper_win_rate = if paper_count > 0 { paper_wins as f64 / paper_count as f64 * 100.0 } else { 0.0 };
    let closed_win_rate = if paper_closed_count > 0 { paper_closed_wins as f64 / paper_closed_count as f64 * 100.0 } else { 0.0 };
    let avg_pnl = if !paper_closed_pnl_values.is_empty() { paper_closed_pnl_values.iter().sum::<f64>() / paper_closed_pnl_values.len() as f64 } else { 0.0 };
    let best_trade = paper_closed_pnl_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let worst_trade = paper_closed_pnl_values.iter().cloned().fold(f64::INFINITY, f64::min);
    let avg_hold = if !paper_closed_hold_hours.is_empty() { paper_closed_hold_hours.iter().sum::<f64>() / paper_closed_hold_hours.len() as f64 } else { 0.0 };
    let avg_hold_str = if avg_hold < 1.0 { "&lt;1h".to_string() } else if avg_hold < 24.0 { format!("{:.0}h", avg_hold) } else { format!("{:.0}d", avg_hold / 24.0) };

    let equity_svg = build_equity_curve_svg(&eq_cumulative);
    let paper_open_count = follows.iter().filter(|r| r.dry_run && r.is_open()
        && !r.entry_reason.as_deref().map(|e| e.starts_with("crypto:")).unwrap_or(false)).count();

    // Build paper section with exchange-style tabs
    let paper_section = if paper_count == 0 {
        "<h2>Smart Money <span class='count'>0</span></h2><div class='section'><p class='empty'>No paper trades yet. Run monitor with --paper-trade.</p></div>".to_string()
    } else {
        let tab_css = r#"<style>
.tab-radio,.period-radio{display:none}
.tab-nav{display:flex;gap:0;border-bottom:2px solid #1e293b;margin-bottom:1rem}
.tab-nav label{padding:.5rem 1rem;cursor:pointer;color:#64748b;font-size:.8rem;font-weight:600;text-transform:uppercase;letter-spacing:.05em;border-bottom:2px solid transparent;margin-bottom:-2px;transition:all .2s}
.tab-nav label:hover{color:#94a3b8}
.tab-panel{display:none}
#tab-positions:checked~.tab-nav label[for='tab-positions'],
#tab-history:checked~.tab-nav label[for='tab-history'],
#tab-closed:checked~.tab-nav label[for='tab-closed'],
#tab-perf:checked~.tab-nav label[for='tab-perf']{color:#e2e8f0;border-bottom-color:#4ade80}
#tab-positions:checked~.tab-panels #panel-positions,
#tab-history:checked~.tab-panels #panel-history,
#tab-closed:checked~.tab-panels #panel-closed,
#tab-perf:checked~.tab-panels #panel-perf{display:block}
.period-nav{display:flex;gap:.5rem;margin-bottom:.8rem}
.period-nav label{padding:2px 10px;border-radius:3px;cursor:pointer;color:#64748b;font-size:.7rem;background:#1e293b;transition:all .2s}
.period-nav label:hover{color:#94a3b8}
#period-all:checked~.period-nav label[for='period-all'],
#period-today:checked~.period-nav label[for='period-today'],
#period-week:checked~.period-nav label[for='period-week'],
#period-month:checked~.period-nav label[for='period-month']{color:#e2e8f0;background:#334155}
#period-today:checked~.period-wrap tbody tr:not([data-p~='today']),
#period-week:checked~.period-wrap tbody tr:not([data-p~='week']),
#period-month:checked~.period-wrap tbody tr:not([data-p~='month']){display:none}
.row-win{background:rgba(74,222,128,0.04)}
.row-loss{background:rgba(248,113,113,0.04)}
.perf-cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:.6rem}
.perf-card{background:#111827;border-radius:8px;padding:.8rem;border:1px solid #1e293b;text-align:center}
.perf-label{color:#64748b;font-size:.65rem;text-transform:uppercase;letter-spacing:.05em}
.perf-val{font-size:1.3rem;font-weight:700;margin-top:.2rem}
</style>"#;

        // Tab 1: Open Positions
        let tab1 = if open_rows.is_empty() {
            "<p class='empty'>No open positions.</p>".to_string()
        } else {
            let oc = if paper_open_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
            format!(
                "<table><thead><tr><th>Time</th><th>Market</th><th>Outcome</th><th>Side</th><th>Entry</th><th>Current</th><th>Size</th><th>PnL</th><th>ROI</th><th>24h</th><th>Trend</th><th>Entry Reason</th></tr></thead><tbody>{}</tbody></table>\
                 <p style='margin-top:.5rem;color:#94a3b8;font-size:.8rem'>{} positions | ${:.2} invested | Unrealized: <span style='color:{}'>{:+.2}</span></p>",
                open_rows, paper_open_count, paper_open_invested, oc, paper_open_pnl
            )
        };

        // Tab 2: Trade History with period filter
        let tab2 = if history_rows.is_empty() {
            "<p class='empty'>No trades yet.</p>".to_string()
        } else {
            format!(
                "<input type='radio' name='period' id='period-all' checked class='period-radio'>\
                 <input type='radio' name='period' id='period-today' class='period-radio'>\
                 <input type='radio' name='period' id='period-week' class='period-radio'>\
                 <input type='radio' name='period' id='period-month' class='period-radio'>\
                 <div class='period-nav'>\
                   <label for='period-all'>All</label>\
                   <label for='period-today'>Today</label>\
                   <label for='period-week'>Week</label>\
                   <label for='period-month'>Month</label>\
                 </div>\
                 <div class='period-wrap'>\
                 <table><thead><tr><th>Time</th><th>Side</th><th>Market</th><th>Outcome</th><th>Entry</th><th>Amount</th><th>Status</th></tr></thead><tbody>{}</tbody></table>\
                 </div>",
                history_rows
            )
        };

        // Tab 3: Position History (closed only)
        let tab3 = if closed_rows.is_empty() {
            "<p class='empty'>No closed positions yet.</p>".to_string()
        } else {
            format!(
                "<table><thead><tr><th>Opened</th><th>Closed</th><th>Market</th><th>Side</th><th>Entry</th><th>Exit</th><th>PnL</th><th>ROI</th><th>Hold</th><th>Reason</th></tr></thead><tbody>{}</tbody></table>",
                closed_rows
            )
        };

        // Tab 4: Performance
        let best_str = if paper_closed_pnl_values.is_empty() { "—".to_string() } else { format!("${:+.2}", best_trade) };
        let worst_str = if paper_closed_pnl_values.is_empty() { "—".to_string() } else { format!("${:+.2}", worst_trade) };
        let best_color = if paper_closed_pnl_values.is_empty() || best_trade >= 0.0 { "#4ade80" } else { "#f87171" };
        let worst_color = if paper_closed_pnl_values.is_empty() || worst_trade >= 0.0 { "#4ade80" } else { "#f87171" };
        let pnl_c = if paper_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
        let avg_c = if avg_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
        let tab4 = format!(
            "<div class='perf-cards'>\
               <div class='perf-card'><div class='perf-label'>Total Trades</div><div class='perf-val'>{}</div></div>\
               <div class='perf-card'><div class='perf-label'>Win Rate</div><div class='perf-val'>{:.0}%</div></div>\
               <div class='perf-card'><div class='perf-label'>Total PnL</div><div class='perf-val' style='color:{}'>${:+.2}</div></div>\
               <div class='perf-card'><div class='perf-label'>Avg PnL</div><div class='perf-val' style='color:{}'>${:+.2}</div></div>\
               <div class='perf-card'><div class='perf-label'>Best Trade</div><div class='perf-val' style='color:{}'>{}</div></div>\
               <div class='perf-card'><div class='perf-label'>Worst Trade</div><div class='perf-val' style='color:{}'>{}</div></div>\
               <div class='perf-card'><div class='perf-label'>Avg Hold</div><div class='perf-val'>{}</div></div>\
               <div class='perf-card'><div class='perf-label'>Closed</div><div class='perf-val'>{}</div></div>\
             </div>\
             <div style='margin-top:1rem'>{}</div>",
            paper_count, closed_win_rate,
            pnl_c, paper_pnl,
            avg_c, avg_pnl,
            best_color, best_str,
            worst_color, worst_str,
            avg_hold_str, paper_closed_count,
            equity_svg
        );

        format!(
            "{}<h2>Smart Money <span class='count'>{}</span></h2>\
             <div class='section'>\
             <input type='radio' name='paper-tab' id='tab-positions' checked class='tab-radio'>\
             <input type='radio' name='paper-tab' id='tab-history' class='tab-radio'>\
             <input type='radio' name='paper-tab' id='tab-closed' class='tab-radio'>\
             <input type='radio' name='paper-tab' id='tab-perf' class='tab-radio'>\
             <div class='tab-nav'>\
               <label for='tab-positions'>Positions ({})</label>\
               <label for='tab-history'>Trade History</label>\
               <label for='tab-closed'>Closed ({})</label>\
               <label for='tab-perf'>Performance</label>\
             </div>\
             <div class='tab-panels'>\
               <div class='tab-panel' id='panel-positions'>{}</div>\
               <div class='tab-panel' id='panel-history'>{}</div>\
               <div class='tab-panel' id='panel-closed'>{}</div>\
               <div class='tab-panel' id='panel-perf'>{}</div>\
             </div>\
             </div>",
            tab_css, paper_count, paper_open_count, paper_closed_count,
            tab1, tab2, tab3, tab4
        )
    };

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
.collapse{{cursor:pointer}}
.collapse summary{{list-style:none;display:flex;align-items:center;gap:.5rem}}
.collapse summary::-webkit-details-marker{{display:none}}
.collapse summary::before{{content:'▶';font-size:.7rem;color:#64748b;transition:transform .2s}}
.collapse[open] summary::before{{transform:rotate(90deg)}}
.collapse .section{{margin-top:.6rem}}
.tab-radio,.period-radio{{display:none}}
.tab-nav{{display:flex;gap:0;border-bottom:2px solid #1e293b;margin-bottom:1rem}}
.tab-nav label{{padding:.5rem 1rem;cursor:pointer;color:#64748b;font-size:.8rem;font-weight:600;text-transform:uppercase;letter-spacing:.05em;border-bottom:2px solid transparent;margin-bottom:-2px;transition:all .2s}}
.tab-nav label:hover{{color:#94a3b8}}
.tab-panel{{display:none}}
.period-nav{{display:flex;gap:.5rem;margin-bottom:.8rem}}
.period-nav label{{padding:2px 10px;border-radius:3px;cursor:pointer;color:#64748b;font-size:.7rem;background:#1e293b;transition:all .2s}}
.period-nav label:hover{{color:#94a3b8}}
.row-win{{background:rgba(74,222,128,0.04)}}
.row-loss{{background:rgba(248,113,113,0.04)}}
.perf-cards{{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:.6rem}}
.perf-card{{background:#111827;border-radius:8px;padding:.8rem;border:1px solid #1e293b;text-align:center}}
.perf-label{{color:#64748b;font-size:.65rem;text-transform:uppercase;letter-spacing:.05em}}
.perf-val{{font-size:1.3rem;font-weight:700;margin-top:.2rem}}
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
  <div class="card"><div class="label">SM Trades</div><div class="value">{sm_count}</div><div class="meta">{sm_win_rate:.0}% win</div></div>
  <div class="card"><div class="label">SM PnL</div><div class="value" style="color:{sm_pnl_color}">${sm_pnl:+.2}</div><div class="meta">${sm_invested:.0} inv</div></div>
  <div class="card"><div class="label">Crypto 5m</div><div class="value">{crypto_count}</div><div class="meta">{crypto_win_rate:.0}% win</div></div>
  <div class="card"><div class="label">Crypto PnL</div><div class="value" style="color:{crypto_pnl_color}">${crypto_pnl:+.2}</div><div class="meta">${crypto_invested:.0} inv</div></div>
  <div class="card"><div class="label">Live Trades</div><div class="value">{live_count}</div></div>
  <div class="card"><div class="label">Total PnL</div><div class="value" style="color:{pnl_color}">{total_pnl:+.2}</div><div class="meta">${total_invested:.0} total</div></div>
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

<details class="collapse">
<summary><h2 style="display:inline">Watched Wallets <span class="count">{wallet_count}</span></h2></summary>
<div class="section">
{wallets_section}
</div>
</details>
</div>

<h2>Recent Signals <span class="count">{signal_count}</span></h2>
<div class="section">
{signals_section}
</div>

{paper_section}

{crypto_section}

<h2>Live Trades <span class="count">{live_count}</span></h2>
<div class="section">
{live_section}
</div>

<p class="meta" style="margin-top:2rem;text-align:center">PMCC Smart Money System &mdash; polymarket-cli</p>
</body>
</html>"##,
        now = now,
        wallet_count = wallets.len(),
        signal_count = signals.len(),
        odds_count = odds_watches.len(),
        odds_alert_count = odds_alerts.len(),
        sm_count = paper_count,
        sm_pnl = paper_pnl,
        sm_pnl_color = if paper_pnl >= 0.0 { "#4ade80" } else { "#f87171" },
        sm_invested = paper_invested,
        sm_win_rate = paper_win_rate,
        crypto_count = crypto_stats.0,
        crypto_pnl = crypto_stats.1,
        crypto_pnl_color = if crypto_stats.1 >= 0.0 { "#4ade80" } else { "#f87171" },
        crypto_invested = crypto_stats.2,
        crypto_win_rate = crypto_stats.3,
        live_count = follows.iter().filter(|r| !r.dry_run).count(),
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
        paper_section = paper_section,
        crypto_section = crypto_section,
        live_section = if live_rows.is_empty() {
            "<p class='empty'>No live trades yet.</p>".into()
        } else {
            let live_roi = if live_invested > 0.0 { live_pnl / live_invested * 100.0 } else { 0.0 };
            let lc = if live_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
            format!(
                "<table><thead><tr><th>Time</th><th>Status</th><th>Side</th><th>Market</th><th>Invested</th><th>Entry</th><th>Now/Exit</th><th>PnL</th><th>ROI</th></tr></thead><tbody>{live_rows}</tbody></table>\
                 <p style='margin-top:.5rem;color:#94a3b8;font-size:.8rem'>Live total: ${live_invested:.2} | PnL: <span style='color:{lc}'>{live_pnl:+.2}</span> ({live_roi:+.1}%)</p>"
            )
        },
    )
}

/// Build a separate paper trading section for crypto 5m trades.
/// Returns (html_section, (count, pnl, invested, win_rate)).
fn build_crypto_paper_section(
    follows: &[FollowRecord],
    price_map: &std::collections::HashMap<(String, String), f64>,
    price_history: &[crate::smart::PriceSnapshot],
) -> (String, (u32, f64, f64, f64)) {
    let utc_now = Utc::now();
    let crypto_follows: Vec<&FollowRecord> = follows.iter()
        .filter(|r| r.dry_run && r.entry_reason.as_deref().map(|e| e.starts_with("crypto:")).unwrap_or(false))
        .collect();

    if crypto_follows.is_empty() {
        return (
            "<h2>Crypto 5m Paper Trading <span class='count'>0</span></h2>\
             <div class='section'><p class='empty'>No crypto trades yet. Run: <code>polymarket smart crypto monitor</code></p></div>"
                .to_string(),
            (0, 0.0, 0.0, 0.0),
        );
    }

    let mut invested = 0.0f64;
    let mut total_pnl = 0.0f64;
    let mut count = 0u32;
    let mut wins = 0u32;
    let mut open_count = 0u32;
    let mut open_invested = 0.0f64;
    let mut open_pnl = 0.0f64;
    let mut closed_count = 0u32;
    let mut closed_wins = 0u32;
    let mut closed_pnl_values: Vec<f64> = Vec::new();
    let mut closed_hold_hours: Vec<f64> = Vec::new();

    let mut open_rows = String::new();
    let mut closed_rows = String::new();
    let mut history_rows_vec: Vec<(chrono::DateTime<Utc>, String)> = Vec::new();
    let mut equity_points: Vec<(i64, f64)> = Vec::new();

    for r in &crypto_follows {
        let entry = r.effective_entry();
        invested += r.amount_usdc;
        count += 1;

        let age_hours = (utc_now - r.timestamp).num_hours();
        let mut periods = Vec::new();
        if age_hours < 24 { periods.push("today"); }
        if age_hours < 168 { periods.push("week"); }
        if age_hours < 720 { periods.push("month"); }
        let p_attr = periods.join(" ");

        if r.is_open() {
            let current = price_map.get(&(r.condition_id.clone(), r.outcome.clone())).copied().unwrap_or(entry);
            let pnl = calc_open_pnl(&r.side, r.amount_usdc, entry, current);
            let roi = if r.amount_usdc > 0.0 { pnl / r.amount_usdc * 100.0 } else { 0.0 };
            let pnl_cls = if pnl >= 0.0 { "text-green" } else { "text-red" };
            total_pnl += pnl;
            if pnl > 0.0 { wins += 1; }
            open_invested += r.amount_usdc;
            open_pnl += pnl;
            open_count += 1;

            let price_key = format!("{}:{}", r.condition_id, r.outcome);
            let hist_prices: Vec<f64> = price_history.iter()
                .filter_map(|s| s.prices.get(&price_key).copied())
                .collect();
            let sparkline = build_mini_sparkline(&hist_prices, current);
            let change_24h = if let Some(&first) = hist_prices.first() {
                if first > 0.0 { ((current - first) / first) * 100.0 } else { 0.0 }
            } else { 0.0 };
            let ch_cls = if change_24h >= 0.0 { "text-green" } else { "text-red" };
            let ch_str = if hist_prices.is_empty() { "—".to_string() } else { format!("{:+.1}%", change_24h) };
            let entry_reason = r.entry_reason.as_deref().unwrap_or("—");

            open_rows.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td><td>{:.3}</td><td>${:.2}</td><td class='{}'>{:+.2}</td><td class='{}'>{:+.1}%</td><td class='{}'>{}</td><td>{}</td><td>{}</td></tr>",
                r.timestamp.format("%m-%d %H:%M"),
                html_escape(&r.market_title), html_escape(&r.outcome), html_escape(&r.side),
                entry, current, r.amount_usdc, pnl_cls, pnl, pnl_cls, roi,
                ch_cls, ch_str, sparkline, html_escape(entry_reason)
            ));

            history_rows_vec.push((r.timestamp, format!(
                "<tr data-p='{}'><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td><td>${:.2}</td><td style='color:#94a3b8'>OPEN</td></tr>",
                p_attr, r.timestamp.format("%m-%d %H:%M"), html_escape(&r.side),
                html_escape(&r.market_title), html_escape(&r.outcome), entry, r.amount_usdc
            )));
        } else {
            let pnl = r.realized_pnl.unwrap_or(0.0);
            let exit = r.exit_price.unwrap_or(entry);
            let roi = if r.amount_usdc > 0.0 { pnl / r.amount_usdc * 100.0 } else { 0.0 };
            let pnl_cls = if pnl >= 0.0 { "text-green" } else { "text-red" };
            let row_cls = if pnl >= 0.0 { "row-win" } else { "row-loss" };
            total_pnl += pnl;
            if pnl > 0.0 { wins += 1; }
            closed_count += 1;
            if pnl > 0.0 { closed_wins += 1; }
            closed_pnl_values.push(pnl);

            let close_time = r.closed_at.unwrap_or(utc_now);
            let hold_h = (close_time - r.timestamp).num_hours() as f64;
            closed_hold_hours.push(hold_h);
            let hold_str = if hold_h < 1.0 {
                format!("{}m", (close_time - r.timestamp).num_minutes())
            } else if hold_h < 24.0 {
                format!("{:.0}h", hold_h)
            } else {
                format!("{:.0}d {:.0}h", (hold_h / 24.0).floor(), hold_h % 24.0)
            };

            let exit_reason = r.exit_reason.as_deref().unwrap_or("—");
            closed_rows.push_str(&format!(
                "<tr class='{}'><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td><td>{:.3}</td><td class='{}'>{:+.2}</td><td class='{}'>{:+.1}%</td><td>{}</td><td>{}</td></tr>",
                row_cls, r.timestamp.format("%m-%d %H:%M"),
                r.closed_at.map_or("—".into(), |t| t.format("%m-%d %H:%M").to_string()),
                html_escape(&r.market_title), html_escape(&r.side),
                entry, exit, pnl_cls, pnl, pnl_cls, roi, hold_str,
                html_escape(exit_reason)
            ));

            let status_str = r.status.as_ref().map(|s| s.to_string()).unwrap_or_else(|| "CLOSED".to_string());
            history_rows_vec.push((r.timestamp, format!(
                "<tr data-p='{}'><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td><td>${:.2}</td><td style='color:#60a5fa'>{}</td></tr>",
                p_attr, r.timestamp.format("%m-%d %H:%M"), html_escape(&r.side),
                html_escape(&r.market_title), html_escape(&r.outcome), entry, r.amount_usdc, status_str
            )));

            equity_points.push((close_time.timestamp_millis(), pnl));
        }
    }

    // Sort trade history newest first
    history_rows_vec.sort_by(|a, b| b.0.cmp(&a.0));
    let history_rows: String = history_rows_vec.into_iter().map(|(_, r)| r).collect();

    // Build equity curve
    equity_points.sort_by_key(|p| p.0);
    let mut cumulative = 0.0f64;
    let eq_cumulative: Vec<(i64, f64)> = equity_points.iter().map(|&(t, pnl)| {
        cumulative += pnl;
        (t, cumulative)
    }).collect();
    let equity_svg = build_equity_curve_svg(&eq_cumulative);

    // Stats
    let win_rate = if count > 0 { wins as f64 / count as f64 * 100.0 } else { 0.0 };
    let closed_win_rate = if closed_count > 0 { closed_wins as f64 / closed_count as f64 * 100.0 } else { 0.0 };
    let avg_pnl = if !closed_pnl_values.is_empty() { closed_pnl_values.iter().sum::<f64>() / closed_pnl_values.len() as f64 } else { 0.0 };
    let best_trade = closed_pnl_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let worst_trade = closed_pnl_values.iter().cloned().fold(f64::INFINITY, f64::min);
    let avg_hold = if !closed_hold_hours.is_empty() { closed_hold_hours.iter().sum::<f64>() / closed_hold_hours.len() as f64 } else { 0.0 };
    let avg_hold_str = if avg_hold < 1.0 { "&lt;1h".to_string() } else if avg_hold < 24.0 { format!("{:.0}h", avg_hold) } else { format!("{:.0}d", avg_hold / 24.0) };

    // Scoped tab CSS with c5m- prefix
    let tab_css = r#"<style>
#c5m-tab-positions:checked~.tab-nav label[for='c5m-tab-positions'],
#c5m-tab-history:checked~.tab-nav label[for='c5m-tab-history'],
#c5m-tab-closed:checked~.tab-nav label[for='c5m-tab-closed'],
#c5m-tab-perf:checked~.tab-nav label[for='c5m-tab-perf']{color:#e2e8f0;border-bottom-color:#60a5fa}
#c5m-tab-positions:checked~.tab-panels #c5m-panel-positions,
#c5m-tab-history:checked~.tab-panels #c5m-panel-history,
#c5m-tab-closed:checked~.tab-panels #c5m-panel-closed,
#c5m-tab-perf:checked~.tab-panels #c5m-panel-perf{display:block}
#c5m-period-all:checked~.period-nav label[for='c5m-period-all'],
#c5m-period-today:checked~.period-nav label[for='c5m-period-today'],
#c5m-period-week:checked~.period-nav label[for='c5m-period-week'],
#c5m-period-month:checked~.period-nav label[for='c5m-period-month']{color:#e2e8f0;background:#334155}
#c5m-period-today:checked~.c5m-period-wrap tbody tr:not([data-p~='today']),
#c5m-period-week:checked~.c5m-period-wrap tbody tr:not([data-p~='week']),
#c5m-period-month:checked~.c5m-period-wrap tbody tr:not([data-p~='month']){display:none}
</style>"#;

    // Tab 1: Open Positions
    let tab1 = if open_rows.is_empty() {
        "<p class='empty'>No open positions.</p>".to_string()
    } else {
        let oc = if open_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
        format!(
            "<table><thead><tr><th>Time</th><th>Market</th><th>Outcome</th><th>Side</th><th>Entry</th><th>Current</th><th>Size</th><th>PnL</th><th>ROI</th><th>24h</th><th>Trend</th><th>Entry Reason</th></tr></thead><tbody>{}</tbody></table>\
             <p style='margin-top:.5rem;color:#94a3b8;font-size:.8rem'>{} positions | ${:.2} invested | Unrealized: <span style='color:{}'>{:+.2}</span></p>",
            open_rows, open_count, open_invested, oc, open_pnl
        )
    };

    // Tab 2: Trade History
    let tab2 = if history_rows.is_empty() {
        "<p class='empty'>No trades yet.</p>".to_string()
    } else {
        format!(
            "<input type='radio' name='c5m-period' id='c5m-period-all' checked class='period-radio'>\
             <input type='radio' name='c5m-period' id='c5m-period-today' class='period-radio'>\
             <input type='radio' name='c5m-period' id='c5m-period-week' class='period-radio'>\
             <input type='radio' name='c5m-period' id='c5m-period-month' class='period-radio'>\
             <div class='period-nav'>\
               <label for='c5m-period-all'>All</label>\
               <label for='c5m-period-today'>Today</label>\
               <label for='c5m-period-week'>Week</label>\
               <label for='c5m-period-month'>Month</label>\
             </div>\
             <div class='c5m-period-wrap'>\
             <table><thead><tr><th>Time</th><th>Side</th><th>Market</th><th>Outcome</th><th>Entry</th><th>Amount</th><th>Status</th></tr></thead><tbody>{history_rows}</tbody></table>\
             </div>"
        )
    };

    // Tab 3: Position History
    let tab3 = if closed_rows.is_empty() {
        "<p class='empty'>No closed positions yet.</p>".to_string()
    } else {
        format!(
            "<table><thead><tr><th>Opened</th><th>Closed</th><th>Market</th><th>Side</th><th>Entry</th><th>Exit</th><th>PnL</th><th>ROI</th><th>Hold</th><th>Reason</th></tr></thead><tbody>{}</tbody></table>",
            closed_rows
        )
    };

    // Tab 4: Performance
    let best_str = if closed_pnl_values.is_empty() { "—".to_string() } else { format!("${:+.2}", best_trade) };
    let worst_str = if closed_pnl_values.is_empty() { "—".to_string() } else { format!("${:+.2}", worst_trade) };
    let best_color = if closed_pnl_values.is_empty() || best_trade >= 0.0 { "#4ade80" } else { "#f87171" };
    let worst_color = if closed_pnl_values.is_empty() || worst_trade >= 0.0 { "#4ade80" } else { "#f87171" };
    let pnl_c = if total_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
    let avg_c = if avg_pnl >= 0.0 { "#4ade80" } else { "#f87171" };
    let tab4 = format!(
        "<div class='perf-cards'>\
           <div class='perf-card'><div class='perf-label'>Total Trades</div><div class='perf-val'>{}</div></div>\
           <div class='perf-card'><div class='perf-label'>Win Rate</div><div class='perf-val'>{:.0}%</div></div>\
           <div class='perf-card'><div class='perf-label'>Total PnL</div><div class='perf-val' style='color:{}'>${:+.2}</div></div>\
           <div class='perf-card'><div class='perf-label'>Avg PnL</div><div class='perf-val' style='color:{}'>${:+.2}</div></div>\
           <div class='perf-card'><div class='perf-label'>Best Trade</div><div class='perf-val' style='color:{}'>{}</div></div>\
           <div class='perf-card'><div class='perf-label'>Worst Trade</div><div class='perf-val' style='color:{}'>{}</div></div>\
           <div class='perf-card'><div class='perf-label'>Avg Hold</div><div class='perf-val'>{}</div></div>\
           <div class='perf-card'><div class='perf-label'>Closed</div><div class='perf-val'>{}</div></div>\
         </div>\
         <div style='margin-top:1rem'>{}</div>",
        count, closed_win_rate,
        pnl_c, total_pnl,
        avg_c, avg_pnl,
        best_color, best_str,
        worst_color, worst_str,
        avg_hold_str, closed_count,
        equity_svg
    );

    let section = format!(
        "{tab_css}<h2>Crypto 5m Paper Trading <span class='count'>{count}</span></h2>\
         <div class='section'>\
         <input type='radio' name='c5m-tab' id='c5m-tab-positions' checked class='tab-radio'>\
         <input type='radio' name='c5m-tab' id='c5m-tab-history' class='tab-radio'>\
         <input type='radio' name='c5m-tab' id='c5m-tab-closed' class='tab-radio'>\
         <input type='radio' name='c5m-tab' id='c5m-tab-perf' class='tab-radio'>\
         <div class='tab-nav'>\
           <label for='c5m-tab-positions'>Positions ({open_count})</label>\
           <label for='c5m-tab-history'>Trade History</label>\
           <label for='c5m-tab-closed'>Closed ({closed_count})</label>\
           <label for='c5m-tab-perf'>Performance</label>\
         </div>\
         <div class='tab-panels'>\
           <div class='tab-panel' id='c5m-panel-positions'>{tab1}</div>\
           <div class='tab-panel' id='c5m-panel-history'>{tab2}</div>\
           <div class='tab-panel' id='c5m-panel-closed'>{tab3}</div>\
           <div class='tab-panel' id='c5m-panel-perf'>{tab4}</div>\
         </div>\
         </div>"
    );

    (section, (count, total_pnl, invested, win_rate))
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

/// Check if a market resolves within `max_days` days based on its title.
/// Returns true (allow) if no date found or if within the window.
fn market_within_horizon(title: &str, max_days: i64) -> bool {
    let lower = title.to_lowercase();
    let today = Utc::now().date_naive();

    let months: &[(&str, u32)] = &[
        ("january", 1), ("february", 2), ("march", 3), ("april", 4),
        ("may", 5), ("june", 6), ("july", 7), ("august", 8),
        ("september", 9), ("october", 10), ("november", 11), ("december", 12),
    ];

    // Find month in title
    let mut found_month: Option<u32> = None;
    let mut month_end_idx = 0usize;
    for &(name, num) in months {
        if let Some(pos) = lower.find(name) {
            found_month = Some(num);
            month_end_idx = pos + name.len();
            break;
        }
    }
    let month = match found_month {
        Some(m) => m,
        None => {
            // Check "end of YYYY" without month
            for yr in &["2026", "2027", "2028", "2029"] {
                if lower.contains(&format!("end of {yr}")) || lower.contains(&format!("by {yr}")) {
                    if let Ok(y) = yr.parse::<i32>() {
                        if let Some(d) = chrono::NaiveDate::from_ymd_opt(y, 12, 31) {
                            return (d - today).num_days() <= max_days;
                        }
                    }
                }
            }
            return true; // no date pattern found, allow
        }
    };

    // Extract day: look for digits right after month name
    let rest = &lower[month_end_idx..];
    let day: u32 = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(28); // default to ~end of month
    let day = day.clamp(1, 28);

    // Extract year: find "20XX" anywhere in title
    let year: i32 = lower
        .as_bytes()
        .windows(4)
        .filter_map(|w| {
            let s = std::str::from_utf8(w).ok()?;
            if s.starts_with("20") { s.parse::<i32>().ok() } else { None }
        })
        .find(|&y| (2025..=2030).contains(&y))
        .unwrap_or(chrono::Datelike::year(&today));

    match chrono::NaiveDate::from_ymd_opt(year, month, day) {
        Some(resolve) => (resolve - today).num_days() <= max_days,
        None => true, // can't construct date, allow
    }
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
            // If any wallet in aggregation is a holder, skip include check
            let has_holder = agg.signals.iter().any(|s| s.wallet_tag.as_deref().map_or(false, |t| t.contains("-holder")));
            if has_holder {
                if matches_exclude(&agg.market_title) { continue; }
            } else {
                if !matches_include(&agg.market_title) || matches_exclude(&agg.market_title) { continue; }
            }
            // Skip near-resolved markets
            if agg.avg_price < 0.05 || agg.avg_price > 0.95 { continue; }

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
        // Only trigger on NewPosition — ClosePosition is handled separately (closes our BUY positions)
        if !matches!(sig.signal_type, crate::smart::SignalType::NewPosition) {
            continue;
        }
        if confidence_rank(&sig.confidence) < confidence_rank(&config.min_confidence) { continue; }

        // Market-first wallets (tag contains "-holder"): skip include check, only exclude
        // Leaderboard wallets: apply both include + exclude
        let is_holder = sig.wallet_tag.as_deref().map_or(false, |t| t.contains("-holder"));
        if is_holder {
            if matches_exclude(&sig.market_title) { continue; }
        } else {
            if !matches_include(&sig.market_title) || matches_exclude(&sig.market_title) { continue; }
        }
        // Tighter price filter: skip near-resolved markets
        let sig_price: f64 = sig.price.parse().unwrap_or(0.0);
        if sig_price < 0.05 || sig_price > 0.95 { continue; }
        // Min position size filter: skip small/test positions (<$200)
        let sig_size: f64 = sig.size.parse().unwrap_or(0.0);
        if sig_size < 200.0 { continue; }
        // Resolution horizon: only trade markets resolving within 30 days
        if !market_within_horizon(&sig.market_title, 30) { continue; }

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

    // Confirmation delay: queue triggers and wait before executing paper trades
    let confirm_secs = 600i64; // 10 minutes
    let mut pending_triggers: Vec<(crate::smart::TriggerEvent, chrono::DateTime<Utc>)> = Vec::new();
    let mut cancelled_keys: std::collections::HashMap<(String, String), chrono::DateTime<Utc>> = std::collections::HashMap::new();

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

                // Scan wallets (rate-limit: 100ms between calls)
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
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }

                // Persist signals
                if !all_signals.is_empty() {
                    if let Err(e) = store::append_signals(&all_signals) {
                        eprintln!("warn: failed to save signals: {e}");
                    }
                }

                // Close follow positions on ClosePosition
                for sig in &all_signals {
                    if matches!(sig.signal_type, crate::smart::SignalType::ClosePosition) {
                        let exit_price: f64 = sig.price.parse().unwrap_or(0.0);
                        if exit_price > 0.0 {
                            let tag = sig.wallet_tag.as_deref().unwrap_or("unknown");
                            let reason = format!("whale exit: {} closed", tag);
                            if let Err(e) = store::close_follow_position(&sig.condition_id, &sig.outcome, exit_price, &reason) {
                                eprintln!("warn: failed to close position: {e}");
                            }
                        }
                    }
                }

                // Record price history + stop-loss for open positions
                {
                    use polymarket_client_sdk::clob;
                    use polymarket_client_sdk::clob::types::request::MidpointRequest;
                    use polymarket_client_sdk::types::U256;
                    use rust_decimal::prelude::ToPrimitive;

                    let wallet_price_map = store::current_price_map().unwrap_or_default();
                    let open_follows = store::load_follow_records().unwrap_or_default();
                    let open_positions: Vec<&FollowRecord> = open_follows.iter()
                        .filter(|r| r.dry_run && r.is_open())
                        .collect();

                    // Build live price map: wallet snapshots + midpoint API fallback
                    let mut live_prices: std::collections::HashMap<(String, String), f64> = std::collections::HashMap::new();
                    let clob_client = clob::Client::default();

                    for r in &open_positions {
                        let key = (r.condition_id.clone(), r.outcome.clone());
                        if live_prices.contains_key(&key) { continue; }
                        if let Some(&p) = wallet_price_map.get(&key) {
                            live_prices.insert(key, p);
                        } else {
                            // Fetch midpoint for positions not in wallet snapshots
                            if let Ok(token_id) = r.asset.parse::<U256>() {
                                let req = MidpointRequest::builder().token_id(token_id).build();
                                if let Ok(result) = clob_client.midpoint(&req).await {
                                    let mid = result.mid.to_f64().unwrap_or(0.0);
                                    if mid > 0.0 {
                                        live_prices.insert(key, mid);
                                    }
                                }
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            }
                        }
                    }

                    // Record price history (serialize keys as "condition_id:outcome")
                    if !live_prices.is_empty() {
                        let serialized: std::collections::HashMap<String, f64> = live_prices.iter()
                            .map(|((cid, out), &p)| (format!("{cid}:{out}"), p))
                            .collect();
                        let snap = PriceSnapshot { timestamp: Utc::now(), prices: serialized };
                        if let Err(e) = store::append_price_snapshot(&snap) {
                            eprintln!("warn: failed to save price snapshot: {e}");
                        }
                    }
                    if cycle % 100 == 0 {
                        if let Err(e) = store::prune_price_history(48) {
                            eprintln!("warn: failed to prune price history: {e}");
                        }
                    }

                    // Stop-loss + trailing stop
                    let stop_loss_pct = -45.0f64;
                    let trailing_activate_pct = 30.0f64;  // activate trailing stop after +30% ROI
                    let trailing_drawdown_pct = 50.0f64;  // close if ROI drops 50% from peak
                    let mut peak_roi = store::load_peak_roi().unwrap_or_default();
                    let mut peak_changed = false;

                    for r in &open_positions {
                        let entry = r.effective_entry();
                        if entry <= 0.0 { continue; }
                        let pos_id = r.position_id.as_deref().unwrap_or(&r.condition_id);
                        if let Some(&current) = live_prices.get(&(r.condition_id.clone(), r.outcome.clone())) {
                            if current <= 0.0 { continue; }
                            let pnl = calc_open_pnl(&r.side, r.amount_usdc, entry, current);
                            let roi = if r.amount_usdc > 0.0 { pnl / r.amount_usdc * 100.0 } else { 0.0 };

                            // Stop-loss
                            if roi <= stop_loss_pct {
                                let reason = format!("stop-loss: {:.1}% (limit {:.0}%)", roi, stop_loss_pct);
                                if let Err(e) = store::close_follow_position(&r.condition_id, &r.outcome, current, &reason) {
                                    eprintln!("warn: failed to close stop-loss position: {e}");
                                }
                                peak_roi.remove(pos_id);
                                peak_changed = true;
                                eprintln!("  STOP-LOSS: {} @ {:.3} -> {:.3} ({:+.1}%)", r.market_title, entry, current, roi);
                                continue;
                            }

                            // Trailing stop: update peak and check drawdown
                            let prev_peak = peak_roi.get(pos_id).copied().unwrap_or(0.0f64);
                            if roi > prev_peak {
                                peak_roi.insert(pos_id.to_string(), roi);
                                peak_changed = true;
                            }
                            let current_peak = peak_roi.get(pos_id).copied().unwrap_or(0.0);

                            if current_peak >= trailing_activate_pct {
                                let threshold = current_peak * (1.0 - trailing_drawdown_pct / 100.0);
                                if roi < threshold {
                                    let reason = format!("trailing-stop: peak {:.1}% -> {:.1}% (drawdown {:.0}%)", current_peak, roi, trailing_drawdown_pct);
                                    if let Err(e) = store::close_follow_position(&r.condition_id, &r.outcome, current, &reason) {
                                        eprintln!("warn: failed to close trailing-stop position: {e}");
                                    }
                                    peak_roi.remove(pos_id);
                                    peak_changed = true;
                                    eprintln!("  TRAILING-STOP: {} peak {:.1}% -> now {:.1}% (threshold {:.1}%) @ {:.3}", r.market_title, current_peak, roi, threshold, current);
                                }
                            }
                        }
                    }

                    // Clean up peak_roi for closed positions
                    let open_pos_ids: std::collections::HashSet<String> = open_positions.iter()
                        .filter_map(|r| r.position_id.clone())
                        .collect();
                    peak_roi.retain(|k, _| open_pos_ids.contains(k));

                    if peak_changed {
                        if let Err(e) = store::save_peak_roi(&peak_roi) {
                            eprintln!("warn: failed to save peak ROI: {e}");
                        }
                    }
                }

                // Aggregate
                let aggregated = signals::aggregate_signals(&all_signals);

                // Scan odds (always scan if watches exist; each watch has its own threshold)
                let odds_alerts = {
                    let alerts = odds::scan_odds().await.unwrap_or_default();
                    if !alerts.is_empty() {
                        if let Err(e) = store::append_odds_alerts(&alerts) {
                            eprintln!("warn: failed to save odds alerts: {e}");
                        }
                    }
                    alerts
                };

                // Evaluate triggers
                let triggers = evaluate_triggers(&all_signals, &aggregated, &odds_alerts, &config);

                // Paper trade with confirmation delay
                let mut paper_count = 0u32;
                let mut pending_count = 0u32;

                if config.paper_trade {
                    // Cancel pending triggers if ClosePosition seen for same market
                    for sig in &all_signals {
                        if matches!(sig.signal_type, crate::smart::SignalType::ClosePosition) {
                            cancelled_keys.insert((sig.condition_id.clone(), sig.outcome.clone()), Utc::now());
                        }
                    }

                    // Queue new triggers (instead of immediate execution)
                    // Anti-hedge: track condition_ids already pending to prevent Yes+No on same market
                    let mut pending_cids: std::collections::HashSet<(String, String)> = pending_triggers
                        .iter().map(|(t, _)| (t.condition_id.clone(), t.outcome.clone())).collect();
                    let mut pending_markets: std::collections::HashSet<String> = pending_triggers
                        .iter().map(|(t, _)| t.condition_id.clone()).collect();
                    // Also include existing open positions
                    {
                        let existing = store::load_follow_records().unwrap_or_default();
                        for r in &existing {
                            if r.dry_run && r.is_open() {
                                pending_markets.insert(r.condition_id.clone());
                            }
                        }
                    }

                    for trigger in &triggers {
                        if matches!(trigger.trigger_type, crate::smart::TriggerType::OddsAlert) {
                            continue;
                        }
                        if trigger.price < 0.15 || trigger.price > 0.80 {
                            continue;
                        }
                        let key = (trigger.condition_id.clone(), trigger.outcome.clone());
                        if pending_cids.contains(&key) || cancelled_keys.contains_key(&key) {
                            continue;
                        }
                        // Anti-hedge: skip if same market already pending or open
                        if pending_markets.contains(&trigger.condition_id) {
                            continue;
                        }
                        pending_triggers.push((trigger.clone(), Utc::now()));
                        pending_cids.insert(key);
                        pending_markets.insert(trigger.condition_id.clone());
                        pending_count += 1;
                    }

                    // Execute confirmed triggers (>= 10 min old, not cancelled)
                    let now_utc = Utc::now();
                    let mut confirmed = Vec::new();
                    let mut still_pending = Vec::new();
                    for (trigger, created_at) in pending_triggers.drain(..) {
                        let key = (trigger.condition_id.clone(), trigger.outcome.clone());
                        if cancelled_keys.contains_key(&key) {
                            continue; // discard cancelled
                        }
                        if (now_utc - created_at).num_seconds() >= confirm_secs {
                            confirmed.push(trigger);
                        } else {
                            still_pending.push((trigger, created_at));
                        }
                    }
                    pending_triggers = still_pending;

                    // Create paper trades from confirmed triggers
                    if !confirmed.is_empty() {
                        let today_spent = store::today_spend().unwrap_or(0.0);
                        let mut spent = 0.0f64;

                        let existing_follows = store::load_follow_records().unwrap_or_default();
                        let mut open_positions: std::collections::HashSet<(String, String)> = existing_follows.iter()
                            .filter(|r| r.dry_run && r.is_open())
                            .map(|r| (r.condition_id.clone(), r.outcome.clone()))
                            .collect();
                        let mut open_markets: std::collections::HashSet<String> = existing_follows.iter()
                            .filter(|r| r.dry_run && r.is_open())
                            .map(|r| r.condition_id.clone())
                            .collect();

                        for trigger in &confirmed {
                            if open_positions.contains(&(trigger.condition_id.clone(), trigger.outcome.clone())) {
                                continue;
                            }
                            if open_markets.contains(&trigger.condition_id) {
                                continue;
                            }
                            if today_spent + spent + config.amount > config.max_per_day {
                                break;
                            }

                            // Use latest known price instead of stale trigger price (queued 10min ago)
                            let current_price = store::current_price_map()
                                .unwrap_or_default()
                                .get(&(trigger.condition_id.clone(), trigger.outcome.clone()))
                                .copied()
                                .unwrap_or(trigger.price);

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
                                price: current_price,
                                dry_run: true,
                                order_id: None,
                                fill_price: None,
                                status: Some(TradeStatus::Open),
                                closed_at: None,
                                exit_price: None,
                                realized_pnl: None,
                                position_id: Some(pos_id),
                                entry_reason: Some(format!("monitor: {}", trigger.reason)),
                                exit_reason: None,
                            };
                            if let Err(e) = store::append_follow_record(&record) {
                                eprintln!("warn: failed to save follow record: {e}");
                            }
                            // Track this trade to prevent hedge in same batch
                            open_positions.insert((trigger.condition_id.clone(), trigger.outcome.clone()));
                            open_markets.insert(trigger.condition_id.clone());
                            paper_count += 1;
                            spent += config.amount;
                        }
                    }

                    // Prune cancelled keys older than 1 hour
                    let prune_cutoff = Utc::now() - chrono::Duration::hours(1);
                    cancelled_keys.retain(|_, t| *t > prune_cutoff);
                }

                // Summary line
                let err_str = if scan_errors > 0 { format!(", {scan_errors} error(s)") } else { String::new() };
                let paper_str = if paper_count > 0 { format!(", {paper_count} paper trade(s)") } else { String::new() };
                let pending_str = if pending_count > 0 { format!(", {pending_count} pending") } else if !pending_triggers.is_empty() { format!(", {} queued", pending_triggers.len()) } else { String::new() };
                eprintln!(
                    "{} signal(s), {} trigger(s){paper_str}{pending_str}{err_str}",
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
                            osascript_safe(&short), osascript_safe(&title)
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

/// Sanitize a string for safe use in osascript `display notification`.
/// Strips characters that could break out of the AppleScript string literal.
fn osascript_safe(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '"' | '\\' | '\n' | '\r' | '\0'))
        .collect()
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
                current = price_map.get(&(r.condition_id.clone(), r.outcome.clone())).copied().unwrap_or(entry);
                pnl = calc_open_pnl(&r.side, r.amount_usdc, entry, current);
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

fn build_mini_sparkline(prices: &[f64], current: f64) -> String {
    let mut pts: Vec<f64> = prices.to_vec();
    pts.push(current);
    if pts.len() < 2 {
        return "<span style='color:#475569;font-size:.7rem'>—</span>".to_string();
    }
    // Downsample to max 30 points for clean sparkline
    let max_pts = 30;
    let sampled: Vec<f64> = if pts.len() > max_pts {
        let step = pts.len() as f64 / max_pts as f64;
        (0..max_pts).map(|i| pts[(i as f64 * step) as usize]).collect()
    } else {
        pts.clone()
    };

    let w = 100.0f64;
    let h = 24.0f64;
    let pad = 2.0f64;
    let min_v = sampled.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_v = sampled.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let v_range = (max_v - min_v).max(0.001);
    let n = sampled.len() as f64;

    let x = |i: usize| -> f64 { pad + (i as f64 / (n - 1.0)) * (w - pad * 2.0) };
    let y = |v: f64| -> f64 { h - pad - (v - min_v) / v_range * (h - pad * 2.0) };

    let mut path = String::new();
    for (i, &v) in sampled.iter().enumerate() {
        let cmd = if i == 0 { "M" } else { "L" };
        path.push_str(&format!("{cmd}{:.1},{:.1} ", x(i), y(v)));
    }

    let final_v = *sampled.last().unwrap();
    let first_v = sampled[0];
    let color = if final_v >= first_v { "#4ade80" } else { "#f87171" };

    format!(
        "<svg viewBox='0 0 {w} {h}' width='100' height='24' xmlns='http://www.w3.org/2000/svg' style='vertical-align:middle'>\
         <path d='{path}' fill='none' stroke='{color}' stroke-width='1.5'/>\
         <circle cx='{lx:.1}' cy='{ly:.1}' r='2' fill='{color}'/>\
         </svg>",
        w = w, h = h, path = path, color = color,
        lx = x(sampled.len() - 1), ly = y(final_v),
    )
}

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
                            let token_preview = config.bot_token.get(..10).unwrap_or(&config.bot_token);
                            println!("Token:    {}...", token_preview);
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
    if signals.is_empty() { return; }
    let title = format!("Polymarket: {} signal(s) detected", signals.len());
    let body = if let Some(agg) = aggregated.first() {
        format!(
            "{} wallets {} on {} [{}]",
            agg.wallet_count, agg.direction, agg.market_title, agg.outcome
        )
    } else if let Some(sig) = signals.first() {
        format!(
            "{} {} — {} [{}]",
            sig.signal_type, sig.confidence, sig.market_title, sig.outcome
        )
    } else {
        return;
    };

    let title = osascript_safe(&title);
    let body = osascript_safe(&body);

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

    let title = osascript_safe(&title);
    let body = osascript_safe(&body);
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

// ── Crypto 5m Trading ────────────────────────────────────────────

async fn cmd_crypto(
    gamma_client: &polymarket_client_sdk::gamma::Client,
    command: CryptoCommand,
    output: &OutputFormat,
) -> Result<()> {
    match command {
        CryptoCommand::Feed { asset } => cmd_crypto_feed(&asset, output).await,
        CryptoCommand::Signal { asset } => cmd_crypto_signal(&asset, output).await,
        CryptoCommand::Market { asset } => cmd_crypto_market(gamma_client, &asset, output).await,
        CryptoCommand::Backtest { asset, hours } => {
            cmd_crypto_backtest(&asset, hours, output).await
        }
        CryptoCommand::Monitor {
            asset,
            amount,
            max_per_hour,
            max_per_day,
            min_confidence,
            notify,
        } => {
            cmd_crypto_monitor(
                gamma_client, &asset, amount, max_per_hour, max_per_day,
                min_confidence, notify, output,
            )
            .await
        }
        CryptoCommand::Status => cmd_crypto_status(output),
    }
}

async fn cmd_crypto_feed(asset_str: &str, output: &OutputFormat) -> Result<()> {
    let asset: crypto::CryptoAsset = asset_str.parse()?;
    let feed = crypto::feed::BinanceFeed::new();

    let (candles, depth, trades) =
        tokio::join!(
            feed.fetch_klines(asset, "1m", 30),
            feed.fetch_depth(asset, 20),
            feed.fetch_trades(asset, 100),
        );
    let candles = candles?;
    let depth = depth?;
    let trades = trades?;

    match output {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({
                "asset": asset.to_string(),
                "candles": candles.len(),
                "last_price": candles.last().map(|c| c.close),
                "bid_levels": depth.bids.len(),
                "ask_levels": depth.asks.len(),
                "recent_trades": trades.len(),
            }));
        }
        _ => {
            let last = candles.last();
            let price = last.map(|c| c.close).unwrap_or(0.0);
            let open = candles.first().map(|c| c.open).unwrap_or(0.0);
            let change_pct = if open > 0.0 {
                (price - open) / open * 100.0
            } else {
                0.0
            };

            println!("--- {} Live Feed ---", asset);
            println!(
                "Price: ${:.2}  ({:+.2}% over {}m)",
                price,
                change_pct,
                candles.len()
            );

            // Order book summary
            let bid_vol: f64 = depth.bids.iter().map(|l| l.qty).sum();
            let ask_vol: f64 = depth.asks.iter().map(|l| l.qty).sum();
            let best_bid = depth.bids.first().map(|l| l.price).unwrap_or(0.0);
            let best_ask = depth.asks.first().map(|l| l.price).unwrap_or(0.0);
            println!(
                "Book: bid ${:.2} ({:.4}) | ask ${:.2} ({:.4}) | imbal {:.1}%",
                best_bid,
                bid_vol,
                best_ask,
                ask_vol,
                if bid_vol + ask_vol > 0.0 {
                    (bid_vol - ask_vol) / (bid_vol + ask_vol) * 100.0
                } else {
                    0.0
                }
            );

            // Trade flow
            let mut buy_vol = 0.0f64;
            let mut sell_vol = 0.0f64;
            for t in &trades {
                let notional = t.price * t.qty;
                if t.is_buyer_maker {
                    sell_vol += notional;
                } else {
                    buy_vol += notional;
                }
            }
            let total = buy_vol + sell_vol;
            println!(
                "Trades: {} recent | buy ${:.0} ({:.0}%) | sell ${:.0} ({:.0}%)",
                trades.len(),
                buy_vol,
                if total > 0.0 { buy_vol / total * 100.0 } else { 0.0 },
                sell_vol,
                if total > 0.0 { sell_vol / total * 100.0 } else { 0.0 },
            );

            // Last 5 candles
            println!("\nLast 5 candles (1m):");
            for c in candles.iter().rev().take(5) {
                let ts = chrono::DateTime::from_timestamp_millis(c.close_time)
                    .map(|dt| dt.format("%H:%M").to_string())
                    .unwrap_or_default();
                let dir = if c.close >= c.open { "+" } else { "-" };
                println!(
                    "  {} {}{:.2}  O:{:.2} H:{:.2} L:{:.2} C:{:.2} V:{:.2}",
                    ts, dir,
                    (c.close - c.open).abs(),
                    c.open, c.high, c.low, c.close, c.volume
                );
            }
        }
    }

    Ok(())
}

async fn cmd_crypto_signal(asset_str: &str, output: &OutputFormat) -> Result<()> {
    let asset: crypto::CryptoAsset = asset_str.parse()?;
    let feed = crypto::feed::BinanceFeed::new();
    let futures_feed = crypto::feed::BinanceFuturesFeed::new();

    let (candles, depth, trades, futures) =
        tokio::join!(
            feed.fetch_klines(asset, "1m", 30),
            feed.fetch_depth(asset, 20),
            feed.fetch_trades(asset, 500),
            futures_feed.fetch_all(asset),
        );
    let candles = candles?;
    let depth = depth?;
    let trades = trades?;
    let futures_data = futures.ok();

    let signal = crypto::momentum::compute_signal_enhanced(
        asset, &candles, &depth, &trades, futures_data.as_ref(),
    );

    let has_futures = futures_data.is_some();

    match output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&signal)?);
        }
        _ => {
            let c = &signal.components;
            if has_futures {
                println!("--- {} Enhanced Signal (spot + futures) ---", asset);
            } else {
                println!("--- {} Momentum Signal (spot only) ---", asset);
            }
            println!("Direction: {}  (confidence: {:.0}%)", signal.direction, signal.confidence * 100.0);
            println!("Price: ${:.2}", signal.price);
            println!();
            println!("Spot components:");
            let (w1, w2, w3, w4) = if has_futures { (15.0, 10.0, 20.0, 20.0) } else { (30.0, 25.0, 25.0, 20.0) };
            println!("  Price mom 1m:  {:+.4}  (w={:.0}%)", c.price_mom_1m, w1);
            println!("  Price mom 5m:  {:+.4}  (w={:.0}%)", c.price_mom_5m, w2);
            println!("  OB imbalance:  {:+.4}  (w={:.0}%)", c.ob_imbalance, w3);
            println!("  Trade flow:    {:+.4}  (w={:.0}%)", c.trade_flow, w4);
            println!("  Volatility:    {:.4}   (threshold: 0.0030)", c.volatility);
            if has_futures {
                println!();
                println!("Futures components:");
                println!("  Funding sig:   {:+.4}  (w=15%)", c.funding_signal);
                println!("  OI delta sig:  {:+.4}  (w=10%)", c.oi_delta_signal);
                println!("  Liquidation:   {:+.4}  (w=10%)", c.liquidation_signal);
                if let Some(ref fd) = futures_data {
                    println!();
                    println!("Futures raw data:");
                    println!("  Funding rate:  {:.6} ({:.4}%)", fd.funding_rate, fd.funding_rate * 100.0);
                    println!("  Mark price:    ${:.2}", fd.mark_price);
                    println!("  Open interest: ${:.0}M", fd.open_interest_usd / 1_000_000.0);
                    println!("  Liquidations:  {} (last 5m)", fd.liquidations.iter()
                        .filter(|l| l.time > chrono::Utc::now().timestamp_millis() - 5 * 60 * 1000).count());
                }
            }
            println!();
            println!("  Raw score:     {:+.4}  (threshold: 0.10)", c.raw_score);
        }
    }

    Ok(())
}

async fn cmd_crypto_market(
    gamma_client: &polymarket_client_sdk::gamma::Client,
    asset_str: &str,
    output: &OutputFormat,
) -> Result<()> {
    let asset: crypto::CryptoAsset = asset_str.parse()?;
    let market = crypto::market::find_next_5m_market(gamma_client, asset).await?;

    match market {
        Some(m) => {
            match output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&m)?);
                }
                _ => {
                    let start = chrono::DateTime::from_timestamp_millis(m.start_time)
                        .map(|dt| dt.format("%H:%M:%S UTC").to_string())
                        .unwrap_or_default();
                    let end = chrono::DateTime::from_timestamp_millis(m.end_time)
                        .map(|dt| dt.format("%H:%M:%S UTC").to_string())
                        .unwrap_or_default();
                    let now = chrono::Utc::now().timestamp_millis();
                    let until = (m.start_time - now) / 1000;

                    println!("--- Next {} 5m Market ---", asset);
                    println!("Q: {}", m.question);
                    println!("Window: {} - {}", start, end);
                    if until > 0 {
                        println!("Starts in: {}m {}s", until / 60, until % 60);
                    } else {
                        println!("Status: IN PROGRESS");
                    }
                    println!("Token UP:   {}", m.token_id_up);
                    println!("Token DOWN: {}", m.token_id_down);
                    println!("Slug: {}", m.slug);
                }
            }
        }
        None => {
            println!("No upcoming {} 5-minute market found", asset);
        }
    }

    Ok(())
}

async fn cmd_crypto_backtest(asset_str: &str, hours: u32, output: &OutputFormat) -> Result<()> {
    let asset: crypto::CryptoAsset = asset_str.parse()?;
    let feed = crypto::feed::BinanceFeed::new();

    // Fetch historical 1m candles (max 1000 per request)
    let limit = (hours * 60).min(1000);
    println!("Fetching {}h of 1m candles for {}...", hours, asset);
    let candles = feed.fetch_klines(asset, "1m", limit).await?;
    println!("Got {} candles", candles.len());

    let result = crypto::momentum::backtest_signals(asset, &candles);

    match output {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({
                "asset": asset.to_string(),
                "candles": candles.len(),
                "total_windows": result.total_windows,
                "signals_generated": result.signals_generated,
                "correct": result.correct,
                "wrong": result.wrong,
                "win_rate": format!("{:.1}%", result.win_rate * 100.0),
                "skip_rate": format!("{:.1}%", result.skip_rate * 100.0),
            }));
        }
        _ => {
            println!("\n--- {} Backtest Results ({}h) ---", asset, hours);
            println!("Total 5m windows:   {}", result.total_windows);
            println!("Signals generated:  {} ({:.0}% of windows)",
                result.signals_generated,
                (1.0 - result.skip_rate) * 100.0
            );
            println!("Correct:            {}", result.correct);
            println!("Wrong:              {}", result.wrong);
            println!("Win rate:           {:.1}%", result.win_rate * 100.0);
            println!("Skip rate:          {:.1}%", result.skip_rate * 100.0);

            if !result.details.is_empty() {
                println!("\nLast 10 signals:");
                for entry in result.details.iter().rev().take(10) {
                    let ts = chrono::DateTime::from_timestamp_millis(entry.time)
                        .map(|dt| dt.format("%H:%M").to_string())
                        .unwrap_or_default();
                    let mark = if entry.correct { "OK" } else { "XX" };
                    println!(
                        "  {} [{}] pred={} actual={} conf={:.0}% open={:.2} close={:.2}",
                        ts, mark, entry.predicted, entry.actual,
                        entry.confidence * 100.0,
                        entry.window_open, entry.window_close,
                    );
                }

                // Win rate by confidence bucket
                let high: Vec<_> = result.details.iter().filter(|e| e.confidence >= 0.7).collect();
                let med: Vec<_> = result.details.iter().filter(|e| e.confidence >= 0.3 && e.confidence < 0.7).collect();
                let low: Vec<_> = result.details.iter().filter(|e| e.confidence < 0.3).collect();

                println!("\nWin rate by confidence:");
                for (label, bucket) in [("High (70%+)", &high), ("Med (30-70%)", &med), ("Low (<30%)", &low)] {
                    if bucket.is_empty() {
                        println!("  {}: no signals", label);
                    } else {
                        let correct = bucket.iter().filter(|e| e.correct).count();
                        println!("  {}: {}/{} ({:.1}%)", label, correct, bucket.len(),
                            correct as f64 / bucket.len() as f64 * 100.0);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn cmd_crypto_monitor(
    gamma_client: &polymarket_client_sdk::gamma::Client,
    asset_str: &str,
    amount: f64,
    max_per_hour: u32,
    max_per_day: f64,
    min_confidence: f64,
    notify: bool,
    _output: &OutputFormat,
) -> Result<()> {
    use crate::smart::TradeStatus;

    let assets: Vec<crypto::CryptoAsset> = if asset_str.eq_ignore_ascii_case("all") {
        vec![crypto::CryptoAsset::BTC, crypto::CryptoAsset::ETH]
    } else {
        vec![asset_str.parse()?]
    };

    let feed = crypto::feed::BinanceFeed::new();
    let futures_feed = crypto::feed::BinanceFuturesFeed::new();

    println!("=== Crypto 5m Monitor (Enhanced) ===");
    println!("  Assets:         {}", assets.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", "));
    println!("  Signal:         7-component (spot + futures)");
    println!("  Amount:         ${:.2}/trade", amount);
    println!("  Max/hour:       {}", max_per_hour);
    println!("  Max/day:        ${:.2}", max_per_day);
    println!("  Min confidence: {:.0}%", min_confidence * 100.0);
    println!("  Notifications:  {}", if notify { "ON" } else { "OFF" });
    println!("  Press Ctrl+C to stop.\n");

    let interval = std::time::Duration::from_secs(60);
    let mut timer = tokio::time::interval(interval);
    timer.tick().await;
    let mut cycle = 0u64;
    let mut trades_this_hour: Vec<chrono::DateTime<Utc>> = Vec::new();

    loop {
        tokio::select! {
            _ = timer.tick() => {
                cycle += 1;
                let now = Utc::now();
                let now_fmt = now.format("%H:%M:%S");

                trades_this_hour.retain(|t| (now - *t).num_seconds() < 3600);
                let daily_spent = crypto_daily_spend();

                eprint!("[{now_fmt}] #{cycle}: ");

                // Resolve expired positions
                let resolved = resolve_crypto_positions(&feed).await;
                if resolved > 0 {
                    eprint!("{resolved} resolved, ");
                }

                if trades_this_hour.len() >= max_per_hour as usize {
                    eprintln!("hourly limit ({}/{})", trades_this_hour.len(), max_per_hour);
                    continue;
                }
                if daily_spent >= max_per_day {
                    eprintln!("daily limit (${:.0}/${:.0})", daily_spent, max_per_day);
                    continue;
                }

                for &asset in &assets {
                    let (candles_res, depth_res, trades_res, futures_res) = tokio::join!(
                        feed.fetch_klines(asset, "1m", 30),
                        feed.fetch_depth(asset, 20),
                        feed.fetch_trades(asset, 500),
                        futures_feed.fetch_all(asset),
                    );

                    let (candles, depth, trades) = match (candles_res, depth_res, trades_res) {
                        (Ok(c), Ok(d), Ok(t)) => (c, d, t),
                        _ => { eprint!("{asset} err, "); continue; }
                    };
                    let futures_data = futures_res.ok();

                    let signal = crypto::momentum::compute_signal_enhanced(
                        asset, &candles, &depth, &trades, futures_data.as_ref(),
                    );

                    if signal.direction == crypto::Direction::Skip {
                        eprint!("{asset} SKIP, ");
                        continue;
                    }
                    if signal.confidence < min_confidence {
                        eprint!("{asset} {} {:.0}%<{:.0}%, ", signal.direction, signal.confidence * 100.0, min_confidence * 100.0);
                        continue;
                    }

                    let market = match crypto::market::find_next_5m_market(gamma_client, asset).await {
                        Ok(Some(m)) => m,
                        Ok(None) => { eprint!("{asset} {} {:.0}% no market, ", signal.direction, signal.confidence * 100.0); continue; }
                        Err(_) => { eprint!("{asset} search err, "); continue; }
                    };

                    // Skip if market window already ended
                    let secs_until_end = (market.end_time - now.timestamp_millis()) / 1000;
                    if secs_until_end < 0 {
                        eprint!("{asset} ended, ");
                        continue;
                    }

                    let existing = store::load_follow_records().unwrap_or_default();
                    let already = existing.iter().any(|r|
                        r.condition_id == market.condition_id
                        && r.entry_reason.as_deref().map(|e| e.starts_with("crypto:")).unwrap_or(false)
                    );
                    if already { eprint!("{asset} dup, "); continue; }

                    let (token_id, outcome) = match signal.direction {
                        crypto::Direction::Up => (&market.token_id_up, "Up"),
                        crypto::Direction::Down => (&market.token_id_down, "Down"),
                        _ => continue,
                    };

                    let entry_price = 0.50;
                    let record = FollowRecord {
                        timestamp: now,
                        signal_id: format!("crypto-{}-{}", asset, now.timestamp()),
                        market_title: market.question.clone(),
                        condition_id: market.condition_id.clone(),
                        asset: token_id.clone(),
                        outcome: outcome.to_string(),
                        side: "BUY".to_string(),
                        amount_usdc: amount,
                        price: entry_price,
                        dry_run: true,
                        order_id: None,
                        fill_price: Some(entry_price),
                        status: Some(TradeStatus::Open),
                        closed_at: None,
                        exit_price: None,
                        realized_pnl: None,
                        position_id: Some(format!("crypto:{}", market.condition_id)),
                        entry_reason: Some(format!("crypto:momentum:{} {} conf={:.0}% score={:+.3}",
                            asset, signal.direction, signal.confidence * 100.0, signal.components.raw_score)),
                        exit_reason: None,
                    };

                    if let Err(e) = store::append_follow_record(&record) {
                        eprint!("{asset} save err: {e}, ");
                        continue;
                    }

                    trades_this_hour.push(now);
                    let window_mins = secs_until_end / 60;
                    eprintln!("\n  TRADE: {asset} {} {} @ {:.2} (${:.2}) conf={:.0}% window={window_mins}m",
                        signal.direction, market.question, entry_price, amount, signal.confidence * 100.0);

                    if notify {
                        // macOS notification
                        let notif_msg = format!("Crypto {asset}: {} {outcome} @ {entry_price:.2} (${amount:.2}) conf={:.0}%",
                            signal.direction, signal.confidence * 100.0);
                        let _ = std::process::Command::new("osascript")
                            .args(["-e", &format!(
                                "display notification \"{}\" with title \"PMCC Crypto\" sound name \"Glass\"",
                                osascript_safe(&notif_msg)
                            )])
                            .output();

                        // Telegram
                        if let Ok(Some(tg)) = store::load_telegram_config() {
                            let text = format!("*PMCC Crypto Trade*\n{asset} {} {outcome} @ {entry_price:.2} (${amount:.2})\nConf: {:.0}%\n{}",
                                signal.direction, signal.confidence * 100.0, market.question);
                            let _ = send_telegram_message(&tg, &text).await;
                        }
                    }
                }

                eprintln!();
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nMonitor stopped.");
                break;
            }
        }
    }

    Ok(())
}

/// Resolve expired crypto 5m positions.
async fn resolve_crypto_positions(feed: &crypto::feed::BinanceFeed) -> u32 {
    let mut records = store::load_follow_records().unwrap_or_default();
    let now_ms = Utc::now().timestamp_millis();
    let mut resolved = 0u32;
    let mut changed = false;

    for r in &mut records {
        if !r.is_open() { continue; }
        let reason = match &r.entry_reason {
            Some(e) if e.starts_with("crypto:momentum:") => e.clone(),
            _ => continue,
        };

        let asset_str = reason
            .strip_prefix("crypto:momentum:")
            .and_then(|s| s.split_whitespace().next())
            .unwrap_or("BTC");
        let asset: crypto::CryptoAsset = match asset_str.parse() {
            Ok(a) => a,
            Err(_) => continue,
        };

        let end_time = match parse_crypto_end_time(&r.market_title) {
            Some(t) => t,
            None => r.timestamp.timestamp_millis() + 10 * 60 * 1000,
        };

        if now_ms < end_time + 30_000 { continue; }

        // Fetch recent candles to determine actual direction
        let candles = match feed.fetch_klines(asset, "1m", 10).await {
            Ok(c) => c,
            Err(_) => continue,
        };

        let window_start = end_time - 5 * 60 * 1000;
        let window_candles: Vec<_> = candles.iter()
            .filter(|c| c.open_time >= window_start - 60_000 && c.close_time <= end_time + 60_000)
            .collect();

        if window_candles.len() < 2 {
            r.status = Some(crate::smart::TradeStatus::Expired);
            r.closed_at = Some(Utc::now());
            r.exit_reason = Some("5m-expired: insufficient data".to_string());
            changed = true;
            resolved += 1;
            continue;
        }

        let open_price = window_candles.first().map(|c| c.open).unwrap_or(0.0);
        let close_price = window_candles.last().map(|c| c.close).unwrap_or(0.0);
        let actual_dir = if close_price > open_price { "Up" } else { "Down" };
        let won = r.outcome == actual_dir;

        if won {
            r.exit_price = Some(0.95);
            r.realized_pnl = Some(r.amount_usdc * (0.95 / 0.50 - 1.0));
        } else {
            r.exit_price = Some(0.05);
            r.realized_pnl = Some(r.amount_usdc * (0.05 / 0.50 - 1.0));
        }

        r.status = Some(crate::smart::TradeStatus::Closed);
        r.closed_at = Some(Utc::now());
        r.exit_reason = Some(format!("5m-resolved: actual={actual_dir} {open_price:.2}->{close_price:.2} {}",
            if won { "WIN" } else { "LOSS" }));
        changed = true;
        resolved += 1;
    }

    if changed {
        if let Err(e) = store::save_follow_records(&records) {
            eprintln!("warn: failed to save follow records: {e}");
        }
    }
    resolved
}

/// Parse end time from crypto market title.
fn parse_crypto_end_time(title: &str) -> Option<i64> {
    let time_part = title.split(" - ").nth(1)?;
    let time_part = time_part.trim().trim_end_matches(" ET").trim();
    let comma_pos = time_part.find(',')?;
    let date_str = time_part[..comma_pos].trim();
    let time_range = time_part[comma_pos + 1..].trim().replace(' ', "");
    let parts: Vec<&str> = time_range.splitn(2, '-').collect();
    if parts.len() != 2 { return None; }

    use chrono::Datelike;
    let year = chrono::Utc::now().year();
    let end_str = parts[1];
    let time_upper = end_str.to_uppercase();
    let is_pm = time_upper.contains("PM");
    let digits = time_upper.trim_end_matches("AM").trim_end_matches("PM");
    let tp: Vec<&str> = digits.split(':').collect();
    if tp.len() != 2 { return None; }
    let mut hour: u32 = tp[0].parse().ok()?;
    let min: u32 = tp[1].parse().ok()?;
    if hour == 12 { hour = if is_pm { 12 } else { 0 }; } else if is_pm { hour += 12; }

    let month_str = date_str.split_whitespace().next()?;
    let day_str = date_str.split_whitespace().nth(1)?;
    let month = match month_str.to_lowercase().as_str() {
        "january" | "jan" => 1u32, "february" | "feb" => 2, "march" | "mar" => 3,
        "april" | "apr" => 4, "may" => 5, "june" | "jun" => 6,
        "july" | "jul" => 7, "august" | "aug" => 8, "september" | "sep" => 9,
        "october" | "oct" => 10, "november" | "nov" => 11, "december" | "dec" => 12,
        _ => return None,
    };
    let day: u32 = day_str.parse().ok()?;

    use chrono::TimeZone;
    let date = chrono::NaiveDate::from_ymd_opt(year, month, day)?;
    let time = chrono::NaiveTime::from_hms_opt(hour, min, 0)?;
    let et_offset = if month >= 3 && month <= 11 {
        chrono::FixedOffset::west_opt(4 * 3600)?
    } else {
        chrono::FixedOffset::west_opt(5 * 3600)?
    };
    let et_dt = et_offset.from_local_datetime(&date.and_time(time)).single()?;
    Some(et_dt.timestamp_millis())
}

/// Calculate total USDC spent on crypto trades today.
fn crypto_daily_spend() -> f64 {
    let records = store::load_follow_records().unwrap_or_default();
    let today = Utc::now().date_naive();
    records.iter()
        .filter(|r| r.entry_reason.as_deref().map(|e| e.starts_with("crypto:")).unwrap_or(false)
            && r.timestamp.date_naive() == today)
        .map(|r| r.amount_usdc)
        .sum()
}

fn cmd_crypto_status(output: &OutputFormat) -> Result<()> {
    let records = store::load_follow_records().unwrap_or_default();
    let crypto_trades: Vec<&FollowRecord> = records.iter()
        .filter(|r| r.entry_reason.as_deref().map(|e| e.starts_with("crypto:")).unwrap_or(false))
        .collect();

    if crypto_trades.is_empty() {
        println!("No crypto paper trades yet. Run: polymarket smart crypto monitor");
        return Ok(());
    }

    let open: Vec<_> = crypto_trades.iter().filter(|r| r.is_open()).collect();
    let closed: Vec<_> = crypto_trades.iter().filter(|r| !r.is_open()).collect();
    let wins = closed.iter().filter(|r| r.realized_pnl.unwrap_or(0.0) > 0.0).count();
    let losses = closed.len() - wins;
    let total_pnl: f64 = closed.iter().map(|r| r.realized_pnl.unwrap_or(0.0)).sum();
    let total_spent: f64 = crypto_trades.iter().map(|r| r.amount_usdc).sum();
    let win_rate = if !closed.is_empty() { wins as f64 / closed.len() as f64 * 100.0 } else { 0.0 };

    match output {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({
                "total_trades": crypto_trades.len(),
                "open": open.len(),
                "closed": closed.len(),
                "wins": wins,
                "losses": losses,
                "win_rate": format!("{:.1}%", win_rate),
                "total_pnl": format!("${:.2}", total_pnl),
                "total_spent": format!("${:.2}", total_spent),
            }));
        }
        _ => {
            println!("--- Crypto 5m Paper Trading ---");
            println!("Total: {}  (open: {}, closed: {})", crypto_trades.len(), open.len(), closed.len());
            println!("W/L: {}/{} ({:.1}%)", wins, losses, win_rate);
            println!("PnL: ${:.2}  Spent: ${:.2}  ROI: {:.1}%",
                total_pnl, total_spent,
                if total_spent > 0.0 { total_pnl / total_spent * 100.0 } else { 0.0 });

            if !open.is_empty() {
                println!("\nOpen:");
                for r in &open {
                    let age = (Utc::now() - r.timestamp).num_seconds();
                    println!("  {} {} @ {:.2} ${:.2} ({}s) | {}",
                        r.outcome, r.market_title, r.price, r.amount_usdc, age,
                        r.entry_reason.as_deref().unwrap_or("?"));
                }
            }

            if !closed.is_empty() {
                println!("\nLast 10:");
                for r in closed.iter().rev().take(10) {
                    let pnl = r.realized_pnl.unwrap_or(0.0);
                    println!("  {} {:+.2} | {} | {}",
                        r.outcome, pnl, r.market_title,
                        r.exit_reason.as_deref().unwrap_or("?"));
                }
            }
        }
    }

    Ok(())
}
