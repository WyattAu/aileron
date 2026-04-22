use std::collections::HashSet;
use std::io::BufRead;
use std::path::PathBuf;
use std::str::FromStr;
use tracing::{info, warn};

const HTTPS_SAFE_LIST_URL: &str =
    "https://raw.githubusercontent.com/easylist/easylist/master/easylist_httpseasy.txt";

const TRACKING_DOMAINS_URL: &str =
    "https://raw.githubusercontent.com/disconnectme/disconnect-tracking-protection/master/services.json";

const DEFAULT_TRACKING_DOMAINS: &[&str] = &[
    "doubleclick.net",
    "googlesyndication.com",
    "googleadservices.com",
    "google-analytics.com",
    "googletagmanager.com",
    "facebook.net",
    "facebook.com",
    "fbcdn.net",
    "scorecardresearch.com",
    "amazon-adsystem.com",
    "adsrvr.org",
    "adnxs.com",
    "adsymptotic.com",
    "rubiconproject.com",
    "pubmatic.com",
    "openx.net",
    "casalemedia.com",
    "quantserve.com",
    "moatads.com",
    "taboola.com",
    "outbrain.com",
    "criteo.com",
    "criteo.net",
    "media.net",
    "contextweb.com",
    "bidswitch.net",
    "adform.net",
    "agkn.com",
    "adsrvr.org",
    "pixel.facebook.com",
    "analytics.facebook.com",
    "analytics.twitter.com",
    "bat.bing.com",
    "connect.facebook.net",
    "platform.twitter.com",
    "cdn.mxpnl.com",
    "cdn.segment.com",
    "cdn.amplitude.com",
    "fullstory.com",
    "hotjar.com",
    "mouseflow.com",
    "crazyegg.com",
    "optimizely.com",
];

pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(crate::platform::paths::config_dir)
}

pub fn https_safe_list_path() -> PathBuf {
    config_dir().join("https_safe_list.txt")
}

pub fn tracking_domains_path() -> PathBuf {
    config_dir().join("tracking_domains.txt")
}

pub fn load_https_safe_list() -> HashSet<String> {
    let path = https_safe_list_path();
    if !path.exists()
        && let Err(e) = download_https_safe_list()
    {
        warn!("Failed to download HTTPS safe list: {}", e);
    }
    let list = read_domain_list(&path);
    info!("Loaded {} HTTPS safe domains from {:?}", list.len(), path);
    list
}

pub fn download_https_safe_list() -> anyhow::Result<()> {
    let path = https_safe_list_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let response = std::io::BufReader::new(
        attohttpc::get(HTTPS_SAFE_LIST_URL)
            .send()
            .map_err(|e| anyhow::anyhow!("Download failed: {}", e))?,
    );
    let mut domains = HashSet::new();

    for line in response.lines() {
        let line = line.map_err(|e| anyhow::anyhow!("Read line failed: {}", e))?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('!') || line.starts_with('[') {
            continue;
        }
        let domain = line
            .strip_prefix("||")
            .unwrap_or(line)
            .trim_end_matches('^')
            .trim_end_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("");
        if !domain.is_empty() {
            domains.insert(domain.to_lowercase());
        }
    }

    let mut all_domains: Vec<String> = domains.into_iter().collect();
    all_domains.sort();
    let content = all_domains.join("\n") + "\n";
    std::fs::write(&path, content)?;
    info!("Downloaded {} HTTPS safe domains", all_domains.len());
    Ok(())
}

pub fn load_tracking_domains() -> Vec<String> {
    let path = tracking_domains_path();
    if !path.exists()
        && let Err(e) = download_tracking_domains()
    {
        warn!("Failed to download tracking domains: {}", e);
    }
    let mut domains = read_domain_list(&path);
    if domains.is_empty() {
        for d in DEFAULT_TRACKING_DOMAINS {
            domains.insert(d.to_lowercase());
        }
    }
    info!("Loaded {} tracking domains", domains.len());
    domains.into_iter().collect()
}

pub fn download_tracking_domains() -> anyhow::Result<()> {
    let path = tracking_domains_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut domains = HashSet::new();
    for d in DEFAULT_TRACKING_DOMAINS {
        domains.insert(d.to_lowercase());
    }

    if let Ok(response) = attohttpc::get(TRACKING_DOMAINS_URL).send()
        && let Ok(body) = response.text()
        && let Ok(json) = serde_json::Value::from_str(&body)
    {
        extract_domains_from_disconnect(&json, &mut domains);
    }

    let mut all_domains: Vec<String> = domains.into_iter().collect();
    all_domains.sort();
    let content = all_domains.join("\n") + "\n";
    std::fs::write(&path, content)?;
    info!("Downloaded {} tracking domains", all_domains.len());
    Ok(())
}

fn extract_domains_from_disconnect(json: &serde_json::Value, domains: &mut HashSet<String>) {
    if let Some(categories) = json.as_object() {
        for (_category_name, category) in categories {
            if let Some(category_obj) = category.as_object() {
                for (_network_name, network) in category_obj {
                    for domains_list in network
                        .get("properties")
                        .or_else(|| network.get("resources"))
                        .and_then(|v| v.as_array())
                        .into_iter()
                        .flatten()
                    {
                        if let Some(domain) = domains_list.as_str() {
                            domains.insert(domain.to_lowercase());
                        }
                    }
                }
            }
        }
    }
}

fn read_domain_list(path: &PathBuf) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            let domain = line.trim().to_lowercase();
            if !domain.is_empty() && !domain.starts_with('#') {
                set.insert(domain);
            }
        }
    }
    set
}

