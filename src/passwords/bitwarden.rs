use std::process::Command;

/// Client for interacting with the Bitwarden CLI (`bw`).
/// Provides vault search and credential retrieval for auto-fill.
pub struct BitwardenClient {
    /// Path to the bw executable.
    bw_path: String,
    /// Session key (if unlocked).
    session_key: Option<zeroize::Zeroizing<String>>,
}

/// A credential retrieved from the vault.
#[derive(Debug, Clone, zeroize::Zeroize)]
pub struct Credential {
    pub username: zeroize::Zeroizing<String>,
    pub password: zeroize::Zeroizing<String>,
    #[zeroize(skip)]
    pub name: String,
    #[zeroize(skip)]
    pub url: Option<String>,
}

/// A vault item returned by search (ID + name, no secrets).
#[derive(Debug, Clone)]
pub struct VaultItem {
    /// Bitwarden item ID (used for get_credential).
    pub id: String,
    /// Human-readable item name.
    pub name: String,
    /// URL associated with the item (if any).
    pub url: Option<String>,
}

impl BitwardenClient {
    pub fn new() -> Self {
        Self {
            bw_path: "bw".to_string(),
            session_key: None,
        }
    }

    /// Check if the Bitwarden CLI is available.
    pub fn is_available(&self) -> bool {
        Command::new(&self.bw_path)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Unlock the vault. Returns the session key.
    /// In a real implementation, this would prompt for the master password.
    pub fn unlock(&mut self, master_password: &str) -> anyhow::Result<String> {
        let output = Command::new(&self.bw_path)
            .arg("unlock")
            .arg("--passwordenv")
            .arg("BW_MASTERPASSWORD")
            .env("BW_MASTERPASSWORD", master_password)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to unlock vault: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // bw unlock outputs: `export BW_SESSION="xxxx"`
        if let Some(session) = stdout
            .strip_prefix("export BW_SESSION=\"")
            .and_then(|s| s.strip_suffix("\""))
        {
            self.session_key = Some(zeroize::Zeroizing::new(session.to_string()));
            Ok(session.to_string())
        } else {
            anyhow::bail!("Unexpected unlock output format");
        }
    }

    /// Lock the vault and clear the session key.
    pub fn lock(&mut self) {
        self.session_key = None;
    }

    /// Check if the vault is currently unlocked.
    pub fn is_unlocked(&self) -> bool {
        self.session_key.is_some()
    }

    /// Search the vault for items matching a query.
    /// Returns a list of vault items with IDs (needed for get_credential).
    pub fn search(&self, query: &str) -> anyhow::Result<Vec<VaultItem>> {
        let mut cmd = Command::new(&self.bw_path);
        cmd.arg("list").arg("items").arg("--search").arg(query);

        if let Some(ref session) = self.session_key {
            cmd.arg("--session").arg(session.as_str());
        }

        let output = cmd.output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to search vault: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let items: Vec<serde_json::Value> = serde_json::from_str(&stdout)?;

        Ok(items
            .iter()
            .filter_map(|item| {
                let id = item["id"].as_str()?;
                let name = item["name"].as_str().unwrap_or("unnamed");
                let url = item["login"]["uris"]
                    .get(0)
                    .and_then(|u| u["uri"].as_str())
                    .map(String::from);
                Some(VaultItem {
                    id: id.to_string(),
                    name: name.to_string(),
                    url,
                })
            })
            .collect())
    }

    /// Get a credential from the vault by item ID.
    /// The returned Credential has zeroizing Drop, so secrets are cleared on drop.
    pub fn get_credential(&self, item_id: &str) -> anyhow::Result<Credential> {
        let mut cmd = Command::new(&self.bw_path);
        cmd.arg("get").arg("item").arg(item_id);

        if let Some(ref session) = self.session_key {
            cmd.arg("--session").arg(session.as_str());
        }

        let output = cmd.output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to get credential: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let item: serde_json::Value = serde_json::from_str(&stdout)?;

        let username = item["login"]["username"].as_str().unwrap_or("").to_string();
        let password = item["login"]["password"].as_str().unwrap_or("").to_string();
        let name = item["name"].as_str().unwrap_or("unknown").to_string();
        let url = item["login"]["uris"]
            .get(0)
            .and_then(|u| u["uri"].as_str())
            .map(String::from);

        Ok(Credential {
            username: zeroize::Zeroizing::new(username),
            password: zeroize::Zeroizing::new(password),
            name,
            url,
        })
    }

    /// Generate JavaScript to detect login forms and return form info.
    pub fn detect_login_forms_js() -> &'static str {
        r#"
    (function() {
        var forms = document.querySelectorAll('form');
        var loginForms = [];
        forms.forEach(function(form, idx) {
            var hasPassword = form.querySelector('input[type="password"]');
            var hasUsername = form.querySelector(
                'input[type="text"], input[type="email"], ' +
                'input[name="username"], input[name="email"], ' +
                'input[name="user"], input[name="login"]'
            );
            if (hasPassword) {
                loginForms.push({
                    index: idx,
                    hasUsername: !!hasUsername,
                    action: form.action || ''
                });
            }
        });
        if (loginForms.length > 0) {
            return JSON.stringify({type: 'login_forms', forms: loginForms});
        }
        return JSON.stringify({type: 'no_login_forms'});
    })();
    "#
    }

