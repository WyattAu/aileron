//! WebExtensions permission model.
//!
//! Maps API methods to required permissions and enforces access control.
//! Extensions declare permissions in their manifest.json; this module
//! validates that an extension has the necessary permission before
//! executing an API call.

use std::collections::HashSet;

/// Well-known WebExtensions permissions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Permission {
    // Core
    ActiveTab,
    Tabs,
    TabHide,
    TopSites,
    Bookmarks,
    History,
    Downloads,
    DownloadsOpen,
    DownloadsUI,
    Storage,
    UnlimitedStorage,
    Scripting,
    ClipboardWrite,
    ClipboardRead,
    Notifications,
    Alarms,
    // Networking
    WebRequest,
    WebRequestBlocking,
    WebRequestFilterResponse,
    DeclarativeNetRequest,
    Proxy,
    Dns,
    // Identity / privacy
    Identity,
    Privacy,
    BrowsingData,
    // Context menus
    ContextMenus,
    // Devtools
    Devtools,
    // Override pages
    Override,
    // Management
    Management,
    // Theme
    Theme,
    // Custom / unknown
    Custom(String),
}

impl Permission {
    /// Parse a permission string from a manifest into a Permission enum.
    pub fn parse(s: &str) -> Self {
        match s {
            "activeTab" => Self::ActiveTab,
            "tabs" => Self::Tabs,
            "tabHide" => Self::TabHide,
            "topSites" => Self::TopSites,
            "bookmarks" => Self::Bookmarks,
            "history" => Self::History,
            "downloads" => Self::Downloads,
            "downloads.open" => Self::DownloadsOpen,
            "downloads.ui" => Self::DownloadsUI,
            "storage" => Self::Storage,
            "unlimitedStorage" => Self::UnlimitedStorage,
            "scripting" => Self::Scripting,
            "clipboardWrite" => Self::ClipboardWrite,
            "clipboardRead" => Self::ClipboardRead,
            "notifications" => Self::Notifications,
            "alarms" => Self::Alarms,
            "webRequest" => Self::WebRequest,
            "webRequestBlocking" => Self::WebRequestBlocking,
            "webRequestFilterResponse" => Self::WebRequestFilterResponse,
            "declarativeNetRequest" => Self::DeclarativeNetRequest,
            "proxy" => Self::Proxy,
            "dns" => Self::Dns,
            "identity" => Self::Identity,
            "privacy" => Self::Privacy,
            "browsingData" => Self::BrowsingData,
            "contextMenus" => Self::ContextMenus,
            "devtools" => Self::Devtools,
            "override" => Self::Override,
            "management" => Self::Management,
            "theme" => Self::Theme,
            other => Self::Custom(other.to_string()),
        }
    }

    /// The API namespace this permission gates.
    pub fn api_namespace(&self) -> &'static str {
        match self {
            Self::Tabs | Self::ActiveTab | Self::TabHide => "tabs",
            Self::Bookmarks => "bookmarks",
            Self::History => "history",
            Self::Downloads | Self::DownloadsOpen | Self::DownloadsUI => "downloads",
            Self::Storage | Self::UnlimitedStorage => "storage",
            Self::Scripting => "scripting",
            Self::WebRequest | Self::WebRequestBlocking | Self::WebRequestFilterResponse => {
                "webRequest"
            }
            Self::Notifications => "notifications",
            Self::Alarms => "alarms",
            Self::ClipboardWrite | Self::ClipboardRead => "clipboard",
            Self::ContextMenus => "contextMenus",
            _ => "unknown",
        }
    }
}

/// Maps an API method to the permission(s) required to call it.
/// Returns the set of permissions needed; if empty, no permission is required.
pub fn required_permissions(api: &str, method: &str) -> Vec<Permission> {
    match (api, method) {
        // Tabs API
        ("tabs", "query") => vec![Permission::Tabs],
        ("tabs", "get") => vec![Permission::Tabs],
        ("tabs", "create") => vec![Permission::Tabs],
        ("tabs", "update") => vec![Permission::Tabs],
        ("tabs", "remove") => vec![Permission::Tabs],
        ("tabs", "duplicate") => vec![Permission::Tabs],
        ("tabs", "sendMessage") => vec![Permission::ActiveTab],
        ("tabs", "captureVisibleTab") => vec![Permission::ActiveTab],
        // Scripting API
        ("scripting", "executeScript") => vec![Permission::Scripting],
        ("scripting", "insertCSS") => vec![Permission::Scripting],
        ("scripting", "removeCSS") => vec![Permission::Scripting],
        // WebRequest API
        ("webRequest", _) => vec![Permission::WebRequest],
        // Bookmarks
        ("bookmarks", _) => vec![Permission::Bookmarks],
        // History
        ("history", _) => vec![Permission::History],
        // Downloads
        ("downloads", "open") => vec![Permission::DownloadsOpen],
        ("downloads", "show") => vec![Permission::DownloadsUI],
        ("downloads", "erase") => vec![Permission::Downloads],
        ("downloads", "setShelfEnabled") => vec![Permission::DownloadsUI],
        // Storage — always allowed if "storage" permission declared
        ("storage", _) => vec![Permission::Storage],
        // Runtime — always allowed (intrinsic)
        ("runtime", _) => vec![],
        // Notifications
        ("notifications", _) => vec![Permission::Notifications],
        // Default: no permission required
        _ => vec![],
    }
}

/// Parse a list of permission strings from a manifest into Permission enums.
pub fn parse_permissions(permissions: &[String]) -> HashSet<Permission> {
    permissions.iter().map(|s| Permission::parse(s)).collect()
}

