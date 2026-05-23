//! WebDAV transport for sync protocol.
//!
//! Implements PUT/GET/DELETE/PROPFIND operations over HTTP using `reqwest`.
//! Supports HTTP Basic and Bearer token authentication with retry logic.

use std::path::Path;

use anyhow::Result;
use tracing::{debug, warn};

/// WebDAV client configuration.
#[derive(Debug, Clone)]
pub struct WebdavConfig {
    /// Base URL of the WebDAV server (e.g., `https://dav.example.com/aileron/`).
    pub base_url: String,
    /// Authentication credentials.
    pub auth: WebdavAuth,
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Initial retry delay in milliseconds.
    pub retry_delay_ms: u64,
    /// Maximum retry delay in milliseconds (caps exponential backoff).
    pub max_retry_delay_ms: u64,
}

/// Authentication method for WebDAV.
#[derive(Debug, Clone)]
pub enum WebdavAuth {
    /// HTTP Basic authentication (username + password).
    Basic { username: String, password: String },
    /// Bearer token authentication (OAuth2, etc.).
    Bearer { token: String },
    /// No authentication.
    None,
}

/// WebDAV file metadata from PROPFIND response.
#[derive(Debug, Clone)]
pub struct WebdavFileInfo {
    pub href: String,
    pub is_directory: bool,
    pub content_length: Option<u64>,
    pub last_modified: Option<String>,
}

/// WebDAV transport client.
pub struct WebdavClient {
    config: WebdavConfig,
    http: reqwest::blocking::Client,
}

impl WebdavClient {
    pub fn new(config: WebdavConfig) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { config, http }
    }

    /// Build the full URL for a relative path.
    fn url(&self, relative_path: &str) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        format!("{base}/{relative_path}")
    }

    /// Apply authentication to a request builder.
    fn apply_auth(
        &self,
        builder: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        match &self.config.auth {
            WebdavAuth::Basic { username, password } => {
                builder.basic_auth(username, Some(password))
            }
            WebdavAuth::Bearer { token } => builder.bearer_auth(token),
            WebdavAuth::None => builder,
        }
    }

    /// Execute a request with exponential backoff retry.
    fn execute_with_retry(
        &self,
        build_request: impl Fn(&reqwest::blocking::Client) -> reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::Response> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let delay = std::cmp::min(
                    self.config.retry_delay_ms * 2u64.pow(attempt - 1),
                    self.config.max_retry_delay_ms,
                );
                debug!("Retry attempt {attempt}, waiting {delay}ms");
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }

            let builder = build_request(&self.http);
            let builder = self.apply_auth(builder);

            match builder.send() {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
                        return Ok(response);
                    }
                    if status.is_server_error() && attempt < self.config.max_retries {
                        warn!(
                            "Server error {status}, will retry (attempt {attempt}/{})",
                            self.config.max_retries
                        );
                        last_error = Some(anyhow::anyhow!("HTTP {status}"));
                        continue;
                    }
                    if status == reqwest::StatusCode::CONFLICT {
                        // Collection doesn't exist, caller should create it
                        return Ok(response);
                    }
                    return Ok(response);
                }
                Err(e) => {
                    if attempt < self.config.max_retries {
                        warn!(
                            "Request failed: {e}, will retry (attempt {attempt}/{})",
                            self.config.max_retries
                        );
                        last_error = Some(anyhow::anyhow!(e.to_string()));
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Max retries exceeded")))
    }

    /// Create a collection (directory) on the WebDAV server.
    pub fn create_collection(&self, path: &str) -> Result<()> {
        let url = self.url(path);
        debug!("MKCOL {url}");

        let response = self.execute_with_retry(|http| {
            http.request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), &url)
        })?;

        let status = response.status();
        if status.is_success() || status == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            // METHOD_NOT_ALLOWED means the collection already exists
            Ok(())
        } else {
            Err(anyhow::anyhow!("MKCOL failed: HTTP {status}"))
        }
    }

    /// Upload a file to the WebDAV server.
    pub fn put(&self, remote_path: &str, data: &[u8]) -> Result<()> {
        let url = self.url(remote_path);
        debug!("PUT {url} ({} bytes)", data.len());

        let data = data.to_vec();
        let response = self.execute_with_retry(|http| http.put(&url).body(data.clone()))?;

        let status = response.status();
        if status.is_success() || status == reqwest::StatusCode::CREATED {
            Ok(())
        } else {
            Err(anyhow::anyhow!("PUT failed: HTTP {status}"))
        }
    }

    /// Download a file from the WebDAV server.
    pub fn get(&self, remote_path: &str) -> Result<Option<Vec<u8>>> {
        let url = self.url(remote_path);
        debug!("GET {url}");

        let response = self.execute_with_retry(|http| http.get(&url))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(anyhow::anyhow!("GET failed: HTTP {status}"));
        }

        let bytes = response.bytes()?;
        Ok(Some(bytes.to_vec()))
    }

    /// Delete a file from the WebDAV server.
    pub fn delete(&self, remote_path: &str) -> Result<bool> {
        let url = self.url(remote_path);
        debug!("DELETE {url}");

        let response = self.execute_with_retry(|http| http.delete(&url))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if status.is_success() {
            Ok(true)
        } else {
            Err(anyhow::anyhow!("DELETE failed: HTTP {status}"))
        }
    }

    /// List files in a collection via PROPFIND.
    pub fn propfind(&self, path: &str) -> Result<Vec<WebdavFileInfo>> {
        let url = self.url(path);
        debug!("PROPFIND {url}");

        let depth_header = "1";
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
    <d:prop>
        <d:getcontentlength/>
        <d:getlastmodified/>
        <d:resourcetype/>
    </d:prop>
</d:propfind>"#;

        let response = self.execute_with_retry(|http| {
            let method = reqwest::Method::from_bytes(b"PROPFIND").unwrap();
            http.request(method, &url)
                .header("Depth", depth_header)
                .header("Content-Type", "application/xml")
                .body(body.to_string())
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }
        if !status.is_success() && status != reqwest::StatusCode::MULTI_STATUS {
            return Err(anyhow::anyhow!("PROPFIND failed: HTTP {status}"));
        }

        let body = response.text()?;
        parse_propfind_response(&body)
    }

    /// Ensure a directory path exists on the server by creating collections.
    pub fn ensure_collection(&self, path: &str) -> Result<()> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = String::new();

        for part in parts {
            current.push('/');
            current.push_str(part);
            self.create_collection(&current)?;
        }

        Ok(())
    }

    /// Upload a local file to the remote path.
    pub fn upload_file(&self, local_path: &Path, remote_path: &str) -> Result<()> {
        let data = std::fs::read(local_path)?;
        // Ensure parent collection exists
        if let Some(parent) = Path::new(remote_path).parent() {
            let parent_str = parent.to_str().unwrap_or("");
            if !parent_str.is_empty() {
                self.ensure_collection(parent_str)?;
            }
        }
        self.put(remote_path, &data)
    }

    /// Download a remote file to a local path.
    pub fn download_file(&self, remote_path: &str, local_path: &Path) -> Result<bool> {
        let data = match self.get(remote_path)? {
            Some(d) => d,
            None => return Ok(false),
        };

        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(local_path, &data)?;
        Ok(true)
    }
}

