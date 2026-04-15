use tracing::{info, warn};
use url::Url;

/// Open a URL in the user's default system browser.
/// Returns Ok(()) if the open command succeeded, Err on failure.
pub fn open_in_system_browser(url: &Url) -> anyhow::Result<()> {
    let url_str = url.as_str();
    info!("Opening in system browser: {}", url_str);
    open::that(url_str).map_err(|e| {
        warn!("Failed to open system browser for '{}': {}", url_str, e);
        anyhow::anyhow!("Failed to open system browser: {}", e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_url_format() {
        // Just verify URL formatting — we can't actually open a browser in tests
        let url = Url::parse("https://example.com").unwrap();
        assert_eq!(url.as_str(), "https://example.com/");
    }

    #[test]
    fn test_open_with_fragment() {
        let url = Url::parse("https://example.com/docs#section").unwrap();
        assert!(url.as_str().contains("#section"));
    }
}
