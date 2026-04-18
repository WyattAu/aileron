use std::collections::{HashMap, HashSet};
use tracing::info;
use url::Url;

use crate::net::filter_list::{
    CosmeticFilter, FilterList, NetworkFilter,
};

pub struct AdBlocker {
    blocked_domains: HashSet<String>,
    blocked_patterns: Vec<String>,
    whitelisted_domains: HashSet<String>,
    cosmetic_rules: Vec<String>,
    domain_cosmetic_rules: HashMap<String, Vec<String>>,
    network_filters: Vec<NetworkFilter>,
    cosmetic_filters: Vec<CosmeticFilter>,
    enabled: bool,
    blocked_count: u64,
    cosmetic_filtering: bool,
    site_exceptions: HashSet<String>,
}

impl AdBlocker {
    pub fn new() -> Self {
        Self {
            blocked_domains: HashSet::new(),
            blocked_patterns: Vec::new(),
            whitelisted_domains: HashSet::new(),
            cosmetic_rules: Vec::new(),
            domain_cosmetic_rules: HashMap::new(),
            network_filters: Vec::new(),
            cosmetic_filters: Vec::new(),
            enabled: true,
            blocked_count: 0,
            cosmetic_filtering: true,
            site_exceptions: HashSet::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn blocked_count(&self) -> u64 {
        self.blocked_count
    }

    pub fn reset_blocked_count(&mut self) {
        self.blocked_count = 0;
    }

    pub fn set_cosmetic_filtering(&mut self, enabled: bool) {
        self.cosmetic_filtering = enabled;
    }

    pub fn cosmetic_filtering_enabled(&self) -> bool {
        self.cosmetic_filtering
    }

    pub fn toggle_site_exception(&mut self, domain: &str) {
        let domain = domain.to_lowercase();
        if self.site_exceptions.remove(&domain) {
            info!("Removed adblock exception for {}", domain);
        } else {
            self.site_exceptions.insert(domain.clone());
            info!("Added adblock exception for {}", domain);
        }
    }

    pub fn is_site_excepted(&self, domain: &str) -> bool {
        let domain = domain.to_lowercase();
        if self.site_exceptions.contains(&domain) {
            return true;
        }
        if let Some(dot_pos) = domain.find('.') {
            let parent = &domain[dot_pos + 1..];
            if self.site_exceptions.contains(parent) {
                return true;
            }
        }
        false
    }

    pub fn block_domain(&mut self, domain: &str) {
        self.blocked_domains.insert(domain.to_lowercase());
    }

    pub fn block_pattern(&mut self, pattern: &str) {
        self.blocked_patterns.push(pattern.to_lowercase());
    }

    pub fn whitelist_domain(&mut self, domain: &str) {
        self.whitelisted_domains.insert(domain.to_lowercase());
    }

    pub fn add_cosmetic_rule(&mut self, rule: &str) {
        self.cosmetic_rules.push(rule.to_string());
    }

    pub fn load_filter_list(&mut self, content: &str) -> anyhow::Result<usize> {
        let mut rules_loaded = 0;

        for line in content.lines() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('!') || line.starts_with('[') {
                continue;
            }

            if line.starts_with("@@||") {
                let domain = line.trim_start_matches("@@||");
                let domain = domain.split('/').next().unwrap_or(domain);
                let domain = domain.trim_end_matches('^');
                if !domain.is_empty() {
                    self.whitelisted_domains.insert(domain.to_lowercase());
                    rules_loaded += 1;
                }
                continue;
            }

            if line.starts_with("||") {
                let domain = line.trim_start_matches("||");
                let domain = domain.split('/').next().unwrap_or(domain);
                let domain = domain.trim_end_matches('^');
                if !domain.is_empty() {
                    self.blocked_domains.insert(domain.to_lowercase());
                    rules_loaded += 1;
                }
                continue;
            }

            if line.contains("##") {
                if let Some(selector) = line.split("##").nth(1)
                    && !selector.is_empty()
                {
                    let Some(pos) = line.find("##") else { continue; };
                    let domain_part = &line[..pos];
                    let rule = format!("{} {{ display: none !important; }}", selector);
                    if !domain_part.is_empty() {
                        for domain in domain_part.split(',') {
                            let domain = domain.trim().trim_start_matches('~').to_lowercase();
                            if !domain.is_empty() {
                                self.domain_cosmetic_rules
                                    .entry(domain)
                                    .or_default()
                                    .push(rule.clone());
                            }
                        }
                    } else {
                        self.cosmetic_rules.push(rule);
                    }
                    rules_loaded += 1;
                }
                continue;
            }

            if !line.contains(' ') && !line.contains('#') {
                self.blocked_patterns.push(line.to_lowercase());
                rules_loaded += 1;
            }
        }

        info!(target: "adblock", "Loaded {} rules (legacy format)", rules_loaded);
        Ok(rules_loaded)
    }

    pub fn load_filter_list_file(&mut self, path: &std::path::Path) -> anyhow::Result<usize> {
        let content = std::fs::read_to_string(path)?;
        self.load_filter_list(&content)
    }

    pub fn load_from_filter_lists(&mut self, lists: &[FilterList]) -> usize {
        let mut total = 0;

        for list in lists {
            for filter in &list.network_filters {
                self.network_filters.push(filter.clone());
                total += 1;
            }

            for filter in &list.cosmetic_filters {
                self.cosmetic_filters.push(filter.clone());
                total += 1;
            }

            for nf in &list.network_filters {
                if nf.is_exception {
                    if let Some(domain) = Self::extract_domain_from_pattern(&nf.pattern) {
                        self.whitelisted_domains.insert(domain);
                    }
                } else if let Some(domain) = Self::extract_domain_from_pattern(&nf.pattern) {
                    self.blocked_domains.insert(domain);
                }
            }
        }

        info!(target: "adblock", "Loaded {} rules from {} filter lists", total, lists.len());
        total
    }

    fn extract_domain_from_pattern(pattern: &str) -> Option<String> {
        let domain = pattern.strip_prefix("||")?;
        let domain = domain.split('/').next()?;
        let domain = domain.trim_end_matches('^').trim_end_matches('*');
        if domain.is_empty() {
            return None;
        }
        Some(domain.to_lowercase())
    }

    pub fn should_block(&mut self, url: &Url) -> bool {
        if !self.enabled {
            return false;
        }

        let host = match url.host_str() {
            Some(h) => h.to_lowercase(),
            None => return false,
        };

        if self.is_whitelisted(&host) {
            return false;
        }

        if self.blocked_domains.contains(&host) {
            self.blocked_count += 1;
            return true;
        }

        for blocked in &self.blocked_domains {
            if blocked.starts_with("*.") {
                let suffix = &blocked[1..];
                if host.ends_with(suffix) {
                    self.blocked_count += 1;
                    return true;
                }
            }
        }

        let url_str = url.as_str().to_lowercase();
        for pattern in &self.blocked_patterns {
            if url_str.contains(pattern) {
                self.blocked_count += 1;
                return true;
            }
        }

        for filter in &self.network_filters {
            if filter.is_exception {
                continue;
            }

            if !self.pattern_matches_url(filter, &url_str, &host) {
                continue;
            }

            if let Some(ref domains) = filter.domain_specific
                && !self.host_matches_domain(&host, domains) {
                    continue;
            }

            self.blocked_count += 1;
            return true;
        }

        false
    }

    fn pattern_matches_url(&self, filter: &NetworkFilter, url_str: &str, host: &str) -> bool {
        let pattern = &filter.pattern;

        if pattern.starts_with("||") {
            let domain = pattern.strip_prefix("||").unwrap();
            let domain = domain.trim_end_matches('^');
            let domain = domain.split('/').next().unwrap_or(domain);

            if host == domain || host.ends_with(&format!(".{}", domain)) {
                return true;
            }
        } else if let Some(stripped) = pattern.strip_prefix('|') {
            if url_str.starts_with(stripped) {
                return true;
            }
        } else if pattern.ends_with('|') {
            if url_str.ends_with(&pattern[..pattern.len() - 1]) {
                return true;
            }
        } else {
            if url_str.contains(pattern) {
                return true;
            }
        }

        false
    }

    fn host_matches_domain(&self, host: &str, domain: &str) -> bool {
        host == domain || host.ends_with(&format!(".{}", domain))
    }

    fn is_whitelisted(&self, host: &str) -> bool {
        if self.whitelisted_domains.contains(host) {
            return true;
        }
        if let Some(dot_pos) = host.find('.') {
            let parent = &host[dot_pos + 1..];
            if self.whitelisted_domains.contains(parent) {
                return true;
            }
        }
        false
    }

    pub fn cosmetic_css(&self) -> String {
        if !self.cosmetic_filtering {
            return String::new();
        }
        self.cosmetic_rules.join("\n")
    }

    pub fn cosmetic_css_for_domain(&self, domain: &str) -> String {
        if !self.cosmetic_filtering {
            return String::new();
        }

        let mut rules: Vec<String> = Vec::new();
        rules.extend(self.cosmetic_rules.iter().cloned());

        for (filter_domain, filter_rules) in &self.domain_cosmetic_rules {
            if domain == filter_domain
                || domain.ends_with(&format!(".{}", filter_domain))
            {
                rules.extend(filter_rules.iter().cloned());
            }
        }

        for filter in &self.cosmetic_filters {
            if let Some(ref domains) = filter.domains {
                let matches = domains.iter().any(|d| {
                    domain == d || domain.ends_with(&format!(".{}", d))
                });
                if matches {
                    rules.push(format!(
                        "{} {{ display: none !important; }}",
                        filter.selector
                    ));
                }
            } else {
                rules.push(format!(
                    "{} {{ display: none !important; }}",
                    filter.selector
                ));
            }
        }

        rules.join("\n")
    }

    pub fn cosmetic_js_injection(&self, domain: &str) -> Option<String> {
        let css = self.cosmetic_css_for_domain(domain);
        if css.is_empty() {
            return None;
        }

        let escaped = css.replace('\\', "\\\\").replace('`', "\\`").replace('$', "\\$");

        Some(format!(
            "(function() {{ \
                var style = document.createElement('style'); \
                style.id = '__aileron_adblock_css'; \
                style.textContent = `{}`; \
                var existing = document.getElementById('__aileron_adblock_css'); \
                if (existing) existing.remove(); \
                (document.head || document.documentElement).appendChild(style); \
            }})()",
            escaped
        ))
    }

    pub fn rule_count(&self) -> usize {
        self.blocked_domains.len()
            + self.blocked_patterns.len()
            + self.cosmetic_rules.len()
            + self.network_filters.len()
            + self.cosmetic_filters.len()
    }

    pub fn network_filter_count(&self) -> usize {
        self.blocked_domains.len()
            + self.blocked_patterns.len()
            + self.network_filters.len()
    }

    pub fn cosmetic_filter_count(&self) -> usize {
        self.cosmetic_rules.len() + self.cosmetic_filters.len()
    }

    pub fn blocked_domains_iter(&self) -> Vec<String> {
        let mut domains: HashSet<String> = self.blocked_domains.iter().cloned().collect();

        for filter in &self.network_filters {
            if !filter.is_exception
                && let Some(domain) = Self::extract_domain_from_pattern(&filter.pattern) {
                    domains.insert(domain);
            }
        }

        domains.into_iter().collect()
    }

    pub fn filter_list_count(&self) -> usize {
        self.network_filters.len() + self.cosmetic_filters.len()
    }

    pub fn get_csp_headers(&self, url: &str) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }
        let domain = url::Url::parse(url)
            .ok()
            .and_then(|u| u.domain().map(|d| d.to_lowercase()))
            .unwrap_or_default();

