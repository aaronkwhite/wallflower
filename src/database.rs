use chrono::{DateTime, Utc};
use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::freedium::Article;

/// Strip HTML tags and convert to plain text for FTS indexing
pub fn html_to_plain_text(html: &str) -> String {
    // Remove script and style content entirely
    let script_re = Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    let style_re = Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();
    let text = script_re.replace_all(html, "");
    let text = style_re.replace_all(&text, "");

    // Replace block elements with newlines
    let block_re = Regex::new(r"(?i)</?(p|div|br|h[1-6]|li|tr|blockquote)[^>]*>").unwrap();
    let text = block_re.replace_all(&text, "\n");

    // Remove all remaining HTML tags
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let text = tag_re.replace_all(&text, "");

    // Decode common HTML entities
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–");

    // Collapse multiple whitespace/newlines into single spaces
    let whitespace_re = Regex::new(r"\s+").unwrap();
    whitespace_re.replace_all(&text, " ").trim().to_string()
}

/// Database manager for article storage, history, and favorites
pub struct Database {
    conn: Mutex<Connection>,
}

/// A stored article with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredArticle {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub author: String,
    pub author_url: Option<String>,
    pub header_image_url: Option<String>,
    pub content_html: String,
    pub content_text: Option<String>,
    pub fetched_from: String,
    pub cached_at: DateTime<Utc>,
    pub last_read_at: DateTime<Utc>,
    pub read_count: i32,
    pub is_favorite: bool,
}

/// A history entry (lighter than full article)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub author: String,
    pub author_url: Option<String>,
    pub header_image_url: Option<String>,
    pub last_read_at: DateTime<Utc>,
    pub read_count: i32,
    pub is_favorite: bool,
}

impl Database {
    /// Create or open the database
    pub fn new() -> Result<Self, DatabaseError> {
        let path = Self::db_path();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };

