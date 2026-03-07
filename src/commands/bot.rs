use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use teloxide::prelude::*;
use teloxide::types::Me;

use crate::commands::article;
use crate::config;

#[derive(Args)]
pub struct BotArgs {
    #[command(subcommand)]
    pub command: BotCommand,
}

#[derive(Subcommand)]
pub enum BotCommand {
    /// Start the Telegram bot (long-polling, runs in foreground)
    Start,
}

pub async fn execute(args: BotArgs) -> Result<()> {
    match args.command {
        BotCommand::Start => cmd_start().await,
    }
}

async fn cmd_start() -> Result<()> {
    let cfg = config::load_config_or_default();
    let token = cfg
        .telegram_bot_token
        .context("telegram_bot_token not configured. Run `polymarket digest setup`")?;

    let bot = Bot::new(&token);

    // Verify bot identity
    let me: Me = bot
        .get_me()
        .await
        .context("Failed to connect to Telegram. Check your bot token.")?;
    println!("Bot started: @{} ({})", me.username(), me.user.first_name);
    println!("Send a URL to the bot to save and summarize articles.");
    println!("Press Ctrl+C to stop.");
    println!();

    teloxide::repl(bot, |bot: Bot, msg: Message| async move {
        if let Some(text) = msg.text() {
            handle_message(&bot, &msg, text).await;
        }
        Ok(())
    })
    .await;

    Ok(())
}

async fn handle_message(bot: &Bot, msg: &Message, text: &str) {
    // Check if the text contains a URL
    let urls: Vec<&str> = text
        .split_whitespace()
        .filter(|w| url::Url::parse(w).is_ok())
        .collect();

    if urls.is_empty() {
        // Handle commands
        match text.trim() {
            "/start" => {
                let _ = bot
                    .send_message(
                        msg.chat.id,
                        "👋 Welcome! Send me any article URL and I'll save and summarize it for you.\n\nCommands:\n/list - Show recent articles\n/digest - Generate digest now",
                    )
                    .await;
            }
            "/list" => {
                handle_list(bot, msg).await;
            }
            "/digest" => {
                handle_digest(bot, msg).await;
            }
            _ => {
                let _ = bot
                    .send_message(
                        msg.chat.id,
                        "Send me a URL to save an article, or use /list or /digest.",
                    )
                    .await;
            }
        }
        return;
    }

    for url in urls {
        let _ = bot
            .send_message(msg.chat.id, format!("⏳ Processing: {url}"))
            .await;

        match article::add_article(url, "telegram").await {
            Ok((_id, a)) => {
                let title = a.title.as_deref().unwrap_or("(untitled)");
                let mut reply = format!("✅ Saved: {title}\n🔗 {url}");
                if let Some(summary) = &a.summary {
                    // Truncate summary for Telegram to avoid too-long messages
                    let short: String = summary.chars().take(1500).collect();
                    reply.push_str(&format!("\n\n{short}"));
                    if summary.len() > 1500 {
                        reply.push_str("\n\n(truncated - use CLI for full summary)");
                    }
                } else {
                    reply.push_str("\n\n⚠️ Summarization skipped (no Gemini API key configured)");
                }
                let _ = bot.send_message(msg.chat.id, reply).await;
            }
            Err(e) => {
                let _ = bot
                    .send_message(msg.chat.id, format!("❌ Failed to process {url}: {e}"))
                    .await;
            }
        }
    }
}

async fn handle_list(bot: &Bot, msg: &Message) {
    match crate::storage::open_db().and_then(|conn| crate::storage::list_articles(&conn, 10)) {
        Ok(articles) => {
            if articles.is_empty() {
                let _ = bot
                    .send_message(msg.chat.id, "No articles saved yet.")
                    .await;
                return;
            }
            let mut reply = String::from("📚 Recent articles:\n\n");
            for a in &articles {
                let title = a.title.as_deref().unwrap_or("(untitled)");
                let status = if a.summary.is_some() { "✅" } else { "⏳" };
                reply.push_str(&format!("{status} [{}] {} — {}\n", a.id, title, a.added_at));
            }
            let _ = bot.send_message(msg.chat.id, reply).await;
        }
        Err(e) => {
            let _ = bot
                .send_message(msg.chat.id, format!("❌ Error: {e}"))
                .await;
        }
    }
}

async fn handle_digest(bot: &Bot, msg: &Message) {
    let since = {
        let yesterday = chrono::Local::now() - chrono::Duration::days(1);
        yesterday.format("%Y-%m-%dT00:00:00Z").to_string()
    };

    match crate::storage::open_db()
        .and_then(|conn| crate::storage::get_articles_since(&conn, &since))
    {
        Ok(articles) => {
            if articles.is_empty() {
                let _ = bot
                    .send_message(msg.chat.id, "No articles from the last 24 hours.")
                    .await;
                return;
            }
            let digest = crate::output::digest::format_digest(&articles);
            // Split long digest for Telegram
            let chunks = split_for_telegram(&digest);
            for chunk in chunks {
                let _ = bot.send_message(msg.chat.id, chunk).await;
            }
        }
        Err(e) => {
            let _ = bot
                .send_message(msg.chat.id, format!("❌ Error: {e}"))
                .await;
        }
    }
}

fn split_for_telegram(text: &str) -> Vec<String> {
    let max_len = 4000;
    if text.len() <= max_len {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }
        let split_at = remaining[..max_len].rfind('\n').unwrap_or(max_len);
        let (chunk, rest) = remaining.split_at(split_at);
        chunks.push(chunk.to_string());
        remaining = rest.trim_start_matches('\n');
    }
    chunks
}
