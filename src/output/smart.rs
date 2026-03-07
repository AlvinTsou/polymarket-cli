use anyhow::Result;
use serde_json::json;
use tabled::settings::Style;
use tabled::{Table, Tabled};

use super::{OutputFormat, truncate};
use crate::commands::smart::ScanSummary;
use crate::smart::tracker::ChangeType;
use crate::smart::{AggregatedSignal, Signal, SmartScore, WatchedWallet};

// ── Discover ─────────────────────────────────────────────────────

pub fn print_discover_results(scores: &[SmartScore], output: &OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Table => {
            if scores.is_empty() {
                println!("No leaderboard entries found.");
                return Ok(());
            }
            #[derive(Tabled)]
            struct Row {
                #[tabled(rename = "#")]
                rank: String,
                #[tabled(rename = "Address")]
                address: String,
                #[tabled(rename = "Name")]
                name: String,
                #[tabled(rename = "Score")]
                score: String,
                #[tabled(rename = "PnL")]
                pnl: String,
                #[tabled(rename = "Volume")]
                volume: String,
            }
            let rows: Vec<Row> = scores
                .iter()
                .map(|s| Row {
                    rank: s.rank.map_or("—".into(), |r| r.to_string()),
                    address: truncate(&s.address, 14),
                    name: s.name.as_deref().unwrap_or("—").to_string(),
                    score: format!("{:.1}", s.score),
                    pnl: format_money(&s.pnl),
                    volume: format_money(&s.volume),
                })
                .collect();
            let table = Table::new(rows).with(Style::rounded()).to_string();
            println!("{table}");
        }
        OutputFormat::Json => super::print_json(&scores)?,
    }
    Ok(())
}

// ── Wallet list ──────────────────────────────────────────────────

pub fn print_wallet_list(wallets: &[WatchedWallet], output: &OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Table => {
            if wallets.is_empty() {
                println!("No wallets being watched. Use `polymarket smart watch <address>`.");
                return Ok(());
            }
            #[derive(Tabled)]
            struct Row {
                #[tabled(rename = "Address")]
                address: String,
                #[tabled(rename = "Tag")]
                tag: String,
                #[tabled(rename = "Score")]
                score: String,
                #[tabled(rename = "Added")]
                added: String,
            }
            let rows: Vec<Row> = wallets
                .iter()
                .map(|w| Row {
                    address: truncate(&w.address, 14),
                    tag: w.tag.as_deref().unwrap_or("—").to_string(),
                    score: w
                        .score
                        .map_or("—".into(), |s| format!("{s:.1}")),
                    added: w.added_at.format("%Y-%m-%d").to_string(),
                })
                .collect();
            let table = Table::new(rows).with(Style::rounded()).to_string();
            println!("{table}");
        }
        OutputFormat::Json => super::print_json(&wallets)?,
    }
    Ok(())
}

// ── Scan results ─────────────────────────────────────────────────

pub fn print_scan_result(
    summaries: &[ScanSummary],
    all_signals: &[Signal],
    aggregated: &[AggregatedSignal],
    output: &OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Table => {
            // Summary table
            #[derive(Tabled)]
            struct SRow {
                #[tabled(rename = "Wallet")]
                address: String,
                #[tabled(rename = "Tag")]
                tag: String,
                #[tabled(rename = "Positions")]
                positions: String,
                #[tabled(rename = "Changes")]
                changes: String,
            }
            let rows: Vec<SRow> = summaries
                .iter()
                .map(|s| SRow {
                    address: truncate(&s.address, 14),
                    tag: s.tag.as_deref().unwrap_or("—").to_string(),
                    positions: s.positions.to_string(),
                    changes: s.changes.to_string(),
                })
                .collect();
            println!("--- Scan Summary ({} wallet(s)) ---", summaries.len());
            let table = Table::new(rows).with(Style::rounded()).to_string();
            println!("{table}");

            // Signal details
            if all_signals.is_empty() {
                println!("\nNo new signals detected.");
            } else {
                println!("\n--- Signals ({}) ---", all_signals.len());
                print_signals_table(all_signals);
            }

            // Aggregated signals (multi-wallet convergence)
            if !aggregated.is_empty() {
                println!("\n--- Convergence ({} group(s)) ---", aggregated.len());
                print_aggregated_table(aggregated);
            }

            // Per-wallet change details
            for summary in summaries {
                if summary.change_details.is_empty() {
                    continue;
                }
                println!(
                    "\n[{}] Changes:",
                    truncate(&summary.address, 14)
                );
                for change in &summary.change_details {
                    let arrow = match change.change_type {
                        ChangeType::New => "+  NEW",
                        ChangeType::Closed => "-  CLOSE",
                        ChangeType::Increased => "^  INCREASE",
                        ChangeType::Decreased => "v  DECREASE",
                    };
                    let size_info = match &change.previous {
                        Some(prev) => format!("{} -> {}", prev.size, change.position.size),
                        None => change.position.size.clone(),
                    };
                    println!(
                        "  {arrow}  {title} [{outcome}]  size: {size}  @ {price}",
                        title = truncate(&change.position.title, 40),
                        outcome = change.position.outcome,
                        size = size_info,
                        price = change.position.cur_price,
                    );
                }
            }
        }
        OutputFormat::Json => {
            let data = json!({
                "wallets_scanned": summaries.len(),
                "total_changes": summaries.iter().map(|s| s.changes).sum::<usize>(),
                "signals": all_signals,
                "aggregated": aggregated,
                "summaries": summaries.iter().map(|s| json!({
                    "address": s.address,
                    "tag": s.tag,
                    "positions": s.positions,
                    "changes": s.changes,
                })).collect::<Vec<_>>(),
            });
            super::print_json(&data)?;
        }
    }
    Ok(())
}

