//! WebExtensions `browser.cookies` API.
//!
//! Provides cookie management for extensions. Extensions can get, set, remove,
//! and query cookies, and register callbacks for cookie changes.

use std::sync::Arc;

use crate::extensions::types::Result;

/// A cookie object matching the WebExtensions API shape.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub host_only: bool,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<SameSiteStatus>,
    pub session: bool,
    /// Expiration date as milliseconds since epoch. None for session cookies.
    pub expiration_date: Option<f64>,
    pub store_id: Option<String>,
}

/// Same-site cookie attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(non_camel_case_types)]
pub enum SameSiteStatus {
    no_restriction,
    lax,
    strict,
}

/// Parameters for the `get` method.
#[derive(Debug, Clone)]
pub struct CookieGetParams {
    pub url: String,
    pub name: String,
    pub store_id: Option<String>,
}

/// Parameters for the `getAll` method.
#[derive(Debug, Clone)]
pub struct CookieGetAllParams {
    pub url: Option<String>,
    pub name: Option<String>,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: Option<bool>,
    pub session: Option<bool>,
    pub store_id: Option<String>,
}

/// Parameters for the `set` method.
#[derive(Debug, Clone)]
pub struct CookieSetParams {
    pub url: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: Option<bool>,
    pub http_only: Option<bool>,
    pub same_site: Option<SameSiteStatus>,
    pub expiration_date: Option<f64>,
    pub store_id: Option<String>,
}

/// Parameters for the `remove` method.
#[derive(Debug, Clone)]
pub struct CookieRemoveParams {
    pub url: String,
    pub name: String,
    pub store_id: Option<String>,
}

/// Details about a cookie change event.
#[derive(Debug, Clone)]
pub struct CookieChangeInfo {
    pub removed: bool,
    pub cookie: Cookie,
    pub cause: CookieChangeCause,
}

/// Why a cookie changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieChangeCause {
    /// The cookie was changed by an explicit call to cookies.set.
    Explicit,
    /// The cookie was automatically removed due to expiry.
    Expired,
    /// The cookie was evicted because too many cookies existed.
    Evicted,
    /// The cookie was automatically removed due to being overwritten.
    Overwritten,
    /// The cookie was created or modified due to a network request.
    /// (Aileron currently does not track this cause.)
    WebRequest,
}

/// Backend trait for cookie storage. The extension API delegates to this.
pub trait CookieStore: Send + Sync {
    /// Get a single cookie by URL + name.
    fn get_cookie(&self, params: &CookieGetParams) -> Result<Option<Cookie>>;

    /// Get all cookies matching the filter.
    fn get_all_cookies(&self, params: &CookieGetAllParams) -> Result<Vec<Cookie>>;

    /// Set a cookie. Returns the set cookie or an error.
    fn set_cookie(&self, params: &CookieSetParams) -> Result<Option<Cookie>>;

    /// Remove a cookie. Returns details of the removed cookie.
    fn remove_cookie(&self, params: &CookieRemoveParams) -> Result<Option<Cookie>>;
}

/// Extension cookies API — read, write, and observe browser cookies.
pub trait CookiesApi: Send + Sync {
    fn get(&self, params: CookieGetParams) -> Result<Option<Cookie>>;

    fn get_all(&self, params: CookieGetAllParams) -> Result<Vec<Cookie>>;

    fn set(&self, params: CookieSetParams) -> Result<Option<Cookie>>;

    fn remove(&self, params: CookieRemoveParams) -> Result<Option<Cookie>>;

    fn on_changed(&self, callback: Arc<dyn Fn(CookieChangeInfo) + Send + Sync>);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie_serialization() {
        let cookie = Cookie {
            name: "session".into(),
            value: "abc123".into(),
            domain: ".example.com".into(),
            host_only: false,
            path: "/".into(),
            secure: true,
            http_only: true,
            same_site: Some(SameSiteStatus::lax),
            session: true,
            expiration_date: None,
            store_id: None,
        };
        let json = serde_json::to_string(&cookie).unwrap();
        assert!(json.contains("\"name\":\"session\""));
        assert!(json.contains("\"value\":\"abc123\""));

        let parsed: Cookie = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "session");
        assert_eq!(parsed.domain, ".example.com");
    }

    #[test]
    fn test_cookie_get_params() {
        let params = CookieGetParams {
            url: "https://example.com".into(),
            name: "foo".into(),
            store_id: None,
        };
        assert_eq!(params.name, "foo");
    }

    #[test]
    fn test_cookie_set_params_minimal() {
        let params = CookieSetParams {
            url: "https://example.com".into(),
            name: None,
            value: None,
            domain: None,
            path: None,
            secure: None,
            http_only: None,
            same_site: None,
            expiration_date: None,
            store_id: None,
        };
        assert_eq!(params.url, "https://example.com");
    }

    #[test]
    fn test_same_site_status() {
        assert_eq!(SameSiteStatus::lax, SameSiteStatus::lax);
        assert_ne!(SameSiteStatus::lax, SameSiteStatus::strict);
    }
}
