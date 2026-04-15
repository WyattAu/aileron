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
