//! Ollama HTTP client for local LLM inference.
//!
//! Communicates with Ollama's REST API (`/api/generate`, `/api/tags`, `/api/show`).
//! No embedded runtime — Ollama must be running separately.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

/// Configuration for the Ollama client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Base URL of the Ollama server (default: `http://localhost:11434`).
    pub base_url: String,
    /// Default model to use for generation.
    pub default_model: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Temperature for generation (0.0 - 2.0).
    pub temperature: f32,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".into(),
            default_model: "llama3.2".into(),
            timeout_secs: 120,
            temperature: 0.7,
            max_tokens: 2048,
        }
    }
}

/// Error type for Ollama operations.
#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    #[error("Ollama server not reachable at {url}: {source}")]
    ConnectionFailed {
        url: String,
        source: attohttpc::Error,
    },
    #[error("HTTP error: {0}")]
    Http(#[from] attohttpc::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Generation failed: {0}")]
    GenerationFailed(String),
    #[error("Server returned error: {status} — {body}")]
    ServerError { status: u16, body: String },
}

/// Information about an available Ollama model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: u64,
    pub parameter_size: String,
    pub quantization: String,
    pub modified_at: String,
}

/// Response from Ollama's `/api/generate` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaResponse {
    pub model: String,
    pub response: String,
    pub done: bool,
    #[serde(default)]
    pub total_duration: Option<u64>,
    #[serde(default)]
    pub eval_count: Option<u64>,
}

/// Request body for `/api/generate`.
#[derive(Debug, Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    stream: bool,
    options: GenerateOptions,
}

#[derive(Debug, Serialize)]
struct GenerateOptions {
    temperature: f32,
    num_predict: u32,
}

/// HTTP client for the Ollama API.
pub struct OllamaClient {
    config: OllamaConfig,
    http: attohttpc::Session,
}

impl OllamaClient {
    /// Create a new client with the given configuration.
    pub fn new(config: OllamaConfig) -> Self {
        let http = attohttpc::Session::new();
        Self { config, http }
    }

    /// Create a client with default configuration.
    pub fn default_client() -> Self {
        Self::new(OllamaConfig::default())
    }

    /// Check if the Ollama server is reachable.
    pub fn health_check(&self) -> Result<bool, OllamaError> {
        let url = format!("{}/api/tags", self.config.base_url);
        match self.http.get(&url).send() {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) => {
                warn!("Ollama health check failed: {e}");
                Ok(false)
            }
        }
    }

    /// List available models on the Ollama server.
    pub fn list_models(&self) -> Result<Vec<OllamaModel>, OllamaError> {
        let url = format!("{}/api/tags", self.config.base_url);
        let resp = self
            .http
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .map_err(OllamaError::Http)?;

        if !resp.status().is_success() {
            return Err(OllamaError::ServerError {
                status: resp.status().into(),
                body: resp.text().unwrap_or_default(),
            });
        }

        let body: serde_json::Value =
            serde_json::from_str(&resp.text().map_err(OllamaError::Http)?)
                .map_err(OllamaError::Json)?;
        let models = body["models"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        Some(OllamaModel {
                            name: m["name"].as_str()?.to_string(),
                            size: m["size"].as_u64().unwrap_or(0),
                            parameter_size: m["details"]["parameter_size"]
                                .as_str()
                                .unwrap_or("unknown")
                                .to_string(),
                            quantization: m["details"]["quantization_level"]
                                .as_str()
                                .unwrap_or("unknown")
                                .to_string(),
                            modified_at: m["modified_at"].as_str().unwrap_or("unknown").to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    /// Generate text using the specified model (or default).
    pub fn generate(
        &self,
        prompt: &str,
        system_prompt: Option<&str>,
        model: Option<&str>,
    ) -> Result<OllamaResponse, OllamaError> {
        let model_name = model.unwrap_or(&self.config.default_model);
        let url = format!("{}/api/generate", self.config.base_url);

        let request = GenerateRequest {
            model: model_name.to_string(),
            prompt: prompt.to_string(),
            system: system_prompt.map(|s| s.to_string()),
            stream: false,
            options: GenerateOptions {
                temperature: self.config.temperature,
                num_predict: self.config.max_tokens,
            },
        };

        let timeout = Duration::from_secs(self.config.timeout_secs);
        let json_body = serde_json::to_string(&request)?;
        let resp = attohttpc::post(&url)
            .timeout(timeout)
            .header("Content-Type", "application/json")
            .body(attohttpc::body::Text(json_body))
            .send()
            .map_err(OllamaError::Http)?;

        if !resp.status().is_success() {
            let status = resp.status().into();
            let body = resp.text().unwrap_or_default();
            return Err(OllamaError::ServerError { status, body });
        }

        let response: OllamaResponse =
            serde_json::from_str(&resp.text().map_err(OllamaError::Http)?)
                .map_err(OllamaError::Json)?;
        info!(
            "Ollama generation complete: model={}, eval_count={:?}, duration={:?}ns",
            response.model, response.eval_count, response.total_duration
        );
        Ok(response)
    }

    /// Summarize text using the default model.
    pub fn summarize(&self, text: &str) -> Result<String, OllamaError> {
        let system = "You are a concise summarizer. Summarize the following text in 2-3 sentences. Be factual and direct. Do not add commentary.";
        let prompt = format!("Summarize this:\n\n{text}");
        let resp = self.generate(&prompt, Some(system), None)?;
        Ok(resp.response)
    }

    /// Translate text to the target language.
    pub fn translate(&self, text: &str, target_lang: &str) -> Result<String, OllamaError> {
        let system = "You are a professional translator. Translate the text to the target language. Return only the translation, no explanations.";
        let prompt = format!("Translate to {target_lang}:\n\n{text}");
        let resp = self.generate(&prompt, Some(system), None)?;
        Ok(resp.response)
    }

    /// Analyze text and extract structured information.
    pub fn analyze(&self, text: &str) -> Result<String, OllamaError> {
        let system = "You are a text analyst. Analyze the following text and return a JSON object with: topic (string), key_points (array of strings), sentiment (positive/negative/neutral), language (detected language code), word_count (integer).";
        let prompt = format!("Analyze this text:\n\n{text}");
        let resp = self.generate(&prompt, Some(system), None)?;
        Ok(resp.response)
    }

    /// Get the current configuration.
    pub fn config(&self) -> &OllamaConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OllamaConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.default_model, "llama3.2");
        assert_eq!(config.timeout_secs, 120);
        assert!((config.temperature - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_client_creation() {
        let client = OllamaClient::default_client();
        assert_eq!(client.config().base_url, "http://localhost:11434");
    }

    #[test]
    fn test_client_custom_config() {
        let config = OllamaConfig {
            base_url: "http://remote:8080".into(),
            default_model: "codellama".into(),
            timeout_secs: 60,
            temperature: 0.3,
            max_tokens: 4096,
        };
        let client = OllamaClient::new(config);
        assert_eq!(client.config().base_url, "http://remote:8080");
        assert_eq!(client.config().default_model, "codellama");
    }

    #[test]
    fn test_generate_request_serialization() {
        let request = GenerateRequest {
            model: "test".into(),
            prompt: "hello".into(),
            system: Some("be helpful".into()),
            stream: false,
            options: GenerateOptions {
                temperature: 0.5,
                num_predict: 100,
            },
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"test\""));
        assert!(json.contains("\"prompt\":\"hello\""));
        assert!(json.contains("\"stream\":false"));
    }
}
