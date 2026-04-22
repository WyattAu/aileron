//! Settings command implementations.
//! Free functions for `:set <key> <value>` and boolean parsing.

/// Parse a boolean-like value from a string.
/// Returns true unless the string contains "off", "false", or "0".
pub fn parse_bool_value(value: &str) -> bool {
    !value.contains("off")
        && !value.contains("false")
        && !value.contains("0")
}

/// Apply a `:set <key> <value>` command. Returns the status message.
/// Mutates the config in place.
pub fn apply_set_setting(config: &mut crate::config::Config, key: &str, value: &str) -> String {
    match key {
        "search_engine" if !value.is_empty() => {
            config.search_engine = value.to_string();
            format!("search_engine = {}", value)
        }
        "homepage" if !value.is_empty() => {
            config.homepage = value.to_string();
            format!("homepage = {}", value)
        }
        "adblock" => {
            config.adblock_enabled = parse_bool_value(value);
            format!("adblock = {}", config.adblock_enabled)
        }
        "https_upgrade" | "https-upgrade" => {
            config.https_upgrade_enabled = parse_bool_value(value);
            format!("https_upgrade = {}", config.https_upgrade_enabled)
        }
        "tracking_protection" | "tracking-protection" => {
            config.tracking_protection_enabled = parse_bool_value(value);
            format!("tracking_protection = {}", config.tracking_protection_enabled)
        }
        "popup_blocker" | "popup-blocker" | "popups" => {
            config.popup_blocker_enabled = parse_bool_value(value);
            format!("popup_blocker = {}", config.popup_blocker_enabled)
        }
        "devtools" => {
            config.devtools = parse_bool_value(value);
            format!("devtools = {}", config.devtools)
        }
        "tab_layout" => {
            let valid = ["sidebar", "topbar", "none"];
            if valid.contains(&value) {
                config.tab_layout = value.to_string();
                format!("tab_layout = {}", value)
            } else {
                format!("Invalid tab_layout '{}' (try: sidebar, topbar, none)", value)
            }
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
        "auto_save" => {
            config.auto_save = parse_bool_value(value);
            format!("auto_save = {}", config.auto_save)
        }
        "theme" if !value.is_empty() => {
            config.theme = value.to_string();
            format!("theme = {}", value)
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
            format!(
                "Unknown setting: {} (try: search_engine, homepage, adblock, https_upgrade, tracking_protection, popup_blocker, devtools, tab_layout, sidebar_width, sidebar_right, cosmetic_filtering, auto_save, theme, adaptive_quality, sync_encrypted, sync_auto)",
                key
            )
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
        assert!(parse_bool_value("yes"));
        assert!(parse_bool_value(""));
    }

    #[test]
    fn parse_bool_off_values() {
        assert!(!parse_bool_value("off"));
        assert!(!parse_bool_value("false"));
        assert!(!parse_bool_value("0"));
        assert!(!parse_bool_value("turnoff"));
        assert!(!parse_bool_value("enablefalse"));
    }

    #[test]
    fn set_search_engine() {
        let mut config = crate::config::Config::default();
        let msg = apply_set_setting(&mut config, "search_engine", "https://google.com?q={query}");
        assert_eq!(msg, "search_engine = https://google.com?q={query}");
        assert_eq!(config.search_engine, "https://google.com?q={query}");
    }

    #[test]
    fn set_adblock_toggle() {
        let mut config = crate::config::Config::default();
        let msg = apply_set_setting(&mut config, "adblock", "off");
        assert_eq!(msg, "adblock = false");
        assert!(!config.adblock_enabled);

        let msg = apply_set_setting(&mut config, "adblock", "on");
        assert_eq!(msg, "adblock = true");
        assert!(config.adblock_enabled);
    }

    #[test]
    fn set_unknown_key() {
        let mut config = crate::config::Config::default();
        let msg = apply_set_setting(&mut config, "nonexistent", "value");
        assert!(msg.contains("Unknown setting"));
    }

    #[test]
    fn set_tab_layout_valid() {
        let mut config = crate::config::Config::default();
        let msg = apply_set_setting(&mut config, "tab_layout", "topbar");
        assert_eq!(msg, "tab_layout = topbar");
        assert_eq!(config.tab_layout, "topbar");
    }

    #[test]
    fn set_tab_layout_invalid() {
        let mut config = crate::config::Config::default();
        let msg = apply_set_setting(&mut config, "tab_layout", "invalid");
        assert!(msg.contains("Invalid tab_layout"));
    }

    #[test]
    fn set_sidebar_width_valid() {
        let mut config = crate::config::Config::default();
        let msg = apply_set_setting(&mut config, "sidebar_width", "300");
        assert_eq!(msg, "sidebar_width = 300");
        assert_eq!(config.tab_sidebar_width, 300.0);
    }

    #[test]
    fn set_sidebar_width_out_of_range() {
        let mut config = crate::config::Config::default();
        let msg = apply_set_setting(&mut config, "sidebar_width", "50");
        assert!(msg.contains("must be between"));
    }

    #[test]
    fn set_homepage_empty_ignored() {
        let mut config = crate::config::Config::default();
        let original = config.homepage.clone();
        let msg = apply_set_setting(&mut config, "homepage", "");
        assert!(msg.contains("Unknown setting"));
        assert_eq!(config.homepage, original);
    }
}
