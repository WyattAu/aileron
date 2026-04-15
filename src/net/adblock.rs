use std::collections::HashSet;
use tracing::info;
use url::Url;

/// A simple, fast ad-blocker that uses domain-based blocking.
///
/// This is a lightweight alternative to the `brave/adblock` crate (which had
/// transitive dependency conflicts). It supports:
/// - EasyList-compatible domain blocking (simplified)
/// - Cosmetic CSS rule injection
/// - Whitelisting
///
/// Per YP-NET-ADBLOCK-001: ALG-ADBLOCK-001 (domain matching) and ALG-ADBLOCK-002 (CSS injection).
pub struct AdBlocker {
    /// Blocked domains (exact match and wildcard suffix).
    blocked_domains: HashSet<String>,
    /// Blocked URL patterns (substring match).
    blocked_patterns: Vec<String>,
    /// Whitelisted domains (never blocked).
    whitelisted_domains: HashSet<String>,
    /// Cosmetic CSS rules to inject.
    cosmetic_rules: Vec<String>,
    /// Whether ad-blocking is enabled.
    enabled: bool,
    /// Number of requests blocked since last reset.
    blocked_count: u64,
}

impl AdBlocker {
    pub fn new() -> Self {
        Self {
            blocked_domains: HashSet::new(),
            blocked_patterns: Vec::new(),
            whitelisted_domains: HashSet::new(),
            cosmetic_rules: Vec::new(),
            enabled: true,
            blocked_count: 0,
        }
    }

    /// Check if ad-blocking is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable ad-blocking.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Get the number of blocked requests.
    pub fn blocked_count(&self) -> u64 {
        self.blocked_count
    }

    /// Reset the blocked request counter.
    pub fn reset_blocked_count(&mut self) {
        self.blocked_count = 0;
    }

    /// Add a domain to the block list.
    /// Supports exact domains (e.g., "ads.example.com") and wildcards (e.g., "*.ads.*").
    pub fn block_domain(&mut self, domain: &str) {
        self.blocked_domains.insert(domain.to_lowercase());
    }

    /// Add a URL pattern to the block list (substring match).
    pub fn block_pattern(&mut self, pattern: &str) {
        self.blocked_patterns.push(pattern.to_lowercase());
    }

    /// Add a domain to the whitelist.
    pub fn whitelist_domain(&mut self, domain: &str) {
        self.whitelisted_domains.insert(domain.to_lowercase());
    }

    /// Add a cosmetic CSS rule (e.g., "div.ad-banner { display: none !important; }").
    pub fn add_cosmetic_rule(&mut self, rule: &str) {
        self.cosmetic_rules.push(rule.to_string());
    }

    /// Load filter rules from an EasyList-compatible text format.
    /// Supports:
    /// - Lines starting with || (domain blocking)
    /// - Lines starting with ! (comments, ignored)
    /// - Lines starting with @@ (whitelist)
    /// - Lines starting with ## (cosmetic rules)
    pub fn load_filter_list(&mut self, content: &str) -> anyhow::Result<usize> {
        let mut rules_loaded = 0;

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('!') || line.starts_with('[') {
                continue;
            }

            // Whitelist rule: @@||domain
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

            // Domain blocking: ||domain
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

            // Cosmetic rule: ##selector
            if line.contains("##") {
                if let Some(selector) = line.split("##").nth(1)
                    && !selector.is_empty() {
                        let rule = format!("{} {{ display: none !important; }}", selector);
                        self.cosmetic_rules.push(rule);
                        rules_loaded += 1;
                    }
                continue;
            }

            // Generic URL pattern blocking
            if !line.contains(' ') && !line.contains('#') {
                self.blocked_patterns.push(line.to_lowercase());
                rules_loaded += 1;
            }
        }

        info!(target: "adblock", "Loaded {} rules", rules_loaded);
        Ok(rules_loaded)
    }

    /// Check if a URL should be blocked.
    /// Returns true if the URL matches a block rule and is not whitelisted.
    pub fn should_block(&mut self, url: &Url) -> bool {
        if !self.enabled {
            return false;
        }

        let host = match url.host_str() {
            Some(h) => h.to_lowercase(),
            None => return false,
        };

        // Check whitelist first
        if self.is_whitelisted(&host) {
            return false;
        }

        // Check exact domain match
        if self.blocked_domains.contains(&host) {
            self.blocked_count += 1;
            return true;
        }

        // Check suffix/wildcard domain match
        for blocked in &self.blocked_domains {
            if blocked.starts_with("*.") {
                let suffix = &blocked[1..]; // ".ads.example.com"
                if host.ends_with(suffix) {
                    self.blocked_count += 1;
                    return true;
                }
            }
        }

        // Check URL pattern match
        let url_str = url.as_str().to_lowercase();
        for pattern in &self.blocked_patterns {
            if url_str.contains(pattern) {
                self.blocked_count += 1;
                return true;
            }
        }

        false
    }

    /// Check if a domain is whitelisted.
    fn is_whitelisted(&self, host: &str) -> bool {
        if self.whitelisted_domains.contains(host) {
            return true;
        }
        // Check parent domains
        let parts: Vec<&str> = host.rsplitn(3, '.').collect();
        if parts.len() >= 2 {
            let parent = format!("{}.{}", parts[1], parts[0]);
            if self.whitelisted_domains.contains(&parent) {
                return true;
            }
        }
        false
    }

    /// Get all cosmetic CSS rules as a single CSS string.
    pub fn cosmetic_css(&self) -> String {
        self.cosmetic_rules.join("\n")
    }

    /// Get the number of loaded rules.
    pub fn rule_count(&self) -> usize {
        self.blocked_domains.len() + self.blocked_patterns.len() + self.cosmetic_rules.len()
    }

    /// Iterate over blocked domains (for sharing with wry navigation callbacks).
    pub fn blocked_domains_iter(&self) -> Vec<String> {
        self.blocked_domains.iter().cloned().collect()
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
        assert_eq!(count, 5); // 2 blocked + 1 whitelist + 2 cosmetic

        // Verify domain blocking
        let url = Url::parse("https://ads.example.com/ad.js").unwrap();
        assert!(blocker.should_block(&url));

        // Verify whitelist
        let url = Url::parse("https://safe.example.com/page").unwrap();
        assert!(!blocker.should_block(&url));

        // Verify cosmetic rules
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
}
