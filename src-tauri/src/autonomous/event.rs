//! Normalized event bus — the single channel by which observers feed the
//! autonomous runtime.
//!
//! All Strawberry observers (window activity, file system, git, build, etc.)
//! translate their raw signals into `NormalizedEvent`s and push them through
//! this bus. The runtime then consumes them, updates world state, and decides
//! what to do next.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};
use super::ids::EventId;

/// Discriminated event types the autonomy core understands.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "data")]
pub enum EventKind {
    /// Active application changed (e.g., focus moved to VS Code).
    ActiveAppChanged { from: Option<String>, to: String },
    /// A file was opened in any tool.
    FileOpened { path: String, project: Option<String> },
    /// A file was modified.
    FileModified { path: String, project: Option<String> },
    /// A chat/root was opened inside Strawberry itself.
    ChatOpened { chat_id: String, title: String },
    /// A new chat/root was created.
    ChatCreated { chat_id: String, title: String },
    /// A folder was opened.
    FolderOpened { folder_id: String, name: String },
    /// A search was executed.
    SearchExecuted { query: String, result_count: usize },
    /// Build or test started/completed.
    BuildStateChanged { state: String, project: Option<String> },
    /// A todo/habit toggled.
    TodoToggled { id: u64, completed: bool },
    /// Focus session started/ended.
    FocusSessionChanged { state: String, minutes: u64 },
    /// Tab visited in browser (from tab memory).
    TabVisited { url: String, title: Option<String> },
    /// Inbox item added.
    InboxAdded { kind: String, preview: String },
    /// Screen capture event.
    ScreenCaptured { window_title: Option<String>, app: Option<String> },
    /// Wellness break recorded.
    WellnessBreak { category: String },
    /// A generic heartbeat from any subsystem — no semantic change.
    Heartbeat { source: String },
    /// An error observed in the environment.
    ErrorObserved { message: String, source: String },
}

/// A normalized event with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedEvent {
    pub id: EventId,
    pub kind: EventKind,
    pub timestamp_ms: i64,
    /// Optional weight (0..=1) for routing priority. Default 0.5.
    pub weight: f32,
}

impl NormalizedEvent {
    pub fn new(kind: EventKind) -> Self {
        Self {
            id: EventId(next_event_id()),
            kind,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            weight: 0.5,
        }
    }

    pub fn with_weight(mut self, w: f32) -> Self {
        self.weight = w.clamp(0.0, 1.0);
        self
    }
}

static EVENT_COUNTER: AtomicU64 = AtomicU64::new(1);
fn next_event_id() -> u64 {
    EVENT_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Bounded, thread-safe event bus.
///
/// Observers call `publish` to push events. The runtime consumes them via
/// `drain` (returns up to N events, clearing the queue).
#[derive(Debug, Clone)]
pub struct EventBus {
    inner: Arc<Mutex<Vec<NormalizedEvent>>>,
    capacity: usize,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        Self { inner: Arc::new(Mutex::new(Vec::with_capacity(capacity))), capacity }
    }

    /// Push an event. Drops the oldest if at capacity.
    pub fn publish(&self, ev: NormalizedEvent) {
        let mut q = self.inner.lock().unwrap();
        if q.len() >= self.capacity {
            q.remove(0);
        }
        q.push(ev);
    }

    /// Drain up to `n` events from the front of the queue.
    pub fn drain(&self, n: usize) -> Vec<NormalizedEvent> {
        let mut q = self.inner.lock().unwrap();
        let take = n.min(q.len());
        q.drain(..take).collect()
    }

    /// Peek current size without removing.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// True if no events are queued.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(512)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_and_drain() {
        let bus = EventBus::new(8);
        bus.publish(NormalizedEvent::new(EventKind::Heartbeat { source: "t".into() }));
        bus.publish(NormalizedEvent::new(EventKind::FileModified { path: "x.rs".into(), project: None }));
        assert_eq!(bus.len(), 2);
        let drained = bus.drain(10);
        assert_eq!(drained.len(), 2);
        assert!(bus.is_empty());
    }

    #[test]
    fn bounded_capacity_drops_oldest() {
        let bus = EventBus::new(2);
        for i in 0..5u64 {
            bus.publish(NormalizedEvent::new(EventKind::Heartbeat { source: format!("s{i}") }));
        }
        assert_eq!(bus.len(), 2);
        let drained = bus.drain(10);
        // oldest two are dropped; remaining are s3 and s4
        assert!(drained.iter().any(|e| matches!(&e.kind, EventKind::Heartbeat { source } if source == "s3")));
        assert!(drained.iter().any(|e| matches!(&e.kind, EventKind::Heartbeat { source } if source == "s4")));
    }
}
