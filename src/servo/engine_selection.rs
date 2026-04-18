//! Engine selection logic for choosing between WebKit and Servo.

use crate::servo::engine::EngineType;

/// Engine selection configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineSelection {
    /// Automatically choose the best engine per page.
    Auto,
    /// Always use Servo (with WebKit fallback on failure).
    Servo,
    /// Always use WebKit (current default, most compatible).
    #[default]
    WebKit,
}

impl std::fmt::Display for EngineSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Servo => write!(f, "servo"),
            Self::WebKit => write!(f, "webkit"),
        }
    }
}

impl std::str::FromStr for EngineSelection {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "servo" => Ok(Self::Servo),
            "webkit" => Ok(Self::WebKit),
            _ => Err(format!(
                "Unknown engine selection: '{}'. Use: auto, servo, webkit",
                s
            )),
        }
    }
}

/// URL patterns that should use WebKit (not Servo) due to known incompatibilities.
/// This list will grow as Servo compatibility is tested.
static WEBKIT_OVERRIDE_DOMAINS: &[&str] = &[
    "docs.google.com",
    "sheets.google.com",
    "slides.google.com",
    "meet.google.com",
    "web.whatsapp.com",
    "web.telegram.org",
    "twitter.com",
    "x.com",
];

/// URL patterns that should prefer Servo (known to work well).
static SERVO_PREFER_DOMAINS: &[&str] = &[
    "developer.mozilla.org",
    "rust-lang.org",
    "github.com",
    "stackoverflow.com",
];

/// Decide which engine to use for a given URL.
///
/// Custom overrides (from `compat_overrides` config) take highest priority,
/// followed by the built-in WebKit override list, then the Servo prefer list.
/// Defaults to WebKit for maximum compatibility.
pub fn select_engine(
    url: &url::Url,
    selection: &EngineSelection,
    custom_overrides: &std::collections::HashMap<String, String>,
) -> EngineType {
    match selection {
        EngineSelection::WebKit => EngineType::WebKit,
        EngineSelection::Servo => EngineType::Servo,
        EngineSelection::Auto => {
            let host = url.host_str().unwrap_or("");

            // 1. Custom overrides (highest priority)
            if let Some(engine) = custom_overrides.get(host) {
                return match engine.as_str() {
                    "servo" => EngineType::Servo,
                    _ => EngineType::WebKit,
                };
            }

            // 2. Check WebKit override list (safety first)
            if WEBKIT_OVERRIDE_DOMAINS.iter().any(|d| host.ends_with(d)) {
                return EngineType::WebKit;
            }

            // 3. Check Servo prefer list
            if SERVO_PREFER_DOMAINS.iter().any(|d| host.ends_with(d)) {
                return EngineType::Servo;
            }

            // 4. Default to WebKit for maximum compatibility
            EngineType::WebKit
        }
    }
}

/// Check if a domain is in the WebKit override list.
pub fn is_webkit_override(host: &str) -> bool {
    WEBKIT_OVERRIDE_DOMAINS.iter().any(|d| host.ends_with(d))
}

/// Check if a domain is in the Servo prefer list.
pub fn is_servo_prefer(host: &str) -> bool {
    SERVO_PREFER_DOMAINS.iter().any(|d| host.ends_with(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_selection_default() {
        assert_eq!(EngineSelection::default(), EngineSelection::WebKit);
    }

    #[test]
    fn test_engine_selection_parse() {
        assert_eq!(
            "auto".parse::<EngineSelection>().unwrap(),
            EngineSelection::Auto
        );
        assert_eq!(
            "servo".parse::<EngineSelection>().unwrap(),
            EngineSelection::Servo
        );
        assert_eq!(
            "webkit".parse::<EngineSelection>().unwrap(),
            EngineSelection::WebKit
        );
        assert_eq!(
            "WebKit".parse::<EngineSelection>().unwrap(),
            EngineSelection::WebKit
        );
        assert!("invalid".parse::<EngineSelection>().is_err());
    }

    #[test]
    fn test_engine_selection_display() {
        assert_eq!(EngineSelection::Auto.to_string(), "auto");
        assert_eq!(EngineSelection::Servo.to_string(), "servo");
        assert_eq!(EngineSelection::WebKit.to_string(), "webkit");
    }

    #[test]
    fn test_select_engine_webkit_always() {
        let url = url::Url::parse("https://example.com").unwrap();
        assert_eq!(
            select_engine(
                &url,
                &EngineSelection::WebKit,
                &std::collections::HashMap::new()
            ),
            EngineType::WebKit
        );
    }

    #[test]
    fn test_select_engine_servo_always() {
        let url = url::Url::parse("https://example.com").unwrap();
        assert_eq!(
            select_engine(
                &url,
                &EngineSelection::Servo,
                &std::collections::HashMap::new()
            ),
            EngineType::Servo
        );
    }

    #[test]
    fn test_select_engine_auto_webkit_override() {
        let url = url::Url::parse("https://docs.google.com/document/d/123").unwrap();
        assert_eq!(
            select_engine(
                &url,
                &EngineSelection::Auto,
                &std::collections::HashMap::new()
            ),
            EngineType::WebKit
        );
    }

    #[test]
    fn test_select_engine_auto_servo_prefer() {
        let url = url::Url::parse("https://github.com/rust-lang/rust").unwrap();
        assert_eq!(
            select_engine(
                &url,
                &EngineSelection::Auto,
                &std::collections::HashMap::new()
            ),
            EngineType::Servo
        );
    }

    #[test]
    fn test_select_engine_auto_default() {
        let url = url::Url::parse("https://example.com/page").unwrap();
        assert_eq!(
            select_engine(
                &url,
                &EngineSelection::Auto,
                &std::collections::HashMap::new()
            ),
            EngineType::WebKit
        );
    }

    #[test]
    fn test_is_webkit_override() {
        assert!(is_webkit_override("docs.google.com"));
        assert!(is_webkit_override("meet.google.com"));
        assert!(!is_webkit_override("example.com"));
    }

    #[test]
    fn test_is_servo_prefer() {
        assert!(is_servo_prefer("github.com"));
        assert!(is_servo_prefer("developer.mozilla.org"));
        assert!(!is_servo_prefer("example.com"));
    }

    #[test]
    fn test_custom_override_takes_priority() {
        let url = url::Url::parse("https://github.com/rust-lang/rust").unwrap();
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("github.com".to_string(), "webkit".to_string());
        assert_eq!(
            select_engine(&url, &EngineSelection::Auto, &overrides),
            EngineType::WebKit
        );
    }

    #[test]
    fn test_custom_override_adds_servo() {
        let url = url::Url::parse("https://example.com").unwrap();
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("example.com".to_string(), "servo".to_string());
        assert_eq!(
            select_engine(&url, &EngineSelection::Auto, &overrides),
            EngineType::Servo
        );
    }

    #[test]
    fn test_custom_override_unknown_engine_falls_back() {
        let url = url::Url::parse("https://example.com").unwrap();
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("example.com".to_string(), "bogus".to_string());
        assert_eq!(
            select_engine(&url, &EngineSelection::Auto, &overrides),
            EngineType::WebKit
        );
    }
}
