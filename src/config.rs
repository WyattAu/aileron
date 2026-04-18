//! User configuration loaded from ~/.config/aileron/config.toml.
//!
//! Config values can be overridden by environment variables.
//! If no config file exists, defaults are used.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// User configuration for Aileron.
///
/// All fields have sensible defaults. Missing fields in config.toml
/// fall back to the Default impl.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    /// Homepage URL loaded when Aileron starts.
    pub homepage: String,

    /// Default window width in logical pixels.
    pub window_width: u32,

    /// Default window height in logical pixels.
    pub window_height: u32,

    /// Enable web developer tools in webviews.
    pub devtools: bool,

    /// Enable ad-blocking (blocks domains from filter lists).
    pub adblock_enabled: bool,

    /// URLs of filter lists to load (EasyList format).
    pub adblock_filter_lists: Vec<String>,

    /// How often to check for filter list updates (hours).
    pub adblock_update_interval_hours: u64,

    /// Enable cosmetic CSS injection (element hiding).
    pub adblock_cosmetic_filtering: bool,

    /// Auto-restore the most recent workspace on startup.
    /// If true, Aileron loads the last-saved workspace instead of the homepage.
    pub restore_session: bool,

    /// Auto-save workspace periodically (for crash recovery).
    pub auto_save: bool,

    /// Auto-save interval in seconds.
    pub auto_save_interval: u64,

    /// Path to a custom init.lua script.
    /// If set, overrides the default XDG config path.
    pub init_lua_path: Option<String>,

    /// Shell command palette appearance.
    pub palette: PaletteConfig,

    /// Default search engine URL template. {query} is replaced with search terms.
    pub search_engine: String,

    /// Additional search engines (beyond the default).
    /// Key is the short name (used in :engine command), value is the URL template.
    #[serde(default)]
    pub search_engines: std::collections::HashMap<String, String>,

    /// Custom CSS to inject into every page (advanced users).
    pub custom_css: Option<String>,

    /// Tab bar layout: "sidebar", "topbar", or "none".
    pub tab_layout: String,

    /// Tab sidebar width in pixels (for sidebar layout).
    pub tab_sidebar_width: f32,

    /// Show tab sidebar on the left (true) or right (false).
    pub tab_sidebar_right: bool,

    /// HTTP/HTTPS/SOCKS5 proxy URL (e.g., "socks5://127.0.0.1:1080" or "http://proxy:8080").
    pub proxy: Option<String>,

    /// Enable automatic HTTPS upgrade for known-safe domains.
    pub https_upgrade_enabled: bool,

    /// Enable tracking protection (block known tracker domains, strip referrer, send DNT/GPC).
    pub tracking_protection_enabled: bool,

    /// Webview rendering mode.
    /// - "native": wry creates visible windows (XWayland on Wayland). Default for now.
    /// - "offscreen": Architecture B — webviews render offscreen, displayed as egui textures.
    pub render_mode: String,

    /// Config format version. Used for migrations.
    #[serde(default)]
    pub config_version: u32,

    /// Enable popup blocking (blocks window.open() except from user gestures).
    pub popup_blocker_enabled: bool,

    /// UI theme: "dark", "light", or a custom theme name.
    pub theme: String,

    /// Custom theme definitions. Key is theme name, value is color overrides.
    #[serde(default)]
    pub themes: std::collections::HashMap<String, ThemeColors>,
}

/// Color overrides for a custom theme.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ThemeColors {
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub fg: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub tab_bar_bg: Option<String>,
    #[serde(default)]
    pub tab_bar_fg: Option<String>,
    #[serde(default)]
    pub status_bar_bg: Option<String>,
    #[serde(default)]
    pub status_bar_fg: Option<String>,
    #[serde(default)]
    pub url_bar_bg: Option<String>,
    #[serde(default)]
    pub url_bar_fg: Option<String>,
    #[serde(default)]
    pub border: Option<String>,
}

impl ThemeColors {
    /// Resolve a color field, falling back to the provided default.
    pub fn resolve(field: &Option<String>, default: &str) -> egui::Color32 {
        field
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| parse_hex_color(default).unwrap_or(egui::Color32::WHITE))
    }
}

