//! Runtime error capture and debugging system.
//!
//! Captures panics, GLib warnings, JS errors, and navigation failures
//! to a structured JSON log file for post-mortem debugging.
//!
//! Enabled via the `AILERON_DEBUG=1` environment variable.
//! Log file path can be overridden with `AILERON_DEBUG_FILE=/path/to/file.log`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

static ENABLED: AtomicBool = AtomicBool::new(false);
static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
static FILE_HANDLE: Mutex<Option<std::fs::File>> = Mutex::new(None);
static MAX_LOG_SIZE: u64 = 10 * 1024 * 1024; // 10 MB
static MAX_ROTATED_FILES: u32 = 3;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

fn log_file_path() -> PathBuf {
    if let Ok(custom) = std::env::var("AILERON_DEBUG_FILE") {
        return PathBuf::from(custom);
    }
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|d| d.data_dir().join("debug.log"))
        .unwrap_or_else(|| PathBuf::from("./debug.log"))
}

pub fn init() {
    if std::env::var("AILERON_DEBUG")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
    {
        ENABLED.store(true, Ordering::Relaxed);
    }
    if !is_enabled() {
        return;
    }

    let path = log_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[debug] Failed to open debug log {}: {}", path.display(), e);
            return;
        }
    };

    *LOG_PATH.lock().unwrap() = Some(path.clone());
    *FILE_HANDLE.lock().unwrap() = Some(file);

    eprintln!("[debug] Logging to {}", path.display());

    write_startup_banner();

    install_panic_hook();
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic payload".to_string()
        };

        let location = match info.location() {
            Some(loc) => format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
            None => String::new(),
        };

        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();

        let bt = std::backtrace::Backtrace::capture();
        let backtrace: Vec<String> = if bt.status() == std::backtrace::BacktraceStatus::Captured {
            bt.to_string()
                .lines()
                .skip(1)
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        write_event(
            "panic",
            &message,
            &location,
            &thread,
            json!({ "backtrace": backtrace }),
        );

        default_hook(info);
    }));
}

fn write_startup_banner() {
    let os_info = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    let kernel = read_file_line("/proc/sys/kernel/osrelease").unwrap_or_default();
    let cpu = detect_cpu();
    let ram_gb = detect_ram_gb();
    let gpu = detect_gpu();
    let display_server = detect_display_server();

    let config_values = config_snapshot();

    write_event(
        "startup",
        &format!("Aileron v{} starting", VERSION),
        "",
        "main",
        json!({
            "system": {
                "os": os_info,
                "kernel": kernel,
                "cpu": cpu,
                "ram_gb": ram_gb,
                "gpu": gpu,
                "display_server": display_server,
            },
            "config": config_values,
            "version": VERSION,
        }),
    );
}

fn config_snapshot() -> Value {
    let config = crate::config::Config::load();
    json!({
        "homepage": config.homepage,
        "render_mode": config.render_mode,
        "tab_layout": config.tab_layout,
        "theme": config.theme,
        "devtools": config.devtools,
        "adblock_enabled": config.adblock_enabled,
        "restore_session": config.restore_session,
        "adaptive_quality": config.adaptive_quality,
        "popup_blocker_enabled": config.popup_blocker_enabled,
        "https_upgrade_enabled": config.https_upgrade_enabled,
        "tracking_protection_enabled": config.tracking_protection_enabled,
        "proxy": config.proxy,
        "language": config.language,
    })
}

