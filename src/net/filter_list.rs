use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceType {
    Script,
    Image,
    Stylesheet,
    Font,
    Media,
    WebSocket,
    Popup,
    XmlHttpRequest,
    Other,
}

#[derive(Debug, Clone)]
pub struct NetworkFilter {
    pub pattern: String,
    pub is_exception: bool,
    pub resource_types: Option<Vec<ResourceType>>,
    pub third_party_only: bool,
    pub domain_specific: Option<String>,
    pub csp: Option<String>,
    pub remove_header: Option<String>,
    pub redirect: Option<String>,
    pub badfilter: bool,
    pub important: bool,
    pub generichide: bool,
    pub document: bool,
    pub all_resources: bool,
}

#[derive(Debug, Clone)]
pub struct CosmeticFilter {
    pub selector: String,
    pub domains: Option<Vec<String>>,
}

pub struct FilterList {
    pub network_filters: Vec<NetworkFilter>,
    pub cosmetic_filters: Vec<CosmeticFilter>,
    pub source_url: Option<String>,
    pub last_updated: Option<std::time::SystemTime>,
}

impl FilterList {
    pub fn new() -> Self {
        Self {
            network_filters: Vec::new(),
            cosmetic_filters: Vec::new(),
            source_url: None,
            last_updated: None,
        }
    }

    pub fn with_source(url: &str) -> Self {
        Self {
            network_filters: Vec::new(),
            cosmetic_filters: Vec::new(),
            source_url: Some(url.to_string()),
            last_updated: None,
        }
    }

    pub fn parse(content: &str) -> Self {
        let mut list = Self::new();
        list.load_from_text(content);
        list
    }

    pub fn load_from_text(&mut self, content: &str) -> usize {
        let mut count = 0;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('!') || line.starts_with('[') {
                continue;
            }
            if let Some(filter) = Self::parse_line(line) {
                match filter {
                    ParsedFilter::Network(f) => {
                        self.network_filters.push(f);
                        count += 1;
                    }
                    ParsedFilter::Cosmetic(f) => {
                        self.cosmetic_filters.push(f);
                        count += 1;
                    }
                }
            }
        }
        count
    }

    fn parse_line(line: &str) -> Option<ParsedFilter> {
        if line.contains("##") {
            return Self::parse_cosmetic(line);
        }

        let (is_exception, rest) = if let Some(r) = line.strip_prefix("@@") {
            (true, r)
        } else {
            (false, line)
        };

        let (pattern, options_str) = if let Some(pos) = rest.find('$') {
            let (p, opts) = rest.split_at(pos);
            (p, Some(&opts[1..]))
        } else {
            (rest, None)
        };

        let pattern = pattern.trim();
        if pattern.is_empty() {
            return None;
        }

        let mut resource_types = None;
        let mut third_party_only = false;
        let mut csp = None;
        let mut remove_header = None;
        let mut redirect = None;
        let mut badfilter = false;
        let mut important = false;
        let mut generichide = false;
        let mut document = false;
        let mut all_resources = false;

        if let Some(opts) = options_str {
            for opt in opts.split(',') {
                let opt = opt.trim();
                if opt == "third-party" || opt == "third_party" {
                    third_party_only = true;
                } else if let Some(val) = opt.strip_prefix("csp=") {
                    csp = Some(val.to_string());
                } else if let Some(val) = opt.strip_prefix("removeheader=") {
                    remove_header = Some(val.to_string());
                } else if let Some(val) = opt.strip_prefix("redirect=") {
                    redirect = Some(val.to_string());
                } else if opt == "badfilter" {
                    badfilter = true;
                } else if opt == "important" {
                    important = true;
                } else if opt == "generichide" {
                    generichide = true;
                } else if opt == "document" {
                    document = true;
                } else if opt == "all" {
                    all_resources = true;
                } else {
                    let rt = match opt {
                        "script" => Some(ResourceType::Script),
                        "image" => Some(ResourceType::Image),
                        "css" | "stylesheet" => Some(ResourceType::Stylesheet),
                        "font" => Some(ResourceType::Font),
                        "media" => Some(ResourceType::Media),
                        "websocket" => Some(ResourceType::WebSocket),
                        "popup" => Some(ResourceType::Popup),
                        "xmlhttprequest" | "xhr" => Some(ResourceType::XmlHttpRequest),
                        _ => None,
                    };
                    if let Some(t) = rt {
                        resource_types.get_or_insert_with(Vec::new).push(t);
                    }
                }
            }
        }

        if all_resources {
            resource_types
                .get_or_insert_with(Vec::new)
                .push(ResourceType::Script);
            resource_types
                .get_or_insert_with(Vec::new)
                .push(ResourceType::Image);
            resource_types
                .get_or_insert_with(Vec::new)
                .push(ResourceType::Stylesheet);
            resource_types
                .get_or_insert_with(Vec::new)
                .push(ResourceType::Font);
            resource_types
                .get_or_insert_with(Vec::new)
                .push(ResourceType::Media);
            resource_types
                .get_or_insert_with(Vec::new)
                .push(ResourceType::WebSocket);
            resource_types
                .get_or_insert_with(Vec::new)
                .push(ResourceType::Popup);
            resource_types
                .get_or_insert_with(Vec::new)
                .push(ResourceType::XmlHttpRequest);
            resource_types
                .get_or_insert_with(Vec::new)
                .push(ResourceType::Other);
        }

        let domain_specific = if let Some(pos) = pattern.find("||") {
            let domain_part = &pattern[..pos];
            let domain_part = domain_part.trim_end_matches('~');
            if !domain_part.is_empty() && !domain_part.contains('*') {
                Some(domain_part.to_lowercase())
            } else {
                None
            }
        } else {
            None
        };

        Some(ParsedFilter::Network(NetworkFilter {
            pattern: pattern.to_lowercase(),
            is_exception,
            resource_types,
            third_party_only,
            domain_specific,
            csp,
            remove_header,
            redirect,
            badfilter,
            important,
            generichide,
            document,
            all_resources,
        }))
    }

    fn parse_cosmetic(line: &str) -> Option<ParsedFilter> {
        if let Some(pos) = line.find("##") {
            let domain_part = &line[..pos];
            let selector = &line[pos + 2..];

            if selector.trim().is_empty() {
                return None;
            }

            let domains = if !domain_part.is_empty() {
                Some(
                    domain_part
                        .split(',')
                        .map(|d| d.trim().trim_start_matches('~').to_lowercase())
                        .filter(|d| !d.is_empty())
                        .collect(),
                )
            } else {
                None
            };

            Some(ParsedFilter::Cosmetic(CosmeticFilter {
                selector: selector.to_string(),
                domains,
            }))
        } else {
            None
        }
    }

    pub fn rule_count(&self) -> usize {
        self.network_filters.len() + self.cosmetic_filters.len()
    }
}

