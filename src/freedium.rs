use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Represents a fetched and parsed Medium article
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub title: String,
    pub author: String,
    pub author_url: Option<String>,
    pub header_image_url: Option<String>,
    pub content_html: String,
    pub original_url: String,
    pub fetched_from: String,
}

/// HTTP client for fetching articles through Freedium proxy
pub struct FreediumClient {
    client: Client,
    endpoints: Vec<String>,
}

impl FreediumClient {
    /// Create a new FreediumClient with the given endpoints
    pub fn new(endpoints: Vec<String>) -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self { client, endpoints }
    }

    /// Update the endpoints list
    pub fn update_endpoints(&mut self, endpoints: Vec<String>) {
        self.endpoints = endpoints;
    }

    /// Fetch an article through Freedium proxy, trying each endpoint until one works
    pub async fn fetch(&self, medium_url: &str) -> Result<Article, FetchError> {
        if self.endpoints.is_empty() {
            return Err(FetchError::NoEndpointsConfigured);
        }

        let mut last_error = None;

        for endpoint in &self.endpoints {
            let url = format!("{}{}", endpoint, urlencoding::encode(medium_url));

            match self.try_fetch(&url, medium_url, endpoint).await {
                Ok(article) => return Ok(article),
                Err(e) => {
                    tracing::warn!("Endpoint {} failed: {}", endpoint, e);
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or(FetchError::AllEndpointsFailed))
    }

    /// Try to fetch from a specific endpoint
    async fn try_fetch(
        &self,
        url: &str,
        original_url: &str,
        endpoint: &str,
    ) -> Result<Article, FetchError> {
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(FetchError::HttpError(response.status().as_u16()));
        }

        let html = response.text().await?;
        self.parse_freedium_html(&html, original_url, endpoint)
    }

    /// Parse the HTML from Freedium and extract article content
    fn parse_freedium_html(
        &self,
        html: &str,
        original_url: &str,
        endpoint: &str,
    ) -> Result<Article, FetchError> {
        let document = Html::parse_document(html);

        // Try multiple selectors for title (Freedium's HTML structure may vary)
        let title = self
            .extract_text(&document, "h1.main-title")
            .or_else(|_| self.extract_text(&document, "h1.post-title"))
            .or_else(|_| self.extract_text(&document, "article h1"))
            .or_else(|_| self.extract_text(&document, ".post-full-title"))
            .or_else(|_| self.extract_text(&document, "h1"))
            .unwrap_or_else(|_| "Untitled Article".to_string());

        // Extract author name and URL from the author info section
        // Freedium uses: <a href="https://medium.com/@username" ...>Author Name</a>
        let (author, author_url) = self
            .extract_author_info(&document)
            .unwrap_or_else(|| ("Unknown Author".to_string(), None));

        // Extract header/preview image
        // Freedium uses: <img alt="Preview image" ... src="...">
        let header_image_url = self.extract_header_image(&document);

        // Try multiple selectors for content
        let content = self
            .extract_html(&document, ".main-content")
            .or_else(|_| self.extract_html(&document, "article"))
            .or_else(|_| self.extract_html(&document, ".post-content"))
            .or_else(|_| self.extract_html(&document, ".article-content"))
            .or_else(|_| self.extract_html(&document, "main"))
            .map_err(|_| FetchError::ParseError("Could not find article content".to_string()))?;

        // Clean up the content
        let cleaned_content = self.clean_content(&content);

        Ok(Article {
            title,
            author,
            author_url,
            header_image_url,
            content_html: cleaned_content,
            original_url: original_url.to_string(),
            fetched_from: endpoint.to_string(),
        })
    }

    /// Extract author name and profile URL
    fn extract_author_info(&self, doc: &Html) -> Option<(String, Option<String>)> {
        // Look for the author link in the author info section
        // Pattern: <a href="https://medium.com/@username" ...>Author Name</a>
        let selector = Selector::parse(".flex-grow a[href*='medium.com/@']").ok()?;

        if let Some(element) = doc.select(&selector).next() {
            let name = element.text().collect::<String>().trim().to_string();
            let url = element.value().attr("href").map(String::from);

            if !name.is_empty() && name != "Follow" {
                return Some((name, url));
            }
        }

        // Fallback: try other author selectors
        let fallback_selectors = [
            ".author-name",
            ".post-author",
            "[class*='author'] a",
        ];

        for selector_str in fallback_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(element) = doc.select(&selector).next() {
                    let name = element.text().collect::<String>().trim().to_string();
                    let url = element.value().attr("href").map(String::from);
                    if !name.is_empty() {
                        return Some((name, url));
                    }
                }
            }
        }

        None
    }

    /// Extract the header/preview image URL
    fn extract_header_image(&self, doc: &Html) -> Option<String> {
        // Look for the preview image before the title
        // Pattern: <img alt="Preview image" ... src="...">
        if let Ok(selector) = Selector::parse("img[alt='Preview image']") {
            if let Some(element) = doc.select(&selector).next() {
                if let Some(src) = element.value().attr("src") {
                    return Some(src.to_string());
                }
            }
        }

        // Fallback: look for first large image in the header area
        if let Ok(selector) = Selector::parse(".font-sans img[src*='miro.medium.com']") {
            if let Some(element) = doc.select(&selector).next() {
                if let Some(src) = element.value().attr("src") {
                    return Some(src.to_string());
                }
            }
        }

        None
    }

    /// Extract text content from the first element matching any of the selectors
    fn extract_text(&self, doc: &Html, selector_str: &str) -> Result<String, FetchError> {
        let selector = Selector::parse(selector_str)
            .map_err(|_| FetchError::ParseError(format!("Invalid selector: {}", selector_str)))?;

        if let Some(element) = doc.select(&selector).next() {
            let text = element.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                return Ok(text);
            }
        }

        Err(FetchError::ParseError(format!(
            "Could not find element: {}",
            selector_str
        )))
    }

    /// Extract HTML content from the first element matching any of the selectors
    fn extract_html(&self, doc: &Html, selector_str: &str) -> Result<String, FetchError> {
        let selector = Selector::parse(selector_str)
            .map_err(|_| FetchError::ParseError(format!("Invalid selector: {}", selector_str)))?;

        if let Some(element) = doc.select(&selector).next() {
            return Ok(element.inner_html());
        }

        Err(FetchError::ParseError(format!(
            "Could not find element: {}",
            selector_str
        )))
    }

    /// Clean up article content HTML
    fn clean_content(&self, content: &str) -> String {
        // Parse and clean up the content
        let doc = Html::parse_fragment(content);

        // Remove unwanted elements (scripts, styles, ads)
        let mut cleaned = content.to_string();

        // Remove script tags
        if let Ok(script_selector) = Selector::parse("script") {
            for element in doc.select(&script_selector) {
                let script_html = element.html();
                cleaned = cleaned.replace(&script_html, "");
            }
        }

        // Remove style tags
        if let Ok(style_selector) = Selector::parse("style") {
            for element in doc.select(&style_selector) {
                let style_html = element.html();
                cleaned = cleaned.replace(&style_html, "");
            }
        }

        // Remove noscript tags
        if let Ok(noscript_selector) = Selector::parse("noscript") {
            for element in doc.select(&noscript_selector) {
                let noscript_html = element.html();
                cleaned = cleaned.replace(&noscript_html, "");
            }
        }

        cleaned
    }

    /// Check if an endpoint is alive and responding
    pub async fn health_check(&self, endpoint: &str) -> bool {
        self.client
            .get(endpoint)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

/// Errors that can occur when fetching articles
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("HTTP error: {0}")]
    HttpError(u16),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("All endpoints failed")]
    AllEndpointsFailed,

    #[error("No endpoints configured")]
    NoEndpointsConfigured,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_client() {
        let endpoints = vec![
            "https://freedium.cfd/".to_string(),
            "https://freedium-mirror.cfd/".to_string(),
        ];
        let client = FreediumClient::new(endpoints.clone());
        assert_eq!(client.endpoints, endpoints);
    }

    #[test]
    fn test_url_encoding() {
        let medium_url = "https://medium.com/@user/article-title-abc123";
        let encoded = urlencoding::encode(medium_url);
        assert!(encoded.contains("%3A%2F%2F"));
    }
}