        db.init_schema()?;
        Ok(db)
    }

    /// Get the database file path
    fn db_path() -> PathBuf {
        directories::ProjectDirs::from("com", "wallflower", "Wallflower")
            .map(|dirs| dirs.data_dir().join("articles.db"))
            .unwrap_or_else(|| PathBuf::from("wallflower.db"))
    }

    /// Initialize the database schema
    fn init_schema(&self) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS articles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT UNIQUE NOT NULL,
                title TEXT NOT NULL,
                author TEXT NOT NULL,
                author_url TEXT,
                header_image_url TEXT,
                content_html TEXT NOT NULL,
                content_text TEXT,
                fetched_from TEXT NOT NULL,
                cached_at TEXT NOT NULL,
                last_read_at TEXT NOT NULL,
                read_count INTEGER DEFAULT 1,
                is_favorite INTEGER DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_articles_url ON articles(url);
            CREATE INDEX IF NOT EXISTS idx_articles_last_read ON articles(last_read_at DESC);
            CREATE INDEX IF NOT EXISTS idx_articles_favorite ON articles(is_favorite);
            "#,
        )?;

        // Migration: add new columns if they don't exist (for existing databases)
        let _ = conn.execute("ALTER TABLE articles ADD COLUMN author_url TEXT", []);
        let _ = conn.execute("ALTER TABLE articles ADD COLUMN header_image_url TEXT", []);
        let _ = conn.execute("ALTER TABLE articles ADD COLUMN content_text TEXT", []);

        // Initialize FTS5 virtual table for full-text search
        Self::init_fts(&conn)?;

        Ok(())
    }

    /// Initialize FTS5 virtual table and triggers
    fn init_fts(conn: &Connection) -> Result<(), DatabaseError> {
        // Create FTS5 virtual table (external content mode linked to articles table)
        conn.execute_batch(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS articles_fts USING fts5(
                title,
                author,
                content_text,
                content='articles',
                content_rowid='id'
            );
            "#,
        )?;

        // Create triggers to keep FTS in sync
        // Note: We use INSERT OR REPLACE pattern, so handle both INSERT and UPDATE via triggers

        // Trigger for INSERT
        let _ = conn.execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS articles_fts_insert AFTER INSERT ON articles BEGIN
                INSERT INTO articles_fts(rowid, title, author, content_text)
                VALUES (new.id, new.title, new.author, new.content_text);
            END;
            "#,
        );

        // Trigger for DELETE
        let _ = conn.execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS articles_fts_delete AFTER DELETE ON articles BEGIN
                INSERT INTO articles_fts(articles_fts, rowid, title, author, content_text)
                VALUES ('delete', old.id, old.title, old.author, old.content_text);
            END;
            "#,
        );

        // Trigger for UPDATE
        let _ = conn.execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS articles_fts_update AFTER UPDATE ON articles BEGIN
                INSERT INTO articles_fts(articles_fts, rowid, title, author, content_text)
                VALUES ('delete', old.id, old.title, old.author, old.content_text);
                INSERT INTO articles_fts(rowid, title, author, content_text)
                VALUES (new.id, new.title, new.author, new.content_text);
            END;
            "#,
        );

        // Migrate existing articles: populate content_text where NULL and rebuild FTS
        // This is non-fatal - if migration fails, search just won't work for old articles
        if let Err(e) = Self::migrate_existing_articles(conn) {
            tracing::warn!("FTS migration failed: {}. Search may not include all articles.", e);
        }

        Ok(())
    }

    /// Migrate existing articles to populate content_text and FTS index
    fn migrate_existing_articles(conn: &Connection) -> Result<(), DatabaseError> {
        // Check if there are articles with NULL content_text
        let needs_migration: i32 = conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE content_text IS NULL",
            [],
            |row| row.get(0),
        )?;

        if needs_migration > 0 {
            tracing::info!("Migrating {} articles to populate content_text", needs_migration);

            // Get all articles with NULL content_text
            let mut stmt = conn.prepare(
                "SELECT id, content_html FROM articles WHERE content_text IS NULL"
            )?;

            let articles: Vec<(i64, String)> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(|r| r.ok())
                .collect();

            // Update each article with plain text content
            // Continue on errors to avoid blocking startup
            let mut success_count = 0;
            let mut error_count = 0;
            for (id, content_html) in articles {
                let content_text = html_to_plain_text(&content_html);
                match conn.execute(
                    "UPDATE articles SET content_text = ? WHERE id = ?",
                    params![content_text, id],
                ) {
                    Ok(_) => success_count += 1,
                    Err(e) => {
                        tracing::warn!("Failed to migrate article {}: {}", id, e);
                        error_count += 1;
                    }
                }
            }
            if error_count > 0 {
                tracing::warn!("Migration completed with {} errors out of {} articles", error_count, success_count + error_count);
            } else {
                tracing::info!("Migration completed: {} articles updated", success_count);
            }
        }

        // Rebuild FTS index from articles table
        // Use 'delete-all' first to clear any existing index, then populate manually
        // This is more resilient to database issues than 'rebuild'
        let rebuild_result = conn.execute_batch(
            r#"
            INSERT INTO articles_fts(articles_fts) VALUES ('delete-all');
            "#,
        );

        if rebuild_result.is_err() {
            tracing::warn!("FTS delete-all failed, attempting manual population");
        }

        // Manually populate FTS index from existing articles
        let populate_result = conn.execute_batch(
            r#"
            INSERT INTO articles_fts(rowid, title, author, content_text)
            SELECT id, title, author, content_text FROM articles WHERE content_text IS NOT NULL;
            "#,
        );

        if let Err(e) = populate_result {
            tracing::warn!("FTS population failed: {}. Search may not work correctly until articles are re-fetched.", e);
        }

        Ok(())
    }

    /// Get a cached article by URL
    pub fn get_article(&self, url: &str) -> Result<Option<StoredArticle>, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;

        let mut stmt = conn.prepare(
            "SELECT id, url, title, author, author_url, header_image_url, content_html, content_text, fetched_from, cached_at, last_read_at, read_count, is_favorite
             FROM articles WHERE url = ?"
        )?;

        let result = stmt.query_row(params![url], |row| {
            Ok(StoredArticle {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                author: row.get(3)?,
                author_url: row.get(4)?,
                header_image_url: row.get(5)?,
                content_html: row.get(6)?,
                content_text: row.get(7)?,
                fetched_from: row.get(8)?,
                cached_at: row.get::<_, String>(9)?.parse().unwrap_or_else(|_| Utc::now()),
                last_read_at: row.get::<_, String>(10)?.parse().unwrap_or_else(|_| Utc::now()),
                read_count: row.get(11)?,
                is_favorite: row.get::<_, i32>(12)? != 0,
            })
        });

        match result {
            Ok(article) => Ok(Some(article)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Save or update an article in the cache
    pub fn save_article(&self, article: &Article) -> Result<StoredArticle, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;
        let now = Utc::now();

        // Extract plain text from HTML for FTS indexing
        let content_text = html_to_plain_text(&article.content_html);

        // Try to update existing or insert new
        conn.execute(
            r#"
            INSERT INTO articles (url, title, author, author_url, header_image_url, content_html, content_text, fetched_from, cached_at, last_read_at, read_count, is_favorite)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, 1, 0)
            ON CONFLICT(url) DO UPDATE SET
                title = ?2,
                author = ?3,
                author_url = ?4,
                header_image_url = ?5,
                content_html = ?6,
                content_text = ?7,
                fetched_from = ?8,
                cached_at = ?9,
                last_read_at = ?9,
                read_count = read_count + 1
            "#,
            params![
                article.original_url,
                article.title,
                article.author,
                article.author_url,
                article.header_image_url,
                article.content_html,
                content_text,
                article.fetched_from,
                now.to_rfc3339(),
            ],
        )?;

        // Get the saved article
        drop(conn);
        self.get_article(&article.original_url)?
            .ok_or(DatabaseError::NotFound)
    }

    /// Update the last read time for an article
    pub fn touch_article(&self, url: &str) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;
        let now = Utc::now();

        conn.execute(
            "UPDATE articles SET last_read_at = ?, read_count = read_count + 1 WHERE url = ?",
            params![now.to_rfc3339(), url],
        )?;

        Ok(())
    }

    /// Toggle favorite status for an article
    pub fn toggle_favorite(&self, url: &str) -> Result<bool, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;

        conn.execute(
            "UPDATE articles SET is_favorite = NOT is_favorite WHERE url = ?",
            params![url],
        )?;

        // Get the new favorite status
        let is_favorite: i32 = conn.query_row(
            "SELECT is_favorite FROM articles WHERE url = ?",
            params![url],
            |row| row.get(0),
        )?;

        Ok(is_favorite != 0)
    }

    /// Set favorite status for an article
    pub fn set_favorite(&self, url: &str, is_favorite: bool) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;

        conn.execute(
            "UPDATE articles SET is_favorite = ? WHERE url = ?",
            params![is_favorite as i32, url],
        )?;

        Ok(())
    }

    /// Get recent articles (history)
    pub fn get_history(&self, limit: i32) -> Result<Vec<HistoryEntry>, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;

        let mut stmt = conn.prepare(
            "SELECT id, url, title, author, author_url, header_image_url, last_read_at, read_count, is_favorite
             FROM articles
             ORDER BY last_read_at DESC
             LIMIT ?"
        )?;

        let entries = stmt.query_map(params![limit], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                author: row.get(3)?,
                author_url: row.get(4)?,
                header_image_url: row.get(5)?,
                last_read_at: row.get::<_, String>(6)?.parse().unwrap_or_else(|_| Utc::now()),
                read_count: row.get(7)?,
                is_favorite: row.get::<_, i32>(8)? != 0,
            })
        })?;

        entries.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get favorite articles
    pub fn get_favorites(&self) -> Result<Vec<HistoryEntry>, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;

        let mut stmt = conn.prepare(
            "SELECT id, url, title, author, author_url, header_image_url, last_read_at, read_count, is_favorite
             FROM articles
             WHERE is_favorite = 1
             ORDER BY last_read_at DESC"
        )?;

        let entries = stmt.query_map([], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                author: row.get(3)?,
                author_url: row.get(4)?,
                header_image_url: row.get(5)?,
                last_read_at: row.get::<_, String>(6)?.parse().unwrap_or_else(|_| Utc::now()),
                read_count: row.get(7)?,
                is_favorite: row.get::<_, i32>(8)? != 0,
            })
        })?;

        entries.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Delete an article from history/cache
    pub fn delete_article(&self, url: &str) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;
        conn.execute("DELETE FROM articles WHERE url = ?", params![url])?;
        Ok(())
    }

    /// Clear all non-favorite articles older than the specified hours
    pub fn clear_old_cache(&self, hours: i64) -> Result<i32, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;
        let cutoff = Utc::now() - chrono::Duration::hours(hours);

        let deleted = conn.execute(
            "DELETE FROM articles WHERE is_favorite = 0 AND cached_at < ?",
            params![cutoff.to_rfc3339()],
        )?;

        Ok(deleted as i32)
    }

    /// Check if an article exists in cache and is recent enough
    pub fn is_cached(&self, url: &str, max_age_hours: i64) -> Result<bool, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;
        let cutoff = Utc::now() - chrono::Duration::hours(max_age_hours);

        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE url = ? AND cached_at > ?",
            params![url, cutoff.to_rfc3339()],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    /// Get all articles for export
    pub fn get_all_articles(&self) -> Result<Vec<StoredArticle>, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;

        let mut stmt = conn.prepare(
            "SELECT id, url, title, author, author_url, header_image_url, content_html, content_text, fetched_from, cached_at, last_read_at, read_count, is_favorite
             FROM articles
             ORDER BY last_read_at DESC"
        )?;

        let entries = stmt.query_map([], |row| {
            Ok(StoredArticle {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                author: row.get(3)?,
                author_url: row.get(4)?,
                header_image_url: row.get(5)?,
                content_html: row.get(6)?,
                content_text: row.get(7)?,
                fetched_from: row.get(8)?,
                cached_at: row.get::<_, String>(9)?.parse().unwrap_or_else(|_| Utc::now()),
                last_read_at: row.get::<_, String>(10)?.parse().unwrap_or_else(|_| Utc::now()),
                read_count: row.get(11)?,
                is_favorite: row.get::<_, i32>(12)? != 0,
            })
        })?;

        entries.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get the database file path (public for export)
    pub fn get_db_path() -> PathBuf {
        Self::db_path()
    }

    /// Search articles using FTS5 full-text search
    /// Returns matching articles ranked by BM25 relevance
    pub fn search_articles(&self, query: &str) -> Result<Vec<HistoryEntry>, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;

        // Sanitize query for FTS5 - escape special characters and handle empty query
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        // Build FTS5 query with prefix matching for partial words
        // Sanitize each word: remove FTS5 special characters and wrap in quotes for safety
        let fts_query: String = query
            .split_whitespace()
            .filter_map(|word| {
                // Remove FTS5 special characters: " * - ^ ( ) { } [ ] : OR AND NOT NEAR
                let sanitized: String = word
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '\'')
                    .collect();
                if sanitized.is_empty() {
                    None
                } else {
                    // Use prefix matching with * for partial word search
                    Some(format!("\"{}\"*", sanitized))
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        // If all words were filtered out, return empty
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(
            r#"
            SELECT a.id, a.url, a.title, a.author, a.author_url, a.header_image_url, a.last_read_at, a.read_count, a.is_favorite
            FROM articles a
            JOIN articles_fts f ON a.id = f.rowid
            WHERE articles_fts MATCH ?1
            ORDER BY bm25(articles_fts)
            LIMIT 50
            "#,
        )?;

        let entries = stmt.query_map(params![fts_query], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                author: row.get(3)?,
                author_url: row.get(4)?,
                header_image_url: row.get(5)?,
                last_read_at: row.get::<_, String>(6)?.parse().unwrap_or_else(|_| Utc::now()),
                read_count: row.get(7)?,
                is_favorite: row.get::<_, i32>(8)? != 0,
            })
        })?;

        entries.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

