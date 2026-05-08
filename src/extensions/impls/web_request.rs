use std::sync::Mutex;

use url::Url;

use crate::extensions::permissions::Permission;
use crate::extensions::types::{ExtensionError, ListenerId, Result};
use crate::extensions::web_request::{
    AuthRequiredDetails, BeforeSendHeadersDetails, BlockingResponse, CompletedDetails,
    ErrorOccurredDetails, ExtraInfoSpec, HeadersReceivedDetails, RedirectDetails, RequestDetails,
    RequestFilter, WebRequestApi, WebRequestInterceptor,
};

use super::next_listener_id;

type BlockingHandler<D> = Mutex<
    Vec<(
        ListenerId,
        RequestFilter,
        Box<dyn Fn(D) -> BlockingResponse + Send + Sync>,
    )>,
>;

type CallbackHandler<D> = Mutex<Vec<(ListenerId, RequestFilter, Box<dyn Fn(D) + Send + Sync>)>>;

pub(crate) struct AileronWebRequestApi {
    before_request_handlers: BlockingHandler<RequestDetails>,
    before_send_headers_handlers: BlockingHandler<BeforeSendHeadersDetails>,
    headers_received_handlers: BlockingHandler<HeadersReceivedDetails>,
    auth_required_handlers: BlockingHandler<AuthRequiredDetails>,
    before_redirect_handlers: CallbackHandler<RedirectDetails>,
    completed_handlers: CallbackHandler<CompletedDetails>,
    error_occurred_handlers: CallbackHandler<ErrorOccurredDetails>,
    granted_permissions: std::collections::HashSet<Permission>,
}

impl AileronWebRequestApi {
    pub(super) fn new() -> Self {
        Self {
            before_request_handlers: Mutex::new(Vec::new()),
            before_send_headers_handlers: Mutex::new(Vec::new()),
            headers_received_handlers: Mutex::new(Vec::new()),
            auth_required_handlers: Mutex::new(Vec::new()),
            before_redirect_handlers: Mutex::new(Vec::new()),
            completed_handlers: Mutex::new(Vec::new()),
            error_occurred_handlers: Mutex::new(Vec::new()),
            granted_permissions: std::collections::HashSet::new(),
        }
    }

    pub(super) fn set_permissions(&mut self, permissions: std::collections::HashSet<Permission>) {
        self.granted_permissions = permissions;
    }

    fn has_web_request_permission(&self) -> bool {
        self.granted_permissions.contains(&Permission::WebRequest)
    }

    /// Check if a URL matches any pattern in the filter.
    fn url_matches_filter(url: &Url, filter: &RequestFilter) -> bool {
        // If no URL patterns, match all
        if filter.urls.is_empty() {
            return true;
        }
        filter.urls.iter().any(|pattern| {
            let pat_str = pattern.0.as_str();
            simple_url_pattern_match(pat_str, url.as_str())
        })
    }

    /// Fire all registered on_before_request handlers for a request.
    /// Returns the first non-default BlockingResponse (first handler wins).
    pub fn fire_on_before_request(&self, details: &RequestDetails) -> BlockingResponse {
        let handlers = self
            .before_request_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                let response = handler(details.clone());
                if response.cancel == Some(true) || response.redirect_url.is_some() {
                    return response;
                }
            }
        }
        BlockingResponse::default()
    }

    /// Fire all registered on_headers_received handlers.
    #[allow(dead_code)]
    pub fn fire_on_headers_received(&self, details: &HeadersReceivedDetails) -> BlockingResponse {
        let handlers = self
            .headers_received_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                let response = handler(details.clone());
                if response.cancel == Some(true) || response.response_headers.is_some() {
                    return response;
                }
            }
        }
        BlockingResponse::default()
    }

    /// Fire all registered on_before_send_headers handlers.
    #[allow(dead_code)]
    pub fn fire_on_before_send_headers(
        &self,
        details: &BeforeSendHeadersDetails,
    ) -> BlockingResponse {
        let handlers = self
            .before_send_headers_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                let response = handler(details.clone());
                if response.cancel == Some(true)
                    || response.request_headers.is_some()
                    || response.redirect_url.is_some()
                {
                    return response;
                }
            }
        }
        BlockingResponse::default()
    }

    /// Fire all registered on_completed handlers.
    pub fn fire_on_completed(&self, details: &CompletedDetails) {
        let handlers = self
            .completed_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                handler(details.clone());
            }
        }
    }

    /// Fire all registered on_error_occurred handlers.
    #[allow(dead_code)]
    pub fn fire_on_error_occurred(&self, details: &ErrorOccurredDetails) {
        let handlers = self
            .error_occurred_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                handler(details.clone());
            }
        }
    }

    /// Fire all registered on_before_redirect handlers.
    #[allow(dead_code)]
    pub fn fire_on_before_redirect(&self, details: &RedirectDetails) {
        let handlers = self
            .before_redirect_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                handler(details.clone());
            }
        }
    }
}

