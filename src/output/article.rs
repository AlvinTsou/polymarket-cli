use tabled::settings::Style;
use tabled::{Table, Tabled};

use super::{detail_field, print_detail_table, truncate};
use crate::storage::Article;

#[derive(Tabled)]
struct ArticleRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "URL")]
    url: String,
    #[tabled(rename = "Source")]
    source: String,
    #[tabled(rename = "Summarized")]
    summarized: String,
    #[tabled(rename = "Added")]
    added: String,
}

fn article_to_row(a: &Article) -> ArticleRow {
    ArticleRow {
        id: a.id.to_string(),
        title: truncate(a.title.as_deref().unwrap_or("(untitled)"), 40),
        url: truncate(&a.url, 50),
        source: a.source.clone(),
        summarized: if a.summary.is_some() {
            "Yes".into()
        } else {
            "No".into()
        },
        added: truncate(&a.added_at, 19),
    }
}

pub fn print_articles_table(articles: &[Article]) {
    if articles.is_empty() {
        println!("No articles found.");
        return;
    }
    let rows: Vec<ArticleRow> = articles.iter().map(article_to_row).collect();
    let table = Table::new(rows).with(Style::rounded()).to_string();
    println!("{table}");
}

#[allow(clippy::vec_init_then_push)]
pub fn print_article_detail(a: &Article) {
    let mut rows: Vec<[String; 2]> = Vec::new();

    detail_field!(rows, "ID", a.id.to_string());
    detail_field!(
        rows,
        "Title",
        a.title.clone().unwrap_or_else(|| "(untitled)".into())
    );
    detail_field!(rows, "URL", a.url.clone());
    detail_field!(rows, "Source", a.source.clone());
    detail_field!(rows, "Added At", a.added_at.clone());
    detail_field!(
        rows,
        "Summarized At",
        a.summarized_at
            .clone()
            .unwrap_or_else(|| "(not yet)".into())
    );

    print_detail_table(rows);

    if let Some(summary) = &a.summary {
        println!();
        println!("{summary}");
    }
}
