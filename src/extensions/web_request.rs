use std::net::IpAddr;
use url::Url;

use crate::extensions::types::{
    FrameId, ListenerId, RequestId, Result, TabId, UrlPattern, WindowId,
};

/// Resource types for request filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    MainFrame,
    SubFrame,
    Stylesheet,
    Script,
    Image,
    Font,
    Object,
    XmlHttpRequest,
    Ping,
    Media,
    Websocket,
    Other,
}

/// What extra information to include in request details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtraInfoSpec {
    RequestHeaders,
    ResponseHeaders,
    Blocking,
}

/// Filter for which requests to observe.
#[derive(Debug, Clone)]
pub struct RequestFilter {
    pub urls: Vec<UrlPattern>,
    pub types: Option<Vec<ResourceType>>,
    pub tab_id: Option<TabId>,
    pub window_id: Option<WindowId>,
}

/// Response from a blocking webRequest handler.
#[derive(Debug, Clone, Default)]
pub struct BlockingResponse {
    pub cancel: Option<bool>,
    pub redirect_url: Option<Url>,
    pub request_headers: Option<Vec<HttpHeader>>,
    pub response_headers: Option<Vec<HttpHeader>>,
    pub auth_credentials: Option<AuthCredentials>,
}

/// An HTTP header with name and value.
#[derive(Debug, Clone)]
pub struct HttpHeader {
    pub name: String,
    pub value: Option<String>,
}

/// Authentication credentials for onAuthRequired.
#[derive(Debug, Clone)]
pub struct AuthCredentials {
    pub username: String,
    pub password: zeroize::Zeroizing<String>,
}

/// Details provided to onBeforeRequest handler.
#[derive(Debug, Clone)]
pub struct RequestDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub method: String,
    pub frame_id: FrameId,
    pub parent_frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub origin_url: Option<Url>,
    pub timestamp: f64,
    pub request_headers: Option<Vec<HttpHeader>>,
}

/// Details provided to onBeforeSendHeaders handler.
#[derive(Debug, Clone)]
pub struct BeforeSendHeadersDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub method: String,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub request_headers: Vec<HttpHeader>,
}

/// Details provided to onHeadersReceived handler.
#[derive(Debug, Clone)]
pub struct HeadersReceivedDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub status_line: String,
    pub status_code: u16,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub response_headers: Vec<HttpHeader>,
}

/// Details provided to onBeforeRedirect handler.
#[derive(Debug, Clone)]
pub struct RedirectDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub from_url: Url,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub status_code: u32,
    pub redirect_url: Url,
}

/// Details provided to onCompleted handler.
#[derive(Debug, Clone)]
pub struct CompletedDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub from_cache: bool,
    pub status_code: u16,
    pub ip: Option<IpAddr>,
    pub timestamp: f64,
}

/// Details provided to onErrorOccurred handler.
#[derive(Debug, Clone)]
pub struct ErrorOccurredDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub error: String,
    pub timestamp: f64,
}

/// Details provided to onAuthRequired handler.
#[derive(Debug, Clone)]
pub struct AuthRequiredDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub realm: Option<String>,
    pub challenger: AuthChallenger,
    pub is_proxy: bool,
}

#[derive(Debug, Clone)]
pub struct AuthChallenger {
    pub host: String,
    pub port: u16,
}

/// Intercept and modify network requests in-flight.
pub trait WebRequestApi: Send + Sync {
    fn on_before_request(
        &self,
        filter: RequestFilter,
        extra_info_spec: Vec<ExtraInfoSpec>,
        handler: Box<dyn Fn(RequestDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId;

    fn on_before_send_headers(
        &self,
        filter: RequestFilter,
        extra_info_spec: Vec<ExtraInfoSpec>,
        handler: Box<dyn Fn(BeforeSendHeadersDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId;

    fn on_headers_received(
        &self,
        filter: RequestFilter,
        extra_info_spec: Vec<ExtraInfoSpec>,
        handler: Box<dyn Fn(HeadersReceivedDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId;

    fn on_auth_required(
        &self,
        filter: RequestFilter,
        handler: Box<dyn Fn(AuthRequiredDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId;

    fn on_before_redirect(
        &self,
        filter: RequestFilter,
        callback: Box<dyn Fn(RedirectDetails) + Send + Sync>,
    ) -> ListenerId;

    fn on_completed(
        &self,
        filter: RequestFilter,
        callback: Box<dyn Fn(CompletedDetails) + Send + Sync>,
    ) -> ListenerId;

    fn on_error_occurred(
        &self,
        filter: RequestFilter,
        callback: Box<dyn Fn(ErrorOccurredDetails) + Send + Sync>,
    ) -> ListenerId;

    fn remove_listener(&self, listener_id: ListenerId) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocking_response_default() {
        let resp = BlockingResponse::default();
        assert!(resp.cancel.is_none());
        assert!(resp.redirect_url.is_none());
    }

    #[test]
    fn test_blocking_response_cancel() {
        let resp = BlockingResponse {
            cancel: Some(true),
            ..Default::default()
        };
        assert_eq!(resp.cancel, Some(true));
    }

    #[test]
    fn test_http_header() {
        let header = HttpHeader {
            name: "Content-Type".into(),
            value: Some("text/html".into()),
        };
        assert_eq!(header.name, "Content-Type");
    }

    #[test]
    fn test_http_header_remove() {
        let header = HttpHeader {
            name: "X-Custom".into(),
            value: None,
        };
        assert!(header.value.is_none());
    }

    #[test]
    fn test_request_filter() {
        let filter = RequestFilter {
            urls: vec![UrlPattern("*://*.example.com/*".into())],
            types: Some(vec![ResourceType::MainFrame]),
            tab_id: Some(TabId(1)),
            window_id: None,
        };
        assert_eq!(filter.urls.len(), 1);
        assert!(filter.window_id.is_none());
    }

    #[test]
    fn test_resource_type_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ResourceType::Script);
        set.insert(ResourceType::Image);
        set.insert(ResourceType::Script);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_auth_credentials() {
        let creds = AuthCredentials {
            username: "admin".into(),
            password: zeroize::Zeroizing::new("secret".into()),
        };
        assert_eq!(creds.username, "admin");
    }

    #[test]
    fn test_request_details() {
        let details = RequestDetails {
            request_id: RequestId(42),
            url: Url::parse("https://example.com").unwrap(),
            method: "GET".into(),
            frame_id: FrameId(0),
            parent_frame_id: FrameId(u32::MAX),
            tab_id: Some(TabId(1)),
            type_: ResourceType::MainFrame,
            origin_url: None,
            timestamp: 1000.0,
            request_headers: None,
        };
        assert_eq!(details.request_id, RequestId(42));
        assert_eq!(details.method, "GET");
    }

    #[test]
    fn test_headers_received_details() {
        let details = HeadersReceivedDetails {
            request_id: RequestId(1),
            url: Url::parse("https://example.com").unwrap(),
            status_line: "HTTP/1.1 200 OK".into(),
            status_code: 200,
            frame_id: FrameId(0),
            tab_id: None,
            type_: ResourceType::MainFrame,
            response_headers: vec![],
        };
        assert_eq!(details.status_code, 200);
        assert_eq!(details.status_line, "HTTP/1.1 200 OK");
    }

    #[test]
    fn test_auth_challenger() {
        let challenger = AuthChallenger {
            host: "proxy.example.com".into(),
            port: 8080,
        };
        assert_eq!(challenger.port, 8080);
    }
}