/// Parse a PROPFIND XML response into file info structs.
fn parse_propfind_response(xml: &str) -> Result<Vec<WebdavFileInfo>> {
    let mut files = Vec::new();

    // Simple XML parsing for PROPFIND response
    // Production code would use a proper XML parser, but this handles common cases
    for response in split_xml_responses(xml) {
        let href = extract_xml_tag(&response, "d:href")
            .or_else(|| extract_xml_tag(&response, "href"))
            .unwrap_or_default();

        let is_directory = response.contains("<d:collection/>")
            || response.contains("<d:collection />")
            || response.contains("<collection/>")
            || response.contains("<collection />")
            || response.contains("<d:resourcetype><d:collection/></d:resourcetype>")
            || response.contains("<resourcetype><collection/></resourcetype>");

        let content_length = extract_xml_tag(&response, "d:getcontentlength")
            .or_else(|| extract_xml_tag(&response, "getcontentlength"))
            .and_then(|s| s.parse::<u64>().ok());

        let last_modified = extract_xml_tag(&response, "d:getlastmodified")
            .or_else(|| extract_xml_tag(&response, "getlastmodified"));

        files.push(WebdavFileInfo {
            href,
            is_directory,
            content_length,
            last_modified,
        });
    }

    Ok(files)
}

fn split_xml_responses(xml: &str) -> Vec<String> {
    let mut responses = Vec::new();
    let mut start = 0;

    while let Some(s) = xml[start..].find("<d:response>") {
        let abs_start = start + s;
        if let Some(end) = xml[abs_start..].find("</d:response>") {
            responses.push(xml[abs_start..abs_start + end + "</d:response>".len()].to_string());
            start = abs_start + end + "</d:response>".len();
        } else {
            break;
        }
    }

    // Also try without namespace prefix
    if responses.is_empty() {
        let mut start = 0;
        while let Some(s) = xml[start..].find("<response>") {
            let abs_start = start + s;
            if let Some(end) = xml[abs_start..].find("</response>") {
                responses.push(xml[abs_start..abs_start + end + "</response>".len()].to_string());
                start = abs_start + end + "</response>".len();
            } else {
                break;
            }
        }
    }

    responses
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)?;
    let content_start = start + open.len();
    let end = xml[content_start..].find(&close)?;
    Some(xml[content_start..content_start + end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webdav_config_builder() {
        let config = WebdavConfig {
            base_url: "https://dav.example.com/aileron".into(),
            auth: WebdavAuth::Basic {
                username: "user".into(),
                password: "pass".into(),
            },
            max_retries: 3,
            retry_delay_ms: 1000,
            max_retry_delay_ms: 30000,
        };
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
    }

    #[test]
    fn test_webdav_auth_bearer() {
        let auth = WebdavAuth::Bearer {
            token: "abc123".into(),
        };
        match auth {
            WebdavAuth::Bearer { token } => assert_eq!(token, "abc123"),
            _ => panic!("Expected Bearer auth"),
        }
    }

    #[test]
    fn test_webdav_auth_none() {
        let auth = WebdavAuth::None;
        assert!(matches!(auth, WebdavAuth::None));
    }

    #[test]
    fn test_url_building() {
        let config = WebdavConfig {
            base_url: "https://dav.example.com/aileron/".into(),
            auth: WebdavAuth::None,
            max_retries: 3,
            retry_delay_ms: 1000,
            max_retry_delay_ms: 30000,
        };
        let client = WebdavClient::new(config);
        assert_eq!(
            client.url("data/file.txt"),
            "https://dav.example.com/aileron/data/file.txt"
        );
    }

    #[test]
    fn test_url_building_no_trailing_slash() {
        let config = WebdavConfig {
            base_url: "https://dav.example.com/aileron".into(),
            auth: WebdavAuth::None,
            max_retries: 3,
            retry_delay_ms: 1000,
            max_retry_delay_ms: 30000,
        };
        let client = WebdavClient::new(config);
        assert_eq!(
            client.url("data/file.txt"),
            "https://dav.example.com/aileron/data/file.txt"
        );
    }

    #[test]
    fn test_parse_propfind_response() {
        let xml = r#"<?xml version="1.0"?>
<multistatus xmlns="DAV:">
    <response>
        <href>/aileron/</href>
        <propstat>
            <prop>
                <resourcetype><collection/></resourcetype>
            </prop>
        </propstat>
    </response>
    <response>
        <href>/aileron/config.toml</href>
        <propstat>
            <prop>
                <getcontentlength>256</getcontentlength>
                <getlastmodified>Mon, 01 Jan 2024 00:00:00 GMT</getlastmodified>
                <resourcetype/>
            </prop>
        </propstat>
    </response>
</multistatus>"#;

        let files = parse_propfind_response(xml).unwrap();
        assert_eq!(files.len(), 2);

        assert_eq!(files[0].href, "/aileron/");
        assert!(files[0].is_directory);

        assert_eq!(files[1].href, "/aileron/config.toml");
        assert!(!files[1].is_directory);
        assert_eq!(files[1].content_length, Some(256));
        assert_eq!(
            files[1].last_modified.as_deref(),
            Some("Mon, 01 Jan 2024 00:00:00 GMT")
        );
    }

    #[test]
    fn test_parse_propfind_namespaced() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
    <d:response>
        <d:href>/aileron/data.json</d:href>
        <d:propstat>
            <d:prop>
                <d:getcontentlength>1024</d:getcontentlength>
            </d:prop>
        </d:propstat>
    </d:response>
</d:multistatus>"#;

        let files = parse_propfind_response(xml).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].href, "/aileron/data.json");
        assert_eq!(files[0].content_length, Some(1024));
        assert!(!files[0].is_directory);
    }

    #[test]
    fn test_parse_empty_propfind() {
        let xml = r#"<?xml version="1.0"?><multistatus/>"#;
        let files = parse_propfind_response(xml).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_extract_xml_tag() {
        let xml = "<foo>bar</foo>";
        assert_eq!(extract_xml_tag(xml, "foo"), Some("bar".to_string()));
        assert_eq!(extract_xml_tag(xml, "baz"), None);
    }

    #[test]
    fn test_webdav_file_info() {
        let info = WebdavFileInfo {
            href: "/test.txt".into(),
            is_directory: false,
            content_length: Some(100),
            last_modified: Some("date".into()),
        };
        assert_eq!(info.href, "/test.txt");
        assert!(!info.is_directory);
    }
}
