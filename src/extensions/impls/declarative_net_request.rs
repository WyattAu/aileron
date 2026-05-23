//! Concrete implementation of [`DeclarativeNetRequestApi`] for Aileron.
//!
//! Evaluates DNR rules against URLs using filter pattern matching.
//! Supports urlFilter, regexFilter, resource type filtering, and domain conditions.

use std::collections::HashMap;

use parking_lot::RwLock;

use crate::extensions::declarative_net_request::{
    DeclarativeNetRequestApi, DnrAction, DnrCondition, DnrResourceType, DnrRule, DnrRuleset,
    DnrVerdict,
};
use crate::extensions::types::Result;

pub struct AileronDeclarativeNetRequestApi {
    static_rulesets: RwLock<HashMap<String, DnrRuleset>>,
    dynamic_rules: RwLock<Vec<DnrRule>>,
}

impl Default for AileronDeclarativeNetRequestApi {
    fn default() -> Self {
        Self::new()
    }
}

impl AileronDeclarativeNetRequestApi {
    pub fn new() -> Self {
        Self {
            static_rulesets: RwLock::new(HashMap::new()),
            dynamic_rules: RwLock::new(Vec::new()),
        }
    }

    /// Collect all active rules from static + dynamic sets, sorted by priority (desc).
    fn active_rules(&self) -> Vec<DnrRule> {
        // We need to collect into a vec of owned references, which is tricky with RwLock.
        // Instead, collect rule data into a temporary vec.
        let mut all: Vec<DnrRule> = Vec::new();

        {
            let static_sets = self.static_rulesets.read();
            for (_, rs) in static_sets.iter() {
                if rs.enabled {
                    all.extend(rs.rules.iter().cloned());
                }
            }
        }

        {
            let dynamic = self.dynamic_rules.read();
            all.extend(dynamic.iter().cloned());
        }

        // Sort by priority descending (higher priority first)
        all.sort_by(|a, b| {
            let pa = a.priority.unwrap_or(1);
            let pb = b.priority.unwrap_or(1);
            pb.cmp(&pa)
        });

        // We return the rules but need them for matching below.
        // For evaluate(), we'll inline the logic.
        // This method is not public; it's used as a helper.
        // Actually, we can't return references to cloned data. Let's restructure.
        all
    }
}

/// Match a DNR urlFilter pattern against a URL.
/// Supports:
///   `*` - matches any sequence of characters
///   `||` - matches the beginning of a domain
///   `^` - matches a separator (end of URL or path separator)
fn url_filter_matches(pattern: &str, url: &str, case_sensitive: bool) -> bool {
    let url_cmp = if case_sensitive {
        url.to_string()
    } else {
        url.to_ascii_lowercase()
    };
    let pattern_cmp = if case_sensitive {
        pattern.to_string()
    } else {
        pattern.to_ascii_lowercase()
    };

    // Handle special || prefix (domain anchor)
    let (pattern_cmp, domain_anchored) = if let Some(rest) = pattern_cmp.strip_prefix("||") {
        (rest.to_string(), true)
    } else {
        (pattern_cmp, false)
    };

    // Handle ^ suffix (separator)
    let (pattern_cmp, separator_end) = if let Some(rest) = pattern_cmp.strip_suffix('^') {
        (rest.to_string(), true)
    } else {
        (pattern_cmp, false)
    };

    // Handle | prefix (start anchor)
    let (pattern_cmp, start_anchored) = if let Some(rest) = pattern_cmp.strip_prefix('|') {
        (rest.to_string(), true)
    } else {
        (pattern_cmp, false)
    };

    // Handle | suffix (end anchor)
    let (pattern_cmp, end_anchored) = if let Some(rest) = pattern_cmp.strip_suffix('|') {
        (rest.to_string(), true)
    } else {
        (pattern_cmp, false)
    };

    if domain_anchored {
        // ||ads.example.com^ should match https://ads.example.com/...
        // and https://sub.ads.example.com/...
        // Extract host from URL
        let url_host = url::Url::parse(&url_cmp)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()));

        let Some(host) = url_host else {
            return false;
        };

        // Check if host is exactly the pattern domain or a subdomain of it
        let domain_match = host == pattern_cmp || host.ends_with(&format!(".{pattern_cmp}"));

        if !domain_match {
            return false;
        }

        // If separator_end, check that the URL path starts at the domain boundary
        if separator_end {
            // The ^ matches end of string, ?, #, or / after the domain
            // For simplicity, just check the domain matches
            return true;
        }

        return true;
    }

    // Wildcard matching
    if pattern_cmp.contains('*') {
        let regex_str = regex::escape(&pattern_cmp).replace("\\*", ".*");
        let Ok(re) = regex::RegexBuilder::new(&regex_str)
            .case_insensitive(!case_sensitive)
            .build()
        else {
            return false;
        };

        let url_str = if start_anchored && end_anchored {
            re.is_match(&url_cmp)
        } else if start_anchored {
            re.is_match(&url_cmp) && re.find(&url_cmp).is_some_and(|m| m.start() == 0)
        } else {
            re.is_match(&url_cmp)
        };
        return url_str;
    }

    // Exact or substring match
    if start_anchored && end_anchored {
        url_cmp == pattern_cmp
    } else if start_anchored {
        url_cmp.starts_with(&pattern_cmp)
    } else if end_anchored {
        url_cmp.ends_with(&pattern_cmp)
    } else {
        url_cmp.contains(&pattern_cmp)
    }
}

