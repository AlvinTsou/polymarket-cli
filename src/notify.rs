use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config::Config;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum NotifyChannel {
    Telegram,
    Email,
    All,
}

// ── Telegram ──

#[derive(Serialize)]
struct TelegramSendMessage {
    chat_id: String,
    text: String,
    parse_mode: String,
}

pub async fn send_telegram(token: &str, chat_id: &str, message: &str) -> Result<()> {
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");

    // Telegram has a 4096 char limit per message; split if needed.
    let chunks = split_message(message, 4000);

    let client = reqwest::Client::new();
    for chunk in chunks {
        let body = TelegramSendMessage {
            chat_id: chat_id.to_string(),
            text: chunk,
            parse_mode: "Markdown".to_string(),
        };

        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send Telegram message")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("Telegram API error ({}): {}", status, text);
        }
    }

    Ok(())
}

fn split_message(text: &str, max_len: usize) -> Vec<String> {
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
        // Try to split at a newline boundary
        let split_at = remaining[..max_len].rfind('\n').unwrap_or(max_len);
        let (chunk, rest) = remaining.split_at(split_at);
        chunks.push(chunk.to_string());
        remaining = rest.trim_start_matches('\n');
    }
    chunks
}

// ── Email ──

pub async fn send_email(config: &Config, subject: &str, body: &str) -> Result<()> {
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    let smtp_host = config
        .smtp_host
        .as_deref()
        .context("smtp_host not configured")?;
    let smtp_username = config
        .smtp_username
        .as_deref()
        .context("smtp_username not configured")?;
    let smtp_password = config
        .smtp_password
        .as_deref()
        .context("smtp_password not configured")?;
    let email_from = config
        .email_from
        .as_deref()
        .context("email_from not configured")?;
    let email_to = config
        .email_to
        .as_deref()
        .context("email_to not configured")?;

    let email = Message::builder()
        .from(email_from.parse().context("Invalid email_from address")?)
        .to(email_to.parse().context("Invalid email_to address")?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .context("Failed to build email")?;

    let creds = Credentials::new(smtp_username.to_string(), smtp_password.to_string());

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_host)
        .context("Failed to create SMTP transport")?
        .credentials(creds)
        .build();

    mailer.send(email).await.context("Failed to send email")?;

    Ok(())
}

// ── Dispatch ──

pub async fn send_digest(config: &Config, channel: NotifyChannel, digest: &str) -> Result<()> {
    match channel {
        NotifyChannel::Telegram => {
            let token = config
                .telegram_bot_token
                .as_deref()
                .context("telegram_bot_token not configured. Run `polymarket digest setup`")?;
            let chat_id = config
                .telegram_chat_id
                .as_deref()
                .context("telegram_chat_id not configured. Run `polymarket digest setup`")?;
            send_telegram(token, chat_id, digest).await?;
            println!("Digest sent via Telegram.");
        }
        NotifyChannel::Email => {
            let subject = format!(
                "Daily Article Digest - {}",
                chrono::Local::now().format("%Y-%m-%d")
            );
            send_email(config, &subject, digest).await?;
            println!("Digest sent via Email.");
        }
        NotifyChannel::All => {
            let has_telegram =
                config.telegram_bot_token.is_some() && config.telegram_chat_id.is_some();
            let has_email = config.smtp_host.is_some() && config.email_to.is_some();

            if !has_telegram && !has_email {
                bail!("No notification channels configured. Run `polymarket digest setup`");
            }

            if has_telegram {
                let token = config.telegram_bot_token.as_deref().unwrap();
                let chat_id = config.telegram_chat_id.as_deref().unwrap();
                send_telegram(token, chat_id, digest).await?;
                println!("Digest sent via Telegram.");
            }
            if has_email {
                let subject = format!(
                    "Daily Article Digest - {}",
                    chrono::Local::now().format("%Y-%m-%d")
                );
                send_email(config, &subject, digest).await?;
                println!("Digest sent via Email.");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_message_short() {
        let chunks = split_message("hello", 100);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn split_message_long() {
        let text = "line1\nline2\nline3\nline4";
        let chunks = split_message(text, 12);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "line1\nline2");
    }
}
