//! 🧠 Intelligence Provider Layer — provider-neutral abstraction for optional AI.
//!
//! Strawberry must remain fully functional WITHOUT any AI provider. This module
//! provides a thin, provider-neutral contract that capabilities can consume.
//! The router dispatches to deterministic fallback, Ollama (local), or BYOK
//! (cloud) based on user configuration.
//!
//! Architecture:
//! ```text
//!     FEATURE / CAPABILITY
//!             │
//!             ▼
//!     IntelligenceRequest → IntelligenceProvider::complete()
//!             │
//!         ProviderRouter
//!             │
//!     ┌───────┼───────┐
//!     ▼       ▼       ▼
//!   None   Ollama   BYOK
//!   (err)  (local)  (cloud)
//! ```

pub mod credential;
pub mod config;
pub mod ollama;
pub mod byok;

use serde::{Deserialize, Serialize};

// ─── Provider Identity ──────────────────────────────────────────────────────

/// Identifies which provider backend is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// No AI configured — deterministic fallback only.
    None,
    /// Local Ollama instance.
    Ollama,
    /// User-provided OpenAI-compatible API (BYOK).
    Byok,
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Ollama => write!(f, "ollama"),
            Self::Byok => write!(f, "byok"),
        }
    }
}

// ─── Capability Metadata ────────────────────────────────────────────────────

/// What a capability needs from the intelligence layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityMeta {
    /// Does this capability require AI at all?
    pub requires_ai: bool,
    /// Is cloud processing allowed for this capability?
    pub cloud_allowed: bool,
    /// Minimum provider capability level needed (0 = any).
    pub min_level: u8,
}

impl Default for CapabilityMeta {
    fn default() -> Self {
        Self {
            requires_ai: false,
            cloud_allowed: false,
            min_level: 0,
        }
    }
}

// ─── Request / Response ─────────────────────────────────────────────────────

/// A request to the intelligence layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelligenceRequest {
    /// Free-text prompt or instruction.
    pub prompt: String,
    /// Optional system instruction prepended to the prompt.
    pub system: Option<String>,
    /// Maximum tokens in the response (provider may ignore).
    pub max_tokens: Option<u32>,
    /// Temperature (0.0–2.0, provider may ignore).
    pub temperature: Option<f32>,
    /// Whether structured JSON output is desired.
    pub json_mode: bool,
    /// Who initiated this request (for audit trail).
    pub actor: String,
    /// Which capability is asking (for privacy policy).
    pub capability: String,
}

/// The result from the intelligence layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelligenceResponse {
    /// The generated text.
    pub text: String,
    /// Which provider actually handled it.
    pub provider: ProviderKind,
    /// Model name used.
    pub model: String,
    /// Tokens used (if reported by provider).
    pub tokens_used: Option<u32>,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Whether the response was from cache (always false for now).
    pub cached: bool,
}

// ─── Provider Status ────────────────────────────────────────────────────────

/// Current status of a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub kind: ProviderKind,
    pub available: bool,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub last_check_ms: Option<u64>,
    pub error: Option<String>,
}

// ─── Provider Trait ─────────────────────────────────────────────────────────

/// The contract every provider adapter must implement.
///
/// Implementors must NOT leak provider-specific SDK types outside this module.
pub trait IntelligenceProvider: Send + Sync {
    /// Provider identity.
    fn kind(&self) -> ProviderKind;

    /// Check if the provider is currently reachable.
    fn check_health(&self) -> ProviderStatus;

    /// Send a completion request.
    fn complete(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse, String>;

    /// Provider model name.
    fn model_name(&self) -> &str;
}

// ─── Provider Router ────────────────────────────────────────────────────────

/// Routes intelligence requests to the appropriate provider.
///
/// The router is the ONLY place that dispatches to a concrete provider.
/// All capability code goes through this router.
pub struct ProviderRouter {
    ollama: Option<Box<dyn IntelligenceProvider>>,
    byok: Option<Box<dyn IntelligenceProvider>>,
    active: ProviderKind,
}

impl ProviderRouter {
    pub fn new() -> Self {
        Self {
            ollama: None,
            byok: None,
            active: ProviderKind::None,
        }
    }

    /// Register an Ollama provider (called when Ollama is detected).
    pub fn set_ollama(&mut self, provider: Box<dyn IntelligenceProvider>) {
        self.ollama = Some(provider);
        if self.active == ProviderKind::None {
            self.active = ProviderKind::Ollama;
        }
    }

    /// Register a BYOK provider.
    pub fn set_byok(&mut self, provider: Box<dyn IntelligenceProvider>) {
        self.byok = Some(provider);
        // BYOK takes priority if explicitly configured
        self.active = ProviderKind::Byok;
    }

    /// Set the active provider kind.
    pub fn set_active(&mut self, kind: ProviderKind) {
        self.active = kind;
    }

    /// Get the currently active provider kind.
    pub fn active_kind(&self) -> ProviderKind {
        self.active
    }

    /// Get status of all providers.
    pub fn all_status(&self) -> Vec<ProviderStatus> {
        let mut statuses = vec![ProviderStatus {
            kind: ProviderKind::None,
            available: true,
            model: None,
            endpoint: None,
            last_check_ms: None,
            error: None,
        }];
        if let Some(ref o) = self.ollama {
            statuses.push(o.check_health());
        }
        if let Some(ref b) = self.byok {
            statuses.push(b.check_health());
        }
        statuses
    }

    /// Dispatch a request to the active provider.
    ///
    /// Returns `Err` when no provider is configured or the provider fails.
    /// Capabilities should catch this and fall back to deterministic behavior.
    pub fn complete(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse, String> {
        match self.active {
            ProviderKind::None => {
                Err("No AI provider configured. Enable Ollama or BYOK in settings.".into())
            }
            ProviderKind::Ollama => {
                let p = self
                    .ollama
                    .as_ref()
                    .ok_or("Ollama provider not initialized")?;
                p.complete(request)
            }
            ProviderKind::Byok => {
                let p = self
                    .byok
                    .as_ref()
                    .ok_or("BYOK provider not configured")?;
                p.complete(request)
            }
        }
    }
}

impl Default for ProviderRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_provider_returns_error() {
        let router = ProviderRouter::new();
        let req = IntelligenceRequest {
            prompt: "hello".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            actor: "user".into(),
            capability: "test".into(),
        };
        let result = router.complete(&req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No AI provider"));
    }

    #[test]
    fn default_status_includes_none() {
        let router = ProviderRouter::new();
        let statuses = router.all_status();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].kind, ProviderKind::None);
        assert!(statuses[0].available);
    }

    #[test]
    fn provider_kind_display() {
        assert_eq!(ProviderKind::None.to_string(), "none");
        assert_eq!(ProviderKind::Ollama.to_string(), "ollama");
        assert_eq!(ProviderKind::Byok.to_string(), "byok");
    }
}