pub fn is_https_safe(domain: &str, safe_list: &HashSet<String>) -> bool {
    let domain_lower = domain.to_lowercase();
    if safe_list.contains(&domain_lower) {
        return true;
    }
    if let Some(dot_pos) = domain_lower.find('.') {
        let parent = &domain_lower[dot_pos + 1..];
        if safe_list.contains(parent) {
            return true;
        }
    }
    false
}

pub fn should_upgrade_to_https(url: &str, safe_list: &HashSet<String>) -> Option<String> {
    if let Ok(parsed) = url::Url::parse(url)
        && parsed.scheme() == "http"
        && let Some(host) = parsed.host_str()
        && is_https_safe(host, safe_list)
    {
        let mut https_url = parsed;
        https_url.set_scheme("https").ok()?;
        return Some(https_url.to_string());
    }
    None
}

pub fn privacy_initialization_script(tracking_protection_enabled: bool) -> String {
    let mut script = String::new();

    script.push_str(
        r#"try {
    var meta = document.createElement('meta');
    meta.name = 'referrer';
    meta.content = 'strict-origin-when-cross-origin';
    (document.head || document.documentElement).appendChild(meta);
} catch(e) {}
"#,
    );

    if tracking_protection_enabled {
        script.push_str(
            r#"try {
    var csp = document.createElement('meta');
    csp['http-equiv'] = 'Content-Security-Policy';
    csp.content = "referrer strict-origin-when-cross-origin";
    (document.head || document.documentElement).appendChild(csp);
} catch(e) {}
"#,
        );
    }

    script
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_is_https_safe_exact() {
        let mut list = HashSet::new();
        list.insert("example.com".to_string());
        assert!(is_https_safe("example.com", &list));
        assert!(!is_https_safe("other.com", &list));
    }

    #[test]
    fn test_is_https_safe_subdomain() {
        let mut list = HashSet::new();
        list.insert("example.com".to_string());
        assert!(is_https_safe("www.example.com", &list));
        assert!(is_https_safe("api.example.com", &list));
    }

    #[test]
    fn test_is_https_safe_not_found() {
        let list = HashSet::new();
        assert!(!is_https_safe("example.com", &list));
    }

    #[test]
    fn test_should_upgrade_to_https() {
        let mut list = HashSet::new();
        list.insert("example.com".to_string());

        assert_eq!(
            should_upgrade_to_https("http://example.com/path", &list),
            Some("https://example.com/path".to_string())
        );
        assert_eq!(
            should_upgrade_to_https("http://www.example.com/path", &list),
            Some("https://www.example.com/path".to_string())
        );
        assert_eq!(
            should_upgrade_to_https("https://example.com/path", &list),
            None
        );
        assert_eq!(
            should_upgrade_to_https("http://unknown.com/path", &list),
            None
        );
    }

    #[test]
    fn test_should_upgrade_preserves_query() {
        let mut list = HashSet::new();
        list.insert("example.com".to_string());

        assert_eq!(
            should_upgrade_to_https("http://example.com/search?q=test&page=1", &list),
            Some("https://example.com/search?q=test&page=1".to_string())
        );
    }

    #[test]
    fn test_should_upgrade_preserves_port() {
        let mut list = HashSet::new();
        list.insert("example.com".to_string());

        assert_eq!(
            should_upgrade_to_https("http://example.com:8080/path", &list),
            Some("https://example.com:8080/path".to_string())
        );
    }

    #[test]
    fn test_should_upgrade_non_http_unchanged() {
        let mut list = HashSet::new();
        list.insert("example.com".to_string());

        assert_eq!(should_upgrade_to_https("aileron://welcome", &list), None);
        assert_eq!(
            should_upgrade_to_https("ftp://example.com/file", &list),
            None
        );
    }

    #[test]
    fn test_read_domain_list_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_domains.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "example.com\nsub.example.com\n\n# comment\n").unwrap();

        let list = read_domain_list(&path);
        assert!(list.contains("example.com"));
        assert!(list.contains("sub.example.com"));
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_read_domain_list_missing_file() {
        let path = PathBuf::from("/nonexistent/path/domains.txt");
        let list = read_domain_list(&path);
        assert!(list.is_empty());
    }

    #[test]
    fn test_privacy_initialization_script_content() {
        let script = privacy_initialization_script(true);
        assert!(script.contains("strict-origin-when-cross-origin"));
        assert!(script.contains("Content-Security-Policy"));
    }

    #[test]
    fn test_privacy_initialization_script_tracking_disabled() {
        let script = privacy_initialization_script(false);
        assert!(script.contains("strict-origin-when-cross-origin"));
        assert!(!script.contains("Content-Security-Policy"));
    }

    #[test]
    fn test_default_tracking_domains_not_empty() {
        assert!(!DEFAULT_TRACKING_DOMAINS.is_empty());
        for d in DEFAULT_TRACKING_DOMAINS {
            assert!(d.contains('.'));
        }
    }

    #[test]
    fn test_extract_domains_from_disconnect() {
        let json = serde_json::json!({
            "Advertising": {
                "TestNetwork": {
                    "properties": ["ads.example.com", "tracker.example.net"]
                }
            },
            "Analytics": {
                "TestAnalytics": {
                    "resources": ["analytics.example.com"]
                }
            }
        });

        let mut domains = HashSet::new();
        extract_domains_from_disconnect(&json, &mut domains);
        assert!(domains.contains("ads.example.com"));
        assert!(domains.contains("tracker.example.net"));
        assert!(domains.contains("analytics.example.com"));
        assert_eq!(domains.len(), 3);
    }
}
