//! Concrete implementation of [`CookiesApi`] for Aileron.
//!
//! Uses an in-memory cookie jar backed by `Vec<Cookie>`. A production build
//! would delegate to the real browser cookie store (WebKitGTK cookies DB).

use std::sync::Arc;

use parking_lot::RwLock;

use crate::extensions::cookies::{
    Cookie, CookieChangeCause, CookieChangeInfo, CookieGetAllParams, CookieGetParams,
    CookieRemoveParams, CookieSetParams, CookieStore, CookiesApi,
};
use crate::extensions::types::{ListenerId, Result};

type ChangeCallback = Arc<dyn Fn(CookieChangeInfo) + Send + Sync>;

/// In-memory cookie store for extensions.
pub struct InMemoryCookieStore {
    cookies: RwLock<Vec<Cookie>>,
}

impl Default for InMemoryCookieStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryCookieStore {
    pub fn new() -> Self {
        Self {
            cookies: RwLock::new(Vec::new()),
        }
    }
}

/// Check if a cookie matches a URL's domain and path.
fn cookie_matches_url(cookie: &Cookie, url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };

    let host = match parsed.host_str() {
        Some(h) => h,
        None => return false,
    };

    // Domain matching: strip leading dot from cookie domain for comparison
    let cookie_domain = cookie.domain.strip_prefix('.').unwrap_or(&cookie.domain);

    // Domain matching
    let domain_matches = if cookie.host_only {
        host == cookie_domain
    } else {
        host == cookie_domain || host.ends_with(&format!(".{cookie_domain}"))
    };

    if !domain_matches {
        return false;
    }

    // Path matching
    let path = parsed.path();
    path.starts_with(&cookie.path) || (cookie.path == "/" && !path.is_empty())
}

/// Build a domain from a URL for setting cookies.
fn domain_from_url(url: &str) -> Option<String> {
    url::Url::parse(url).ok().and_then(|u| {
        u.host_str().map(|h| {
            if h.starts_with('.') {
                h.to_string()
            } else {
                format!(".{h}")
            }
        })
    })
}

impl CookieStore for InMemoryCookieStore {
    fn get_cookie(&self, params: &CookieGetParams) -> Result<Option<Cookie>> {
        let cookies = self.cookies.read();
        Ok(cookies
            .iter()
            .find(|c| c.name == params.name && cookie_matches_url(c, &params.url))
            .cloned())
    }

    fn get_all_cookies(&self, params: &CookieGetAllParams) -> Result<Vec<Cookie>> {
        let cookies = self.cookies.read();
        let filtered: Vec<Cookie> = cookies
            .iter()
            .filter(|c| {
                if let Some(ref url) = params.url
                    && !cookie_matches_url(c, url)
                {
                    return false;
                }
                if let Some(ref name) = params.name
                    && c.name != *name
                {
                    return false;
                }
                if let Some(ref domain) = params.domain
                    && c.domain != *domain
                {
                    return false;
                }
                if let Some(ref path) = params.path
                    && c.path != *path
                {
                    return false;
                }
                if let Some(secure) = params.secure
                    && c.secure != secure
                {
                    return false;
                }
                if let Some(session) = params.session
                    && c.session != session
                {
                    return false;
                }
                true
            })
            .cloned()
            .collect();
        Ok(filtered)
    }

    fn set_cookie(&self, params: &CookieSetParams) -> Result<Option<Cookie>> {
        let name = params.name.clone().unwrap_or_default();
        let value = params.value.clone().unwrap_or_default();
        let domain = params
            .domain
            .clone()
            .or_else(|| domain_from_url(&params.url))
            .unwrap_or_default();
        let path = params.path.clone().unwrap_or_else(|| "/".to_string());
        let secure = params.secure.unwrap_or(false);
        let http_only = params.http_only.unwrap_or(false);
        let same_site = params.same_site;
        let expiration_date = params.expiration_date;
        let session = expiration_date.is_none();

        let cookie = Cookie {
            name,
            value,
            domain,
            host_only: params.domain.is_none(),
            path,
            secure,
            http_only,
            same_site,
            session,
            expiration_date,
            store_id: params.store_id.clone(),
        };

        let mut cookies = self.cookies.write();

        // Remove existing cookie with same name/domain/path (overwrite)
        let _overwritten = cookies
            .iter()
            .position(|c| {
                c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path
            })
            .map(|i| cookies.remove(i));

        cookies.push(cookie.clone());
        Ok(Some(cookie))
    }

