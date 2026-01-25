use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqliteResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::freedium::Article;

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

        Ok(())
    }

    /// Get a cached article by URL
    pub fn get_article(&self, url: &str) -> Result<Option<StoredArticle>, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockError)?;

        let mut stmt = conn.prepare(
            "SELECT id, url, title, author, author_url, header_image_url, content_html, fetched_from, cached_at, last_read_at, read_count, is_favorite
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
                fetched_from: row.get(7)?,
                cached_at: row.get::<_, String>(8)?.parse().unwrap_or_else(|_| Utc::now()),
                last_read_at: row.get::<_, String>(9)?.parse().unwrap_or_else(|_| Utc::now()),
                read_count: row.get(10)?,
                is_favorite: row.get::<_, i32>(11)? != 0,
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

        // Try to update existing or insert new
        conn.execute(
            r#"
            INSERT INTO articles (url, title, author, author_url, header_image_url, content_html, fetched_from, cached_at, last_read_at, read_count, is_favorite)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1, 0)
            ON CONFLICT(url) DO UPDATE SET
                title = ?2,
                author = ?3,
                author_url = ?4,
                header_image_url = ?5,
                content_html = ?6,
                fetched_from = ?7,
                cached_at = ?8,
                last_read_at = ?8,
                read_count = read_count + 1
            "#,
            params![
                article.original_url,
                article.title,
                article.author,
                article.author_url,
                article.header_image_url,
                article.content_html,
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
            "SELECT id, url, title, author, author_url, header_image_url, content_html, fetched_from, cached_at, last_read_at, read_count, is_favorite
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
                fetched_from: row.get(7)?,
                cached_at: row.get::<_, String>(8)?.parse().unwrap_or_else(|_| Utc::now()),
                last_read_at: row.get::<_, String>(9)?.parse().unwrap_or_else(|_| Utc::now()),
                read_count: row.get(10)?,
                is_favorite: row.get::<_, i32>(11)? != 0,
            })
        })?;

        entries.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get the database file path (public for export)
    pub fn get_db_path() -> PathBuf {
        Self::db_path()
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