/// Parse a hex color string like "#1a1a2e" into egui::Color32.
fn parse_hex_color(s: &str) -> Option<egui::Color32> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

fn built_in_themes() -> std::collections::HashMap<String, ThemeColors> {
    let mut m = std::collections::HashMap::new();

    m.insert(
        "dark".into(),
        ThemeColors {
            bg: Some("#191920".into()),
            fg: Some("#e0e0e0".into()),
            accent: Some("#4db4ff".into()),
            tab_bar_bg: Some("#19191e".into()),
            tab_bar_fg: Some("#cccccc".into()),
            status_bar_bg: Some("#1a1a20".into()),
            status_bar_fg: Some("#cccccc".into()),
            url_bar_bg: Some("#1a1a20".into()),
            url_bar_fg: Some("#e0e0e0".into()),
            border: Some("#3c3c3c".into()),
        },
    );

    m.insert(
        "light".into(),
        ThemeColors {
            bg: Some("#ffffff".into()),
            fg: Some("#1a1a1a".into()),
            accent: Some("#0066cc".into()),
            tab_bar_bg: Some("#f0f0f0".into()),
            tab_bar_fg: Some("#333333".into()),
            status_bar_bg: Some("#e8e8e8".into()),
            status_bar_fg: Some("#333333".into()),
            url_bar_bg: Some("#ffffff".into()),
            url_bar_fg: Some("#1a1a1a".into()),
            border: Some("#cccccc".into()),
        },
    );

    m.insert(
        "gruvbox-dark".into(),
        ThemeColors {
            bg: Some("#282828".into()),
            fg: Some("#ebdbb2".into()),
            accent: Some("#fe8019".into()),
            tab_bar_bg: Some("#1d2021".into()),
            tab_bar_fg: Some("#ebdbb2".into()),
            status_bar_bg: Some("#1d2021".into()),
            status_bar_fg: Some("#ebdbb2".into()),
            url_bar_bg: Some("#282828".into()),
            url_bar_fg: Some("#ebdbb2".into()),
            border: Some("#504945".into()),
        },
    );

    m.insert(
        "nord".into(),
        ThemeColors {
            bg: Some("#2e3440".into()),
            fg: Some("#d8dee9".into()),
            accent: Some("#88c0d0".into()),
            tab_bar_bg: Some("#2e3440".into()),
            tab_bar_fg: Some("#d8dee9".into()),
            status_bar_bg: Some("#3b4252".into()),
            status_bar_fg: Some("#d8dee9".into()),
            url_bar_bg: Some("#2e3440".into()),
            url_bar_fg: Some("#d8dee9".into()),
            border: Some("#4c566a".into()),
        },
    );

    m.insert(
        "dracula".into(),
        ThemeColors {
            bg: Some("#282a36".into()),
            fg: Some("#f8f8f2".into()),
            accent: Some("#bd93f9".into()),
            tab_bar_bg: Some("#21222c".into()),
            tab_bar_fg: Some("#f8f8f2".into()),
            status_bar_bg: Some("#21222c".into()),
            status_bar_fg: Some("#f8f8f2".into()),
            url_bar_bg: Some("#282a36".into()),
            url_bar_fg: Some("#f8f8f2".into()),
            border: Some("#44475a".into()),
        },
    );

    m.insert(
        "solarized-dark".into(),
        ThemeColors {
            bg: Some("#002b36".into()),
            fg: Some("#839496".into()),
            accent: Some("#268bd2".into()),
            tab_bar_bg: Some("#073642".into()),
            tab_bar_fg: Some("#93a1a1".into()),
            status_bar_bg: Some("#073642".into()),
            status_bar_fg: Some("#93a1a1".into()),
            url_bar_bg: Some("#002b36".into()),
            url_bar_fg: Some("#839496".into()),
            border: Some("#586e75".into()),
        },
    );

    m.insert(
        "solarized-light".into(),
        ThemeColors {
            bg: Some("#fdf6e3".into()),
            fg: Some("#657b83".into()),
            accent: Some("#268bd2".into()),
            tab_bar_bg: Some("#eee8d5".into()),
            tab_bar_fg: Some("#657b83".into()),
            status_bar_bg: Some("#eee8d5".into()),
            status_bar_fg: Some("#657b83".into()),
            url_bar_bg: Some("#fdf6e3".into()),
            url_bar_fg: Some("#657b83".into()),
            border: Some("#93a1a1".into()),
        },
    );

    m
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PaletteConfig {
    /// Maximum number of results to show in the command palette.
    pub max_results: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            homepage: "aileron://welcome".into(),
            window_width: 1280,
            window_height: 800,
            devtools: false,
            adblock_enabled: true,
            adblock_filter_lists: vec![
                "https://easylist.to/easylist/easylist.txt".into(),
                "https://easylist.to/easylist/easyprivacy.txt".into(),
            ],
            adblock_update_interval_hours: 24,
            adblock_cosmetic_filtering: true,
            restore_session: false,
            auto_save: true,
            auto_save_interval: 30,
            init_lua_path: None,
            palette: PaletteConfig::default(),
            search_engine: "https://duckduckgo.com/?q={query}".into(),
            search_engines: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "google".into(),
                    "https://www.google.com/search?q={query}".into(),
                );
                m.insert("ddg".into(), "https://duckduckgo.com/?q={query}".into());
                m.insert("gh".into(), "https://github.com/search?q={query}".into());
                m.insert(
                    "yt".into(),
                    "https://www.youtube.com/results?search_query={query}".into(),
                );
                m.insert(
                    "wiki".into(),
                    "https://en.wikipedia.org/w/index.php?search={query}".into(),
                );
                m
            },
            custom_css: None,
            proxy: None,
            tab_layout: "sidebar".into(),
            tab_sidebar_width: 180.0,
            tab_sidebar_right: false,
            render_mode: "offscreen".into(),
            https_upgrade_enabled: true,
            tracking_protection_enabled: true,
            config_version: 2,
            popup_blocker_enabled: true,
            theme: "dark".into(),
            themes: built_in_themes(),
        }
    }
}