        self.network_filters
            .iter()
            .filter(|f| {
                f.csp.is_some()
                    && !f.is_exception
                    && self.pattern_matches_host(f, &domain)
            })
            .filter_map(|f| f.csp.clone())
            .collect()
    }

    pub fn get_headers_to_remove(&self, url: &str) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }
        let domain = url::Url::parse(url)
            .ok()
            .and_then(|u| u.domain().map(|d| d.to_lowercase()))
            .unwrap_or_default();

        self.network_filters
            .iter()
            .filter(|f| {
                f.remove_header.is_some()
                    && !f.is_exception
                    && self.pattern_matches_host(f, &domain)
            })
            .filter_map(|f| f.remove_header.clone())
            .collect()
    }

    fn pattern_matches_host(&self, filter: &NetworkFilter, host: &str) -> bool {
        if filter.pattern.starts_with("||") {
            let domain = filter
                .pattern
                .strip_prefix("||")
                .unwrap()
                .trim_end_matches('^')
                .split('/')
                .next()
                .unwrap_or("");
            host == domain || host.ends_with(&format!(".{}", domain))
        } else {
            false
        }
    }
}

impl Default for AdBlocker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_exact_domain() {
        let mut blocker = AdBlocker::new();
        blocker.block_domain("ads.example.com");

