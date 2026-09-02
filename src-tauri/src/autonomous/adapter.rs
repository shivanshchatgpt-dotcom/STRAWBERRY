//! 🔌 Source Adapter Contract — thin adapters that translate raw signals
//! into canonical events for the unified event spine.
//!
//! Each adapter:
//! 1. Receives a raw signal from its source
//! 2. Applies privacy screening
//! 3. Normalizes into a CanonicalEvent
//! 4. Deduplicates
//! 5. Publishes to the EventBus
//!
//! Adapters do NOT:
//! - Own a database
//! - Create a new event bus
//! - Spawn new background threads
//! - Require AI

use crate::autonomous::event::{EventBus, NormalizedEvent, EventKind};

/// Adapter metadata — describes what a source adapter can produce.
pub struct AdapterInfo {
    /// Unique adapter identifier (e.g. "clipboard", "file_watcher").
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Adapter version.
    pub version: &'static str,
    /// What privacy level this adapter typically produces.
    pub default_privacy: &'static str,
}

/// The contract every source adapter must implement.
///
/// This is intentionally lightweight — just a trait with metadata and
/// a single `adapt` method. The adapter handles normalization internally
/// and publishes to the provided EventBus.
pub trait SourceAdapter: Send + Sync {
    /// Adapter identity metadata.
    fn info(&self) -> AdapterInfo;

    /// Process a raw signal and publish canonical events to the bus.
    ///
    /// The adapter is responsible for:
    /// - Privacy screening (block/redact/allow)
    /// - Deduplication
    /// - Normalization into NormalizedEvent
    /// - Publishing via bus.publish()
    ///
    /// Returns the number of events published (0 if filtered out).
    fn adapt(&self, signal: RawSignal, bus: &EventBus) -> Result<usize, String>;
}

/// A raw signal from a source. The adapter knows how to interpret it.
pub struct RawSignal {
    /// The source-specific signal type (adapter interprets this).
    pub kind: String,
    /// Raw payload from the source.
    pub payload: String,
    /// When the signal was captured (unix ms).
    pub timestamp_ms: i64,
    /// Optional source metadata.
    pub metadata: Option<String>,
}

impl RawSignal {
    pub fn new(kind: impl Into<String>, payload: impl Into<String>) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            kind: kind.into(),
            payload: payload.into(),
            timestamp_ms: now_ms,
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, m: impl Into<String>) -> Self {
        self.metadata = Some(m.into());
        self
    }
}

/// A simple example adapter for clipboard captures. This demonstrates the
/// contract without being production code.
pub struct ClipboardAdapter;

impl SourceAdapter for ClipboardAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            id: "clipboard",
            name: "Clipboard Capture",
            version: "1.0",
            default_privacy: "sensitive",
        }
    }

    fn adapt(&self, signal: RawSignal, bus: &EventBus) -> Result<usize, String> {
        if signal.payload.trim().is_empty() {
            return Ok(0);
        }

        let ev = NormalizedEvent::new(EventKind::ChatOpened {
            chat_id: signal.payload.chars().take(20).collect(),
            title: signal.payload.chars().take(70).collect(),
        })
        .with_weight(0.6);

        bus.publish(ev);
        Ok(1)
    }
}

/// A simple example adapter for file change signals.
pub struct FileChangeAdapter;

impl SourceAdapter for FileChangeAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            id: "file_watcher",
            name: "File Change Watcher",
            version: "1.0",
            default_privacy: "public",
        }
    }

    fn adapt(&self, signal: RawSignal, bus: &EventBus) -> Result<usize, String> {
        let path = signal.payload.trim().to_string();
        if path.is_empty() {
            return Ok(0);
        }

        let ev = NormalizedEvent::new(EventKind::FileModified {
            path: path.clone(),
            project: None,
        });

        bus.publish(ev);
        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_info_metadata() {
        let a = ClipboardAdapter;
        let info = a.info();
        assert_eq!(info.id, "clipboard");
        assert_eq!(info.version, "1.0");
    }

    #[test]
    fn clipboard_adapter_publishes() {
        let bus = EventBus::new(8);
        let a = ClipboardAdapter;
        let signal = RawSignal::new("text", "hello world");
        let count = a.adapt(signal, &bus).unwrap();
        assert_eq!(count, 1);
        assert_eq!(bus.len(), 1);
    }

    #[test]
    fn clipboard_adapter_ignores_empty() {
        let bus = EventBus::new(8);
        let a = ClipboardAdapter;
        let signal = RawSignal::new("text", "  ");
        let count = a.adapt(signal, &bus).unwrap();
        assert_eq!(count, 0);
        assert!(bus.is_empty());
    }

    #[test]
    fn file_adapter_publishes() {
        let bus = EventBus::new(8);
        let a = FileChangeAdapter;
        let signal = RawSignal::new("modified", "/tmp/test.rs");
        let count = a.adapt(signal, &bus).unwrap();
        assert_eq!(count, 1);
        let events = bus.drain(1);
        assert!(matches!(&events[0].kind, EventKind::FileModified { path, .. } if path == "/tmp/test.rs"));
    }

    #[test]
    fn raw_signal_builder() {
        let s = RawSignal::new("text", "data").with_metadata("source=clip");
        assert_eq!(s.kind, "text");
        assert_eq!(s.payload, "data");
        assert_eq!(s.metadata.as_deref(), Some("source=clip"));
    }
}
