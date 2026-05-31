use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
            // Freedium is a SvelteKit SSR app: the rendered article is served as
            // devalue-encoded JSON at <urlencoded medium url>/__data.json.
            let url = format!(
                "{}{}/__data.json",
                endpoint,
                urlencoding::encode(medium_url)
            );

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

        let body = response.text().await?;
        self.parse_data_json(&body, original_url, endpoint)
    }

    /// Parse Freedium's SvelteKit `__data.json` response and extract the article.
    ///
    /// The page itself only ships a loading skeleton; the rendered article is
    /// streamed as a "chunk" whose `data` array uses devalue's flattened-index
    /// encoding. We locate that chunk, decode it, and read the `html` body plus
    /// the `article` metadata (`title`, `author.name`).
    fn parse_data_json(
        &self,
        body: &str,
        original_url: &str,
        endpoint: &str,
    ) -> Result<Article, FetchError> {
        // The response is newline-delimited JSON; the page payload is a "chunk".
        let mut root = None;
        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if value.get("type").and_then(Value::as_str) == Some("chunk") {
                if let Some(data) = value.get("data").and_then(Value::as_array) {
                    root = Some(devalue_resolve(data, 0, 0));
                    break;
                }
            }
        }

        let root = root.ok_or_else(|| {
            FetchError::ParseError("No article chunk found in __data.json".to_string())
        })?;

        // Surface a server-side render failure instead of rendering an empty page.
        let html = root
            .get("html")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if html.trim().is_empty() {
            return Err(FetchError::ParseError(
                "Freedium returned no rendered article content".to_string(),
            ));
        }

        let article = root.get("article");
        let title = article
            .and_then(|a| a.get("title"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Untitled Article")
            .to_string();
        let author = article
            .and_then(|a| a.get("author"))
            .and_then(|a| a.get("name"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Unknown Author")
            .to_string();

        // Header/hero image. Freedium serves images as host-relative paths
        // (e.g. "/img/700/0*abc"), so make it absolute against the endpoint.
        let header_image_url = article
            .and_then(|a| a.get("postImage"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(|s| absolutize_url(s, endpoint));

        // Inline article images are host-relative too (src + srcset); rewrite
        // them against the endpoint host so the webview can load them.
        let html = absolutize_img_paths(&html, endpoint);
        let cleaned_content = self.clean_content(&html);

        Ok(Article {
            title,
            author,
            // The API exposes only the author's name, not a profile URL.
            author_url: None,
            header_image_url,
            content_html: cleaned_content,
            original_url: original_url.to_string(),
            fetched_from: endpoint.to_string(),
        })
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

/// Join a possibly host-relative URL against the endpoint base.
///
/// Freedium serves images as paths like `/img/700/0*abc`; absolute URLs
/// (`http://`, `https://`, protocol-relative `//`, or `data:`) are left as-is.
fn absolutize_url(url: &str, endpoint: &str) -> String {
    let u = url.trim();
    if u.starts_with("http://")
        || u.starts_with("https://")
        || u.starts_with("//")
        || u.starts_with("data:")
    {
        return u.to_string();
    }
    format!("{}{}", endpoint.trim_end_matches('/'), u)
}

/// Rewrite host-relative `src="/..."` and `srcset="..."` image references in
/// `html` so they resolve against the endpoint host inside the webview.
fn absolutize_img_paths(html: &str, endpoint: &str) -> String {
    let base = endpoint.trim_end_matches('/');

    // src="/img/..." -> src="<base>/img/..."  (only paths starting with a single
    // slash; "//" is protocol-relative and already absolute).
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let needle = "src=\"/";
    while let Some(pos) = rest.find(needle) {
        out.push_str(&rest[..pos]);
        // Keep `src="` then insert base before the leading slash.
        out.push_str("src=\"");
        let after = &rest[pos + needle.len() - 1..]; // points at the leading '/'
        if after.starts_with("//") {
            // protocol-relative; leave untouched
            out.push('/');
            rest = &after[1..];
        } else {
            out.push_str(base);
            rest = after;
        }
    }
    out.push_str(rest);

    // srcset entries are comma-separated "<url> <descriptor>"; rewrite each
    // host-relative URL. Process the accumulated string in place.
    rewrite_srcset(&out, base)
}

/// Rewrite host-relative URLs inside every `srcset="..."` attribute.
fn rewrite_srcset(html: &str, base: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let attr = "srcset=\"";
    while let Some(pos) = rest.find(attr) {
        out.push_str(&rest[..pos + attr.len()]);
        let after = &rest[pos + attr.len()..];
        let end = match after.find('"') {
            Some(e) => e,
            None => {
                rest = after;
                break;
            }
        };
        let value = &after[..end];
        let rewritten = value
            .split(',')
            .map(|candidate| {
                let candidate = candidate.trim();
                let mut parts = candidate.splitn(2, char::is_whitespace);
                let url = parts.next().unwrap_or("");
                let descriptor = parts.next();
                let abs = if url.starts_with('/') && !url.starts_with("//") {
                    format!("{}{}", base, url)
                } else {
                    url.to_string()
                };
                match descriptor {
                    Some(d) => format!("{} {}", abs, d),
                    None => abs,
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&rewritten);
        out.push('"');
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Resolve a value from devalue's flattened-array encoding.
///
/// devalue stores every value in a single flat array; container fields and
/// array elements hold the *index* of their value rather than the value itself.
/// We follow those indices to rebuild a normal [`serde_json::Value`]. `depth`
/// guards against cyclic references.
fn devalue_resolve(arr: &[Value], index: i64, depth: usize) -> Value {
    if depth > 64 || index < 0 {
        return Value::Null;
    }
    let node = match arr.get(index as usize) {
        Some(node) => node,
        None => return Value::Null,
    };
    match node {
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, child) in map {
                let resolved = match child.as_i64() {
                    Some(i) => devalue_resolve(arr, i, depth + 1),
                    None => child.clone(),
                };
                out.insert(key.clone(), resolved);
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            let resolved = items
                .iter()
                .map(|item| match item.as_i64() {
                    Some(i) => devalue_resolve(arr, i, depth + 1),
                    None => item.clone(),
                })
                .collect();
            Value::Array(resolved)
        }
        literal => literal.clone(),
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

    #[test]
    fn test_parse_data_json_extracts_article() {
        // Minimal reproduction of SvelteKit's __data.json devalue format: a
        // "chunk" line whose `data` array references its values by index.
        // index: 0=root 1=html 2=markdown 3=article 4=cacheStatus 5=renderTimeMs
        //        6=error 7=title 8=subtitle 9=author 10=name 11=avatar 12=postImage
        let body = concat!(
            "{\"type\":\"data\",\"nodes\":[null,{\"type\":\"data\",\"data\":[{\"slug\":1},\"https://medium.com/@x/y\"]}]}\n",
            "{\"type\":\"chunk\",\"id\":1,\"data\":[",
            "{\"html\":1,\"markdown\":2,\"article\":3,\"cacheStatus\":4,\"renderTimeMs\":5,\"error\":6},",
            "\"<h3>Heading</h3><p>Hello world</p><img src=\\\"/img/4000/pic.png\\\" srcset=\\\"/img/700/pic.png 700w, /img/4000/pic.png 4000w\\\"><script>bad()</script>\",",
            "\"# Heading\",",
            "{\"title\":7,\"subtitle\":8,\"author\":9,\"postImage\":12},",
            "\"hit\",",
            "13,",
            "null,",
            "\"My Title\",",
            "\"A subtitle\",",
            "{\"name\":10,\"avatar\":11},",
            "\"Jane Doe\",",
            "\"/img/avatar.png\",",
            "\"/img/700/hero.jpg\"",
            "]}"
        );
        let client = FreediumClient::new(vec!["https://freedium-mirror.cfd/".to_string()]);
        let article = client
            .parse_data_json(body, "https://medium.com/@x/y", "https://freedium-mirror.cfd/")
            .expect("should parse the chunk");

        assert_eq!(article.title, "My Title");
        assert_eq!(article.author, "Jane Doe");
        assert!(article.content_html.contains("Hello world"));
        // clean_content must strip the <script> tag.
        assert!(!article.content_html.contains("bad()"));
        assert_eq!(article.fetched_from, "https://freedium-mirror.cfd/");
        // Header image comes from `postImage` and is absolutized.
        assert_eq!(
            article.header_image_url.as_deref(),
            Some("https://freedium-mirror.cfd/img/700/hero.jpg")
        );
        // Inline image src + srcset are absolutized against the endpoint host.
        assert!(article
            .content_html
            .contains("src=\"https://freedium-mirror.cfd/img/4000/pic.png\""));
        assert!(article
            .content_html
            .contains("https://freedium-mirror.cfd/img/700/pic.png 700w"));
        assert!(article
            .content_html
            .contains("https://freedium-mirror.cfd/img/4000/pic.png 4000w"));
    }

    #[test]
    fn test_absolutize_url() {
        let ep = "https://freedium-mirror.cfd/";
        assert_eq!(
            absolutize_url("/img/700/x.jpg", ep),
            "https://freedium-mirror.cfd/img/700/x.jpg"
        );
        // Already-absolute URLs are untouched.
        assert_eq!(
            absolutize_url("https://miro.medium.com/x.png", ep),
            "https://miro.medium.com/x.png"
        );
        assert_eq!(absolutize_url("//cdn.example/x.png", ep), "//cdn.example/x.png");
        assert_eq!(
            absolutize_url("data:image/png;base64,AAA", ep),
            "data:image/png;base64,AAA"
        );
    }

    #[test]
    fn test_absolutize_img_paths_leaves_links_and_absolute() {
        let ep = "https://freedium-mirror.cfd/";
        let html = "<a href=\"/about\">x</a><img src=\"/img/1.png\"><img src=\"https://miro.medium.com/2.png\">";
        let out = absolutize_img_paths(html, ep);
        // Anchor href is NOT rewritten, only image src.
        assert!(out.contains("href=\"/about\""));
        assert!(out.contains("src=\"https://freedium-mirror.cfd/img/1.png\""));
        assert!(out.contains("src=\"https://miro.medium.com/2.png\""));
    }

    #[test]
    fn test_parse_data_json_errors_on_empty_render() {
        // A chunk whose html resolves to an empty string is a failed render.
        let body = "{\"type\":\"chunk\",\"id\":1,\"data\":[{\"html\":1},\"\"]}";
        let client = FreediumClient::new(vec![]);
        assert!(client.parse_data_json(body, "u", "e").is_err());
    }

    #[test]
    fn test_devalue_resolve_nested() {
        let arr: Vec<Value> = serde_json::from_str(
            "[{\"a\":1,\"b\":2},\"x\",{\"c\":3},\"y\"]",
        )
        .unwrap();
        let resolved = devalue_resolve(&arr, 0, 0);
        assert_eq!(resolved["a"], serde_json::json!("x"));
        assert_eq!(resolved["b"]["c"], serde_json::json!("y"));
    }
}