impl Default for FilterList {
    fn default() -> Self {
        Self::new()
    }
}

enum ParsedFilter {
    Network(NetworkFilter),
    Cosmetic(CosmeticFilter),
}

pub fn filter_list_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.config_dir().join("filter_lists"))
        .unwrap_or_else(|| PathBuf::from("~/.config/aileron/filter_lists"))
}

pub fn ensure_filter_list_dir() -> PathBuf {
    let dir = filter_list_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn load_filter_lists_from_disk() -> Vec<FilterList> {
    let dir = ensure_filter_list_dir();
    let mut lists = Vec::new();

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return lists,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map(|e| e == "txt").unwrap_or(false) {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    let mut fl = FilterList::with_source(
                        path.file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .as_deref()
                            .unwrap_or("unknown"),
                    );
                    let count = fl.load_from_text(&content);
                    fl.last_updated = std::fs::metadata(&path)
                        .ok()
                        .and_then(|m| m.modified().ok());
                    info!("Loaded filter list {:?}: {} rules", path.file_name(), count);
                    lists.push(fl);
                }
                Err(e) => {
                    warn!("Failed to read filter list {:?}: {}", path, e);
                }
            }
        }
    }

    lists
}

pub fn save_filter_list(name: &str, content: &str) -> std::io::Result<PathBuf> {
    let dir = ensure_filter_list_dir();
    let path = dir.join(format!("{}.txt", name));
    std::fs::write(&path, content)?;
    Ok(path)
}

pub fn download_filter_list(url: &str) -> anyhow::Result<String> {
    let response = attohttpc::get(url)
        .header("User-Agent", "Aileron/0.8.1")
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .send()
        .map_err(|e| anyhow::anyhow!("Failed to download filter list: {}", e))?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP request failed with status {}", response.status());
    }

    let body = response
        .text()
        .map_err(|e| anyhow::anyhow!("Response not UTF-8: {}", e))?;
    if body.is_empty() {
        anyhow::bail!("Empty response from {}", url);
    }

    info!("Downloaded filter list from {} ({} bytes)", url, body.len());
    Ok(body)
}

pub fn url_to_filename(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.replace('.', "_")))
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn default_filter_list_urls() -> Vec<(&'static str, &'static str)> {
    vec![
        ("easylist", "https://easylist.to/easylist/easylist.txt"),
        (
            "easyprivacy",
            "https://easylist.to/easylist/easyprivacy.txt",
        ),
        (
            "peter_lowe",
            "https://pgl.yoyo.org/asbserverlist/asbserverlist.txt",
        ),
    ]
}

