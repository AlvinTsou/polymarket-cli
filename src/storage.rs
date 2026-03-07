use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Serialize;

fn db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".config").join("polymarket").join("articles.db"))
}

pub fn open_db() -> Result<Connection> {
    let path = db_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create config directory")?;
    }
    let conn = Connection::open(&path).context("Failed to open articles database")?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS articles (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            url           TEXT NOT NULL UNIQUE,
            title         TEXT,
            raw_content   TEXT NOT NULL,
            summary       TEXT,
            source        TEXT NOT NULL DEFAULT 'cli',
            added_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            summarized_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_articles_added_at ON articles(added_at);",
    )?;
    Ok(conn)
}

#[derive(Debug, Serialize)]
pub struct Article {
    pub id: i64,
    pub url: String,
    pub title: Option<String>,
    pub raw_content: String,
    pub summary: Option<String>,
    pub source: String,
    pub added_at: String,
    pub summarized_at: Option<String>,
}

pub fn insert_article(
    conn: &Connection,
    url: &str,
    title: Option<&str>,
    raw_content: &str,
    source: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO articles (url, title, raw_content, source) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![url, title, raw_content, source],
    )
    .context("Failed to insert article (URL may already exist)")?;
    Ok(conn.last_insert_rowid())
}

pub fn update_summary(conn: &Connection, id: i64, summary: &str) -> Result<()> {
    conn.execute(
        "UPDATE articles SET summary = ?1, summarized_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?2",
        rusqlite::params![summary, id],
    )?;
    Ok(())
}

pub fn get_article(conn: &Connection, id: i64) -> Result<Option<Article>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, title, raw_content, summary, source, added_at, summarized_at FROM articles WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], row_to_article)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn list_articles(conn: &Connection, limit: u32) -> Result<Vec<Article>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, title, raw_content, summary, source, added_at, summarized_at FROM articles ORDER BY added_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit], row_to_article)?;
    rows.collect::<Result<Vec<_>, _>>()
        .context("Failed to list articles")
}

pub fn delete_article(conn: &Connection, id: i64) -> Result<bool> {
    let affected = conn.execute("DELETE FROM articles WHERE id = ?1", rusqlite::params![id])?;
    Ok(affected > 0)
}

pub fn get_unsummarized(conn: &Connection) -> Result<Vec<Article>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, title, raw_content, summary, source, added_at, summarized_at FROM articles WHERE summary IS NULL ORDER BY added_at ASC",
    )?;
    let rows = stmt.query_map([], row_to_article)?;
    rows.collect::<Result<Vec<_>, _>>()
        .context("Failed to get unsummarized articles")
}

pub fn get_articles_since(conn: &Connection, since: &str) -> Result<Vec<Article>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, title, raw_content, summary, source, added_at, summarized_at FROM articles WHERE added_at >= ?1 ORDER BY added_at ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![since], row_to_article)?;
    rows.collect::<Result<Vec<_>, _>>()
        .context("Failed to get articles since date")
}

fn row_to_article(row: &rusqlite::Row) -> rusqlite::Result<Article> {
    Ok(Article {
        id: row.get(0)?,
        url: row.get(1)?,
        title: row.get(2)?,
        raw_content: row.get(3)?,
        summary: row.get(4)?,
        source: row.get(5)?,
        added_at: row.get(6)?,
        summarized_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE articles (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                url           TEXT NOT NULL UNIQUE,
                title         TEXT,
                raw_content   TEXT NOT NULL,
                summary       TEXT,
                source        TEXT NOT NULL DEFAULT 'cli',
                added_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                summarized_at TEXT
            );
            CREATE INDEX idx_articles_added_at ON articles(added_at);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn insert_and_get_article() {
        let conn = in_memory_db();
        let id =
            insert_article(&conn, "https://example.com", Some("Test"), "content", "cli").unwrap();
        let article = get_article(&conn, id).unwrap().unwrap();
        assert_eq!(article.url, "https://example.com");
        assert_eq!(article.title.as_deref(), Some("Test"));
        assert!(article.summary.is_none());
    }

    #[test]
    fn update_summary_sets_fields() {
        let conn = in_memory_db();
        let id = insert_article(&conn, "https://example.com", None, "content", "cli").unwrap();
        update_summary(&conn, id, "A summary").unwrap();
        let article = get_article(&conn, id).unwrap().unwrap();
        assert_eq!(article.summary.as_deref(), Some("A summary"));
        assert!(article.summarized_at.is_some());
    }

    #[test]
    fn duplicate_url_fails() {
        let conn = in_memory_db();
        insert_article(&conn, "https://example.com", None, "content", "cli").unwrap();
        assert!(insert_article(&conn, "https://example.com", None, "content2", "cli").is_err());
    }

    #[test]
    fn delete_article_removes_row() {
        let conn = in_memory_db();
        let id = insert_article(&conn, "https://example.com", None, "content", "cli").unwrap();
        assert!(delete_article(&conn, id).unwrap());
        assert!(get_article(&conn, id).unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let conn = in_memory_db();
        assert!(!delete_article(&conn, 999).unwrap());
    }

    #[test]
    fn list_articles_returns_recent_first() {
        let conn = in_memory_db();
        insert_article(&conn, "https://a.com", Some("A"), "content", "cli").unwrap();
        insert_article(&conn, "https://b.com", Some("B"), "content", "cli").unwrap();
        let articles = list_articles(&conn, 10).unwrap();
        assert_eq!(articles.len(), 2);
        // Most recent (B) comes first
        assert_eq!(articles[0].url, "https://b.com");
    }

    #[test]
    fn get_unsummarized_returns_only_null_summary() {
        let conn = in_memory_db();
        let id1 = insert_article(&conn, "https://a.com", None, "content", "cli").unwrap();
        insert_article(&conn, "https://b.com", None, "content", "cli").unwrap();
        update_summary(&conn, id1, "done").unwrap();
        let unsummarized = get_unsummarized(&conn).unwrap();
        assert_eq!(unsummarized.len(), 1);
        assert_eq!(unsummarized[0].url, "https://b.com");
    }
}
