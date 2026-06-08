use std::borrow::Cow;

use wry::http::{Request, Response, Uri, header::HeaderValue};

use super::wry_pages::aileron_404_page;

pub type PageResolver = Box<dyn Fn(&str, &Uri) -> Option<String>>;

pub fn aileron_protocol_handler(
    resolver: PageResolver,
) -> impl Fn(&str, Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> + 'static {
    move |_webview_id: &str, req: Request<Vec<u8>>| {
        let host = req.uri().host().unwrap_or("new");
        let page_name = host.trim_start_matches('/').trim_end_matches('/');

        if let Some(body) = resolver(page_name, req.uri()) {
            Response::builder()
                .header("Content-Type", HeaderValue::from_static("text/html"))
                .body(Cow::Owned(body.into_bytes()))
                .expect("valid http response builder")
        } else {
            Response::builder()
                .header("Content-Type", HeaderValue::from_static("text/html"))
                .body(Cow::Owned(
                    aileron_404_page(&req.uri().to_string()).into_bytes(),
                ))
                .expect("valid http response builder")
        }
    }
}