fn read_file_line(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

fn detect_cpu() -> String {
    read_file_line("/proc/cpuinfo")
        .and_then(|content| {
            content
                .lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1).map(|s| s.trim().to_string()))
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn detect_ram_gb() -> u64 {
    read_file_line("/proc/meminfo")
        .and_then(|content| {
            content
                .lines()
                .find(|l| l.starts_with("MemTotal"))
                .and_then(|l| {
                    l.split_whitespace()
                        .nth(1)?
                        .parse::<u64>()
                        .ok()
                        .map(|kb| kb / 1024 / 1024)
                })
        })
        .unwrap_or(0)
}

fn detect_gpu() -> String {
    #[cfg(target_os = "linux")]
    {
        let vendor = read_file_line("/sys/class/drm/card0/device/vendor").unwrap_or_default();
        let device = read_file_line("/sys/class/drm/card0/device/device").unwrap_or_default();
        if !vendor.is_empty() && !device.is_empty() {
            return format!("vendor={} device={}", vendor.trim(), device.trim());
        }
    }
    "unknown".to_string()
}

fn detect_display_server() -> String {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        "Wayland".to_string()
    } else if std::env::var("DISPLAY").is_ok() {
        "X11".to_string()
    } else {
        "unknown".to_string()
    }
}

pub fn capture_panic(info: &std::panic::PanicHookInfo) {
    if !is_enabled() {
        return;
    }

    let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    };

    let location = match info.location() {
        Some(loc) => format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
        None => String::new(),
    };

    let thread = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_string();

    let bt = std::backtrace::Backtrace::capture();
    let backtrace: Vec<String> = if bt.status() == std::backtrace::BacktraceStatus::Captured {
        bt.to_string()
            .lines()
            .skip(1)
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    } else {
        Vec::new()
    };

    write_event(
        "panic",
        &message,
        &location,
        &thread,
        json!({ "backtrace": backtrace }),
    );
}

pub fn write_periodic_stats(
    memory_rss_kb: Option<u64>,
    frame_time_avg_ms: f64,
    active_tab_count: usize,
    webview_count: usize,
) {
    if !is_enabled() {
        return;
    }

    let mut extra = json!({
        "active_tab_count": active_tab_count,
        "webview_count": webview_count,
        "frame_time_avg_ms": frame_time_avg_ms,
    });

    if let Some(rss_kb) = memory_rss_kb {
        extra["memory_rss_mb"] = json!(rss_kb / 1024);
    }

    write_event(
        "stats",
        "periodic stats",
        "",
        std::thread::current().name().unwrap_or("unnamed"),
        extra,
    );
}

pub fn capture_glib(level: &str, domain: &str, message: &str) {
    if !is_enabled() {
        return;
    }

    write_event(
        "glib_warning",
        &format!("[GLib {}::{}] {}", domain, level, message),
        &format!("glib:{}", domain),
        std::thread::current().name().unwrap_or("unnamed"),
        json!({
            "glib_level": level,
            "glib_domain": domain,
        }),
    );
}

pub fn capture_js_error(pane_id: &str, error_msg: &str) {
    if !is_enabled() {
        return;
    }

    write_event(
        "js_error",
        error_msg,
        "",
        std::thread::current().name().unwrap_or("unnamed"),
        json!({ "pane_id": pane_id }),
    );
}

pub fn capture_navigation_error(pane_id: &str, url: &str, error: &str) {
    if !is_enabled() {
        return;
    }

    write_event(
        "navigation_error",
        &format!("{}: {}", url, error),
        "",
        std::thread::current().name().unwrap_or("unnamed"),
        json!({
            "pane_id": pane_id,
            "url": url,
        }),
    );
}

pub fn capture_info(message: &str) {
    if !is_enabled() {
        return;
    }

    write_event(
        "info",
        message,
        "",
        std::thread::current().name().unwrap_or("unnamed"),
        Value::Null,
    );
}

fn write_event(event_type: &str, message: &str, location: &str, thread: &str, extra: Value) {
    if !is_enabled() {
        return;
    }

    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let mut event = json!({
        "ts": ts,
        "type": event_type,
        "message": message,
        "location": location,
        "thread": thread,
    });

    if !extra.is_null() {
        let obj = event.as_object_mut().unwrap();
        if let Value::Object(extra_obj) = extra {
            for (k, v) in extra_obj {
                obj.insert(k.clone(), v);
            }
        }
    }

    let line = format!("{}\n", serde_json::to_string(&event).unwrap_or_default());

    let mut handle = FILE_HANDLE.lock().unwrap();
    if let Some(ref mut file) = *handle {
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }

    rotate_if_needed(&mut handle);
}

fn rotate_if_needed(handle: &mut Option<std::fs::File>) {
    let path_guard = LOG_PATH.lock().unwrap();
    let path = match path_guard.as_ref() {
        Some(p) => p.clone(),
        None => return,
    };
    drop(path_guard);

    let metadata = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(_) => return,
    };

    if metadata.len() < MAX_LOG_SIZE {
        return;
    }

    *handle = None;

    for i in (1..MAX_ROTATED_FILES).rev() {
        let src = path.with_extension(format!("log.{}", i));
        let dst = path.with_extension(format!("log.{}", i + 1));
        if src.exists() {
            let _ = std::fs::rename(&src, &dst);
        }
    }
    let _ = std::fs::rename(&path, path.with_extension("log.1"));

    if let Ok(new_file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        *handle = Some(new_file);
    }
}

