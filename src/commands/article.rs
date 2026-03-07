use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use crate::config;
use crate::output::OutputFormat;
use crate::{gemini, storage};

#[derive(Args)]
pub struct ArticleArgs {
    #[command(subcommand)]
    pub command: ArticleCommand,
}

#[derive(Subcommand)]
pub enum ArticleCommand {
    /// Add an article by URL (fetches, extracts content, and summarizes)
    Add {
        /// Article URL
        url: String,
    },
    /// List stored articles
    List {
        /// Maximum number of articles to show
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Show article detail with summary
    Get {
        /// Article ID
        id: i64,
    },
    /// Delete an article
    Delete {
        /// Article ID
        id: i64,
    },
    /// Summarize unsummarized articles (or a specific one)
    Summarize {
        /// Specific article ID to summarize
        #[arg(long)]
        id: Option<i64>,
    },
}

pub async fn execute(args: ArticleArgs, output: OutputFormat) -> Result<()> {
    match args.command {
        ArticleCommand::Add { url } => cmd_add(&url, output).await,
        ArticleCommand::List { limit } => cmd_list(limit, output),
        ArticleCommand::Get { id } => cmd_get(id, output),
        ArticleCommand::Delete { id } => cmd_delete(id, output),
        ArticleCommand::Summarize { id } => cmd_summarize(id, output).await,
    }
}

async fn fetch_and_extract(url: &str) -> Result<(Option<String>, String)> {
    let resp = reqwest::get(url).await.context("Failed to fetch URL")?;

    if !resp.status().is_success() {
        bail!("HTTP error {}: {}", resp.status(), url);
    }

    let html = resp.text().await.context("Failed to read response body")?;

    // Use scraper to extract main content
    use scraper::{Html, Selector};
    let document = Html::parse_document(&html);

    // Try to extract title
    let title = Selector::parse("title")
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<String>())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    // Extract main content: try <article>, <main>, then fall back to <body>
    let content = extract_content(&document);

    if content.trim().is_empty() {
        bail!("Could not extract meaningful content from {url}");
    }

    Ok((title, content))
}

fn extract_content(document: &scraper::Html) -> String {
    use scraper::Selector;

    // Try selectors in order of preference
    let selectors = [
        "article",
        "main",
        "[role=main]",
        ".post-content",
        ".article-content",
        "body",
    ];

    for sel_str in selectors {
        if let Ok(sel) = Selector::parse(sel_str)
            && let Some(el) = document.select(&sel).next()
        {
            let text: String = el.text().collect::<Vec<_>>().join(" ");
            let cleaned = clean_text(&text);
            if cleaned.len() > 100 {
                return cleaned;
            }
        }
    }

    // Final fallback: all text
    let text: String = document.root_element().text().collect::<Vec<_>>().join(" ");
    clean_text(&text)
}

fn clean_text(text: &str) -> String {
    // Collapse whitespace and clean up
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub async fn add_article(url: &str, source: &str) -> Result<(i64, storage::Article)> {
    // Validate URL
    url::Url::parse(url).context("Invalid URL")?;

    let (title, content) = fetch_and_extract(url).await?;

    let conn = storage::open_db()?;
    let id = storage::insert_article(&conn, url, title.as_deref(), &content, source)?;

    // Try to summarize if Gemini is configured
    let cfg = config::load_config_or_default();
    if let Some(api_key) = &cfg.gemini_api_key {
        match gemini::summarize(api_key, &content).await {
            Ok(summary) => {
                storage::update_summary(&conn, id, &summary)?;
            }
            Err(e) => {
                eprintln!("Warning: Summarization failed: {e}");
                eprintln!(
                    "Article saved without summary. Run `polymarket article summarize` later."
                );
            }
        }
    }

    let article = storage::get_article(&conn, id)?.context("Article not found after insert")?;
    Ok((id, article))
}

async fn cmd_add(url: &str, output: OutputFormat) -> Result<()> {
    let (_id, article) = add_article(url, "cli").await?;

    match output {
        OutputFormat::Json => crate::output::print_json(&article)?,
        OutputFormat::Table => {
            println!("Article saved!");
            crate::output::article::print_article_detail(&article);
        }
    }
    Ok(())
}

fn cmd_list(limit: u32, output: OutputFormat) -> Result<()> {
    let conn = storage::open_db()?;
    let articles = storage::list_articles(&conn, limit)?;

    match output {
        OutputFormat::Json => crate::output::print_json(&articles)?,
        OutputFormat::Table => crate::output::article::print_articles_table(&articles),
    }
    Ok(())
}

fn cmd_get(id: i64, output: OutputFormat) -> Result<()> {
    let conn = storage::open_db()?;
    let article =
        storage::get_article(&conn, id)?.context(format!("Article with id {id} not found"))?;

    match output {
        OutputFormat::Json => crate::output::print_json(&article)?,
        OutputFormat::Table => crate::output::article::print_article_detail(&article),
    }
    Ok(())
}

fn cmd_delete(id: i64, output: OutputFormat) -> Result<()> {
    let conn = storage::open_db()?;
    let deleted = storage::delete_article(&conn, id)?;

    if !deleted {
        bail!("Article with id {id} not found");
    }

    match output {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"deleted": id}));
        }
        OutputFormat::Table => {
            println!("Article {id} deleted.");
        }
    }
    Ok(())
}

async fn cmd_summarize(id: Option<i64>, output: OutputFormat) -> Result<()> {
    let cfg = config::load_config_or_default();
    let api_key = cfg
        .gemini_api_key
        .as_deref()
        .context("gemini_api_key not configured. Run `polymarket digest setup`")?;

    let conn = storage::open_db()?;

    let articles = if let Some(id) = id {
        let article =
            storage::get_article(&conn, id)?.context(format!("Article with id {id} not found"))?;
        vec![article]
    } else {
        storage::get_unsummarized(&conn)?
    };

    if articles.is_empty() {
        match output {
            OutputFormat::Json => {
                println!("{}", serde_json::json!({"summarized": 0}));
            }
            OutputFormat::Table => {
                println!("No articles to summarize.");
            }
        }
        return Ok(());
    }

    let mut count = 0;
    for article in &articles {
        match gemini::summarize(api_key, &article.raw_content).await {
            Ok(summary) => {
                storage::update_summary(&conn, article.id, &summary)?;
                count += 1;
                if matches!(output, OutputFormat::Table) {
                    println!(
                        "Summarized: {} (id: {})",
                        article.title.as_deref().unwrap_or("(untitled)"),
                        article.id
                    );
                }
            }
            Err(e) => {
                eprintln!("Failed to summarize id {}: {e}", article.id);
            }
        }
        // Brief pause between API calls to avoid rate limits
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    match output {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"summarized": count}));
        }
        OutputFormat::Table => {
            println!("\nSummarized {count} article(s).");
        }
    }
    Ok(())
}