    fn remove_cookie(&self, params: &CookieRemoveParams) -> Result<Option<Cookie>> {
        let mut cookies = self.cookies.write();
        let idx = cookies
            .iter()
            .position(|c| c.name == params.name && cookie_matches_url(c, &params.url));
        Ok(idx.map(|i| cookies.remove(i)))
    }
}

/// Extension-facing cookies API implementation.
pub struct AileronCookiesApi {
    store: Arc<dyn CookieStore>,
    callbacks: RwLock<Vec<(ListenerId, ChangeCallback)>>,
}

impl AileronCookiesApi {
    pub fn new(store: Arc<dyn CookieStore>) -> Self {
        Self {
            store,
            callbacks: RwLock::new(Vec::new()),
        }
    }

    /// Create with an in-memory store (for testing / no browser cookie DB).
    pub fn new_in_memory() -> Self {
        Self::new(Arc::new(InMemoryCookieStore::new()))
    }

    fn notify_changed(&self, info: CookieChangeInfo) {
        let callbacks = self.callbacks.read();
        for (_, cb) in callbacks.iter() {
            cb(info.clone());
        }
    }
}

impl CookiesApi for AileronCookiesApi {
    fn get(&self, params: CookieGetParams) -> Result<Option<Cookie>> {
        self.store.get_cookie(&params)
    }

    fn get_all(&self, params: CookieGetAllParams) -> Result<Vec<Cookie>> {
        self.store.get_all_cookies(&params)
    }

    fn set(&self, params: CookieSetParams) -> Result<Option<Cookie>> {
        // Check if cookie was overwritten for change cause
        let existing = self.store.get_cookie(&CookieGetParams {
            url: params.url.clone(),
            name: params.name.clone().unwrap_or_default(),
            store_id: params.store_id.clone(),
        })?;

        let result = self.store.set_cookie(&params)?;

        if let Some(ref cookie) = result {
            let cause = if existing.is_some() {
                CookieChangeCause::Overwritten
            } else {
                CookieChangeCause::Explicit
            };
            self.notify_changed(CookieChangeInfo {
                removed: false,
                cookie: cookie.clone(),
                cause,
            });
        }

        Ok(result)
    }

    fn remove(&self, params: CookieRemoveParams) -> Result<Option<Cookie>> {
        let result = self.store.remove_cookie(&params)?;

        if let Some(ref cookie) = result {
            self.notify_changed(CookieChangeInfo {
                removed: true,
                cookie: cookie.clone(),
                cause: CookieChangeCause::Explicit,
            });
        }

        Ok(result)
    }

    fn on_changed(&self, callback: Arc<dyn Fn(CookieChangeInfo) + Send + Sync>) {
        let mut callbacks = self.callbacks.write();
        let id = ListenerId(super::super::impls::next_listener_id_raw());
        callbacks.push((id, callback));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get_cookie() {
        let api = AileronCookiesApi::new_in_memory();
        api.set(CookieSetParams {
            url: "https://example.com".into(),
            name: Some("foo".into()),
            value: Some("bar".into()),
            domain: None,
            path: None,
            secure: None,
            http_only: None,
            same_site: None,
            expiration_date: None,
            store_id: None,
        })
        .unwrap();

        let cookie = api
            .get(CookieGetParams {
                url: "https://example.com".into(),
                name: "foo".into(),
                store_id: None,
            })
            .unwrap();
        assert!(cookie.is_some());
        assert_eq!(cookie.unwrap().value, "bar");
    }

    #[test]
    fn test_get_all_with_domain_filter() {
        let api = AileronCookiesApi::new_in_memory();
        api.set(CookieSetParams {
            url: "https://example.com".into(),
            name: Some("a".into()),
            value: Some("1".into()),
            domain: None,
            path: None,
            secure: None,
            http_only: None,
            same_site: None,
            expiration_date: None,
            store_id: None,
        })
        .unwrap();
        api.set(CookieSetParams {
            url: "https://other.com".into(),
            name: Some("b".into()),
            value: Some("2".into()),
            domain: None,
            path: None,
            secure: None,
            http_only: None,
            same_site: None,
            expiration_date: None,
            store_id: None,
        })
        .unwrap();

        let cookies = api
            .get_all(CookieGetAllParams {
                url: Some("https://example.com".into()),
                name: None,
                domain: None,
                path: None,
                secure: None,
                session: None,
                store_id: None,
            })
            .unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "a");
    }

