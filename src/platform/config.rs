//! Platform-specific configuration defaults.

use crate::config::Config;

/// Get platform-specific default config values.
pub fn platform_defaults() -> Config {
    let config = Config::default();

    #[cfg(target_os = "macos")]
    let config = {
        let mut c = config;
        c.tab_sidebar_right = true;
        c
    };

    #[cfg(target_os = "windows")]
    let config = {
        let mut c = config;
        c.render_mode = "native".into();
        c
    };

    config
}

/// Get platform-specific wry configuration hints.
pub fn wry_hints() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "webkitgtk"
    }
    #[cfg(target_os = "macos")]
    {
        "wkwebview"
    }
    #[cfg(target_os = "windows")]
    {
        "webview2"
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_defaults_returns_config() {
        let config = platform_defaults();
        assert!(!config.homepage.is_empty());
    }

    #[test]
    fn test_wry_hints_is_known() {
        let hint = wry_hints();
        assert_ne!(hint, "unknown");
    }
}