        let url = Url::parse("https://ads.example.com/banner.js").unwrap();
        assert!(blocker.should_block(&url));
        assert_eq!(blocker.blocked_count(), 1);
    }

    #[test]
    fn test_allow_non_blocked() {
        let mut blocker = AdBlocker::new();
        blocker.block_domain("ads.example.com");

        let url = Url::parse("https://example.com/page").unwrap();
        assert!(!blocker.should_block(&url));
    }

    #[test]
    fn test_whitelist() {
        let mut blocker = AdBlocker::new();
        blocker.block_domain("ads.example.com");
        blocker.whitelist_domain("ads.example.com");

        let url = Url::parse("https://ads.example.com/banner.js").unwrap();
        assert!(!blocker.should_block(&url));
    }

    #[test]
    fn test_disabled() {
        let mut blocker = AdBlocker::new();
        blocker.block_domain("ads.example.com");
        blocker.set_enabled(false);

        let url = Url::parse("https://ads.example.com/banner.js").unwrap();
        assert!(!blocker.should_block(&url));
    }

    #[test]
    fn test_wildcard_domain() {
        let mut blocker = AdBlocker::new();
        blocker.block_domain("*.ads.example.com");

        let url = Url::parse("https://cdn.ads.example.com/banner.js").unwrap();
        assert!(blocker.should_block(&url));
    }

    #[test]
    fn test_url_pattern() {
        let mut blocker = AdBlocker::new();
        blocker.block_pattern("/ads/tracker.js");

        let url = Url::parse("https://cdn.example.com/ads/tracker.js?id=123").unwrap();
        assert!(blocker.should_block(&url));
    }

    #[test]
    fn test_load_filter_list() {
        let mut blocker = AdBlocker::new();
        let filters = r#"
! Comment line
||ads.example.com^
||tracker.evil.net^
@@||safe.example.com^
##div.ad-banner
##.sponsored-content
"#;
        let count = blocker.load_filter_list(filters).unwrap();
        assert_eq!(count, 5);

        let url = Url::parse("https://ads.example.com/ad.js").unwrap();
        assert!(blocker.should_block(&url));

        let url = Url::parse("https://safe.example.com/page").unwrap();
        assert!(!blocker.should_block(&url));

        let css = blocker.cosmetic_css();
        assert!(css.contains("div.ad-banner"));
        assert!(css.contains(".sponsored-content"));
    }

    #[test]
    fn test_cosmetic_css() {
        let mut blocker = AdBlocker::new();
        blocker.add_cosmetic_rule("div.ad { display: none; }");
        blocker.add_cosmetic_rule("span.popup { visibility: hidden; }");

        let css = blocker.cosmetic_css();
        assert!(css.contains("div.ad"));
        assert!(css.contains("span.popup"));
    }

    #[test]
    fn test_rule_count() {
        let mut blocker = AdBlocker::new();
        assert_eq!(blocker.rule_count(), 0);
        blocker.block_domain("ads.com");
        blocker.block_pattern("/tracker");
        blocker.add_cosmetic_rule(".ad { display: none; }");
        assert_eq!(blocker.rule_count(), 3);
    }

    #[test]
    fn test_reset_blocked_count() {
        let mut blocker = AdBlocker::new();
        blocker.block_domain("ads.example.com");
        let url = Url::parse("https://ads.example.com/ad.js").unwrap();
        blocker.should_block(&url);
        assert_eq!(blocker.blocked_count(), 1);
        blocker.reset_blocked_count();
        assert_eq!(blocker.blocked_count(), 0);
    }

    #[test]
    fn test_empty_filter_list() {
        let mut blocker = AdBlocker::new();
        let count = blocker.load_filter_list("").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_comment_and_section_lines_ignored() {
        let mut blocker = AdBlocker::new();
        let filters = r#"
[Adblock Plus 2.0]
! Title: EasyList
! Homepage: https://easylist.to/
"#;
        let count = blocker.load_filter_list(filters).unwrap();
        assert_eq!(count, 0);
        assert_eq!(blocker.rule_count(), 0);
    }

    #[test]
    fn test_load_from_filter_lists() {
        let mut blocker = AdBlocker::new();
        let list = FilterList::parse("||blocked.com^\n##.ad\n||pattern.com/path");
        let count = blocker.load_from_filter_lists(&[list]);
        assert_eq!(count, 3);
        assert!(blocker.blocked_domains.contains("blocked.com"));
        assert!(blocker.blocked_domains.contains("pattern.com"));
    }

    #[test]
    fn test_cosmetic_css_for_domain() {
        let mut blocker = AdBlocker::new();
        let list = FilterList::parse("example.com##.ad-slot\ngeneric.com##.sidebar-ad\n##.global-ad");
        blocker.load_from_filter_lists(&[list]);

        let css = blocker.cosmetic_css_for_domain("example.com");
        assert!(css.contains(".global-ad"));
        assert!(css.contains(".ad-slot"));
        assert!(!css.contains(".sidebar-ad"));
    }

    #[test]
    fn test_cosmetic_js_injection() {
        let mut blocker = AdBlocker::new();
        blocker.add_cosmetic_rule(".ad { display: none !important; }");

        let js = blocker.cosmetic_js_injection("example.com");
        assert!(js.is_some());
        let js = js.unwrap();
        assert!(js.contains("__aileron_adblock_css"));
        assert!(js.contains("display: none !important"));
    }

    #[test]
    fn test_cosmetic_js_injection_empty() {
        let blocker = AdBlocker::new();
        let js = blocker.cosmetic_js_injection("example.com");
        assert!(js.is_none());
    }

    #[test]
    fn test_cosmetic_filtering_disabled() {
        let mut blocker = AdBlocker::new();
        blocker.add_cosmetic_rule(".ad { display: none !important; }");
        blocker.set_cosmetic_filtering(false);

        assert!(blocker.cosmetic_css().is_empty());
        assert!(blocker.cosmetic_css_for_domain("example.com").is_empty());
        assert!(blocker.cosmetic_js_injection("example.com").is_none());
    }

    #[test]
    fn test_site_exception() {
        let mut blocker = AdBlocker::new();
        blocker.block_domain("ads.example.com");

        let url = Url::parse("https://ads.example.com/ad.js").unwrap();
        assert!(blocker.should_block(&url));

        blocker.toggle_site_exception("example.com");
        assert!(blocker.is_site_excepted("example.com"));
        assert!(blocker.is_site_excepted("sub.example.com"));

        let url2 = Url::parse("https://ads.example.com/ad.js").unwrap();
        assert!(blocker.should_block(&url2));

        blocker.toggle_site_exception("example.com");
        assert!(!blocker.is_site_excepted("example.com"));
    }

    #[test]
    fn test_blocked_domains_iter_includes_filter_list_domains() {
        let mut blocker = AdBlocker::new();
        let list = FilterList::parse("||newblocked.com^\n||another.com^");
        blocker.load_from_filter_lists(&[list]);

        let domains = blocker.blocked_domains_iter();
        assert!(domains.contains(&"newblocked.com".to_string()));
        assert!(domains.contains(&"another.com".to_string()));
    }

    #[test]
    fn test_domain_specific_cosmetic_filter() {
        let mut blocker = AdBlocker::new();
        let filters = "example.com##.ad-banner\n";
        let _ = blocker.load_filter_list(filters);

        let css = blocker.cosmetic_css_for_domain("example.com");
        assert!(css.contains(".ad-banner"));

        let css = blocker.cosmetic_css_for_domain("other.com");
        assert!(!css.contains(".ad-banner"));
    }

    #[test]
    fn test_network_filter_count() {
        let mut blocker = AdBlocker::new();
        blocker.block_domain("a.com");
        blocker.block_pattern("/ad");
        assert_eq!(blocker.network_filter_count(), 2);
    }

    #[test]
    fn test_cosmetic_filter_count() {
        let mut blocker = AdBlocker::new();
        blocker.add_cosmetic_rule(".ad { display: none; }");
        assert_eq!(blocker.cosmetic_filter_count(), 1);
    }

    #[test]
    fn test_filter_list_count() {
        let mut blocker = AdBlocker::new();
        let list = FilterList::parse("||a.com^\n##.ad");
        blocker.load_from_filter_lists(&[list]);
        assert_eq!(blocker.filter_list_count(), 2);
    }

    #[test]
    fn test_extract_domain_from_pattern() {
        assert_eq!(
            AdBlocker::extract_domain_from_pattern("||ads.example.com^"),
            Some("ads.example.com".to_string())
        );
        assert_eq!(
            AdBlocker::extract_domain_from_pattern("||example.com/path"),
            Some("example.com".to_string())
        );
        assert_eq!(
            AdBlocker::extract_domain_from_pattern("not_a_domain"),
            None
        );
    }

    #[test]
    fn test_get_csp_headers() {
        let mut blocker = AdBlocker::new();
        let list = FilterList::parse("||example.com^$csp=script-src 'none'");
        blocker.load_from_filter_lists(&[list]);

        let headers = blocker.get_csp_headers("https://example.com/page");
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0], "script-src 'none'");
    }

    #[test]
    fn test_get_csp_headers_no_match() {
        let mut blocker = AdBlocker::new();
        let list = FilterList::parse("||other.com^$csp=script-src 'none'");
        blocker.load_from_filter_lists(&[list]);

        let headers = blocker.get_csp_headers("https://example.com/page");
        assert!(headers.is_empty());
    }

    #[test]
    fn test_get_csp_headers_disabled() {
        let mut blocker = AdBlocker::new();
        let list = FilterList::parse("||example.com^$csp=script-src 'none'");
        blocker.load_from_filter_lists(&[list]);
        blocker.set_enabled(false);

        let headers = blocker.get_csp_headers("https://example.com/page");
        assert!(headers.is_empty());
    }

    #[test]
    fn test_get_headers_to_remove() {
        let mut blocker = AdBlocker::new();
        let list = FilterList::parse("||example.com^$removeheader=X-Tracking");
        blocker.load_from_filter_lists(&[list]);

        let headers = blocker.get_headers_to_remove("https://example.com/page");
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0], "X-Tracking");
    }

    #[test]
    fn test_get_headers_to_remove_no_match() {
        let mut blocker = AdBlocker::new();
        let list = FilterList::parse("||other.com^$removeheader=X-Tracking");
        blocker.load_from_filter_lists(&[list]);

        let headers = blocker.get_headers_to_remove("https://example.com/page");
        assert!(headers.is_empty());
    }

    #[test]
    fn test_pattern_matches_host_subdomain() {
        let blocker = AdBlocker::new();
        let list = FilterList::parse("||example.com^$csp=default-src 'none'");
        let filter = &list.network_filters[0];

        assert!(blocker.pattern_matches_host(filter, "example.com"));
        assert!(blocker.pattern_matches_host(filter, "sub.example.com"));
        assert!(!blocker.pattern_matches_host(filter, "other.com"));
    }
}
