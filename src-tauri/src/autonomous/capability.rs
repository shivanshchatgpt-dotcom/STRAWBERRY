//! 🧩 Capability Registry — the single catalog of everything Strawberry can do.
//!
//! NON-NEGOTIABLE #2: one orchestrator, one event bus, one adaptive
//! scheduler, ONE capability registry. Wellness, Ghost, Project Brain, etc.
//! all become rows here — the scheduler decides when they run, nobody owns
//! a private timer loop anymore.
//!
//! The manifest below is pure code (compile-time truth). User/adaptive
//! overrides live in the `capability_state` table and are layered on top
//! by [`Registry::load`]. Empty table == pure defaults.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How a capability prefers to be woken up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// Immediately after a matching event (0–2 s budget).
    Event,
    /// Event-driven but debounced/batched.
    Debounced,
    /// Fixed cadence, adaptive-adjustable.
    Interval,
    /// Only when the user is idle.
    Idle,
    /// Session start/end boundaries.
    Session,
    /// Once per day (idle-preferred).
    Daily,
    /// Weekly deep review / monthly archive.
    Weekly,
}

/// Risk class of letting this capability act autonomously.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Automatic.
    Low,
    /// Suggest only.
    Medium,
    /// Ask first.
    High,
    /// Never without separate explicit authorization.
    Forbidden,
}

/// Which timing layer (see the timing model) a capability belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    Instant,
    FastBrain,
    Ghost,
    DeepBackground,
    Daily,
    LongTerm,
}

/// One capability's static definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDef {
    pub id: &'static str,
    pub name: &'static str,
    pub trigger: Trigger,
    /// Default cadence in seconds for interval-ish triggers (0 = event-only).
    pub default_interval_secs: u64,
    pub layer: Layer,
    pub risk: RiskLevel,
    /// 1–5 relative resource cost (cpu/mem/io blended).
    pub resource_cost: u8,
    /// 1–5 privacy sensitivity.
    pub privacy_sensitivity: u8,
    /// Normalized value multiplier (≥ 1.0), used by the run_score engine.
    pub value_weight: f32,
    /// Other capability ids this one depends on (serialized as owned vec).
    #[serde(skip_deserializing, default = "empty_deps")]
    pub depends_on: &'static [&'static str],
    pub goal: &'static str,
}

fn empty_deps() -> &'static [&'static str] {
    &[]
}

