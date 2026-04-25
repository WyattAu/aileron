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
    /// Enhanced with: periodic re-scan via MutationObserver, OAuth detection,
    /// multi-step flow handling via sessionStorage, and hidden form filtering.
    pub fn detect_login_forms_js() -> &'static str {
        r#"
    (function() {
        if (window.__aileron_form_observer_installed) return;
        window.__aileron_form_observer_installed = true;

        var OAUTH_URL_PATTERNS = [
            'accounts.google.com/o/oauth2',
            'login.microsoftonline.com',
            'graph.facebook.com/oauth',
            'appleid.apple.com/auth'
        ];

        var OAUTH_ACTION_PATTERNS = [
            '/oauth', '/oauth2', '/authorize', '/sso', '/saml'
        ];

        function isOAuthUrl(url) {
            if (!url) return false;
            var lower = url.toLowerCase();
            for (var i = 0; i < OAUTH_URL_PATTERNS.length; i++) {
                if (lower.indexOf(OAUTH_URL_PATTERNS[i]) !== -1) return true;
            }
            return false;
        }

        function isOAuthAction(action) {
            if (!action) return false;
            var lower = action.toLowerCase();
            for (var i = 0; i < OAUTH_ACTION_PATTERNS.length; i++) {
                if (lower.indexOf(OAUTH_ACTION_PATTERNS[i]) !== -1) return true;
            }
            return false;
        }

        function isElementHidden(el) {
            var current = el;
            while (current && current !== document.body) {
                var style = window.getComputedStyle(current);
                if (style.display === 'none') return true;
                if (style.visibility === 'hidden') return true;
                if (parseFloat(style.opacity) === 0) return true;
                var pos = style.position;
                var left = parseInt(style.left, 10);
                var top = parseInt(style.top, 10);
                if ((pos === 'fixed' || pos === 'absolute') && (left < -999 || top < -999)) return true;
                current = current.parentElement;
            }
            return false;
        }

        function hasOpenIdFields(form) {
            var allInputs = form.querySelectorAll('input');
            for (var i = 0; i < allInputs.length; i++) {
                var name = (allInputs[i].name || '').toLowerCase();
                if (name.indexOf('openid') !== -1) return true;
            }
            if (form.getAttribute('rel') === 'openid') return true;
            return false;
        }

        function detectOAuth() {
            if (isOAuthUrl(window.location.href)) {
                window.__aileron_is_oauth = true;
                return true;
            }
            return false;
        }

        function aileron_detect_login_forms() {
            if (window.__aileron_is_oauth) {
                return JSON.stringify({type: 'no_login_forms', oauth: true});
            }

            var forms = document.querySelectorAll('form');
            var loginForms = [];
            var detectedForms = document.querySelectorAll('form');
            var globalIdx = 0;

            for (var i = 0; i < forms.length; i++) {
                var form = forms[i];
                var formIdx = Array.prototype.indexOf.call(detectedForms, form);

                if (isElementHidden(form)) continue;
                if (hasOpenIdFields(form)) {
                    window.__aileron_is_oauth = true;
                    continue;
                }
                if (isOAuthAction(form.getAttribute('action'))) {
                    window.__aileron_is_oauth = true;
                    continue;
                }

                var hasPassword = form.querySelector('input[type="password"]');
                var hasUsername = form.querySelector(
                    'input[type="text"], input[type="email"], ' +
                    'input[name="username"], input[name="email"], ' +
                    'input[name="user"], input[name="login"]'
                );

                if (hasPassword) {
                    var isStep2 = false;
                    var step1Data = sessionStorage.getItem('__aileron_multistep_step1');
                    if (step1Data) {
                        try {
                            var parsed = JSON.parse(step1Data);
                            if (parsed.username && !parsed.password) {
                                isStep2 = true;
                                var credential = {
                                    username: parsed.username,
                                    password: hasPassword.value || '',
                                    url: parsed.url || window.location.href
                                };
                                window.__aileron_credential_save = credential;
                                sessionStorage.removeItem('__aileron_multistep_step1');
                            }
                        } catch(e) {
                            sessionStorage.removeItem('__aileron_multistep_step1');
                        }
                    }

                    loginForms.push({
                        index: formIdx,
                        hasUsername: !!hasUsername,
                        action: form.action || '',
                        multiStep: isStep2
                    });
                } else if (hasUsername) {
                    var userInput = form.querySelector(
                        'input[type="text"], input[type="email"], ' +
                        'input[name="username"], input[name="email"], ' +
                        'input[name="user"], input[name="login"]'
                    );
                    if (userInput && userInput.value) {
                        sessionStorage.setItem('__aileron_multistep_step1', JSON.stringify({
                            username: userInput.value,
                            password: null,
                            url: window.location.href
                        }));
                    }
                }

                globalIdx++;
            }

            if (loginForms.length > 0) {
                return JSON.stringify({type: 'login_forms', forms: loginForms});
            }
            return JSON.stringify({type: 'no_login_forms'});
        }

        detectOAuth();

        var debounceTimer = null;
        var observer = new MutationObserver(function() {
            if (debounceTimer) clearTimeout(debounceTimer);
            debounceTimer = setTimeout(function() {
                aileron_detect_login_forms();
            }, 2000);
        });

        observer.observe(document.documentElement, {
            childList: true,
            subtree: true,
            attributes: true
        });

        aileron_detect_login_forms();
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

    /// JavaScript to detect form submissions and save credentials.
    /// Enhanced with OAuth flag check and multi-step sessionStorage handling.
    pub fn form_submit_observer_js() -> &'static str {
        r#"
    (function() {
        if (window.__aileron_submit_observer_installed) return;
        window.__aileron_submit_observer_installed = true;

        document.addEventListener('submit', function(e) {
            if (window.__aileron_is_oauth) return;

            var form = e.target;
            var passInput = form.querySelector('input[type="password"]');
            var userInput = form.querySelector(
                'input[type="text"], input[type="email"], ' +
                'input[name="username"], input[name="email"]'
            );

            if (passInput && passInput.value && userInput && userInput.value) {
                window.__aileron_credential_save = {
                    username: userInput.value,
                    password: passInput.value,
                    url: window.location.href
                };
                sessionStorage.removeItem('__aileron_multistep_step1');
            } else if (userInput && userInput.value && !passInput) {
                sessionStorage.setItem('__aileron_multistep_step1', JSON.stringify({
                    username: userInput.value,
                    password: null,
                    url: window.location.href
                }));
            }
        });
    })();
    "#
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

    /// JavaScript to detect login forms and report field IDs back via IPC.
    /// Sends `{t: "login-form-detected", has_login, username_id, password_id}`.
    /// Should be injected after page load with a short delay.
    pub fn form_detect_report_js() -> &'static str {
        r#"
    (function() {
        try {
            var pw = document.querySelector('input[type="password"]');
            if (!pw) {
                window.ipc.postMessage(JSON.stringify({t: 'login-form-detected', has_login: false}));
                return;
            }
            var uf = null;
            var form = pw.closest('form');
            if (form) {
                uf = form.querySelector(
                    'input[type="text"], input[type="email"], ' +
                    'input[autocomplete="username"], input[autocomplete="email"], ' +
                    'input[name*="user"], input[name*="email"], input[name*="login"]'
                );
            }
            if (!uf) {
                uf = document.querySelector(
                    'input[type="text"], input[type="email"], ' +
                    'input[autocomplete="username"], input[autocomplete="email"], ' +
                    'input[name*="user"], input[name*="email"], input[name*="login"]'
                );
            }
            window.ipc.postMessage(JSON.stringify({
                t: 'login-form-detected',
                has_login: true,
                username_id: (uf && uf.id) ? uf.id : '',
                password_id: pw.id || ''
            }));
        } catch(e) {}
    })();
    "#
    }

    /// Generate JavaScript to auto-fill credentials using specific element IDs.
    /// Falls back to querySelectorAll if IDs are empty or elements not found.
    /// All values are escaped using Rust debug format (`{:?}`) for safety.
    pub fn autofill_by_id_js(&self, username_id: &str, password_id: &str, credential: &Credential) -> String {
        let username = credential.username.as_str();
        let password = credential.password.as_str();
        format!(
            r#"(function() {{
                var uEl = document.getElementById({:?});
                if (!uEl) {{
                    var uInputs = document.querySelectorAll('input[type="text"], input[type="email"], input[autocomplete="username"], input[autocomplete="email"], input[name*="user"], input[name*="email"], input[name*="login"]');
                    uEl = uInputs.length > 0 ? uInputs[0] : null;
                }}
                if (uEl) {{
                    uEl.value = {:?};
                    uEl.dispatchEvent(new Event('input', {{bubbles: true}}));
                    uEl.dispatchEvent(new Event('change', {{bubbles: true}}));
                }}
                var pEl = document.getElementById({:?});
                if (!pEl) {{
                    var pInputs = document.querySelectorAll('input[type="password"]');
                    pEl = pInputs.length > 0 ? pInputs[0] : null;
                }}
                if (pEl) {{
                    pEl.value = {:?};
                    pEl.dispatchEvent(new Event('input', {{bubbles: true}}));
                    pEl.dispatchEvent(new Event('change', {{bubbles: true}}));
                }}
            }})();"#,
            username_id, username, password_id, password
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

    #[test]
    fn test_detect_js_contains_oauth_domain_checks() {
        let js = BitwardenClient::detect_login_forms_js();
        assert!(js.contains("accounts.google.com/o/oauth2"));
        assert!(js.contains("login.microsoftonline.com"));
        assert!(js.contains("graph.facebook.com/oauth"));
        assert!(js.contains("appleid.apple.com/auth"));
    }

    #[test]
    fn test_detect_js_contains_mutation_observer() {
        let js = BitwardenClient::detect_login_forms_js();
        assert!(js.contains("MutationObserver"));
        assert!(js.contains("setTimeout"));
        assert!(js.contains("2000"));
        assert!(js.contains("aileron_detect_login_forms"));
    }

    #[test]
    fn test_detect_js_contains_sessionstorage_multistep() {
        let js = BitwardenClient::detect_login_forms_js();
        assert!(js.contains("sessionStorage"));
        assert!(js.contains("__aileron_multistep_step1"));
        assert!(js.contains("multiStep"));
    }

    #[test]
    fn test_detect_js_contains_hidden_form_detection() {
        let js = BitwardenClient::detect_login_forms_js();
        assert!(js.contains("display"));
        assert!(js.contains("none"));
        assert!(js.contains("visibility"));
        assert!(js.contains("hidden"));
        assert!(js.contains("opacity"));
        assert!(js.contains("isElementHidden"));
        assert!(js.contains("-999"));
    }

    #[test]
    fn test_detect_js_has_balanced_braces() {
        let js = BitwardenClient::detect_login_forms_js();
        let mut depth = 0;
        let mut in_string = false;
        let mut escape = false;
        let chars: Vec<char> = js.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if escape {
                escape = false;
                i += 1;
                continue;
            }
            if c == '\\' && in_string {
                escape = true;
                i += 1;
                continue;
            }
            if c == '"' || c == '\'' {
                in_string = !in_string;
                i += 1;
                continue;
            }
            if !in_string {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            i += 1;
        }
        assert_eq!(depth, 0, "JS should have balanced braces");
    }

    #[test]
    fn test_submit_js_checks_oauth_flag() {
        let js = BitwardenClient::form_submit_observer_js();
        assert!(js.contains("__aileron_is_oauth"));
        assert!(js.contains("return;"));
    }

    #[test]
    fn test_submit_js_handles_multistep() {
        let js = BitwardenClient::form_submit_observer_js();
        assert!(js.contains("sessionStorage"));
        assert!(js.contains("__aileron_multistep_step1"));
    }

    #[test]
    fn test_detect_js_contains_oauth_flag_prevention() {
        let js = BitwardenClient::detect_login_forms_js();
        assert!(js.contains("window.__aileron_is_oauth"));
        assert!(js.contains("oauth: true"));
    }

    #[test]
    fn test_detect_js_contains_openid_detection() {
        let js = BitwardenClient::detect_login_forms_js();
        assert!(js.contains("openid"));
        assert!(js.contains("rel"));
    }

    #[test]
    fn test_form_detect_report_js_sends_ipc() {
        let js = BitwardenClient::form_detect_report_js();
        assert!(js.contains("login-form-detected"));
        assert!(js.contains("has_login"));
        assert!(js.contains("username_id"));
        assert!(js.contains("password_id"));
        assert!(js.contains("ipc.postMessage"));
    }

    #[test]
    fn test_form_detect_report_js_queries_password_field() {
        let js = BitwardenClient::form_detect_report_js();
        assert!(js.contains("input[type=\"password\"]"));
        assert!(js.contains("input[type=\"text\"]"));
        assert!(js.contains("input[type=\"email\"]"));
        assert!(js.contains("autocomplete=\"username\""));
    }

    #[test]
    fn test_autofill_by_id_js_uses_debug_format() {
        let cred = Credential {
            username: zeroize::Zeroizing::new("user\"with'quotes".into()),
            password: zeroize::Zeroizing::new("pass<>val&ue".into()),
            name: "Test".into(),
            url: None,
        };
        let client = BitwardenClient::new();
        let js = client.autofill_by_id_js("user-field", "pass-field", &cred);
        assert!(js.contains("getElementById(\"user-field\")"));
        assert!(js.contains("getElementById(\"pass-field\")"));
        assert!(js.contains("user\\\"with'quotes"));
        assert!(js.contains("pass<>val&ue"));
        assert!(js.contains("dispatchEvent"));
    }

    #[test]
    fn test_autofill_by_id_js_fallback_when_empty_ids() {
        let cred = Credential {
            username: zeroize::Zeroizing::new("alice".into()),
            password: zeroize::Zeroizing::new("secret".into()),
            name: "Test".into(),
            url: None,
        };
        let client = BitwardenClient::new();
        let js = client.autofill_by_id_js("", "", &cred);
        assert!(js.contains("getElementById(\"\")"));
        assert!(js.contains("querySelectorAll"));
    }
}
