//! Settings command implementations.
//! Free functions for `:set <key> [value]`.

use crate::config::Config;

const VALID_TAB_LAYOUTS: &[&str] = &["sidebar", "topbar", "bottom", "sidebar-right", "none"];
const VALID_RENDER_MODES: &[&str] = &["native", "offscreen"];
const SETTABLE_KEYS: &[&str] = &[
    "theme",
    "homepage",
    "search_engine",
    "tab_layout",
    "adblock",
    "popup_blocker",
    "tracking_protection",
    "auto_save",
    "render_mode",
    "language",
    "https_upgrade",
    "devtools",
    "sidebar_width",
    "sidebar_right",
    "cosmetic_filtering",
    "adaptive_quality",
    "sync_encrypted",
    "sync_auto",
];

/// Parse a boolean-like value from a string.
/// Returns true unless the string is exactly "false", "0", or "off".
pub fn parse_bool_value(value: &str) -> bool {
    let v = value.trim().to_lowercase();
    v != "false" && v != "0" && v != "off"
}

/// Get the current value of a settable config key.
fn get_current_value(config: &Config, key: &str) -> Option<String> {
    match key {
        "theme" => Some(config.theme.clone()),
        "homepage" => Some(config.homepage.clone()),
        "search_engine" => Some(config.search_engine.clone()),
        "tab_layout" => Some(config.tab_layout.clone()),
        "adblock" => Some(config.adblock_enabled.to_string()),
        "popup_blocker" => Some(config.popup_blocker_enabled.to_string()),
        "tracking_protection" => Some(config.tracking_protection_enabled.to_string()),
        "auto_save" => Some(config.auto_save.to_string()),
        "render_mode" => Some(config.render_mode.clone()),
        "language" => Some(config.language.clone().unwrap_or_else(|| "auto".into())),
        "https_upgrade" | "https-upgrade" => Some(config.https_upgrade_enabled.to_string()),
        "devtools" => Some(config.devtools.to_string()),
        "sidebar_width" => Some(config.tab_sidebar_width.to_string()),
        "sidebar_right" => Some(config.tab_sidebar_right.to_string()),
        "cosmetic_filtering" => Some(config.adblock_cosmetic_filtering.to_string()),
        "adaptive_quality" => Some(config.adaptive_quality.to_string()),
        "sync_encrypted" => Some(config.sync_encrypted.to_string()),
        "sync_auto" => Some(config.sync_auto.to_string()),
        _ => None,
    }
}

/// Apply a `:set <key> [value]` command. Returns the status message.
/// Mutates the config in place and saves to disk on successful changes.
pub fn apply_set_setting(config: &mut Config, key: &str, value: &str) -> String {
    if value.is_empty() {
        match get_current_value(config, key) {
            Some(v) => return format!("{} = {}", key, v),
            None => {
                let keys = SETTABLE_KEYS.join(", ");
                return format!("Unknown setting: {} (try: {})", key, keys);
            }
        }
    }

    let msg = apply_set_value(config, key, value);
    let is_error =
        msg.contains("Unknown setting") || msg.contains("Invalid") || msg.contains("must be");

    if !is_error && let Err(e) = Config::save(config) {
        tracing::warn!("Failed to save config after :set {}: {}", key, e);
    }

    msg
}