/// Check if a set of granted permissions satisfies the requirements for an API call.
pub fn check_permission(granted: &HashSet<Permission>, api: &str, method: &str) -> bool {
    let required = required_permissions(api, method);
    required.is_empty() || required.iter().all(|p| granted.contains(p))
}

/// Check if a host permission pattern matches a URL.
/// Supports simple patterns: `*://*.example.com/*`, `https://example.com/*`
pub fn host_permission_matches(pattern: &str, url: &str) -> bool {
    // Exact match
    if pattern == "<all_urls>" {
        return true;
    }

    // Parse the pattern: scheme://host/path
    let (pat_scheme, pat_host, pat_path) = match parse_url_pattern(pattern) {
        Some(parts) => parts,
        None => return false,
    };

    let (url_scheme, url_host, url_path) = match parse_url_pattern(url) {
        Some(parts) => parts,
        None => return false,
    };

    // Scheme check
    if pat_scheme != "*" && pat_scheme != url_scheme {
        return false;
    }

    // Host check
    if !host_matches(pat_host, url_host) {
        return false;
    }

    // Path check
    if pat_path != "/*" && !path_matches(pat_path, url_path) {
        return false;
    }

    true
}

fn parse_url_pattern(s: &str) -> Option<(&str, &str, &str)> {
    // Find ://
    let sep = s.find("://")?;
    let scheme = &s[..sep];
    let rest = &s[sep + 3..];
    let slash = rest.find('/')?;
    let host = &rest[..slash];
    let path = &rest[slash..];
    Some((scheme, host, path))
}

fn host_matches(pattern: &str, host: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern.starts_with("*.") {
        let suffix = &pattern[1..]; // ".example.com"
        host.ends_with(suffix) || host == &pattern[2..]
    } else {
        pattern == host
    }
}

fn path_matches(pattern: &str, path: &str) -> bool {
    if pattern == "/*" {
        return true;
    }
    if pattern.ends_with("/*") {
        let prefix = &pattern[..pattern.len() - 1]; // "/"
        path.starts_with(prefix)
    } else {
        pattern == path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_from_str() {
        assert_eq!(Permission::parse("tabs"), Permission::Tabs);
        assert_eq!(Permission::parse("storage"), Permission::Storage);
        assert_eq!(Permission::parse("webRequest"), Permission::WebRequest);
        assert_eq!(Permission::parse("scripting"), Permission::Scripting);
        assert_eq!(
            Permission::parse("unknown"),
            Permission::Custom("unknown".into())
        );
    }

    #[test]
    fn test_parse_permissions() {
        let perms = parse_permissions(&["tabs".into(), "storage".into(), "webRequest".into()]);
        assert!(perms.contains(&Permission::Tabs));
        assert!(perms.contains(&Permission::Storage));
        assert!(perms.contains(&Permission::WebRequest));
        assert_eq!(perms.len(), 3);
    }

    #[test]
    fn test_check_permission_allowed() {
        let granted = parse_permissions(&["tabs".into(), "storage".into()]);
        assert!(check_permission(&granted, "tabs", "query"));
        assert!(check_permission(&granted, "tabs", "create"));
        assert!(check_permission(&granted, "storage", "get"));
        // Runtime needs no permission
        assert!(check_permission(&granted, "runtime", "sendMessage"));
    }

    #[test]
    fn test_check_permission_denied() {
        let granted = parse_permissions(&["storage".into()]);
        assert!(!check_permission(&granted, "tabs", "query"));
        assert!(!check_permission(&granted, "scripting", "executeScript"));
    }

    #[test]
    fn test_required_permissions_mapping() {
        let perms = required_permissions("tabs", "query");
        assert!(perms.contains(&Permission::Tabs));

        let perms = required_permissions("scripting", "executeScript");
        assert!(perms.contains(&Permission::Scripting));

        let perms = required_permissions("runtime", "sendMessage");
        assert!(perms.is_empty());
    }

    #[test]
    fn test_host_permission_all_urls() {
        assert!(host_permission_matches(
            "<all_urls>",
            "https://example.com/page"
        ));
        assert!(host_permission_matches(
            "<all_urls>",
            "http://localhost:8080/api"
        ));
    }

    #[test]
    fn test_host_permission_wildcard() {
        assert!(host_permission_matches(
            "*://*.example.com/*",
            "https://sub.example.com/page"
        ));
        assert!(host_permission_matches(
            "*://*.example.com/*",
            "https://example.com/page"
        ));
        assert!(!host_permission_matches(
            "*://*.example.com/*",
            "https://other.com/page"
        ));
    }

    #[test]
    fn test_host_permission_exact() {
        assert!(host_permission_matches(
            "https://example.com/*",
            "https://example.com/page"
        ));
        assert!(!host_permission_matches(
            "https://example.com/*",
            "http://example.com/page"
        ));
        assert!(!host_permission_matches(
            "https://example.com/*",
            "https://other.com/page"
        ));
    }

    #[test]
    fn test_host_permission_scheme_wildcard() {
        assert!(host_permission_matches(
            "*://example.com/*",
            "https://example.com/"
        ));
        assert!(host_permission_matches(
            "*://example.com/*",
            "http://example.com/"
        ));
    }

    #[test]
    fn test_permission_api_namespace() {
        assert_eq!(Permission::Tabs.api_namespace(), "tabs");
        assert_eq!(Permission::Storage.api_namespace(), "storage");
        assert_eq!(Permission::WebRequest.api_namespace(), "webRequest");
        assert_eq!(Permission::Scripting.api_namespace(), "scripting");
    }
}