/// The full manifest. Order = display order.
pub const MANIFEST: &[CapabilityDef] = &[
    // ── Layer 1: instant, event-driven ───────────────────────────────
    CapabilityDef {
        id: "privacy_gate",
        name: "Privacy Policy Gate",
        trigger: Trigger::Event,
        default_interval_secs: 0,
        layer: Layer::Instant,
        risk: RiskLevel::Low,
        resource_cost: 1,
        privacy_sensitivity: 5,
        value_weight: 2.0,
        depends_on: &[],
        goal: "Every capture passes privacy checks before storage",
    },
    CapabilityDef {
        id: "clipboard_capture",
        name: "Clipboard Capture",
        trigger: Trigger::Event,
        default_interval_secs: 0,
        layer: Layer::Instant,
        risk: RiskLevel::Low,
        resource_cost: 1,
        privacy_sensitivity: 4,
        value_weight: 1.5,
        depends_on: &["privacy_gate"],
        goal: "Remember useful copied items, never secrets",
    },
    CapabilityDef {
        id: "ambient_events",
        name: "Ambient Events",
        trigger: Trigger::Event,
        default_interval_secs: 0,
        layer: Layer::Instant,
        risk: RiskLevel::Low,
        resource_cost: 1,
        privacy_sensitivity: 3,
        value_weight: 1.2,
        depends_on: &["privacy_gate"],
        goal: "Capture meaningful system/user events",
    },
    CapabilityDef {
        id: "search_indexing",
        name: "Search / FTS Indexing",
        trigger: Trigger::Debounced,
        default_interval_secs: 5,
        layer: Layer::Instant,
        risk: RiskLevel::Low,
        resource_cost: 2,
        privacy_sensitivity: 2,
        value_weight: 1.4,
        depends_on: &[],
        goal: "New memories searchable immediately",
    },
    CapabilityDef {
        id: "file_code_watch",
        name: "Code / AST Analysis",
        trigger: Trigger::Debounced,
        default_interval_secs: 20,
        layer: Layer::Instant,
        risk: RiskLevel::Low,
        resource_cost: 3,
        privacy_sensitivity: 3,
        value_weight: 1.3,
        depends_on: &["privacy_gate"],
        goal: "Understand code changes without scanning constantly",
    },
    CapabilityDef {
        id: "workspace_state",
        name: "Workspace State",
        trigger: Trigger::Interval,
        default_interval_secs: 180,
        layer: Layer::FastBrain,
        risk: RiskLevel::Low,
        resource_cost: 2,
        privacy_sensitivity: 3,
        value_weight: 1.5,
        depends_on: &[],
        goal: "Understand active project/app/window",
    },
    // ── Layer 2: fast brain ─────────────────────────────────────────
    CapabilityDef {
        id: "world_state",
        name: "Autonomy World-State",
        trigger: Trigger::Interval,
        default_interval_secs: 45,
        layer: Layer::FastBrain,
        risk: RiskLevel::Low,
        resource_cost: 1,
        privacy_sensitivity: 3,
        value_weight: 2.0,
        depends_on: &[],
        goal: "Maintain live model of the current situation",
    },
    CapabilityDef {
        id: "session_track",
        name: "Session Tracking",
        trigger: Trigger::Session,
        default_interval_secs: 0,
        layer: Layer::FastBrain,
        risk: RiskLevel::Low,
        resource_cost: 1,
        privacy_sensitivity: 2,
        value_weight: 1.3,
        depends_on: &[],
        goal: "Know when a working session starts and ends",
    },
    // ── Layer 3: ghost / insight ────────────────────────────────────
    CapabilityDef {
        id: "ghost_insights",
        name: "Ghost Insight Engine",
        trigger: Trigger::Interval,
        default_interval_secs: 300,
        layer: Layer::Ghost,
        risk: RiskLevel::Low,
        resource_cost: 3,
        privacy_sensitivity: 2,
        value_weight: 1.4,
        depends_on: &["search_indexing"],
        goal: "Connect events, detect patterns, surface insights",
    },
    CapabilityDef {
        id: "freeze_resume",
        name: "Freeze / Resume Analysis",
        trigger: Trigger::Session,
        default_interval_secs: 0,
        layer: Layer::Ghost,
        risk: RiskLevel::Low,
        resource_cost: 2,
        privacy_sensitivity: 2,
        value_weight: 1.6,
        depends_on: &["workspace_state"],
        goal: "Capture what you were doing before pause, help resume later",
    },
    CapabilityDef {
        id: "planner_tasks",
        name: "Tasks & Planner",
        trigger: Trigger::Interval,
        default_interval_secs: 900,
        layer: Layer::Ghost,
        risk: RiskLevel::Medium,
        resource_cost: 2,
        privacy_sensitivity: 1,
        value_weight: 1.3,
        depends_on: &[],
        goal: "Maintain open loops, suggest next actions",
    },
    CapabilityDef {
        id: "screen_memory",
        name: "Screen Memory",
        trigger: Trigger::Interval,
        default_interval_secs: 60,
        layer: Layer::Ghost,
        risk: RiskLevel::Medium,
        resource_cost: 4,
        privacy_sensitivity: 5,
        value_weight: 1.2,
        depends_on: &["privacy_gate"],
        goal: "Adaptive lightweight screen context, privacy-aware",
    },
    CapabilityDef {
        id: "ocr_queue",
        name: "OCR / Image Analysis",
        trigger: Trigger::Debounced,
        default_interval_secs: 15,
        layer: Layer::DeepBackground,
        risk: RiskLevel::Low,
        resource_cost: 5,
        privacy_sensitivity: 5,
        value_weight: 1.0,
        depends_on: &["privacy_gate", "screen_memory"],
        goal: "Extract text from screenshots in a cancelable queue",
    },
    // ── Layer 4: deep background (idle) ─────────────────────────────
    CapabilityDef {
        id: "knowledge_tree",
        name: "Knowledge Tree Organization",
        trigger: Trigger::Idle,
        default_interval_secs: 900,
        layer: Layer::DeepBackground,
        risk: RiskLevel::Low,
        resource_cost: 3,
        privacy_sensitivity: 2,
        value_weight: 1.1,
        depends_on: &["search_indexing"],
        goal: "Organize memories/projects/topics into useful structure",
    },
    CapabilityDef {
        id: "project_brain",
        name: "Project Brain",
        trigger: Trigger::Idle,
        default_interval_secs: 1800,
        layer: Layer::DeepBackground,
        risk: RiskLevel::Low,
        resource_cost: 3,
        privacy_sensitivity: 2,
        value_weight: 1.5,
        depends_on: &["workspace_state"],
        goal: "Per-project identity, tasks, errors, next actions",
    },
    CapabilityDef {
        id: "health_lens",
        name: "System Health Lens",
        trigger: Trigger::Interval,
        default_interval_secs: 300,
        layer: Layer::DeepBackground,
        risk: RiskLevel::Low,
        resource_cost: 2,
        privacy_sensitivity: 1,
        value_weight: 1.0,
        depends_on: &[],
        goal: "Lightweight system health awareness",
    },
    CapabilityDef {
        id: "wellness",
        name: "Wellness Awareness",
        trigger: Trigger::Interval,
        default_interval_secs: 600,
        layer: Layer::Ghost,
        risk: RiskLevel::Low,
        resource_cost: 1,
        privacy_sensitivity: 2,
        value_weight: 1.2,
        depends_on: &[],
        goal: "Breaks and hydration nudges, non-intrusive",
    },
    // ── Layer 5/6: daily + long-term ───────────────────────────────
    CapabilityDef {
        id: "daily_capsule",
        name: "Daily Summary Capsule",
        trigger: Trigger::Daily,
        default_interval_secs: 0,
        layer: Layer::Daily,
        risk: RiskLevel::Low,
        resource_cost: 3,
        privacy_sensitivity: 2,
        value_weight: 1.8,
        depends_on: &["ghost_insights", "planner_tasks"],
        goal: "One concise Daily Memory Capsule per day",
    },
    CapabilityDef {
        id: "storage_cleanup",
        name: "Storage Cleanup Analysis",
        trigger: Trigger::Daily,
        default_interval_secs: 0,
        layer: Layer::LongTerm,
        risk: RiskLevel::Medium,
        resource_cost: 3,
        privacy_sensitivity: 1,
        value_weight: 0.8,
        depends_on: &[],
        goal: "Analyze redundancy; never delete without approval",
    },
    CapabilityDef {
        id: "pattern_learning",
        name: "Pattern Learning",
        trigger: Trigger::Idle,
        default_interval_secs: 3600,
        layer: Layer::LongTerm,
        risk: RiskLevel::Low,
        resource_cost: 4,
        privacy_sensitivity: 3,
        value_weight: 1.2,
        depends_on: &["ghost_insights"],
        goal: "Durable patterns, weekly review, monthly archive",
    },
];

