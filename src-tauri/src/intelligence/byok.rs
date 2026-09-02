//! ☁️ BYOK (Bring Your Own Key) provider adapter — OpenAI-compatible API.
//!
//! Supports any provider that implements the OpenAI chat completions format:
//! OpenAI, Anthropic (via compatibility proxy), OpenRouter, Together, etc.
//!
//! API keys are NEVER stored in this module. They are fetched from secure
//! credential storage at request time and never logged.

use super::{
    IntelligenceProvider, IntelligenceRequest, IntelligenceResponse, ProviderKind, ProviderStatus,
};

/// BYOK provider for any OpenAI-compatible API.
pub struct ByokProvider {
    /// Human-readable provider name (e.g. "OpenAI", "OpenRouter").
    name: String,
    /// Base URL (e.g. "https://api.openai.com/v1").
    base_url: String,
    /// Model identifier (e.g. "gpt-4o", "claude-3-opus").
    model: String,
    /// API key — loaded from secure storage, NOT persisted here.
    /// This is the in-memory representation used only during requests.
    api_key: std::sync::Mutex<Option<String>>,
    /// Credential service name for keyring lookup.
    credential_service: String,
}

impl ByokProvider {
    /// Create a new BYOK provider.
    ///
    /// The `credential_service` is used as the keyring service name to
    /// look up the API key at request time.
    pub fn new(name: String, base_url: String, model: String, credential_service: String) -> Self {
        Self {
            name,
            base_url,
            model,
            api_key: std::sync::Mutex::new(None),
            credential_service,
        }
    }

    /// Set the API key directly (for testing or when keyring is unavailable).
    pub fn set_api_key(&self, key: String) {
        if let Ok(mut guard) = self.api_key.lock() {
            *guard = Some(key);
        }
    }

    /// Try to load the API key from secure credential storage.
    pub fn load_key_from_storage(&self) -> Result<(), String> {
        let key = crate::intelligence::credential::load_credential(&self.credential_service)
            .map_err(|e| format!("Failed to load API key: {e}"))?;
        self.set_api_key(key);
        Ok(())
    }

    /// Check connection by hitting the models endpoint.
    pub fn test_connection(&self) -> Result<String, String> {
        let key = self.get_key()?;
        let url = format!("{}/models", self.base_url);
        let resp = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {key}"))
            .call()
            .map_err(|e| format!("Connection test failed: {e}"))?;

        let body: serde_json::Value = resp
            .into_json()
            .map_err(|e| format!("Response parse error: {e}"))?;

        // Return available models or a success indicator
        if let Some(models) = body["data"].as_array() {
            let names: Vec<&str> = models
                .iter()
                .filter_map(|m| m["id"].as_str())
                .take(5)
                .collect();
            Ok(format!("Connected. Models: {}", names.join(", ")))
        } else {
            Ok("Connected".into())
        }
    }

    /// List available models from the provider.
    pub fn list_models(&self) -> Result<Vec<String>, String> {
        let key = self.get_key()?;
        let url = format!("{}/models", self.base_url);
        let resp = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {key}"))
            .call()
            .map_err(|e| format!("Model list failed: {e}"))?;

        let body: serde_json::Value = resp
            .into_json()
            .map_err(|e| format!("Response parse error: {e}"))?;

        Ok(body["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["id"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default())
    }

    fn get_key(&self) -> Result<String, String> {
        self.api_key
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?
            .clone()
            .ok_or_else(|| "API key not configured. Set it in Settings → AI.".into())
    }
}

impl IntelligenceProvider for ByokProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Byok
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn check_health(&self) -> ProviderStatus {
        let key = match self.get_key() {
            Ok(k) => k,
            Err(e) => {
                return ProviderStatus {
                    kind: ProviderKind::Byok,
                    available: false,
                    model: Some(self.model.clone()),
                    endpoint: Some(self.base_url.clone()),
                    last_check_ms: None,
                    error: Some(e),
                };
            }
        };

        let start = std::time::Instant::now();
        let url = format!("{}/models", self.base_url);
        let result = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {key}"))
            .call();

        let latency = start.elapsed().as_millis() as u64;
        match result {
            Ok(_) => ProviderStatus {
                kind: ProviderKind::Byok,
                available: true,
                model: Some(self.model.clone()),
                endpoint: Some(self.base_url.clone()),
                last_check_ms: Some(latency),
                error: None,
            },
            Err(e) => ProviderStatus {
                kind: ProviderKind::Byok,
                available: false,
                model: Some(self.model.clone()),
                endpoint: Some(self.base_url.clone()),
                last_check_ms: Some(latency),
                error: Some(format!("{e}")),
            },
        }
    }

    fn complete(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse, String> {
        let key = self.get_key()?;
        let start = std::time::Instant::now();

        let mut messages = Vec::new();
        if let Some(ref sys) = request.system {
            messages.push(serde_json::json!({
                "role": "system",
                "content": sys
            }));
        }
        messages.push(serde_json::json!({
            "role": "user",
            "content": request.prompt
        }));

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });

        if let Some(tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(tokens);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if request.json_mode {
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }

        let url = format!("{}/chat/completions", self.base_url);
        let resp = ureq::post(&url)
            .timeout(std::time::Duration::from_secs(120))
            .set("Authorization", &format!("Bearer {key}"))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| format!("BYOK request failed: {e}"))?;

        let body: serde_json::Value = resp
            .into_json()
            .map_err(|e| format!("BYOK response parse error: {e}"))?;

        let text = body["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let tokens_used = body["usage"]["total_tokens"].as_u64().map(|v| v as u32);

        Ok(IntelligenceResponse {
            text,
            provider: ProviderKind::Byok,
            model: self.model.clone(),
            tokens_used,
            latency_ms: start.elapsed().as_millis() as u64,
            cached: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byok_provider_kind() {
        let p = ByokProvider::new(
            "OpenAI".into(),
            "https://api.openai.com/v1".into(),
            "gpt-4o".into(),
            "strawberry-openai".into(),
        );
        assert_eq!(p.kind(), ProviderKind::Byok);
        assert_eq!(p.model_name(), "gpt-4o");
    }

    #[test]
    fn byok_requires_key() {
        let p = ByokProvider::new(
            "Test".into(),
            "https://example.com/v1".into(),
            "model".into(),
            "test-svc".into(),
        );
        let req = IntelligenceRequest {
            prompt: "hello".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            actor: "user".into(),
            capability: "test".into(),
        };
        let result = p.complete(&req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("API key not configured"));
    }
}
