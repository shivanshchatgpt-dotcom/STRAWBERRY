//! 🦙 Ollama provider adapter — local AI inference via Ollama's HTTP API.
//!
//! Detects whether Ollama is running locally, discovers available models,
//! and sends completion requests. Ollama availability is never required
//! for core Strawberry functionality.

use super::{
    IntelligenceProvider, IntelligenceRequest, IntelligenceResponse, ProviderKind, ProviderStatus,
};

/// Default Ollama endpoint.
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Ollama provider implementation.
pub struct OllamaProvider {
    base_url: String,
    model: String,
    last_status: std::sync::Mutex<Option<ProviderStatus>>,
}

impl OllamaProvider {
    /// Create a new Ollama provider targeting the given model.
    pub fn new(model: String) -> Self {
        Self {
            base_url: DEFAULT_OLLAMA_URL.to_string(),
            model,
            last_status: std::sync::Mutex::new(None),
        }
    }

    /// Create with a custom endpoint URL.
    pub fn with_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    /// Probe the Ollama endpoint and return available model names.
    pub fn discover_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .call()
            .map_err(|e| format!("Ollama discover failed: {e}"))?;

        let body: serde_json::Value = resp
            .into_json()
            .map_err(|e| format!("Ollama response parse error: {e}"))?;

        let models = body["models"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }
}

impl IntelligenceProvider for OllamaProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Ollama
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn check_health(&self) -> ProviderStatus {
        let start = std::time::Instant::now();
        let url = format!("{}/api/tags", self.base_url);
        let result = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(3))
            .call();

        let latency = start.elapsed().as_millis() as u64;
        let status = match result {
            Ok(_) => ProviderStatus {
                kind: ProviderKind::Ollama,
                available: true,
                model: Some(self.model.clone()),
                endpoint: Some(self.base_url.clone()),
                last_check_ms: Some(latency),
                error: None,
            },
            Err(e) => ProviderStatus {
                kind: ProviderKind::Ollama,
                available: false,
                model: Some(self.model.clone()),
                endpoint: Some(self.base_url.clone()),
                last_check_ms: Some(latency),
                error: Some(format!("{e}")),
            },
        };

        if let Ok(mut last) = self.last_status.lock() {
            *last = Some(status.clone());
        }
        status
    }

    fn complete(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse, String> {
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
            "stream": false,
        });

        if let Some(tokens) = request.max_tokens {
            body["options"] = serde_json::json!({ "num_predict": tokens });
        }
        if let Some(temp) = request.temperature {
            body["options"]["temperature"] = serde_json::json!(temp);
        }
        if request.json_mode {
            body["format"] = serde_json::json!("json");
        }

        let url = format!("{}/api/chat", self.base_url);
        let resp = ureq::post(&url)
            .timeout(std::time::Duration::from_secs(120))
            .send_json(body)
            .map_err(|e| format!("Ollama request failed: {e}"))?;

        let body: serde_json::Value = resp
            .into_json()
            .map_err(|e| format!("Ollama response parse error: {e}"))?;

        let text = body["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let tokens_used = body["eval_count"].as_u64().map(|v| v as u32);

        Ok(IntelligenceResponse {
            text,
            provider: ProviderKind::Ollama,
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
    fn ollama_provider_kind() {
        let p = OllamaProvider::new("llama3".into());
        assert_eq!(p.kind(), ProviderKind::Ollama);
        assert_eq!(p.model_name(), "llama3");
    }

    #[test]
    fn ollama_health_check() {
        let p = OllamaProvider::new("llama3".into());
        let status = p.check_health();
        // The result depends on whether Ollama is running locally.
        // Either outcome is valid — we just verify the status structure.
        assert_eq!(status.kind, ProviderKind::Ollama);
        assert!(status.model.is_some());
        assert!(status.endpoint.is_some());
        assert!(status.last_check_ms.is_some());
        // If available, error must be None; if not, error must be Some.
        if status.available {
            assert!(status.error.is_none());
        } else {
            assert!(status.error.is_some());
        }
    }
}