fn apply_set_value(config: &mut Config, key: &str, value: &str) -> String {
    match key {
        "theme" => {
            config.theme = value.to_string();
            format!("theme = {}", value)
        }
        "homepage" => {
            config.homepage = value.to_string();
            format!("homepage = {}", value)
        }
        "search_engine" => {
            config.search_engine = value.to_string();
            format!("search_engine = {}", value)
        }
        "tab_layout" => {
            if VALID_TAB_LAYOUTS.contains(&value) {
                config.tab_layout = value.to_string();
                format!("tab_layout = {}", value)
            } else {
                format!(
                    "Invalid tab_layout '{}' (try: {})",
                    value,
                    VALID_TAB_LAYOUTS.join(", ")
                )
            }
        }
        "adblock" => {
            config.adblock_enabled = parse_bool_value(value);
            format!("adblock = {}", config.adblock_enabled)
        }
        "popup_blocker" => {
            config.popup_blocker_enabled = parse_bool_value(value);
            format!("popup_blocker = {}", config.popup_blocker_enabled)
        }
        "tracking_protection" => {
            config.tracking_protection_enabled = parse_bool_value(value);
            format!(
                "tracking_protection = {}",
                config.tracking_protection_enabled
            )
        }
        "auto_save" => {
            config.auto_save = parse_bool_value(value);
            format!("auto_save = {}", config.auto_save)
        }
        "render_mode" => {
            if VALID_RENDER_MODES.contains(&value) {
                config.render_mode = value.to_string();
                format!("render_mode = {} (restart required)", value)
            } else {
                format!(
                    "Invalid render_mode '{}' (try: {})",
                    value,
                    VALID_RENDER_MODES.join(", ")
                )
            }
        }
        "language" => {
            if value == "auto" {
                config.language = None;
                "language = auto".into()
            } else {
                config.language = Some(value.to_string());
                format!("language = {}", value)
            }
        }
        "https_upgrade" | "https-upgrade" => {
            config.https_upgrade_enabled = parse_bool_value(value);
            format!("https_upgrade = {}", config.https_upgrade_enabled)
        }
        "devtools" => {
            config.devtools = parse_bool_value(value);
            format!("devtools = {}", config.devtools)
        }
        "sidebar_width" => {
            if let Ok(w) = value.parse::<f32>() {
                if (100.0..=600.0).contains(&w) {
                    config.tab_sidebar_width = w;
                    format!("sidebar_width = {}", w)
                } else {
                    "sidebar_width must be between 100 and 600".into()
                }
            } else {
                "Invalid number for sidebar_width".into()
            }
        }
        "sidebar_right" => {
            config.tab_sidebar_right = parse_bool_value(value);
            format!("sidebar_right = {}", config.tab_sidebar_right)
        }
        "cosmetic_filtering" => {
            config.adblock_cosmetic_filtering = parse_bool_value(value);
            format!("cosmetic_filtering = {}", config.adblock_cosmetic_filtering)
        }
        "adaptive_quality" => {
            config.adaptive_quality = parse_bool_value(value);
            format!("adaptive_quality = {}", config.adaptive_quality)
        }
        "sync_encrypted" => {
            config.sync_encrypted = parse_bool_value(value);
            format!("sync_encrypted = {}", config.sync_encrypted)
        }
        "sync_auto" => {
            config.sync_auto = parse_bool_value(value);
            format!("sync_auto = {}", config.sync_auto)
        }
        _ => {
            let keys = SETTABLE_KEYS.join(", ");
            format!("Unknown setting: {} (try: {})", key, keys)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bool_on_values() {
        assert!(parse_bool_value("on"));
        assert!(parse_bool_value("true"));
        assert!(parse_bool_value("1"));
        assert!(parse_bool_value("ON"));
        assert!(parse_bool_value("True"));
    }

    #[test]
    fn parse_bool_off_values() {
        assert!(!parse_bool_value("off"));
        assert!(!parse_bool_value("false"));
        assert!(!parse_bool_value("0"));
        assert!(!parse_bool_value("OFF"));
        assert!(!parse_bool_value("False"));
    }

    #[test]
    fn set_search_engine() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "search_engine", "https://google.com?q={query}");
        assert_eq!(msg, "search_engine = https://google.com?q={query}");
        assert_eq!(config.search_engine, "https://google.com?q={query}");
    }

    #[test]
    fn set_adblock_toggle() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "adblock", "off");
        assert_eq!(msg, "adblock = false");
        assert!(!config.adblock_enabled);

        let msg = apply_set_setting(&mut config, "adblock", "on");
        assert_eq!(msg, "adblock = true");
        assert!(config.adblock_enabled);
    }

    #[test]
    fn set_unknown_key() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "nonexistent", "value");
        assert!(msg.contains("Unknown setting"));
    }

    #[test]
    fn set_tab_layout_valid() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "tab_layout", "topbar");
        assert_eq!(msg, "tab_layout = topbar");
        assert_eq!(config.tab_layout, "topbar");
    }

    #[test]
    fn set_tab_layout_bottom() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "tab_layout", "bottom");
        assert_eq!(msg, "tab_layout = bottom");
        assert_eq!(config.tab_layout, "bottom");
    }

    #[test]
    fn set_tab_layout_sidebar_right() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "tab_layout", "sidebar-right");
        assert_eq!(msg, "tab_layout = sidebar-right");
        assert_eq!(config.tab_layout, "sidebar-right");
    }

    #[test]
    fn set_tab_layout_invalid() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "tab_layout", "invalid");
        assert!(msg.contains("Invalid tab_layout"));
    }

    #[test]
    fn set_sidebar_width_valid() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "sidebar_width", "300");
        assert_eq!(msg, "sidebar_width = 300");
        assert_eq!(config.tab_sidebar_width, 300.0);
    }

    #[test]
    fn set_sidebar_width_out_of_range() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "sidebar_width", "50");
        assert!(msg.contains("must be between"));
    }

    #[test]
    fn get_value_no_value_provided() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "theme", "");
        assert_eq!(msg, "theme = dark");
        assert_eq!(config.theme, "dark");
    }

    #[test]
    fn get_value_bool_no_value() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "adblock", "");
        assert_eq!(msg, "adblock = true");
    }

    #[test]
    fn get_value_unknown_key() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "nonexistent", "");
        assert!(msg.contains("Unknown setting"));
    }

    #[test]
    fn set_render_mode_valid() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "render_mode", "native");
        assert_eq!(msg, "render_mode = native (restart required)");
        assert_eq!(config.render_mode, "native");
    }

    #[test]
    fn set_render_mode_invalid() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "render_mode", "webgl");
        assert!(msg.contains("Invalid render_mode"));
    }

    #[test]
    fn set_language_auto() {
        let mut config = Config {
            language: Some("en".into()),
            ..Default::default()
        };
        let msg = apply_set_setting(&mut config, "language", "auto");
        assert_eq!(msg, "language = auto");
        assert!(config.language.is_none());
    }

    #[test]
    fn set_language_specific() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "language", "ja");
        assert_eq!(msg, "language = ja");
        assert_eq!(config.language.as_deref(), Some("ja"));
    }

    #[test]
    fn set_popup_blocker_false() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "popup_blocker", "false");
        assert_eq!(msg, "popup_blocker = false");
        assert!(!config.popup_blocker_enabled);
    }

    #[test]
    fn set_tracking_protection_0() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "tracking_protection", "0");
        assert_eq!(msg, "tracking_protection = false");
        assert!(!config.tracking_protection_enabled);
    }

    #[test]
    fn set_homepage() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "homepage", "aileron://new");
        assert_eq!(msg, "homepage = aileron://new");
        assert_eq!(config.homepage, "aileron://new");
    }

    #[test]
    fn set_theme() {
        let mut config = Config::default();
        let msg = apply_set_setting(&mut config, "theme", "light");
        assert_eq!(msg, "theme = light");
        assert_eq!(config.theme, "light");
    }
}
