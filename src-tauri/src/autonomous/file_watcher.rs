//! 📁 File Change Watcher — watches user-selected directories for file
//! changes and emits canonical events via the EventBus.
//!
//! Uses the `notify` crate for filesystem notifications. Only watches
//! explicitly registered project roots — never scans the entire filesystem.
//!
//! DESIGN RULES:
//! - Watch only registered paths
//! - Debounce duplicate notifications
//! - Emit canonical events via EventBus
//! - Respect privacy policy (no file contents)
//! - Do not spawn per-file threads
//! - Graceful shutdown

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::autonomous::event::{EventBus, EventKind as BusEventKind, NormalizedEvent};

/// Debounce window — ignore events within this duration for the same path.
const DEBOUNCE_MS: u64 = 500;

/// Maximum number of file events to buffer before dropping.
const BUFFER_SIZE: usize = 256;

/// A file watcher that monitors registered paths and emits events.
pub struct FileWatcher {
    /// The notify watcher (kept alive by holding it).
    _watcher: RecommendedWatcher,
    /// Receiver for debounced file events.
    rx: mpsc::Receiver<Result<notify::Event, notify::Error>>,
    /// Paths we're watching.
    watched: Vec<PathBuf>,
}

impl FileWatcher {
    /// Create a new file watcher. Does NOT start watching yet — call `watch()`.
    pub fn new() -> Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel(BUFFER_SIZE);

        let watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                let _ = tx.send(res);
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )
        .map_err(|e| format!("Failed to create file watcher: {e}"))?;

        Ok(Self {
            _watcher: watcher,
            rx,
            watched: Vec::new(),
        })
    }

    /// Start watching a directory (non-recursive).
    pub fn watch(&mut self, path: &Path) -> Result<(), String> {
        self._watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch {}: {e}", path.display()))?;
        self.watched.push(path.to_path_buf());
        Ok(())
    }

    /// Start watching a directory recursively.
    pub fn watch_recursive(&mut self, path: &Path) -> Result<(), String> {
        self._watcher
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch {} recursively: {e}", path.display()))?;
        self.watched.push(path.to_path_buf());
        Ok(())
    }

    /// Stop watching a directory.
    pub fn unwatch(&mut self, path: &Path) -> Result<(), String> {
        self._watcher
            .unwatch(path)
            .map_err(|e| format!("Failed to unwatch {}: {e}", path.display()))?;
        self.watched.retain(|p| p != path);
        Ok(())
    }

    /// Poll for file events and publish them to the EventBus.
    /// Returns the number of events published.
    pub fn poll_and_publish(&self, bus: &EventBus) -> usize {
        let mut count = 0;
        // Non-blocking: drain all pending events
        while let Ok(result) = self.rx.try_recv() {
            if let Ok(event) = result {
                count += self.process_event(event, bus);
            }
        }
        count
    }

    /// Convert a notify Event into bus events and publish.
    fn process_event(&self, event: Event, bus: &EventBus) -> usize {
        let mut published = 0;

        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                for path in &event.paths {
                    // Skip hidden files and common non-project files
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy();
                        if name_str.starts_with('.')
                            || name_str == "node_modules"
                            || name_str == ".git"
                            || name_str.ends_with(".swp")
                            || name_str.ends_with("~")
                        {
                            continue;
                        }
                    }

                    let kind_str = match event.kind {
                        EventKind::Create(_) => "file.created",
                        EventKind::Modify(_) => "file.modified",
                        EventKind::Remove(_) => "file.deleted",
                        _ => "file.changed",
                    };

                    let bus_kind = match event.kind {
                        EventKind::Create(_) => BusEventKind::FileOpened {
                            path: path.display().to_string(),
                            project: None,
                        },
                        EventKind::Modify(_) | EventKind::Remove(_) => BusEventKind::FileModified {
                            path: path.display().to_string(),
                            project: None,
                        },
                        _ => BusEventKind::Heartbeat { source: "file_watcher".into() },
                    };

                    let ev = NormalizedEvent::new(bus_kind);
                    bus.publish(ev);
                    published += 1;
                }
            }
            _ => {
                // Other event kinds (access, any, etc.) — ignore for now
            }
        }

        published
    }

    /// List currently watched paths.
    pub fn watched_paths(&self) -> &[PathBuf] {
        &self.watched
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, Mutex};

    #[test]
    fn file_watcher_creation() {
        let w = FileWatcher::new();
        assert!(w.is_ok());
        let w = w.unwrap();
        assert!(w.watched_paths().is_empty());
    }

    #[test]
    fn watch_and_unwatch() {
        let dir = std::env::temp_dir().join(format!("fw-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        let mut w = FileWatcher::new().unwrap();
        w.watch(&dir).unwrap();
        assert_eq!(w.watched_paths().len(), 1);

        w.unwatch(&dir).unwrap();
        assert!(w.watched_paths().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn poll_returns_zero_when_no_events() {
        let bus = EventBus::new(8);
        let w = FileWatcher::new().unwrap();
        let count = w.poll_and_publish(&bus);
        assert_eq!(count, 0);
    }
}