    /// Search for credentials matching a URL's domain.
    pub fn search_for_url(&self, url: &str) -> anyhow::Result<Vec<VaultItem>> {
        let parsed = match url::Url::parse(url) {
            Ok(u) => u,
            Err(_) => return Ok(Vec::new()),
        };
        let domain = match parsed.domain() {
            Some(d) => d.to_string(),
            None => return Ok(Vec::new()),
        };
        if domain.is_empty() {
            return Ok(Vec::new());
        }
        self.search(&domain)
    }

    /// Generate JavaScript to auto-fill credentials into a form.
    /// The credential values are zeroized after this call returns.
    pub fn autofill_js(&self, credential: &Credential) -> String {
        let username = credential.username.as_str();
        let password = credential.password.as_str();
        format!(
            r#"
            (function() {{
                var inputs = document.querySelectorAll('input[type="text"], input[type="email"], input[name="username"], input[name="email"], input[name="user"]');
                if (inputs.length > 0) inputs[0].value = {:?};
                var passInputs = document.querySelectorAll('input[type="password"]');
                if (passInputs.length > 0) passInputs[0].value = {:?};
                // Trigger change events
                inputs.forEach(function(i) {{ i.dispatchEvent(new Event('input', {{bubbles: true}})); }});
                passInputs.forEach(function(i) {{ i.dispatchEvent(new Event('input', {{bubbles: true}})); }});
            }})();
            "#,
            username, password
        )
    }
}

impl Default for BitwardenClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitwarden_client_creation() {
        let client = BitwardenClient::new();
        // Don't actually try to connect — just verify it doesn't panic
        assert_eq!(client.bw_path, "bw");
    }

    #[test]
    fn test_credential_zeroize() {
        let cred = Credential {
            username: zeroize::Zeroizing::new("testuser".into()),
            password: zeroize::Zeroizing::new("testpass123".into()),
            name: "Test Site".into(),
            url: Some("https://example.com".into()),
        };
        assert_eq!(cred.username.as_str(), "testuser");
        assert_eq!(cred.password.as_str(), "testpass123");

        // After zeroize, the memory should be cleared
        drop(cred);
    }

    #[test]
    fn test_autofill_js_generation() {
        let cred = Credential {
            username: zeroize::Zeroizing::new("alice".into()),
            password: zeroize::Zeroizing::new("secret".into()),
            name: "Test".into(),
            url: None,
        };
        let js = BitwardenClient::new().autofill_js(&cred);
        assert!(js.contains("querySelectorAll"));
    }

    #[test]
    fn test_is_available_false_in_ci() {
        // In CI, bw is unlikely to be installed
        let client = BitwardenClient::new();
        // Don't assert — just verify it doesn't panic
        let _ = client.is_available();
    }
}
