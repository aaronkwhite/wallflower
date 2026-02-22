use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

use super::config::AppConfig;
use super::database::{Database, HistoryEntry, StoredArticle};
use super::freedium::FreediumClient;

/// Application state shared across commands
pub struct AppState {
    pub client: Arc<RwLock<FreediumClient>>,
    pub config: Arc<RwLock<AppConfig>>,
    pub database: Arc<Database>,
}

/// Known Medium publication domains (custom domains that host Medium content)
const MEDIUM_DOMAINS: &[&str] = &[
    "medium.com",
    "towardsdatascience.com",
    "betterprogramming.pub",
    "levelup.gitconnected.com",
    "javascript.plainenglish.io",
    "python.plainenglish.io",
    "blog.devgenius.io",
    "uxdesign.cc",
    "bootcamp.uxdesign.cc",
    "betterhumans.pub",
    "eand.co",
    "entrepreneurshandbook.co",
    "writingcooperative.com",
    "psiloveyou.xyz",
    "hackernoon.com",
    "codeburst.io",
    "itnext.io",
    "medium.freecodecamp.org",
];

/// Check if a URL is a valid Medium article URL
fn is_valid_medium_url(url: &str) -> bool {
    if url.is_empty() {
        return false;
    }

    // Try to parse the URL
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let host = match parsed.host_str() {
        Some(h) => h.to_lowercase(),
        None => return false,
    };

    // Check if it's medium.com or a subdomain
    if host == "medium.com" || host.ends_with(".medium.com") {
        return true;
    }

    // Check known Medium publication domains
    for domain in MEDIUM_DOMAINS {
        if host == *domain || host.ends_with(&format!(".{}", domain)) {
            return true;
        }
    }

    // Heuristic: Medium article URLs often end with a hash like -abc123def456
    let path = parsed.path();
    let medium_id_pattern = regex::Regex::new(r"-[a-f0-9]{10,12}$").unwrap();
    if medium_id_pattern.is_match(path) {
        return true;
    }

    false
}

/// Fetch an article from a Medium URL via Freedium proxy (with caching)
#[tauri::command]
pub async fn fetch_article(
    url: String,
    force_refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<StoredArticle, String> {
    // Validate URL
    if !is_valid_medium_url(&url) {
        return Err("Not a recognized Medium URL. Please enter a valid Medium article URL.".to_string());
    }

    let force = force_refresh.unwrap_or(false);
    let db = state.database.clone();

    // Check cache first (unless force refresh)
    if !force {
        // Check if we have a recent cached version (48 hours)
        if let Ok(true) = db.is_cached(&url, 48) {
            if let Ok(Some(cached)) = db.get_article(&url) {
                // Update last read time
                let _ = db.touch_article(&url);
                return Ok(cached);
            }
        }
    }

    // Fetch the article - clone the Arc to release the State borrow
    let client = state.client.clone();
    let client_guard = client.read().await;

    match client_guard.fetch(&url).await {
        Ok(article) => {
            // Save to database
            let stored = db.save_article(&article).map_err(|e| e.to_string())?;
            Ok(stored)
        }
        Err(e) => {
            // If fetch fails but we have a cached version, return it instead of failing
            if let Ok(Some(cached)) = db.get_article(&url) {
                tracing::warn!("Fetch failed, falling back to cached version: {}", e);
                let _ = db.touch_article(&url);
                return Ok(cached);
            }
            Err(e.to_string())
        }
    }
}

/// Get a cached article by URL (without fetching)
#[tauri::command]
pub fn get_cached_article(url: String, state: State<'_, AppState>) -> Result<Option<StoredArticle>, String> {
    state.database.get_article(&url).map_err(|e| e.to_string())
}

/// Get recent reading history
#[tauri::command]
pub fn get_history(limit: Option<i32>, offset: Option<i32>, state: State<'_, AppState>) -> Result<Vec<HistoryEntry>, String> {
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);
    state.database.get_history(limit, offset).map_err(|e| e.to_string())
}

/// Get favorite articles
#[tauri::command]
pub fn get_favorites(state: State<'_, AppState>) -> Result<Vec<HistoryEntry>, String> {
    state.database.get_favorites().map_err(|e| e.to_string())
}

/// Search articles using full-text search
#[tauri::command]
pub fn search_articles(query: String, state: State<'_, AppState>) -> Result<Vec<HistoryEntry>, String> {
    state.database.search_articles(&query).map_err(|e| e.to_string())
}

/// Toggle favorite status for an article
#[tauri::command]
pub fn toggle_favorite(url: String, state: State<'_, AppState>) -> Result<bool, String> {
    state.database.toggle_favorite(&url).map_err(|e| e.to_string())
}

/// Delete an article from history
#[tauri::command]
pub fn delete_from_history(url: String, state: State<'_, AppState>) -> Result<(), String> {
    state.database.delete_article(&url).map_err(|e| e.to_string())
}

/// Clear old cached articles (keeps favorites)
#[tauri::command]
pub fn clear_cache(hours: Option<i64>, state: State<'_, AppState>) -> Result<i32, String> {
    let hours = hours.unwrap_or(24 * 7); // Default: 1 week
    state.database.clear_old_cache(hours).map_err(|e| e.to_string())
}

/// Get the current configuration
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = state.config.clone();
    let config_guard = config.read().await;
    Ok(config_guard.clone())
}

