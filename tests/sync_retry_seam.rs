//! Retry-policy seam tests for the WebDAV sync transport.
//!
//! The transport delegates backoff arithmetic to the estate crate
//! `loop-retry` (`RetryConfig` + `delay_for_attempt`); these tests pin the
//! contract at the WebDAV seam: retry-then-succeed on transient 5xx, and no
//! retry on permanent 4xx errors.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use aileron::sync::webdav::{WebdavAuth, WebdavClient, WebdavConfig};

/// Blocking HTTP server answering each request with the next status from
/// `statuses` (listed in serving order; the last one repeats when the list is
/// exhausted). Tracks total requests served so tests can assert exact retry
/// counts.
fn mock_server(statuses: Vec<u16>) -> (String, Arc<AtomicU32>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let served = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&served);
    let queue = Mutex::new(statuses);

    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let status = {
                let mut q = queue.lock().unwrap();
                if q.len() > 1 { q.remove(0) } else { q[0] }
            };
            counter.fetch_add(1, Ordering::SeqCst);

            // Drain the request head (bodies here are tiny or absent).
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);

            let body = "<multistatus/>";
            let reason = if status == 207 || status == 200 {
                "OK"
            } else {
                "Err"
            };
            let resp = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/xml\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://{addr}"), served)
}

fn config(base_url: String) -> WebdavConfig {
    WebdavConfig {
        base_url,
        auth: WebdavAuth::None,
        max_retries: 2,
        // Tiny delays so retry tests stay fast.
        retry_delay_ms: 1,
        max_retry_delay_ms: 5,
    }
}

#[test]
fn retries_transient_server_error_then_succeeds() {
    // GET: 500 first, 200 on retry.
    let (url, served) = mock_server(vec![500, 200]);
    let client = WebdavClient::new(config(url));

    let body = client.get("file.txt");
    assert!(
        body.as_ref().is_ok_and(Option::is_some),
        "expected retry to recover from 500, got {body:?}"
    );
    assert_eq!(
        served.load(Ordering::SeqCst),
        2,
        "expected exactly one retry"
    );
}

#[test]
fn permanent_client_error_is_not_retried() {
    // GET: 403 must fail fast without burning retries.
    let (url, served) = mock_server(vec![403]);
    let client = WebdavClient::new(config(url));

    let body = client.get("file.txt");
    assert!(body.is_err(), "403 must surface as an error, got {body:?}");
    assert_eq!(
        served.load(Ordering::SeqCst),
        1,
        "permanent 4xx must not be retried"
    );
}

#[test]
fn missing_file_is_none_without_retry() {
    // GET: 404 maps to Ok(None) (the transport treats NOT_FOUND as terminal).
    let (url, served) = mock_server(vec![404]);
    let client = WebdavClient::new(config(url));

    let body = client.get("absent.txt");
    assert_eq!(body.unwrap(), None, "404 must map to Ok(None)");
    assert_eq!(served.load(Ordering::SeqCst), 1);
}