/// Check if a rule's condition matches a URL + resource type + initiator domain.
fn condition_matches(
    condition: &DnrCondition,
    url: &str,
    resource_type: DnrResourceType,
    initiator: Option<&str>,
) -> bool {
    // Resource type filter
    if let Some(ref types) = condition.resource_types
        && !types.contains(&resource_type)
    {
        return false;
    }
    if let Some(ref excluded) = condition.excluded_resource_types
        && excluded.contains(&resource_type)
    {
        return false;
    }

    // Domain filter (initiator domain must be in the list)
    if let Some(ref domains) = condition.domains {
        let Some(init) = initiator else {
            return false;
        };
        let init_domain = extract_domain(init);
        let Some(ref init_d) = init_domain else {
            return false;
        };
        if !domains
            .iter()
            .any(|d| d == init_d || init_d.ends_with(&format!(".{d}")))
        {
            return false;
        }
    }
    if let Some(ref excluded) = condition.excluded_domains
        && let Some(init) = initiator
    {
        let init_domain = extract_domain(init);
        if let Some(ref init_d) = init_domain
            && excluded
                .iter()
                .any(|d| d == init_d || init_d.ends_with(&format!(".{d}")))
        {
            return false;
        }
    }

    // URL filter
    let case_sensitive = condition.is_url_filter_case_sensitive.unwrap_or(false);

    if let Some(ref filter) = condition.url_filter
        && !url_filter_matches(filter, url, case_sensitive)
    {
        return false;
    }

    // Regex filter
    if let Some(ref regex_filter) = condition.regex_filter {
        let Ok(re) = regex::RegexBuilder::new(regex_filter)
            .case_insensitive(!case_sensitive)
            .build()
        else {
            return false;
        };
        if !re.is_match(url) {
            return false;
        }
    }

    // If no urlFilter or regexFilter, the condition doesn't filter by URL
    if condition.url_filter.is_none() && condition.regex_filter.is_none() {
        // Only resource type and domain filters apply — condition matches
    }

    true
}

fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.domain().map(|d| d.to_string()))
}

fn action_to_verdict(action: &DnrAction) -> Option<DnrVerdict> {
    match action {
        DnrAction::block => Some(DnrVerdict::Block),
        DnrAction::allow => Some(DnrVerdict::Allow),
        DnrAction::allow_all_requests => Some(DnrVerdict::Allow),
        DnrAction::redirect {
            url,
            extension_path,
            ..
        } => {
            if let Some(u) = url {
                Some(DnrVerdict::Redirect(u.clone()))
            } else {
                extension_path
                    .as_ref()
                    .map(|ext_path| DnrVerdict::Redirect(ext_path.clone()))
            }
        }
        DnrAction::modify_headers {
            request_headers,
            response_headers,
        } => Some(DnrVerdict::ModifyHeaders {
            request_headers: request_headers.clone().unwrap_or_default(),
            response_headers: response_headers.clone().unwrap_or_default(),
        }),
    }
}

impl DeclarativeNetRequestApi for AileronDeclarativeNetRequestApi {
    fn update_static_ruleset(&self, ruleset_id: &str, enabled: bool) -> Result<()> {
        let mut sets = self.static_rulesets.write();
        if let Some(rs) = sets.get_mut(ruleset_id) {
            rs.enabled = enabled;
        }
        Ok(())
    }

    fn get_enabled_rulesets(&self) -> Vec<DnrRuleset> {
        self.static_rulesets
            .read()
            .values()
            .filter(|rs| rs.enabled)
            .cloned()
            .collect()
    }

    fn load_static_ruleset(&self, ruleset: DnrRuleset) -> Result<()> {
        let mut sets = self.static_rulesets.write();
        sets.insert(ruleset.id.clone(), ruleset);
        Ok(())
    }

