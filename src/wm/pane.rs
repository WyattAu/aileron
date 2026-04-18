use url::Url;
use uuid::Uuid;

/// Metadata for a single browser pane (leaf node in BSP tree).
#[derive(Debug, Clone)]
pub struct Pane {
    pub id: Uuid,
    pub url: Url,
    pub title: String,
    pub session_id: Option<String>,
}

impl Pane {
    pub fn new(url: Url) -> Self {
        Self {
            id: Uuid::new_v4(),
            url: url.clone(),
            title: url.to_string(),
            session_id: None,
        }
    }

    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pane_new() {
        let url = Url::parse("https://example.com").unwrap();
        let pane = Pane::new(url.clone());
        assert_eq!(pane.url.as_str(), "https://example.com/");
        assert_eq!(pane.title, "https://example.com/");
        assert!(pane.session_id.is_none());
    }

    #[test]
    fn test_pane_unique_ids() {
        let url = Url::parse("https://example.com").unwrap();
        let p1 = Pane::new(url.clone());
        let p2 = Pane::new(url);
        assert_ne!(p1.id, p2.id);
    }

    #[test]
    fn test_pane_with_session() {
        let url = Url::parse("https://example.com").unwrap();
        let pane = Pane::new(url).with_session("session-123".to_string());
        assert_eq!(pane.session_id.as_deref(), Some("session-123"));
    }

    #[test]
    fn test_pane_title_defaults_to_url() {
        let url = Url::parse("aileron://new").unwrap();
        let pane = Pane::new(url);
        assert_eq!(pane.title, pane.url.to_string());
    }

    #[test]
    fn test_pane_clone() {
        let url = Url::parse("https://example.com").unwrap();
        let pane = Pane::new(url).with_session("sess".into());
        let cloned = pane.clone();
        assert_eq!(pane.id, cloned.id);
        assert_eq!(pane.session_id, cloned.session_id);
    }

    #[test]
    fn test_pane_debug() {
        let url = Url::parse("https://example.com").unwrap();
        let pane = Pane::new(url);
        let debug = format!("{:?}", pane);
        assert!(debug.contains("Pane"));
        assert!(debug.contains("id:"));
    }
}
