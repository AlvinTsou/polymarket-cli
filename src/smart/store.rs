use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use super::{
    FollowRecord, MonitorConfig, OddsAlert, OddsWatch, Signal, SmartScore, TelegramConfig,
    WalletPnlSnapshot, WalletSnapshot, WatchedWallet,
};

fn smart_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    let dir = home.join(".config").join("polymarket").join("smart");
    fs::create_dir_all(&dir).context("Failed to create smart data directory")?;
    Ok(dir)
}

fn snapshots_dir() -> Result<PathBuf> {
    let dir = smart_dir()?.join("snapshots");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn sanitize_address(addr: &str) -> String {
    addr.to_lowercase()
        .replace("0x", "")
        .replace("0X", "")
}

// ── Watched wallets ──────────────────────────────────────────────

pub fn load_wallets() -> Result<Vec<WatchedWallet>> {
    let path = smart_dir()?.join("wallets.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn save_wallets(wallets: &[WatchedWallet]) -> Result<()> {
    let path = smart_dir()?.join("wallets.json");
    let json = serde_json::to_string_pretty(wallets)?;
    fs::write(&path, json)?;
    Ok(())
}

pub fn add_wallet(wallet: WatchedWallet) -> Result<bool> {
    let mut wallets = load_wallets()?;
    let normalized = wallet.address.to_lowercase();
    if wallets
        .iter()
        .any(|w| w.address.to_lowercase() == normalized)
    {
        return Ok(false);
    }
    wallets.push(wallet);
    save_wallets(&wallets)?;
    Ok(true)
}

pub fn remove_wallet(address: &str) -> Result<bool> {
    let mut wallets = load_wallets()?;
    let normalized = address.to_lowercase();
    let before = wallets.len();
    wallets.retain(|w| w.address.to_lowercase() != normalized);
    if wallets.len() == before {
        return Ok(false);
    }
    save_wallets(&wallets)?;
    Ok(true)
}

// ── Snapshots ────────────────────────────────────────────────────

pub fn load_snapshot(address: &str) -> Result<Option<WalletSnapshot>> {
    let file = snapshots_dir()?.join(format!("{}.json", sanitize_address(address)));
    if !file.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&file)?;
    Ok(Some(serde_json::from_str(&data)?))
}

pub fn save_snapshot(snapshot: &WalletSnapshot) -> Result<()> {
    let file =
        snapshots_dir()?.join(format!("{}.json", sanitize_address(&snapshot.address)));
    let json = serde_json::to_string_pretty(snapshot)?;
    fs::write(&file, json)?;
    Ok(())
}

// ── Signals (append-only JSONL) ──────────────────────────────────

pub fn append_signals(signals: &[Signal]) -> Result<()> {
    use std::io::Write;
    if signals.is_empty() {
        return Ok(());
    }
    let path = smart_dir()?.join("signals.jsonl");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    for signal in signals {
        writeln!(file, "{}", serde_json::to_string(signal)?)?;
    }
    Ok(())
}

pub fn load_signals(limit: usize) -> Result<Vec<Signal>> {
    let path = smart_dir()?.join("signals.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)?;
    let mut signals: Vec<Signal> = data
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    signals.reverse(); // newest first
    signals.truncate(limit);
    Ok(signals)
}

// ── Scores cache ─────────────────────────────────────────────────

pub fn load_scores() -> Result<Vec<SmartScore>> {
    let path = smart_dir()?.join("scores.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn save_scores(scores: &[SmartScore]) -> Result<()> {
    let path = smart_dir()?.join("scores.json");
    let json = serde_json::to_string_pretty(scores)?;
    fs::write(&path, json)?;
    Ok(())
}

// ── Telegram config ─────────────────────────────────────────────

pub fn load_telegram_config() -> Result<Option<TelegramConfig>> {
    let path = smart_dir()?.join("telegram.json");
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&data)?))
}

pub fn save_telegram_config(config: &TelegramConfig) -> Result<()> {
    let path = smart_dir()?.join("telegram.json");
    let json = serde_json::to_string_pretty(config)?;
    fs::write(&path, json)?;
    Ok(())
}

// ── Follow records (append-only JSONL) ──────────────────────────

pub fn append_follow_record(record: &FollowRecord) -> Result<()> {
    use std::io::Write;
    let path = smart_dir()?.join("follows.jsonl");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{}", serde_json::to_string(record)?)?;
    Ok(())
}

