use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use super::{FollowRecord, Signal, SmartScore, TelegramConfig, WalletSnapshot, WatchedWallet};

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
