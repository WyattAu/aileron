//! WebExtensions `browser.declarativeNetRequest` API.
//!
//! Provides declarative network request modification rules. Extensions define
//! rules in JSON that redirect, block, or modify requests based on URL filters.
//! This is the MV3 replacement for webRequest blocking.

use crate::extensions::types::Result;

/// A DNR rule, matching the Chrome MV3 schema.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DnrRule {
    pub id: u32,
    pub priority: Option<u32>,
    pub action: DnrAction,
    pub condition: DnrCondition,
}

/// What to do with a matching request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
#[allow(non_camel_case_types)]
pub enum DnrAction {
    /// Block the request.
    block,
    /// Redirect the request.
    redirect {
        /// Redirect target URL.
        url: Option<String>,
        /// Redirect to extension path.
        #[serde(rename = "extensionPath", default)]
        extension_path: Option<String>,
        /// URL transformation.
        #[serde(default)]
        transform: Option<DnrUrlTransform>,
    },
    /// Modify request/response headers.
    #[serde(rename = "modifyHeaders")]
    modify_headers {
        #[serde(default, rename = "requestHeaders")]
        request_headers: Option<Vec<DnrHeaderOperation>>,
        #[serde(default, rename = "responseHeaders")]
        response_headers: Option<Vec<DnrHeaderOperation>>,
    },
    /// Allow the request (overrides a matching block rule at lower priority).
    allow,
    /// Allow all requests from this extension to bypass matching block rules.
    #[serde(rename = "allowAllRequests")]
    allow_all_requests,
}

/// URL transformation rules.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DnrUrlTransform {
    #[serde(default)]
    pub scheme: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default, rename = "queryTransform")]
    pub query_transform: Option<DnrQueryTransform>,
    #[serde(default)]
    pub fragment: Option<String>,
}

/// Query parameter transformation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DnrQueryTransform {
    #[serde(default, rename = "removeParams")]
    pub remove_params: Option<Vec<String>>,
    #[serde(default, rename = "addOrReplaceParams")]
    pub add_or_replace_params: Option<Vec<DnrQueryParameter>>,
}

/// A query parameter to add or replace.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DnrQueryParameter {
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default, rename = "replaceOnly")]
    pub replace_only: Option<bool>,
}

/// Header modification operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DnrHeaderOperation {
    pub header: String,
    pub operation: DnrHeaderOp,
    #[serde(default)]
    pub value: Option<String>,
}

/// Header operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(non_camel_case_types)]
pub enum DnrHeaderOp {
    append,
    set,
    remove,
}

/// Rule condition — when the rule applies.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DnrCondition {
    /// URL filter pattern. Supports * wildcard.
    #[serde(default, rename = "urlFilter")]
    pub url_filter: Option<String>,
    /// Regex filter (alternative to urlFilter).
    #[serde(default)]
    pub regex_filter: Option<String>,
    /// Request types this rule applies to.
    #[serde(default)]
    pub resource_types: Option<Vec<DnrResourceType>>,
    /// Request types this rule should NOT apply to.
    #[serde(default, rename = "excludedResourceTypes")]
    pub excluded_resource_types: Option<Vec<DnrResourceType>>,
    /// Domains this rule applies to.
    #[serde(default)]
    pub domains: Option<Vec<String>>,
    /// Domains to exclude.
    #[serde(default, rename = "excludedDomains")]
    pub excluded_domains: Option<Vec<String>>,
    /// Only match if the request is first-party (same domain).
    #[serde(default, rename = "isUrlFilterCaseSensitive")]
    pub is_url_filter_case_sensitive: Option<bool>,
}

/// Resource type for DNR rule matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[allow(non_camel_case_types)]
pub enum DnrResourceType {
    main_frame,
    sub_frame,
    stylesheet,
    script,
    image,
    font,
    object,
    xmlhttprequest,
    ping,
    csp_report,
    media,
    websocket,
    webtransport,
    webbundle,
    other,
}

/// Result of evaluating DNR rules against a request.
#[derive(Debug, Clone)]
pub enum DnrVerdict {
    /// Block the request.
    Block,
    /// Redirect to this URL.
    Redirect(String),
    /// Modify headers before sending.
    ModifyHeaders {
        request_headers: Vec<DnrHeaderOperation>,
        response_headers: Vec<DnrHeaderOperation>,
    },
    /// Allow (override matching block rule).
    Allow,
}

/// A ruleset loaded from a static JSON file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DnrRuleset {
    pub id: String,
    pub enabled: bool,
    pub rules: Vec<DnrRule>,
}

/// Extension declarativeNetRequest API.
pub trait DeclarativeNetRequestApi: Send + Sync {
    /// Update the enabled status of a static ruleset.
    fn update_static_ruleset(&self, ruleset_id: &str, enabled: bool) -> Result<()>;

    /// Get all enabled static rulesets.
    fn get_enabled_rulesets(&self) -> Vec<DnrRuleset>;

    /// Load a static ruleset from JSON.
    fn load_static_ruleset(&self, ruleset: DnrRuleset) -> Result<()>;

    /// Add dynamic rules (session-scoped).
    fn add_dynamic_rules(&self, rules: Vec<DnrRule>) -> Result<()>;

    /// Remove dynamic rules by ID.
    fn remove_dynamic_rules(&self, rule_ids: Vec<u32>) -> Result<()>;

    /// Evaluate all loaded rules against a URL and resource type.
    /// Returns the highest-priority matching verdict, or None if no rules match.
    fn evaluate(
        &self,
        url: &str,
        resource_type: DnrResourceType,
        initiator: Option<&str>,
    ) -> Option<DnrVerdict>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dnr_rule() {
        let json = r#"{
            "id": 1,
            "priority": 1,
            "action": { "type": "block" },
            "condition": {
                "urlFilter": "||ads.example.com^",
                "resourceTypes": ["script", "image"]
            }
        }"#;
        let rule: DnrRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.id, 1);
        assert_eq!(rule.priority, Some(1));
        assert!(matches!(rule.action, DnrAction::block));
        assert_eq!(
            rule.condition.url_filter.as_deref(),
            Some("||ads.example.com^")
        );
    }

    #[test]
    fn test_parse_redirect_rule() {
        let json = r#"{
            "id": 2,
            "action": {
                "type": "redirect",
                "url": "https://safe.example.com"
            },
            "condition": {
                "urlFilter": "||tracker.com^"
            }
        }"#;
        let rule: DnrRule = serde_json::from_str(json).unwrap();
        match rule.action {
            DnrAction::redirect { url, .. } => {
                assert_eq!(url.as_deref(), Some("https://safe.example.com"));
            }
            _ => panic!("Expected redirect action"),
        }
    }

    #[test]
    fn test_parse_modify_headers_rule() {
        let json = r#"{
            "id": 3,
            "action": {
                "type": "modifyHeaders",
                "requestHeaders": [
                    { "header": "X-Custom", "operation": "set", "value": "test" }
                ]
            },
            "condition": {
                "urlFilter": "*"
            }
        }"#;
        let rule: DnrRule = serde_json::from_str(json).unwrap();
        match rule.action {
            DnrAction::modify_headers {
                request_headers: Some(headers),
                ..
            } => {
                assert_eq!(headers.len(), 1);
                assert_eq!(headers[0].header, "X-Custom");
            }
            _ => panic!("Expected modifyHeaders action"),
        }
    }
}