    fn add_dynamic_rules(&self, rules: Vec<DnrRule>) -> Result<()> {
        let mut dynamic = self.dynamic_rules.write();
        for rule in rules {
            // Remove existing rule with same ID if present
            dynamic.retain(|r| r.id != rule.id);
            dynamic.push(rule);
        }
        Ok(())
    }

    fn remove_dynamic_rules(&self, rule_ids: Vec<u32>) -> Result<()> {
        let mut dynamic = self.dynamic_rules.write();
        dynamic.retain(|r| !rule_ids.contains(&r.id));
        Ok(())
    }

    fn evaluate(
        &self,
        url: &str,
        resource_type: DnrResourceType,
        initiator: Option<&str>,
    ) -> Option<DnrVerdict> {
        let all_rules = self.active_rules();

        for rule in &all_rules {
            if condition_matches(&rule.condition, url, resource_type, initiator) {
                return action_to_verdict(&rule.action);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_rule(id: u32, filter: &str) -> DnrRule {
        DnrRule {
            id,
            priority: Some(1),
            action: DnrAction::block,
            condition: DnrCondition {
                url_filter: Some(filter.to_string()),
                regex_filter: None,
                resource_types: None,
                excluded_resource_types: None,
                domains: None,
                excluded_domains: None,
                is_url_filter_case_sensitive: None,
            },
        }
    }

    fn redirect_rule(id: u32, filter: &str, target: &str) -> DnrRule {
        DnrRule {
            id,
            priority: Some(1),
            action: DnrAction::redirect {
                url: Some(target.to_string()),
                extension_path: None,
                transform: None,
            },
            condition: DnrCondition {
                url_filter: Some(filter.to_string()),
                regex_filter: None,
                resource_types: None,
                excluded_resource_types: None,
                domains: None,
                excluded_domains: None,
                is_url_filter_case_sensitive: None,
            },
        }
    }

    #[test]
    fn test_url_filter_domain_anchor() {
        assert!(url_filter_matches(
            "||ads.example.com^",
            "https://ads.example.com/banner.js",
            false
        ));
        assert!(url_filter_matches(
            "||ads.example.com^",
            "https://sub.ads.example.com/tracker.js",
            false
        ));
        assert!(!url_filter_matches(
            "||ads.example.com^",
            "https://notads.example.com/page",
            false
        ));
    }

    #[test]
    fn test_url_filter_wildcard() {
        assert!(url_filter_matches(
            "*://*.example.com/*",
            "https://sub.example.com/page",
            false
        ));
        assert!(!url_filter_matches(
            "*://*.example.com/*",
            "https://other.com/page",
            false
        ));
    }

    #[test]
    fn test_url_filter_substring() {
        assert!(url_filter_matches(
            "tracker",
            "https://cdn.tracker.com/script.js",
            false
        ));
    }

    #[test]
    fn test_url_filter_case_insensitive() {
        assert!(url_filter_matches(
            "ADS",
            "https://example.com/ads.js",
            false
        ));
        assert!(!url_filter_matches(
            "ADS",
            "https://example.com/ads.js",
            true
        ));
    }

    #[test]
    fn test_load_and_evaluate_block() {
        let api = AileronDeclarativeNetRequestApi::new();
        api.load_static_ruleset(DnrRuleset {
            id: "default".into(),
            enabled: true,
            rules: vec![block_rule(1, "||ads.example.com^")],
        })
        .unwrap();

        let verdict = api.evaluate(
            "https://ads.example.com/banner.js",
            DnrResourceType::script,
            None,
        );
        assert!(matches!(verdict, Some(DnrVerdict::Block)));
    }

    #[test]
    fn test_evaluate_no_match() {
        let api = AileronDeclarativeNetRequestApi::new();
        api.load_static_ruleset(DnrRuleset {
            id: "default".into(),
            enabled: true,
            rules: vec![block_rule(1, "||ads.example.com^")],
        })
        .unwrap();

        let verdict = api.evaluate(
            "https://safe.example.com/page.html",
            DnrResourceType::main_frame,
            None,
        );
        assert!(verdict.is_none());
    }

    #[test]
    fn test_evaluate_redirect() {
        let api = AileronDeclarativeNetRequestApi::new();
        api.load_static_ruleset(DnrRuleset {
            id: "redirects".into(),
            enabled: true,
            rules: vec![redirect_rule(
                1,
                "||old.example.com^",
                "https://new.example.com",
            )],
        })
        .unwrap();

        let verdict = api.evaluate(
            "https://old.example.com/page",
            DnrResourceType::main_frame,
            None,
        );
        assert!(matches!(
            verdict,
            Some(DnrVerdict::Redirect(ref u)) if u == "https://new.example.com"
        ));
    }

    #[test]
    fn test_disabled_ruleset_not_evaluated() {
        let api = AileronDeclarativeNetRequestApi::new();
        api.load_static_ruleset(DnrRuleset {
            id: "disabled".into(),
            enabled: false,
            rules: vec![block_rule(1, "*")],
        })
        .unwrap();

        let verdict = api.evaluate("https://anything.com", DnrResourceType::main_frame, None);
        assert!(verdict.is_none());
    }

    #[test]
    fn test_update_static_ruleset_toggle() {
        let api = AileronDeclarativeNetRequestApi::new();
        api.load_static_ruleset(DnrRuleset {
            id: "toggle".into(),
            enabled: true,
            rules: vec![block_rule(1, "*")],
        })
        .unwrap();

        api.update_static_ruleset("toggle", false).unwrap();
        let enabled = api.get_enabled_rulesets();
        assert!(enabled.is_empty());

        api.update_static_ruleset("toggle", true).unwrap();
        assert_eq!(api.get_enabled_rulesets().len(), 1);
    }

    #[test]
    fn test_dynamic_rules() {
        let api = AileronDeclarativeNetRequestApi::new();
        api.add_dynamic_rules(vec![block_rule(100, "||dynamic.com^")])
            .unwrap();

        let verdict = api.evaluate("https://dynamic.com/ads", DnrResourceType::image, None);
        assert!(matches!(verdict, Some(DnrVerdict::Block)));

        api.remove_dynamic_rules(vec![100]).unwrap();
        let verdict = api.evaluate("https://dynamic.com/ads", DnrResourceType::image, None);
        assert!(verdict.is_none());
    }

    #[test]
    fn test_resource_type_filter() {
        let api = AileronDeclarativeNetRequestApi::new();
        let mut rule = block_rule(1, "||ads.com^");
        rule.condition.resource_types = Some(vec![DnrResourceType::script]);
        api.load_static_ruleset(DnrRuleset {
            id: "typed".into(),
            enabled: true,
            rules: vec![rule],
        })
        .unwrap();

        // Should block scripts
        assert!(matches!(
            api.evaluate("https://ads.com/ad.js", DnrResourceType::script, None),
            Some(DnrVerdict::Block)
        ));
        // Should NOT block images
        assert!(
            api.evaluate("https://ads.com/ad.png", DnrResourceType::image, None)
                .is_none()
        );
    }

    #[test]
    fn test_priority_ordering() {
        let api = AileronDeclarativeNetRequestApi::new();
        let block = block_rule(1, "||example.com^");
        let allow = DnrRule {
            id: 2,
            priority: Some(100), // Higher priority
            action: DnrAction::allow,
            condition: DnrCondition {
                url_filter: Some("||example.com^".to_string()),
                regex_filter: None,
                resource_types: None,
                excluded_resource_types: None,
                domains: None,
                excluded_domains: None,
                is_url_filter_case_sensitive: None,
            },
        };
        // Make block low priority
        let block_low = DnrRule {
            priority: Some(1),
            ..block
        };

        api.load_static_ruleset(DnrRuleset {
            id: "prio".into(),
            enabled: true,
            rules: vec![block_low, allow],
        })
        .unwrap();

        let verdict = api.evaluate(
            "https://example.com/page",
            DnrResourceType::main_frame,
            None,
        );
        assert!(matches!(verdict, Some(DnrVerdict::Allow)));
    }

    #[test]
    fn test_regex_filter() {
        let api = AileronDeclarativeNetRequestApi::new();
        let rule = DnrRule {
            id: 1,
            priority: Some(1),
            action: DnrAction::block,
            condition: DnrCondition {
                url_filter: None,
                regex_filter: Some(r"^https://[a-z]+\.tracker\.[a-z]+/.*".to_string()),
                resource_types: None,
                excluded_resource_types: None,
                domains: None,
                excluded_domains: None,
                is_url_filter_case_sensitive: None,
            },
        };
        api.load_static_ruleset(DnrRuleset {
            id: "regex".into(),
            enabled: true,
            rules: vec![rule],
        })
        .unwrap();

        assert!(matches!(
            api.evaluate(
                "https://cdn.tracker.com/script.js",
                DnrResourceType::script,
                None
            ),
            Some(DnrVerdict::Block)
        ));
        assert!(
            api.evaluate("https://safe.com/page", DnrResourceType::main_frame, None)
                .is_none()
        );
    }
}
