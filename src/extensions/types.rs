use std::fmt;

/// Unique extension identifier (e.g., "adblock@example.com").
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ExtensionId(pub String);

impl fmt::Display for ExtensionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ExtensionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Unique tab identifier. Opaque to extensions; assigned by the browser.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct TabId(pub u64);

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique window identifier.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct WindowId(pub u64);

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique request identifier.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct RequestId(pub u64);

/// Frame identifier within a tab. 0 is the top-level frame.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct FrameId(pub u32);

/// Listener registration handle for removal.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct ListenerId(pub u64);

/// URL pattern matching (e.g., "*://*.example.com/*").
#[derive(Debug, Clone)]
pub struct UrlPattern(pub String);

/// A message passed between extension contexts.
/// Must be JSON-serializable (structured clone algorithm).
pub type RuntimeMessage = serde_json::Value;

/// Extension API error.
#[derive(Debug, Clone)]
pub enum ExtensionError {
    /// The API method is not supported in Aileron.
    Unsupported(String),
    /// The extension does not have the required permission.
    PermissionDenied(String),
    /// A required argument was missing or invalid.
    InvalidArgument(String),
    /// The target tab, window, or frame was not found.
    NotFound(String),
    /// A runtime error occurred.
    Runtime(String),
    /// JSON serialization/deserialization failed.
    Serialization(String),
    /// Extension manifest loading failed.
    LoadFailed(String),
}

impl fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(msg) => write!(f, "Unsupported: {}", msg),
            Self::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            Self::InvalidArgument(msg) => write!(f, "Invalid argument: {}", msg),
            Self::NotFound(msg) => write!(f, "Not found: {}", msg),
            Self::Runtime(msg) => write!(f, "Runtime error: {}", msg),
            Self::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            Self::LoadFailed(msg) => write!(f, "Load failed: {}", msg),
        }
    }
}

impl std::error::Error for ExtensionError {}

pub type Result<T> = std::result::Result<T, ExtensionError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_id_display() {
        let id = ExtensionId("adblock@example.com".into());
        assert_eq!(id.to_string(), "adblock@example.com");
        assert_eq!(id.as_ref(), "adblock@example.com");
    }

    #[test]
    fn test_extension_id_equality() {
        let a = ExtensionId("test@example.com".into());
        let b = ExtensionId("test@example.com".into());
        let c = ExtensionId("other@example.com".into());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_tab_id_display() {
        let id = TabId(42);
        assert_eq!(id.to_string(), "42");
    }

    #[test]
    fn test_tab_id_copy() {
        let id = TabId(1);
        let id2 = id;
        assert_eq!(id, id2);
    }

    #[test]
    fn test_request_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(RequestId(1));
        set.insert(RequestId(2));
        set.insert(RequestId(1));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_extension_error_display() {
        let err = ExtensionError::Unsupported("onBeforeSendHeaders".into());
        assert!(err.to_string().contains("Unsupported"));

        let err = ExtensionError::PermissionDenied("tabs".into());
        assert!(err.to_string().contains("Permission denied"));

        let err = ExtensionError::NotFound("tab 999".into());
        assert!(err.to_string().contains("Not found"));
    }

    #[test]
    fn test_result_type() {
        let ok_val = 42i32;
        let err: Result<i32> = Err(ExtensionError::InvalidArgument("bad".into()));
        assert_eq!(ok_val, 42);
        assert!(err.is_err());
    }
}

/// Information about a loaded background script.
#[derive(Debug, Clone)]
pub struct BackgroundScript {
    /// The script source code (JavaScript).
    pub source: String,
    /// The filename from the manifest (e.g., "background.js").
    pub filename: String,
}