/// Simple URL pattern matching for extension filters.
/// Supports wildcards: `*://*.example.com/*` matches any subdomain.
#[allow(dead_code)]
pub(super) fn simple_url_pattern_match(pattern: &str, url: &str) -> bool {
    let pat_lower = pattern.to_lowercase();
    let url_lower = url.to_lowercase();

    if pat_lower == "<all_urls>" {
        return true;
    }

    // Split pattern into scheme, host, path parts
    if let Some(star_idx) = pat_lower.find("://") {
        let scheme = &pat_lower[..star_idx];
        let rest = &pat_lower[star_idx + 3..];

        // Check scheme: `*` matches any scheme
        if scheme != "*" && !url_lower.starts_with(&format!("{scheme}://")) {
            return false;
        }

        // Extract the URL portion after the scheme
        let url_rest = if scheme == "*" {
            if let Some(idx) = url_lower.find("://") {
                &url_lower[idx + 3..]
            } else {
                return false;
            }
        } else {
            &url_lower[scheme.len() + 3..]
        };

        // Check host + path
        if rest == "*" || rest == "/*" {
            return true;
        }

        // Handle wildcard host patterns like *.example.com/*
        if let Some(pattern_domain) = rest.strip_prefix("*.") {
            // URL rest should end with the pattern domain
            // e.g., "*.example.com/*" should match "sub.example.com/path"
            if let Some(slash_idx) = pattern_domain.find('/') {
                let domain_pat = &pattern_domain[..slash_idx];
                let path_pat = &pattern_domain[slash_idx..];
                if let Some(url_slash) = url_rest.find('/') {
                    let url_domain = &url_rest[..url_slash];
                    let url_path = &url_rest[url_slash..];
                    if url_domain.ends_with(domain_pat)
                        && (path_pat == "/*" || path_pat == url_path)
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        // Exact host match or host/path prefix match
        if let Some(slash_idx) = rest.find('/') {
            let host_pat = &rest[..slash_idx];
            let path_pat = &rest[slash_idx..];
            if url_rest.starts_with(host_pat)
                && let Some(url_path) = url_rest.strip_prefix(host_pat)
                && (path_pat == "/*" || path_pat == url_path)
            {
                return true;
            }
        } else if url_rest == rest {
            return true;
        }
    }

    false
}

impl WebRequestApi for AileronWebRequestApi {
    fn on_before_request(
        &self,
        filter: RequestFilter,
        _extra_info_spec: Vec<ExtraInfoSpec>,
        handler: Box<dyn Fn(RequestDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId {
        if !self.has_web_request_permission() {
            tracing::warn!(
                target: "extensions",
                "webRequest.onBeforeRequest: denied — 'webRequest' permission not granted"
            );
            return next_listener_id();
        }
        let id = next_listener_id();
        tracing::info!(
            target: "extensions",
            "webRequest.onBeforeRequest registered (listener {:?}, {} url patterns)",
            id,
            filter.urls.len()
        );
        self.before_request_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, handler));
        id
    }

    fn on_before_send_headers(
        &self,
        filter: RequestFilter,
        _extra_info_spec: Vec<ExtraInfoSpec>,
        handler: Box<dyn Fn(BeforeSendHeadersDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId {
        if !self.has_web_request_permission() {
            tracing::warn!(
                target: "extensions",
                "webRequest.onBeforeSendHeaders: denied — 'webRequest' permission not granted"
            );
            return next_listener_id();
        }
        let id = next_listener_id();
        tracing::info!(
            target: "extensions",
            "webRequest.onBeforeSendHeaders registered (listener {:?})",
            id
        );
        self.before_send_headers_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, handler));
        id
    }

    fn on_headers_received(
        &self,
        filter: RequestFilter,
        _extra_info_spec: Vec<ExtraInfoSpec>,
        handler: Box<dyn Fn(HeadersReceivedDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId {
        if !self.has_web_request_permission() {
            tracing::warn!(
                target: "extensions",
                "webRequest.onHeadersReceived: denied — 'webRequest' permission not granted"
            );
            return next_listener_id();
        }
        let id = next_listener_id();
        tracing::info!(
            target: "extensions",
            "webRequest.onHeadersReceived registered (listener {:?})",
            id
        );
        self.headers_received_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, handler));
        id
    }

    fn on_auth_required(
        &self,
        filter: RequestFilter,
        handler: Box<dyn Fn(AuthRequiredDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId {
        if !self.has_web_request_permission() {
            tracing::warn!(
                target: "extensions",
                "webRequest.onAuthRequired: denied — 'webRequest' permission not granted"
            );
            return next_listener_id();
        }
        let id = next_listener_id();
        tracing::info!(
            target: "extensions",
            "webRequest.onAuthRequired registered (listener {:?})",
            id
        );
        self.auth_required_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, handler));
        id
    }

    fn on_before_redirect(
        &self,
        filter: RequestFilter,
        callback: Box<dyn Fn(RedirectDetails) + Send + Sync>,
    ) -> ListenerId {
        if !self.has_web_request_permission() {
            tracing::warn!(
                target: "extensions",
                "webRequest.onBeforeRedirect: denied — 'webRequest' permission not granted"
            );
            return next_listener_id();
        }
        let id = next_listener_id();
        self.before_redirect_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, callback));
        id
    }

    fn on_completed(
        &self,
        filter: RequestFilter,
        callback: Box<dyn Fn(CompletedDetails) + Send + Sync>,
    ) -> ListenerId {
        if !self.has_web_request_permission() {
            tracing::warn!(
                target: "extensions",
                "webRequest.onCompleted: denied — 'webRequest' permission not granted"
            );
            return next_listener_id();
        }
        let id = next_listener_id();
        self.completed_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, callback));
        id
    }

    fn on_error_occurred(
        &self,
        filter: RequestFilter,
        callback: Box<dyn Fn(ErrorOccurredDetails) + Send + Sync>,
    ) -> ListenerId {
        if !self.has_web_request_permission() {
            tracing::warn!(
                target: "extensions",
                "webRequest.onErrorOccurred: denied — 'webRequest' permission not granted"
            );
            return next_listener_id();
        }
        let id = next_listener_id();
        self.error_occurred_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, callback));
        id
    }

    fn remove_listener(&self, listener_id: ListenerId) -> Result<()> {
        macro_rules! remove_from {
            ($field:expr) => {{
                let mut handlers = $field
                    .lock()
                    .map_err(|e| ExtensionError::Runtime(format!("Lock poisoned: {}", e)))?;
                let before = handlers.len();
                handlers.retain(|(id, _, _)| *id != listener_id);
                handlers.len() < before
            }};
        }

        let mut any_removed = false;
        if remove_from!(self.before_request_handlers) {
            any_removed = true;
        }
        if remove_from!(self.before_send_headers_handlers) {
            any_removed = true;
        }
        if remove_from!(self.headers_received_handlers) {
            any_removed = true;
        }
        if remove_from!(self.auth_required_handlers) {
            any_removed = true;
        }
        if remove_from!(self.before_redirect_handlers) {
            any_removed = true;
        }
        if remove_from!(self.completed_handlers) {
            any_removed = true;
        }
        if remove_from!(self.error_occurred_handlers) {
            any_removed = true;
        }

        if any_removed {
            tracing::info!(
                target: "extensions",
                "webRequest listener {:?} removed",
                listener_id
            );
            Ok(())
        } else {
            Err(ExtensionError::NotFound(format!(
                "Listener {listener_id:?} not found"
            )))
        }
    }
}

impl WebRequestInterceptor for AileronWebRequestApi {
    fn fire_on_before_request(&self, details: &RequestDetails) -> BlockingResponse {
        AileronWebRequestApi::fire_on_before_request(self, details)
    }

    fn fire_on_headers_received(&self, details: &HeadersReceivedDetails) -> BlockingResponse {
        AileronWebRequestApi::fire_on_headers_received(self, details)
    }

    fn fire_on_before_send_headers(&self, details: &BeforeSendHeadersDetails) -> BlockingResponse {
        AileronWebRequestApi::fire_on_before_send_headers(self, details)
    }

    fn fire_on_completed(&self, details: &CompletedDetails) {
        AileronWebRequestApi::fire_on_completed(self, details);
    }

    fn fire_on_error_occurred(&self, details: &ErrorOccurredDetails) {
        AileronWebRequestApi::fire_on_error_occurred(self, details);
    }

    fn fire_on_before_redirect(&self, details: &RedirectDetails) {
        AileronWebRequestApi::fire_on_before_redirect(self, details);
    }
}
