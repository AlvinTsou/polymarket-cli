use crate::storage::Article;

pub fn format_digest(articles: &[Article]) -> String {
    if articles.is_empty() {
        return "No articles to digest.".to_string();
    }

    let date = chrono::Local::now().format("%Y-%m-%d");
    let mut out = format!("📖 Daily Article Digest — {date}\n");
    out.push_str(&"─".repeat(40));
    out.push('\n');
    out.push('\n');

    for (i, article) in articles.iter().enumerate() {
        let title = article.title.as_deref().unwrap_or("(untitled)");
        out.push_str(&format!("{}. {}\n", i + 1, title));
        out.push_str(&format!("   🔗 {}\n", article.url));
        if let Some(summary) = &article.summary {
            out.push('\n');
            // Indent each line of the summary
            for line in summary.lines() {
                out.push_str(&format!("   {line}\n"));
            }
        } else {
            out.push_str("   (no summary available)\n");
        }
        out.push('\n');
        out.push_str(&"─".repeat(40));
        out.push('\n');
        out.push('\n');
    }

    out.push_str(&format!("Total: {} article(s)\n", articles.len()));
    out
}

pub fn print_digest_preview(articles: &[Article]) {
    let digest = format_digest(articles);
    println!("{digest}");
}