/// Manifest lookup helper.
pub fn def(id: &str) -> Option<&'static CapabilityDef> {
    MANIFEST.iter().find(|c| c.id == id)
}

/// One capability's live state after applying DB overrides.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityState {
    #[serde(flatten)]
    pub def: CapabilityDef,
    /// Effective after user/adaptive overrides.
    pub enabled: bool,
    pub interval_secs: u64,
    /// Last reason an override was applied (from capability_state).
    pub override_reason: Option<String>,
}

/// The registry: manifest + overrides + persistence helpers.
pub struct Registry;

impl Registry {
    /// Load all capabilities with user overrides from `capability_state`.
    pub fn load(conn: &rusqlite::Connection) -> Result<Vec<CapabilityState>, String> {
        let mut overrides: HashMap<String, (bool, Option<u64>, Option<String>)> = HashMap::new();
        {
            let mut stmt = conn
                .prepare("SELECT capability_id, enabled, interval_secs, changed_reason FROM capability_state")
                .map_err(|e| format!("capability_state: {e}"))?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)? != 0,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                })
                .map_err(|e| format!("capability_state: {e}"))?;
            for row in rows {
                let (id, enabled, interval, reason) = row.map_err(|e| e.to_string())?;
                overrides.insert(
                    id,
                    (enabled, interval.map(|i| i.max(0) as u64), reason),
                );
            }
        }

        let out = MANIFEST
            .iter()
            .map(|d| {
                let (enabled, interval, reason) = overrides
                    .get(d.id)
                    .cloned()
                    .unwrap_or((true, None, None));
                CapabilityState {
                    def: d.clone(),
                    enabled,
                    interval_secs: interval.unwrap_or(d.default_interval_secs),
                    override_reason: reason,
                }
            })
            .collect();
        Ok(out)
    }

    /// Persist a user override (upsert).
    pub fn set_enabled(
        conn: &rusqlite::Connection,
        capability_id: &str,
        enabled: bool,
        reason: &str,
    ) -> Result<(), String> {
        conn.execute(
            "INSERT INTO capability_state(capability_id, enabled, changed_reason, changed_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(capability_id) DO UPDATE SET
                enabled = excluded.enabled,
                changed_reason = excluded.changed_reason,
                changed_at = excluded.changed_at",
            rusqlite::params![
                capability_id,
                enabled as i64,
                reason,
                crate::db::now_iso()
            ],
        )
        .map_err(|e| format!("save capability_state: {e}"))?;
        Ok(())
    }

    /// Persist an interval override (upsert).
    pub fn set_interval(
        conn: &rusqlite::Connection,
        capability_id: &str,
        interval_secs: u64,
        reason: &str,
    ) -> Result<(), String> {
        conn.execute(
            "INSERT INTO capability_state(capability_id, enabled, interval_secs, changed_reason, changed_at)
             VALUES(?1, 1, ?2, ?3, ?4)
             ON CONFLICT(capability_id) DO UPDATE SET
                interval_secs = excluded.interval_secs,
                changed_reason = excluded.changed_reason,
                changed_at = excluded.changed_at",
            rusqlite::params![
                capability_id,
                interval_secs as i64,
                reason,
                crate::db::now_iso()
            ],
        )
        .map_err(|e| format!("save capability_state: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    #[test]
    fn manifest_has_20_unique_ids() {
        assert_eq!(MANIFEST.len(), 20, "spec: 20 capabilities");
        let mut seen = std::collections::HashSet::new();
        for c in MANIFEST {
            assert!(seen.insert(c.id), "duplicate id: {}", c.id);
        }
    }

    #[test]
    fn dependencies_all_resolve() {
        for c in MANIFEST {
            for dep in c.depends_on {
                assert!(def(dep).is_some(), "{} depends on unknown {dep}", c.id);
            }
        }
    }

    #[test]
    fn load_returns_defaults_on_empty_table() {
        let conn = setup();
        let all = Registry::load(&conn).unwrap();
        assert_eq!(all.len(), 20);
        assert!(all.iter().all(|c| c.enabled));
        assert!(all
            .iter()
            .all(|c| c.interval_secs == c.def.default_interval_secs));
    }

    #[test]
    fn overrides_are_applied_and_persisted() {
        let conn = setup();
        Registry::set_enabled(&conn, "screen_memory", false, "user opted out").unwrap();
        Registry::set_interval(&conn, "ghost_insights", 600, "adaptive: low value").unwrap();

        let all = Registry::load(&conn).unwrap();
        let sm = all.iter().find(|c| c.def.id == "screen_memory").unwrap();
        assert!(!sm.enabled);
        assert_eq!(sm.override_reason.as_deref(), Some("user opted out"));

        let gi = all.iter().find(|c| c.def.id == "ghost_insights").unwrap();
        assert_eq!(gi.interval_secs, 600);
        assert!(gi.enabled, "set_interval must not disable");
    }

    #[test]
    fn forbidden_risk_never_in_automatic_layers() {
        // Sanity: nothing in the manifest is Forbidden today — the class
        // exists for user-authorization-gated future capabilities.
        assert!(MANIFEST.iter().all(|c| c.risk != RiskLevel::Forbidden));
    }
}