/// Database errors
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to acquire database lock")]
    LockError,

    #[error("Article not found")]
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_plain_text_basic() {
        let html = "<p>Hello <strong>world</strong>!</p>";
        let text = html_to_plain_text(html);
        assert_eq!(text, "Hello world!");
    }

    #[test]
    fn test_html_to_plain_text_strips_scripts() {
        let html = "<p>Before</p><script>alert('xss')</script><p>After</p>";
        let text = html_to_plain_text(html);
        assert!(!text.contains("alert"));
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
    }

    #[test]
    fn test_html_to_plain_text_decodes_entities() {
        let html = "<p>Tom &amp; Jerry &mdash; classic</p>";
        let text = html_to_plain_text(html);
        assert!(text.contains("Tom & Jerry"));
        assert!(text.contains("—"));
    }

    #[test]
    fn test_html_to_plain_text_collapses_whitespace() {
        let html = "<p>Hello</p>\n\n\n<p>World</p>";
        let text = html_to_plain_text(html);
        // Should not have excessive whitespace
        assert!(!text.contains("  "));
    }

    #[test]
    fn test_fts_query_sanitization() {
        // Test that special FTS5 characters don't cause issues
        // We can't easily test the actual search without a full DB setup,
        // but we can verify the query building logic
        let special_queries = vec![
            "hello*world",      // asterisk
            "test OR other",    // FTS operator
            "foo AND bar",      // FTS operator
            "NOT this",         // FTS operator
            "\"quoted\"",       // quotes
            "test-case",        // hyphen
            "(parentheses)",    // parens
            "***",              // only special chars
            "",                 // empty
            "   ",              // whitespace only
        ];

        // These shouldn't panic - actual search behavior tested via integration
        for query in special_queries {
            let sanitized: String = query
                .split_whitespace()
                .filter_map(|word| {
                    let clean: String = word
                        .chars()
                        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '\'')
                        .collect();
                    if clean.is_empty() { None } else { Some(format!("\"{}\"*", clean)) }
                })
                .collect::<Vec<_>>()
                .join(" ");
            // Should not contain raw special chars outside quotes
            assert!(!sanitized.contains("OR ") || sanitized.contains("\"OR\""));
            assert!(!sanitized.contains("AND ") || sanitized.contains("\"AND\""));
        }
    }
}