pub fn initialize_default_filter_lists() -> usize {
    let dir = ensure_filter_list_dir();
    let mut total_rules = 0;

    for (name, url) in default_filter_list_urls() {
        let file_path = dir.join(format!("{}.txt", name));
        if !file_path.exists() {
            info!("Filter list {} not found, downloading from {}", name, url);
            match download_filter_list(url) {
                Ok(content) => {
                    if let Err(e) = std::fs::write(&file_path, &content) {
                        warn!("Failed to save filter list {}: {}", name, e);
                    } else {
                        info!("Saved filter list {} ({} bytes)", name, content.len());
                    }
                    total_rules += FilterList::parse(&content).rule_count();
                }
                Err(e) => {
                    warn!("Failed to download filter list {}: {}", name, e);
                }
            }
        } else {
            info!("Filter list {} already exists at {:?}", name, file_path);
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                total_rules += FilterList::parse(&content).rule_count();
            }
        }
    }

    total_rules
}

pub fn update_all_filter_lists() -> usize {
    let dir = filter_list_dir();
    if !dir.exists() {
        return 0;
    }

    let mut updated = 0;

    for (name, url) in default_filter_list_urls() {
        let file_path = dir.join(format!("{}.txt", name));
        match download_filter_list(url) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&file_path, &content) {
                    warn!("Failed to save updated filter list {}: {}", name, e);
                } else {
                    info!("Updated filter list {}", name);
                    updated += 1;
                }
            }
            Err(e) => {
                warn!("Failed to update filter list {}: {}", name, e);
            }
        }
    }

    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_domain_blocking() {
        let content = "||ads.example.com^\n||tracker.evil.net^";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 2);
        assert_eq!(list.network_filters[0].pattern, "||ads.example.com^");
        assert!(!list.network_filters[0].is_exception);
    }

    #[test]
    fn test_parse_whitelist() {
        let content = "@@||safe.example.com^";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        assert!(list.network_filters[0].is_exception);
    }

    #[test]
    fn test_parse_cosmetic_generic() {
        let content = "##div.ad-banner\n##.sponsored-content";
        let list = FilterList::parse(content);
        assert_eq!(list.cosmetic_filters.len(), 2);
        assert_eq!(list.cosmetic_filters[0].selector, "div.ad-banner");
        assert!(list.cosmetic_filters[0].domains.is_none());
    }

    #[test]
    fn test_parse_cosmetic_domain_specific() {
        let content = "example.com##.ad-slot";
        let list = FilterList::parse(content);
        assert_eq!(list.cosmetic_filters.len(), 1);
        assert_eq!(list.cosmetic_filters[0].selector, ".ad-slot");
        let domains = list.cosmetic_filters[0].domains.as_ref().unwrap();
        assert!(domains.contains(&"example.com".to_string()));
    }

    #[test]
    fn test_parse_resource_type_options() {
        let content = "||cdn.example.com/ads.js$script";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        let types = list.network_filters[0].resource_types.as_ref().unwrap();
        assert!(types.contains(&ResourceType::Script));
    }

    #[test]
    fn test_parse_third_party() {
        let content = "||tracker.example.com^$third-party";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        assert!(list.network_filters[0].third_party_only);
    }

    #[test]
    fn test_parse_combined_options() {
        let content = "||cdn.example.com/ad.js$script,third-party";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        let f = &list.network_filters[0];
        assert!(f.third_party_only);
        let types = f.resource_types.as_ref().unwrap();
        assert!(types.contains(&ResourceType::Script));
    }

    #[test]
    fn test_parse_image_filter() {
        let content = "||ads.example.com/banner$image";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        let types = list.network_filters[0].resource_types.as_ref().unwrap();
        assert!(types.contains(&ResourceType::Image));
    }

    #[test]
    fn test_parse_font_filter() {
        let content = "||fonts.example.com^$font";
        let list = FilterList::parse(content);
        let types = list.network_filters[0].resource_types.as_ref().unwrap();
        assert!(types.contains(&ResourceType::Font));
    }

    #[test]
    fn test_parse_websocket_filter() {
        let content = "||ws.example.com^$websocket";
        let list = FilterList::parse(content);
        let types = list.network_filters[0].resource_types.as_ref().unwrap();
        assert!(types.contains(&ResourceType::WebSocket));
    }

    #[test]
    fn test_parse_popup_filter() {
        let content = "||pop.example.com^$popup";
        let list = FilterList::parse(content);
        let types = list.network_filters[0].resource_types.as_ref().unwrap();
        assert!(types.contains(&ResourceType::Popup));
    }

    #[test]
    fn test_parse_media_filter() {
        let content = "||ads.example.com/video$media";
        let list = FilterList::parse(content);
        let types = list.network_filters[0].resource_types.as_ref().unwrap();
        assert!(types.contains(&ResourceType::Media));
    }

    #[test]
    fn test_parse_css_filter() {
        let content = "||cdn.example.com/overlay.css$css";
        let list = FilterList::parse(content);
        let types = list.network_filters[0].resource_types.as_ref().unwrap();
        assert!(types.contains(&ResourceType::Stylesheet));
    }

    #[test]
    fn test_parse_domain_specific_filter() {
        let content = "example.com||ads.tracker.com^";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        assert_eq!(
            list.network_filters[0].domain_specific,
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_skip_comments_and_empty() {
        let content = "! Comment\n[Adblock Plus 2.0]\n\n||ads.com^\n";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
    }

    #[test]
    fn test_rule_count() {
        let content = "||ads.com^\n##.ad\n@@||safe.com^";
        let list = FilterList::parse(content);
        assert_eq!(list.rule_count(), 3);
    }

    #[test]
    fn test_load_from_text() {
        let mut list = FilterList::new();
        let count = list.load_from_text("||a.com^\n||b.com^\n##.ad");
        assert_eq!(count, 3);
        assert_eq!(list.network_filters.len(), 2);
        assert_eq!(list.cosmetic_filters.len(), 1);
    }

    #[test]
    fn test_url_to_filename() {
        assert_eq!(
            url_to_filename("https://easylist.to/easylist/easylist.txt"),
            "easylist_to"
        );
    }

    #[test]
    fn test_parse_csp_option() {
        let content = "||domain.com^$csp=script-src 'self'";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        assert_eq!(
            list.network_filters[0].csp.as_deref(),
            Some("script-src 'self'")
        );
    }

    #[test]
    fn test_parse_removeheader_option() {
        let content = "||domain.com^$removeheader=X-Custom-Header";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        assert_eq!(
            list.network_filters[0].remove_header.as_deref(),
            Some("X-Custom-Header")
        );
    }

    #[test]
    fn test_parse_redirect_option() {
        let content = "||domain.com/ad.js$redirect=noop.js";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        assert_eq!(list.network_filters[0].redirect.as_deref(), Some("noop.js"));
    }

    #[test]
    fn test_parse_combined_with_csp() {
        let content = "||domain.com^$script,third-party,csp=default-src 'none'";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        let f = &list.network_filters[0];
        assert!(f.third_party_only);
        assert!(f
            .resource_types
            .as_ref()
            .unwrap()
            .contains(&ResourceType::Script));
        assert_eq!(f.csp.as_deref(), Some("default-src 'none'"));
    }

    #[test]
    fn test_parse_badfilter() {
        let content = "||ads.example.com^$badfilter";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        assert!(list.network_filters[0].badfilter);
        assert!(!list.network_filters[0].is_exception);
    }

    #[test]
    fn test_parse_important() {
        let content = "||ads.example.com^$important";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        assert!(list.network_filters[0].important);
    }

    #[test]
    fn test_parse_document() {
        let content = "||example.com^$document";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        assert!(list.network_filters[0].document);
    }

    #[test]
    fn test_parse_all() {
        let content = "||tracker.com^$all";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        let types = list.network_filters[0].resource_types.as_ref().unwrap();
        assert!(types.contains(&ResourceType::Script));
        assert!(types.contains(&ResourceType::Image));
        assert!(types.contains(&ResourceType::Media));
        assert!(types.contains(&ResourceType::Popup));
    }

    #[test]
    fn test_parse_generichide() {
        let content = "||example.com^$generichide";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        assert!(list.network_filters[0].generichide);
    }

    #[test]
    fn test_parse_combined_badfilter_with_options() {
        let content = "||ads.example.com^$script,third-party,badfilter";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        let f = &list.network_filters[0];
        assert!(f.badfilter);
        assert!(f.third_party_only);
        assert!(f
            .resource_types
            .as_ref()
            .unwrap()
            .contains(&ResourceType::Script));
    }

    #[test]
    fn test_parse_important_with_options() {
        let content = "||ads.example.com^$image,important";
        let list = FilterList::parse(content);
        assert_eq!(list.network_filters.len(), 1);
        let f = &list.network_filters[0];
        assert!(f.important);
        assert!(f
            .resource_types
            .as_ref()
            .unwrap()
            .contains(&ResourceType::Image));
    }
}