    #[test]
    fn test_remove_cookie() {
        let api = AileronCookiesApi::new_in_memory();
        api.set(CookieSetParams {
            url: "https://example.com".into(),
            name: Some("rm".into()),
            value: Some("val".into()),
            domain: None,
            path: None,
            secure: None,
            http_only: None,
            same_site: None,
            expiration_date: None,
            store_id: None,
        })
        .unwrap();

        let removed = api
            .remove(CookieRemoveParams {
                url: "https://example.com".into(),
                name: "rm".into(),
                store_id: None,
            })
            .unwrap();
        assert!(removed.is_some());

        let cookie = api
            .get(CookieGetParams {
                url: "https://example.com".into(),
                name: "rm".into(),
                store_id: None,
            })
            .unwrap();
        assert!(cookie.is_none());
    }

    #[test]
    fn test_set_overwrites_existing() {
        let api = AileronCookiesApi::new_in_memory();
        api.set(CookieSetParams {
            url: "https://example.com".into(),
            name: Some("key".into()),
            value: Some("old".into()),
            domain: None,
            path: None,
            secure: None,
            http_only: None,
            same_site: None,
            expiration_date: None,
            store_id: None,
        })
        .unwrap();
        api.set(CookieSetParams {
            url: "https://example.com".into(),
            name: Some("key".into()),
            value: Some("new".into()),
            domain: None,
            path: None,
            secure: None,
            http_only: None,
            same_site: None,
            expiration_date: None,
            store_id: None,
        })
        .unwrap();

        let cookie = api
            .get(CookieGetParams {
                url: "https://example.com".into(),
                name: "key".into(),
                store_id: None,
            })
            .unwrap()
            .unwrap();
        assert_eq!(cookie.value, "new");
    }

    #[test]
    fn test_on_changed_fires_on_set() {
        let api = AileronCookiesApi::new_in_memory();
        let changes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let changes_clone = changes.clone();
        api.on_changed(Arc::new(move |info| {
            if !info.removed {
                changes_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }));

        api.set(CookieSetParams {
            url: "https://example.com".into(),
            name: Some("test".into()),
            value: Some("val".into()),
            domain: None,
            path: None,
            secure: None,
            http_only: None,
            same_site: None,
            expiration_date: None,
            store_id: None,
        })
        .unwrap();

        assert_eq!(changes.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn test_on_changed_fires_on_remove() {
        let api = AileronCookiesApi::new_in_memory();
        api.set(CookieSetParams {
            url: "https://example.com".into(),
            name: Some("test".into()),
            value: Some("val".into()),
            domain: None,
            path: None,
            secure: None,
            http_only: None,
            same_site: None,
            expiration_date: None,
            store_id: None,
        })
        .unwrap();

        let removes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let removes_clone = removes.clone();
        api.on_changed(Arc::new(move |info| {
            if info.removed {
                removes_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }));

        api.remove(CookieRemoveParams {
            url: "https://example.com".into(),
            name: "test".into(),
            store_id: None,
        })
        .unwrap();

        assert_eq!(removes.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let store = InMemoryCookieStore::new();
        let result = store
            .get_cookie(&CookieGetParams {
                url: "https://example.com".into(),
                name: "nope".into(),
                store_id: None,
            })
            .unwrap();
        assert!(result.is_none());
    }
}