// ── Signals ──────────────────────────────────────────────────────

pub fn print_signals(signals: &[Signal], output: &OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Table => {
            if signals.is_empty() {
                println!("No signals yet. Run `polymarket smart scan` first.");
                return Ok(());
            }
            println!("--- Recent Signals ({}) ---", signals.len());
            print_signals_table(signals);
        }
        OutputFormat::Json => super::print_json(&signals)?,
    }
    Ok(())
}

fn print_signals_table(signals: &[Signal]) {
    #[derive(Tabled)]
    struct Row {
        #[tabled(rename = "Time")]
        time: String,
        #[tabled(rename = "Type")]
        signal_type: String,
        #[tabled(rename = "Conf")]
        confidence: String,
        #[tabled(rename = "Wallet")]
        wallet: String,
        #[tabled(rename = "Market")]
        market: String,
        #[tabled(rename = "Outcome")]
        outcome: String,
        #[tabled(rename = "Size")]
        size: String,
        #[tabled(rename = "Price")]
        price: String,
    }
    let rows: Vec<Row> = signals
        .iter()
        .map(|s| Row {
            time: s.timestamp.format("%m-%d %H:%M").to_string(),
            signal_type: s.signal_type.to_string(),
            confidence: s.confidence.to_string(),
            wallet: s
                .wallet_tag
                .as_deref()
                .unwrap_or_else(|| &s.wallet)
                .to_string(),
            market: truncate(&s.market_title, 30),
            outcome: s.outcome.clone(),
            size: s.size.clone(),
            price: s.price.clone(),
        })
        .collect();
    let table = Table::new(rows).with(Style::rounded()).to_string();
    println!("{table}");
}

// ── Aggregated signals ──────────────────────────────────────────

fn print_aggregated_table(aggregated: &[AggregatedSignal]) {
    #[derive(Tabled)]
    struct Row {
        #[tabled(rename = "Dir")]
        direction: String,
        #[tabled(rename = "Conf")]
        confidence: String,
        #[tabled(rename = "Wallets")]
        wallet_count: String,
        #[tabled(rename = "Market")]
        market: String,
        #[tabled(rename = "Outcome")]
        outcome: String,
        #[tabled(rename = "Total Size")]
        total_size: String,
        #[tabled(rename = "Avg Price")]
        avg_price: String,
        #[tabled(rename = "Who")]
        who: String,
    }
    let rows: Vec<Row> = aggregated
        .iter()
        .map(|a| Row {
            direction: a.direction.to_string(),
            confidence: a.confidence.to_string(),
            wallet_count: a.wallet_count.to_string(),
            market: truncate(&a.market_title, 30),
            outcome: a.outcome.clone(),
            total_size: format!("{:.1}", a.total_size),
            avg_price: format!("{:.2}", a.avg_price),
            who: a
                .wallets
                .iter()
                .map(|w| truncate(w, 10))
                .collect::<Vec<_>>()
                .join(", "),
        })
        .collect();
    let table = Table::new(rows).with(Style::rounded()).to_string();
    println!("{table}");
}

// ── Profile ──────────────────────────────────────────────────────

pub fn print_profile(score: &SmartScore, is_watched: bool, output: &OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Table => {
            println!("=== Wallet Profile ===");
            println!("Address:        {}", score.address);
            if let Some(name) = &score.name {
                println!("Name:           {name}");
            }
            println!("Score:          {:.1}/100", score.score);
            println!("PnL:            {}", format_money(&score.pnl));
            println!("Portfolio:      {}", format_money(&score.volume));
            println!("Positions:      {}", score.positions_count);
            println!("Markets traded: {}", score.markets_traded);
            println!(
                "Watched:        {}",
                if is_watched { "Yes" } else { "No" }
            );
        }
        OutputFormat::Json => {
            let data = json!({
                "address": score.address,
                "name": score.name,
                "score": score.score,
                "pnl": score.pnl,
                "volume": score.volume,
                "positions_count": score.positions_count,
                "markets_traded": score.markets_traded,
                "is_watched": is_watched,
                "updated_at": score.updated_at.to_rfc3339(),
            });
            super::print_json(&data)?;
        }
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────

fn format_money(s: &str) -> String {
    let v: f64 = s.parse().unwrap_or(0.0);
    if v.abs() >= 1_000_000.0 {
        format!("${:.1}M", v / 1_000_000.0)
    } else if v.abs() >= 1_000.0 {
        format!("${:.1}K", v / 1_000.0)
    } else {
        format!("${v:.2}")
    }
}