/// Save configuration
#[tauri::command]
pub async fn save_config(config: AppConfig, state: State<'_, AppState>) -> Result<(), String> {
    // Validate the new config
    config.validate().map_err(|e| e.to_string())?;

    // Save to disk
    config.save().map_err(|e| e.to_string())?;

    // Update client endpoints if they changed
    {
        let client = state.client.clone();
        let mut client_guard = client.write().await;
        client_guard.update_endpoints(config.endpoints.clone());
    }

    // Update stored config
    {
        let config_arc = state.config.clone();
        let mut stored_config = config_arc.write().await;
        *stored_config = config;
    }

    Ok(())
}

/// Check the health of all configured endpoints
#[tauri::command]
pub async fn check_endpoints(state: State<'_, AppState>) -> Result<Vec<(String, bool)>, String> {
    // Clone arcs to release State borrow before await
    let config_arc = state.config.clone();
    let client_arc = state.client.clone();

    let config_guard = config_arc.read().await;
    let endpoints = config_guard.endpoints.clone();
    drop(config_guard); // Release lock before async operations

    let client_guard = client_arc.read().await;

    let mut results = Vec::new();
    for endpoint in &endpoints {
        let alive = client_guard.health_check(endpoint).await;
        results.push((endpoint.clone(), alive));
    }

    Ok(results)
}

/// Validate a URL without fetching
#[tauri::command]
pub fn validate_url(url: String) -> bool {
    is_valid_medium_url(&url)
}

/// Save Markdown content to a file
#[tauri::command]
pub fn save_markdown_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("Failed to save file: {}", e))
}

/// Export database file to a specified path
#[tauri::command]
pub fn export_database(path: String) -> Result<(), String> {
    let db_path = Database::get_db_path();
    std::fs::copy(&db_path, &path).map_err(|e| format!("Failed to export database: {}", e))?;
    Ok(())
}

/// Import articles from a backup database file
#[tauri::command]
pub fn import_database(path: String, state: State<'_, AppState>) -> Result<i32, String> {
    state.database.import_database(&path).map_err(|e| e.to_string())
}

/// Export all articles as Markdown files to a directory
#[tauri::command]
pub fn export_as_markdown(path: String, state: State<'_, AppState>) -> Result<i32, String> {
    let articles = state.database.get_all_articles().map_err(|e| e.to_string())?;
    let base_path = std::path::Path::new(&path);

    // Create a subfolder with timestamp
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let export_path = base_path.join(format!("wallflower-export-{}", timestamp));

    // Create export directory
    std::fs::create_dir_all(&export_path).map_err(|e| format!("Failed to create directory: {}", e))?;

    let mut count = 0;
    for article in &articles {
        // Generate safe filename from title
        let safe_title: String = article.title
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
            .to_lowercase();
        let safe_title = if safe_title.len() > 50 { &safe_title[..50] } else { &safe_title };

        let filename = format!("{}.md", safe_title);
        let file_path = export_path.join(&filename);

        // Convert HTML to simple text (basic conversion)
        let content = format!(
            "# {}\n\nBy: {}\nURL: {}\nSaved: {}\n\n---\n\n{}",
            article.title,
            article.author,
            article.url,
            article.cached_at.format("%Y-%m-%d %H:%M"),
            html_to_markdown(&article.content_html)
        );

        if std::fs::write(&file_path, content).is_ok() {
            count += 1;
        }
    }

    Ok(count)
}

/// Simple HTML to Markdown conversion
fn html_to_markdown(html: &str) -> String {
    // Basic HTML to text conversion
    let text = html
        // Convert headers
        .replace("<h1>", "\n# ")
        .replace("</h1>", "\n")
        .replace("<h2>", "\n## ")
        .replace("</h2>", "\n")
        .replace("<h3>", "\n### ")
        .replace("</h3>", "\n")
        .replace("<h4>", "\n#### ")
        .replace("</h4>", "\n")
        // Convert paragraphs
        .replace("<p>", "\n")
        .replace("</p>", "\n")
        // Convert line breaks
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        // Convert emphasis
        .replace("<strong>", "**")
        .replace("</strong>", "**")
        .replace("<b>", "**")
        .replace("</b>", "**")
        .replace("<em>", "*")
        .replace("</em>", "*")
        .replace("<i>", "*")
        .replace("</i>", "*")
        // Convert lists
        .replace("<ul>", "\n")
        .replace("</ul>", "\n")
        .replace("<ol>", "\n")
        .replace("</ol>", "\n")
        .replace("<li>", "- ")
        .replace("</li>", "\n")
        // Convert blockquotes
        .replace("<blockquote>", "\n> ")
        .replace("</blockquote>", "\n")
        // Convert code
        .replace("<code>", "`")
        .replace("</code>", "`")
        .replace("<pre>", "\n```\n")
        .replace("</pre>", "\n```\n")
        // Convert horizontal rules
        .replace("<hr>", "\n---\n")
        .replace("<hr/>", "\n---\n")
        .replace("<hr />", "\n---\n");

    // Strip remaining HTML tags
    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    let text = re.replace_all(&text, "");

    // Decode common HTML entities
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        // Clean up extra whitespace
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_medium_urls() {
        assert!(is_valid_medium_url("https://medium.com/@user/article-abc123def4"));
        assert!(is_valid_medium_url("https://towardsdatascience.com/article-abc123def4"));
        assert!(is_valid_medium_url("https://betterprogramming.pub/article-abc123def4"));
    }

    #[test]
    fn test_invalid_urls() {
        assert!(!is_valid_medium_url(""));
        assert!(!is_valid_medium_url("not a url"));
        assert!(!is_valid_medium_url("https://google.com"));
        assert!(!is_valid_medium_url("https://example.com/article"));
    }

    #[test]
    fn test_medium_subdomains() {
        assert!(is_valid_medium_url("https://blog.medium.com/article-abc123def4"));
        assert!(is_valid_medium_url("https://engineering.medium.com/article-abc123def4"));
    }
}