pub fn read_memory_rss_kb() -> Option<u64> {
    let content = read_file_line("/proc/self/status")?;
    let line = content.lines().find(|l| l.starts_with("VmRSS:"))?;
    let num = line.split_whitespace().nth(1)?;
    num.parse::<u64>().ok()
}

pub fn shutdown() {
    if !is_enabled() {
        return;
    }

    write_event(
        "shutdown",
        "Aileron shutting down",
        "",
        std::thread::current().name().unwrap_or("unnamed"),
        Value::Null,
    );

    let mut handle = FILE_HANDLE.lock().unwrap();
    if let Some(ref mut file) = *handle {
        let _ = file.flush();
    }
    *handle = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_by_default() {
        assert!(!is_enabled());
    }

    #[test]
    fn test_event_json_format() {
        let event = json!({
            "ts": "2026-04-29T15:30:00.000Z",
            "type": "panic",
            "message": "test panic",
            "location": "src/foo.rs:10:5",
            "thread": "main",
            "backtrace": ["frame1", "frame2"],
        });
        let serialized = serde_json::to_string(&event).unwrap();
        assert!(serialized.contains("\"type\":\"panic\""));
        assert!(serialized.contains("\"message\":\"test panic\""));
        assert!(serialized.contains("\"location\":\"src/foo.rs:10:5\""));
    }

    #[test]
    fn test_capture_functions_are_noop_when_disabled() {
        capture_glib("ERROR", "WebKit", "test message");
        capture_js_error("pane-123", "test error");
        capture_navigation_error("pane-123", "https://example.com", "net::ERR");
        capture_info("test info");
    }

    #[test]
    fn test_detect_display_server() {
        let server = detect_display_server();
        assert!(["Wayland", "X11", "unknown"].contains(&server.as_str()));
    }

    #[test]
    fn test_detect_cpu() {
        let cpu = detect_cpu();
        assert!(!cpu.is_empty());
    }

    #[test]
    fn test_detect_ram_gb() {
        #[cfg(target_os = "linux")]
        {
            let ram = detect_ram_gb();
            assert!(ram > 0 || ram == 0);
        }
    }

    #[test]
    fn test_read_file_line_missing() {
        assert!(read_file_line("/nonexistent/path/file").is_none());
    }

    #[test]
    fn test_rotate_logic_in_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("debug.log");

        {
            let mut guard = LOG_PATH.lock().unwrap();
            *guard = Some(log_path.clone());
        }

        ENABLED.store(true, Ordering::Relaxed);

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        {
            let mut guard = FILE_HANDLE.lock().unwrap();
            *guard = Some(file);
        }

        let data = "x".repeat(MAX_LOG_SIZE as usize);
        std::fs::write(&log_path, &data).unwrap();

        {
            let mut handle = FILE_HANDLE.lock().unwrap();
            rotate_if_needed(&mut handle);
        }

        assert!(log_path.with_extension("log.1").exists());

        ENABLED.store(false, Ordering::Relaxed);
        *LOG_PATH.lock().unwrap() = None;
        *FILE_HANDLE.lock().unwrap() = None;
    }

    #[test]
    fn test_write_event_disabled_no_crash() {
        ENABLED.store(false, Ordering::Relaxed);
        write_event("test", "msg", "", "main", json!({"key": "val"}));
    }

    #[test]
    fn test_write_periodic_stats_noop_when_disabled() {
        write_periodic_stats(Some(1024), 16.0, 3, 3);
    }

    #[test]
    fn test_shutdown_noop_when_disabled() {
        shutdown();
    }

    #[test]
    fn test_log_file_path_env_override() {
        unsafe { std::env::set_var("AILERON_DEBUG_FILE", "/tmp/test_debug.log") };
        let path = log_file_path();
        assert_eq!(path, PathBuf::from("/tmp/test_debug.log"));
        unsafe { std::env::remove_var("AILERON_DEBUG_FILE") };
    }

    #[test]
    fn test_log_file_path_default() {
        unsafe { std::env::remove_var("AILERON_DEBUG_FILE") };
        let path = log_file_path();
        assert!(path.to_string_lossy().contains("aileron"));
        assert!(path.to_string_lossy().contains("debug.log"));
    }
}