pub fn load_follow_records() -> Result<Vec<FollowRecord>> {
    let path = smart_dir()?.join("follows.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)?;
    Ok(data
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect())
}

/// Rewrite all follow records (used when closing positions).
pub fn save_follow_records(records: &[FollowRecord]) -> Result<()> {
    use std::io::Write;
    let path = smart_dir()?.join("follows.jsonl");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;
    for record in records {
        writeln!(file, "{}", serde_json::to_string(record)?)?;
    }
    Ok(())
}

/// Close a matching open follow record by condition_id + outcome.
/// Returns true if a record was closed.
pub fn close_follow_position(
    condition_id: &str,
    outcome: &str,
    exit_price: f64,
) -> Result<bool> {
    let mut records = load_follow_records()?;
    let now = chrono::Utc::now();
    let mut closed = false;

    for r in records.iter_mut() {
        if r.condition_id == condition_id
            && r.outcome == outcome
            && r.side == "BUY"
            && r.is_open()
        {
            let entry = r.effective_entry();
            let shares = if entry > 0.0 { r.amount_usdc / entry } else { 0.0 };
            let pnl = shares * exit_price - r.amount_usdc;

            r.status = Some(super::TradeStatus::Closed);
            r.closed_at = Some(now);
            r.exit_price = Some(exit_price);
            r.realized_pnl = Some(pnl);
            closed = true;
            break; // close oldest matching first
        }
    }

    if closed {
        save_follow_records(&records)?;
    }
    Ok(closed)
}

/// Load all snapshots (all watched wallets).
pub fn load_all_snapshots() -> Result<Vec<WalletSnapshot>> {
    let dir = snapshots_dir()?;
    let mut snapshots = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|e| e == "json") {
            let data = fs::read_to_string(entry.path())?;
            if let Ok(snap) = serde_json::from_str::<WalletSnapshot>(&data) {
                snapshots.push(snap);
            }
        }
    }
    Ok(snapshots)
}

/// Build a map of (condition_id) -> cur_price from all snapshots.
pub fn current_price_map() -> Result<std::collections::HashMap<String, f64>> {
    let snapshots = load_all_snapshots()?;
    let mut map = std::collections::HashMap::new();
    for snap in &snapshots {
        for pos in &snap.positions {
            if let Ok(price) = pos.cur_price.parse::<f64>() {
                map.insert(pos.condition_id.clone(), price);
            }
        }
    }
    Ok(map)
}

/// Sum of USDC spent today (for daily limit).
pub fn today_spend() -> Result<f64> {
    let records = load_follow_records()?;
    let today = chrono::Utc::now().date_naive();
    Ok(records
        .iter()
        .filter(|r| !r.dry_run && r.timestamp.date_naive() == today)
        .map(|r| r.amount_usdc)
        .sum())
}

// ── Odds watches ────────────────────────────────────────────────

pub fn load_odds_watches() -> Result<Vec<OddsWatch>> {
    let path = smart_dir()?.join("odds.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn save_odds_watches(watches: &[OddsWatch]) -> Result<()> {
    let path = smart_dir()?.join("odds.json");
    let json = serde_json::to_string_pretty(watches)?;
    fs::write(&path, json)?;
    Ok(())
}

pub fn add_odds_watch(watch: OddsWatch) -> Result<bool> {
    let mut watches = load_odds_watches()?;
    if watches.iter().any(|w| w.token_id == watch.token_id) {
        return Ok(false);
    }
    watches.push(watch);
    save_odds_watches(&watches)?;
    Ok(true)
}

pub fn remove_odds_watch(token_id: &str) -> Result<bool> {
    let mut watches = load_odds_watches()?;
    let before = watches.len();
    watches.retain(|w| w.token_id != token_id);
    if watches.len() == before {
        return Ok(false);
    }
    save_odds_watches(&watches)?;
    Ok(true)
}

// ── Odds alerts (append-only JSONL) ─────────────────────────────

pub fn append_odds_alerts(alerts: &[OddsAlert]) -> Result<()> {
    use std::io::Write;
    if alerts.is_empty() {
        return Ok(());
    }
    let path = smart_dir()?.join("odds_alerts.jsonl");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    for alert in alerts {
        writeln!(file, "{}", serde_json::to_string(alert)?)?;
    }
    Ok(())
}

// ── Monitor config ───────────────────────────────────────────────

pub fn load_monitor_config() -> Result<Option<MonitorConfig>> {
    let path = smart_dir()?.join("monitor.json");
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&data)?))
}

pub fn save_monitor_config(config: &MonitorConfig) -> Result<()> {
    let path = smart_dir()?.join("monitor.json");
    let json = serde_json::to_string_pretty(config)?;
    fs::write(&path, json)?;
    Ok(())
}

pub fn load_odds_alerts(limit: usize) -> Result<Vec<OddsAlert>> {
    let path = smart_dir()?.join("odds_alerts.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)?;
    let mut alerts: Vec<OddsAlert> = data
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    alerts.reverse();
    alerts.truncate(limit);
    Ok(alerts)
}

// ── PnL history ──────────────────────────────────────────────────

fn pnl_history_dir() -> Result<PathBuf> {
    let dir = smart_dir()?.join("pnl_history");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn append_pnl_snapshot(address: &str, snapshot: &WalletPnlSnapshot) -> Result<()> {
    use std::io::Write;
    let file_path = pnl_history_dir()?.join(format!("{}.jsonl", sanitize_address(address)));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)?;
    writeln!(file, "{}", serde_json::to_string(snapshot)?)?;
    Ok(())
}

pub fn load_pnl_history(address: &str) -> Result<Vec<WalletPnlSnapshot>> {
    let file_path = pnl_history_dir()?.join(format!("{}.jsonl", sanitize_address(address)));
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&file_path)?;
    Ok(data
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect())
}

/// Update a wallet in the watch list (by address match).
pub fn update_wallet(address: &str, updater: impl FnOnce(&mut WatchedWallet)) -> Result<bool> {
    let mut wallets = load_wallets()?;
    let normalized = address.to_lowercase();
    let mut found = false;
    for w in wallets.iter_mut() {
        if w.address.to_lowercase() == normalized {
            updater(w);
            found = true;
            break;
        }
    }
    if found {
        save_wallets(&wallets)?;
    }
    Ok(found)
}