impl Default for PaletteConfig {
    fn default() -> Self {
        Self { max_results: 20 }
    }
}

impl Config {
    /// Load configuration from the XDG config directory.
    /// Returns defaults if no config file exists.
    pub fn load() -> Self {
        let config_path = Self::config_path();
        if config_path.exists() {
            match Self::load_from_file(&config_path) {
                Ok(mut config) => {
                    let migrated = Self::migrate(&mut config);
                    if migrated {
                        info!("Config migrated to version {}", config.config_version);
                        if let Err(e) = Self::save(&config) {
                            warn!("Failed to save migrated config: {}", e);
                        }
                    }
                    info!("Loaded config from {}", config_path.display());
                    return config;
                }
                Err(e) => {
                    warn!("Failed to load config: {} — using defaults", e);
                }
            }
        } else {
            info!(
                "No config found at {} — using defaults",
                config_path.display()
            );
        }
        Self::default()
    }

    /// Load configuration from a specific file path.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Get the path to the config file.
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Get the XDG config directory for Aileron.
    pub fn config_dir() -> PathBuf {
        crate::platform::paths::config_dir()
    }

    /// Get the path to the session-active flag file.
    pub fn session_active_path() -> PathBuf {
        Self::config_dir().join(".session_active")
    }

    /// Write the session-active flag file (called on startup).
    pub fn set_session_active() {
        let path = Self::session_active_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, std::process::id().to_string());
    }

    /// Remove the session-active flag file (called on clean shutdown).
    pub fn clear_session_active() {
        let _ = std::fs::remove_file(Self::session_active_path());
    }

    /// Check whether the previous session ended uncleanly (flag still exists).
    pub fn was_previous_session_unclean() -> bool {
        Self::session_active_path().exists()
    }

    /// Get the path to the init.lua file.
    /// Uses custom path from config if set, otherwise XDG default.
    pub fn init_lua_path(&self) -> PathBuf {
        if let Some(ref custom) = self.init_lua_path {
            PathBuf::from(custom)
        } else {
            crate::platform::paths::config_dir().join("init.lua")
        }
    }

    /// Migrate config from older versions to current.
    /// Returns true if any migration was applied.
    fn migrate(config: &mut Config) -> bool {
        let current_version = 2u32;
        if config.config_version >= current_version {
            return false;
        }

        let old_version = config.config_version;

        if config.config_version < 1 {
            config.config_version = 1;
        }

        if config.config_version < 2 {
            config.config_version = 2;
        }

        info!(
            "Migrated config from version {} to {}",
            old_version, config.config_version
        );
        true
    }

    /// Save config to the default config path.
    pub fn save(config: &Config) -> Result<()> {
        let config_path = Self::config_path();
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml = toml::to_string_pretty(config)
            .map_err(|e| anyhow::anyhow!("Failed to serialize config: {}", e))?;
        std::fs::write(&config_path, toml)?;
        Ok(())
    }

    /// Generate a sample config.toml file as a string.
    pub fn sample() -> String {
        r#"# Aileron Configuration
# Location: ~/.config/aileron/config.toml

# Homepage URL (loaded on startup)
homepage = "aileron://welcome"

# Window size (logical pixels)
window_width = 1280
window_height = 800

# Enable web developer tools (auto-enabled in debug builds)
devtools = false

# Enable ad-blocking
adblock_enabled = true

# Enable automatic HTTPS upgrade for known-safe domains
https_upgrade_enabled = true

# Enable tracking protection (blocks trackers, sends DNT/GPC, strict referrer)
tracking_protection_enabled = true

# Auto-restore the most recent workspace on startup
restore_session = false

# Auto-save workspace periodically for crash recovery
auto_save = true
auto_save_interval = 30

# Custom init.lua path (overrides default XDG location)
# init_lua_path = "/home/user/.config/aileron/init.lua"

# Default search engine URL template. {query} is replaced with search terms.
search_engine = "https://duckduckgo.com/?q={query}"

# Custom CSS to inject into every page (advanced users)
# custom_css = "body { background: #000 !important; }"

# Proxy URL (supports http, https, socks5)
# proxy = "socks5://127.0.0.1:1080"

# Tab bar layout: "sidebar", "topbar", or "none"
tab_layout = "sidebar"

# Tab sidebar width in pixels (for sidebar layout)
tab_sidebar_width = 180.0

# Show tab sidebar on the right instead of left
tab_sidebar_right = false

# Webview rendering mode: "offscreen" (Architecture B, default) or "native" (XWayland on Wayland)
render_mode = "offscreen"

[palette]
# Maximum search results in command palette
max_results = 20

# Additional search engines (short name = URL template)
# Use :engine <name> to switch between them
[search_engines]
google = "https://www.google.com/search?q={query}"
ddg = "https://duckduckgo.com/?q={query}"
gh = "https://github.com/search?q={query}"
yt = "https://www.youtube.com/results?search_query={query}"
wiki = "https://en.wikipedia.org/w/index.php?search={query}"
"#
        .to_string()
    }

    /// Build a search URL from a query string.
    pub fn search_url(&self, query: &str) -> Option<url::Url> {
        let encoded = query.replace(' ', "+");
        let url_str = self.search_engine.replace("{query}", &encoded);
        url::Url::parse(&url_str).ok()
    }

    /// Whether Architecture B offscreen rendering is enabled.
    pub fn is_offscreen(&self) -> bool {
        self.render_mode == "offscreen"
    }

    /// Get the resolved ThemeColors for the current theme.
    /// Falls back to built-in "dark" theme if the configured theme is not found.
    pub fn active_theme(&self) -> ThemeColors {
        if let Some(colors) = self.themes.get(&self.theme) {
            colors.clone()
        } else {
            built_in_themes().get("dark").cloned().unwrap_or_default()
        }
    }

    /// List available theme names (built-in + custom).
    pub fn available_themes(&self) -> Vec<String> {
        let mut names: Vec<String> = self.themes.keys().cloned().collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.homepage, "aileron://welcome");
        assert_eq!(config.window_width, 1280);
        assert_eq!(config.window_height, 800);
        assert!(!config.devtools);
        assert!(config.adblock_enabled);
        assert!(!config.restore_session);
        assert!(config.auto_save);
        assert_eq!(config.auto_save_interval, 30);
        assert!(config.init_lua_path.is_none());
        assert_eq!(config.palette.max_results, 20);
    }

    #[test]
    fn test_parse_empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.homepage, "aileron://welcome");
    }

    #[test]
    fn test_parse_partial_config() {
        let config: Config = toml::from_str(
            r#"
            homepage = "https://duckduckgo.com"
            window_width = 1920
            devtools = true
            "#,
        )
        .unwrap();
        assert_eq!(config.homepage, "https://duckduckgo.com");
        assert_eq!(config.window_width, 1920);
        assert_eq!(config.window_height, 800); // default
        assert!(config.devtools);
    }

    #[test]
    fn test_parse_with_palette_section() {
        let config: Config = toml::from_str(
            r#"
            [palette]
            max_results = 50
            "#,
        )
        .unwrap();
        assert_eq!(config.palette.max_results, 50);
    }

    #[test]
    fn test_sample_config_parses() {
        let sample = Config::sample();
        let config: Config = toml::from_str(&sample).unwrap();
        assert_eq!(config.homepage, "aileron://welcome");
        assert_eq!(config.palette.max_results, 20);
    }

    #[test]
    fn test_init_lua_path_override() {
        let config: Config = toml::from_str(r#"init_lua_path = "/tmp/my_init.lua""#).unwrap();
        assert_eq!(config.init_lua_path(), PathBuf::from("/tmp/my_init.lua"));
    }

    #[test]
    fn test_init_lua_path_default() {
        let config = Config::default();
        let path = config.init_lua_path();
        assert!(path.to_string_lossy().contains("init.lua"));
    }

    #[test]
    fn test_search_url() {
        let config = Config::default();
        let url = config.search_url("rust programming").unwrap();
        assert_eq!(url.as_str(), "https://duckduckgo.com/?q=rust+programming");
    }

    #[test]
    fn test_search_url_custom_engine() {
        let config: Config =
            toml::from_str(r#"search_engine = "https://www.google.com/search?q={query}""#).unwrap();
        let url = config.search_url("hello world").unwrap();
        assert_eq!(url.as_str(), "https://www.google.com/search?q=hello+world");
    }

    #[test]
    fn test_parse_tab_layout_config() {
        let config: Config = toml::from_str(
            r#"
            tab_layout = "topbar"
            tab_sidebar_width = 200.0
            tab_sidebar_right = true
            "#,
        )
        .unwrap();
        assert_eq!(config.tab_layout, "topbar");
        assert_eq!(config.tab_sidebar_width, 200.0);
        assert!(config.tab_sidebar_right);

        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.tab_layout, "sidebar");
        assert_eq!(config.tab_sidebar_width, 180.0);
        assert!(!config.tab_sidebar_right);
    }

    #[test]
    fn test_config_migration_old_version() {
        let mut config = Config::default();
        config.config_version = 0;
        let migrated = Config::migrate(&mut config);
        assert!(migrated);
        assert_eq!(config.config_version, 2);
    }

    #[test]
    fn test_config_migration_current_version() {
        let mut config = Config::default();
        config.config_version = 2;
        let migrated = Config::migrate(&mut config);
        assert!(!migrated);
    }

    #[test]
    fn test_proxy_config() {
        let config: Config = toml::from_str(r#"proxy = "socks5://127.0.0.1:1080""#).unwrap();
        assert_eq!(config.proxy, Some("socks5://127.0.0.1:1080".to_string()));
    }

    #[test]
    fn test_no_proxy_default() {
        let config = Config::default();
        assert!(config.proxy.is_none());
    }

    #[test]
    fn test_render_mode_default_offscreen() {
        let config = Config::default();
        assert_eq!(config.render_mode, "offscreen");
        assert!(config.is_offscreen());
    }

    #[test]
    fn test_render_mode_offscreen() {
        let config: Config = toml::from_str(r#"render_mode = "offscreen""#).unwrap();
        assert!(config.is_offscreen());
    }

    #[test]
    fn test_render_mode_invalid_treated_as_native() {
        let config: Config = toml::from_str(r#"render_mode = "webgl""#).unwrap();
        assert!(!config.is_offscreen());
    }
}
