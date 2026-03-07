use std::io::{self, BufRead, Write};

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config;
use crate::notify::NotifyChannel;
use crate::output::OutputFormat;
use crate::{gemini, storage};

#[derive(Args)]
pub struct DigestArgs {
    #[command(subcommand)]
    pub command: DigestCommand,
}

#[derive(Subcommand)]
pub enum DigestCommand {
    /// Generate and send the daily digest via configured channels
    Send {
        /// Notification channel
        #[arg(long, default_value = "all")]
        channel: NotifyChannel,
        /// Include articles since this date (YYYY-MM-DD). Defaults to yesterday.
        #[arg(long)]
        since: Option<String>,
    },
    /// Preview the digest without sending
    Preview {
        /// Include articles since this date (YYYY-MM-DD). Defaults to yesterday.
        #[arg(long)]
        since: Option<String>,
    },
    /// Interactive setup for digest notifications
    Setup,
}

pub async fn execute(args: DigestArgs, output: OutputFormat) -> Result<()> {
    match args.command {
        DigestCommand::Send { channel, since } => cmd_send(channel, since, output).await,
        DigestCommand::Preview { since } => cmd_preview(since, output),
        DigestCommand::Setup => cmd_setup(),
    }
}

fn resolve_since(since: Option<String>) -> String {
    since.unwrap_or_else(|| {
        let yesterday = chrono::Local::now() - chrono::Duration::days(1);
        yesterday.format("%Y-%m-%dT00:00:00Z").to_string()
    })
}

async fn ensure_all_summarized(conn: &rusqlite::Connection) -> Result<()> {
    let unsummarized = storage::get_unsummarized(conn)?;
    if unsummarized.is_empty() {
        return Ok(());
    }

    let cfg = config::load_config_or_default();
    let api_key = match cfg.gemini_api_key.as_deref() {
        Some(key) => key,
        None => {
            eprintln!(
                "Warning: {} unsummarized article(s) but no Gemini API key configured.",
                unsummarized.len()
            );
            return Ok(());
        }
    };

    for article in &unsummarized {
        match gemini::summarize(api_key, &article.raw_content).await {
            Ok(summary) => {
                storage::update_summary(conn, article.id, &summary)?;
            }
            Err(e) => {
                eprintln!("Warning: Failed to summarize id {}: {e}", article.id);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    Ok(())
}

async fn cmd_send(
    channel: NotifyChannel,
    since: Option<String>,
    output: OutputFormat,
) -> Result<()> {
    let since = resolve_since(since);
    let conn = storage::open_db()?;

    ensure_all_summarized(&conn).await?;

    let articles = storage::get_articles_since(&conn, &since)?;

    if articles.is_empty() {
        match output {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::json!({"sent": false, "reason": "no articles"})
                );
            }
            OutputFormat::Table => {
                println!("No articles found since {since}. Nothing to send.");
            }
        }
        return Ok(());
    }

    let digest = crate::output::digest::format_digest(&articles);
    let cfg = config::load_config_or_default();
    crate::notify::send_digest(&cfg, channel, &digest).await?;

    match output {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({"sent": true, "articles": articles.len()})
            );
        }
        OutputFormat::Table => {} // send_digest already prints confirmation
    }
    Ok(())
}

fn cmd_preview(since: Option<String>, output: OutputFormat) -> Result<()> {
    let since = resolve_since(since);
    let conn = storage::open_db()?;
    let articles = storage::get_articles_since(&conn, &since)?;

    match output {
        OutputFormat::Json => crate::output::print_json(&articles)?,
        OutputFormat::Table => crate::output::digest::print_digest_preview(&articles),
    }
    Ok(())
}

fn prompt(msg: &str) -> Result<String> {
    print!("{msg}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_optional(msg: &str, current: Option<&str>) -> Result<Option<String>> {
    let hint = match current {
        Some(v) => format!(" [current: {}]", mask_secret(v)),
        None => " [not set]".to_string(),
    };
    let input = prompt(&format!("{msg}{hint}: "))?;
    if input.is_empty() {
        Ok(current.map(String::from))
    } else if input == "-" {
        Ok(None)
    } else {
        Ok(Some(input))
    }
}

fn mask_secret(s: &str) -> String {
    if s.len() <= 8 {
        return "****".to_string();
    }
    format!("{}...{}", &s[..4], &s[s.len() - 4..])
}

fn cmd_setup() -> Result<()> {
    println!("  Digest Setup");
    println!("  ════════════");
    println!();
    println!("  Configure API keys and notification channels.");
    println!("  Press Enter to keep current value. Enter '-' to clear a field.");
    println!();

    let mut cfg = config::load_config_or_default();

    // Step 1: Gemini API Key
    println!("  [1/3] Google Gemini API Key");
    println!("  ──────────────────────────");
    println!("  Get your key at: https://aistudio.google.com/apikey");
    cfg.gemini_api_key = prompt_optional("  Gemini API key", cfg.gemini_api_key.as_deref())?;
    println!();

    // Step 2: Telegram
    println!("  [2/3] Telegram Notification");
    println!("  ──────────────────────────");
    println!("  Create a bot via @BotFather on Telegram to get a token.");
    println!("  Send a message to your bot, then use the Telegram API to get your chat_id.");
    cfg.telegram_bot_token =
        prompt_optional("  Telegram bot token", cfg.telegram_bot_token.as_deref())?;
    cfg.telegram_chat_id = prompt_optional("  Telegram chat ID", cfg.telegram_chat_id.as_deref())?;
    println!();

    // Step 3: Email
    println!("  [3/3] Email Notification");
    println!("  ──────────────────────");
    println!("  SMTP settings for sending digest emails.");
    cfg.smtp_host = prompt_optional(
        "  SMTP host (e.g. smtp.gmail.com)",
        cfg.smtp_host.as_deref(),
    )?;
    cfg.smtp_username = prompt_optional("  SMTP username", cfg.smtp_username.as_deref())?;
    cfg.smtp_password = prompt_optional("  SMTP password", cfg.smtp_password.as_deref())?;
    cfg.email_from = prompt_optional("  From email", cfg.email_from.as_deref())?;
    cfg.email_to = prompt_optional("  To email", cfg.email_to.as_deref())?;
    println!();

    // Save
    if cfg.private_key.is_empty() {
        // No wallet configured yet, set a placeholder chain_id
        cfg.chain_id = 137;
    }
    config::save_config(&cfg)?;

    println!(
        "  ✓ Configuration saved to {}",
        config::config_path()?.display()
    );
    println!();

    // Print cron example
    println!("  To receive a daily digest, add this to your crontab (crontab -e):");
    println!();
    println!("  0 8 * * * polymarket digest send --channel all >> ~/.polymarket-digest.log 2>&1");
    println!();
    println!("  This will send the digest every day at 8:00 AM.");
    println!();

    Ok(())
}
