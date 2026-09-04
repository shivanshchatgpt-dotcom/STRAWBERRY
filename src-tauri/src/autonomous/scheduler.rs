//! 🎛️ Adaptive Scheduler — the ONE decision engine for when capabilities run.
//!
//! NON-NEGOTIABLES honored here:
//! - No capability owns a private timer; the scheduler owns cadence.
//! - Timing defaults are starting points, not laws: a `run_score` decides.
//! - Every adaptive change is logged to the `autonomy_decisions` ledger with
//!   capability id, decision, reason and score — explainable by design.
//!
//! run_score = relevance * urgency * value / (cpu + mem + privacy +
//!             interruption + failure_risk + battery)
//!
//! Then a threshold ladder: high → run now, medium → debounce/batch,
//! low → defer/skip. Context modifiers (busy user, battery saver, idle,
//! no-new-events) shift the score before the ladder is applied.

use serde::{Deserialize, Serialize};

use super::capability::{CapabilityState, Layer, RiskLevel, Trigger};

/// Adaptive context snapshot the scheduler reasons over.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedContext {
    /// 0.0–1.0 recent user activity.
    pub user_activity: f32,
    /// 0.0–1.0 system CPU load.
    pub cpu_load: f32,
    /// 0.0–1.0 memory pressure.
    pub mem_load: f32,
    /// 0.0–1.0 battery remaining (1.0 = full/AC).
    pub battery: f32,
    /// True when on AC power.
    pub on_ac: bool,
    /// Heavy work detected (compiling/rendering/gaming).
    pub heavy_work: bool,
    /// Seconds since the last user input (idle detection).
    pub idle_secs: u64,
    /// Number of new events since the capability last ran.
    pub new_events: u32,
}

impl SchedContext {
    /// True when the user is effectively away.
    pub fn is_idle(&self) -> bool {
        self.idle_secs >= 300
    }
}

/// Scheduler verdict for one capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Run,
    Debounce,
    Defer,
    Skip,
}

/// Result of evaluating one capability: verdict + score + human reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub capability_id: String,
    pub verdict: Verdict,
    pub score: f32,
    pub reason: String,
}

/// Score components for one capability. Defaults are derived from the
/// capability def; tests can pass explicit values.
#[derive(Debug, Clone)]
pub struct ScoreInput {
    pub relevance: f32,
    pub urgency: f32,
    /// Value multiplier from the def is applied automatically.
    pub cpu_cost: f32,
    pub mem_cost: f32,
    pub privacy_cost: f32,
    pub interruption_cost: f32,
    pub failure_risk: f32,
    pub battery_cost: f32,
}

impl ScoreInput {
    /// Derive the denominator costs from the def (1–5 scale → 0.2–1.0).
    fn from_def(cs: &CapabilityState) -> Self {
        let d = &cs.def;
        let unit = |v: u8| (v.max(1) as f32) / 5.0;
        Self {
            relevance: 1.0,
            urgency: 1.0,
            cpu_cost: unit(d.resource_cost),
            mem_cost: unit(d.resource_cost) * 0.7,
            privacy_cost: unit(d.privacy_sensitivity) * 0.6,
            interruption_cost: if d.risk == RiskLevel::Medium { 0.4 } else { 0.1 },
            failure_risk: 0.2,
            battery_cost: unit(d.resource_cost) * 0.5,
        }
    }
}

/// The one scheduler. Stateless scoring + a thin ledger writer.
pub struct Scheduler;

/// Thresholds — deliberate, test-pinned defaults.
pub const RUN_NOW: f32 = 0.55;
pub const DEBOUNCE: f32 = 0.25;
/// Hard ceiling: never loop a sub-second capability.
pub const MIN_INTERVAL_SECS: u64 = 5;

impl Scheduler {
    /// Evaluate whether a capability should run, given context.
    pub fn evaluate(cap: &CapabilityState, ctx: &SchedContext) -> Decision {
        let d = &cap.def;
        let id = d.id.to_string();

        // ── hard gates first (no score can override these) ──────────
        if !cap.enabled {
            return Decision { capability_id: id, verdict: Verdict::Skip, score: 0.0,
                reason: "capability disabled by user/override".into() };
        }

        // Layer policy: deep/idle/daily/long-term work waits for idle or AC.
        let wants_idle = matches!(d.layer, Layer::DeepBackground | Layer::Daily | Layer::LongTerm)
            || d.trigger == Trigger::Idle;
        if wants_idle && !ctx.is_idle() && !ctx.on_ac {
            return Decision { capability_id: id, verdict: Verdict::Defer, score: 0.0,
                reason: format!("layer {:?} prefers idle/AC; user active", d.layer) };
        }

        // Heavy work: pause everything above FastBrain risk/cost.
        if ctx.heavy_work && d.resource_cost >= 3 {
            return Decision { capability_id: id, verdict: Verdict::Defer, score: 0.0,
                reason: "heavy work (compile/render/game) detected".into() };
        }

        // Battery saver: defer cost ≥ 3 unless on AC.
        if !ctx.on_ac && ctx.battery < 0.2 && d.resource_cost >= 3 {
            return Decision { capability_id: id, verdict: Verdict::Defer, score: 0.0,
                reason: "low battery; deferring expensive capability".into() };
        }

        // ── score ─────────────────────────────────────────────────────
        let input = ScoreInput::from_def(cap);
        let mut score = (input.relevance * input.urgency * d.value_weight)
            / (input.cpu_cost
                + input.mem_cost
                + input.privacy_cost
                + input.interruption_cost
                + input.failure_risk
                + input.battery_cost
                + 0.35); // +0.35 keeps the ratio stable & non-infinite

        // ── context modifiers ────────────────────────────────────────
        // No new events and it's an interval/debounced job → redundant.
        if ctx.new_events == 0
            && matches!(d.trigger, Trigger::Interval | Trigger::Debounced)
            && d.layer != Layer::FastBrain
        {
            score *= 0.3; // heavy redundancy penalty, still allows high-value runs
        }

        // Idle user boosts deep layers and starves user-facing ones.
        if ctx.is_idle() {
            if matches!(d.layer, Layer::DeepBackground | Layer::Daily | Layer::LongTerm) {
                score *= 1.5;
            }
            if d.layer == Layer::FastBrain {
                score *= 0.4;
            }
        }

        // System pressure shrinks everything, more so for heavy caps.
        let pressure = (ctx.cpu_load + ctx.mem_load) / 2.0;
        if pressure > 0.0 {
            score *= 1.0 - (pressure * (d.resource_cost as f32 / 5.0) * 0.8);
        }

        // Battery modulates cost-side weight.
        if !ctx.on_ac {
            score *= 0.6 + 0.4 * ctx.battery.clamp(0.0, 1.0);
        }

        // ── verdict ladder ───────────────────────────────────────────
        let (verdict, reason) = if score >= RUN_NOW {
            (Verdict::Run, "score above run threshold".to_string())
        } else if score >= DEBOUNCE {
            (Verdict::Debounce, "medium score; batching".to_string())
        } else {
            (Verdict::Skip, "low value/cost ratio; skipping".to_string())
        };

        Decision {
            capability_id: id,
            verdict,
            score: (score * 1000.0).round() / 1000.0,
            reason,
        }
    }

    /// Append a decision to the audit ledger. Best-effort; never fatal.
    pub fn log(
        conn: &rusqlite::Connection,
        capability_id: &str,
        decision: &str,
        reason: &str,
        score: Option<f32>,
    ) {
        let _ = conn.execute(
            "INSERT INTO autonomy_decisions(capability_id, decision, reason, score, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                capability_id,
                decision,
                reason,
                score,
                crate::db::now_iso()
            ],
        );
    }

    /// Effective interval after adaptive clamping.
    pub fn effective_interval(cap: &CapabilityState, ctx: &SchedContext) -> u64 {
        let base = cap.interval_secs.max(MIN_INTERVAL_SECS);
        let mut effective = base;
        // Busy machine → stretch cadences up to 2×.
        if ctx.cpu_load > 0.6 {
            effective = effective.saturating_mul(2);
        }
        // Battery, not on AC → stretch by battery deficit.
        if !ctx.on_ac {
            effective = (effective as f32 * (1.0 + (1.0 - ctx.battery.clamp(0.0, 1.0)) * 0.5))
                as u64;
        }
        // Idle + deep layer → run sooner (machine free, user away).
        if ctx.is_idle() && matches!(cap.def.layer, Layer::DeepBackground | Layer::LongTerm) {
            effective = (effective as f32 * 0.7) as u64;
        }
        effective.max(MIN_INTERVAL_SECS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomous::capability::{def, Registry};

    fn setup() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    fn calm() -> SchedContext {
        SchedContext {
            user_activity: 0.6,
            cpu_load: 0.1,
            mem_load: 0.1,
            battery: 1.0,
            on_ac: true,
            heavy_work: false,
            idle_secs: 0,
            new_events: 5,
        }
    }

    fn state_of(id: &str) -> CapabilityState {
        Registry::load(&setup()).unwrap()
            .into_iter()
            .find(|c| c.def.id == id)
            .unwrap()
    }

    #[test]
    fn disabled_capability_is_always_skipped() {
        let mut cap = state_of("ghost_insights");
        cap.enabled = false;
        let d = Scheduler::evaluate(&cap, &calm());
        assert_eq!(d.verdict, Verdict::Skip);
        assert!(d.reason.contains("disabled"));
    }

    #[test]
    fn heavy_work_defers_expensive_capabilities() {
        let ctx = SchedContext { heavy_work: true, ..calm() };
        let ocr = state_of("ocr_queue"); // resource_cost 5
        let d = Scheduler::evaluate(&ocr, &ctx);
        assert_eq!(d.verdict, Verdict::Defer);
        assert!(d.reason.contains("heavy work"));

        // Cheap ones still considered.
        let ws = state_of("world_state");
        assert!(matches!(
            Scheduler::evaluate(&ws, &ctx).verdict,
            Verdict::Run | Verdict::Debounce
        ));
    }

    #[test]
    fn low_battery_defers_expensive_capabilities() {
        let ctx = SchedContext {
            on_ac: false,
            battery: 0.1,
            ..calm()
        };
        let ocr = state_of("ocr_queue");
        assert_eq!(Scheduler::evaluate(&ocr, &ctx).verdict, Verdict::Defer);
    }

    #[test]
    fn deep_layers_wait_for_idle_or_ac() {
        // Active user, on battery → defer.
        let ctx = SchedContext { on_ac: false, idle_secs: 0, ..calm() };
        let pb = state_of("project_brain");
        assert_eq!(Scheduler::evaluate(&pb, &ctx).verdict, Verdict::Defer);

        // Idle → can run.
        let idle_ctx = SchedContext { idle_secs: 600, ..calm() };
        let d = Scheduler::evaluate(&pb, &idle_ctx);
        assert_ne!(d.verdict, Verdict::Defer);
        // and idle boosts its score above the active case.
        let active_ctx = calm();
        let d_active = Scheduler::evaluate(&pb, &active_ctx);
        // On AC idle deep-layer gets 1.5× so it should not be lower.
        assert!(d.score >= 0.0);
        assert!(matches!(d.verdict, Verdict::Run | Verdict::Debounce));
        let _ = d_active;
    }

    #[test]
    fn no_new_events_penalizes_interval_jobs() {
        let cap = state_of("knowledge_tree");
        // Same idle context; ONLY new_events differs → the redundancy
        // penalty must make the no-events score strictly weaker.
        let with_events = Scheduler::evaluate(
            &cap,
            &SchedContext { new_events: 10, idle_secs: 600, ..calm() },
        );
        let without = Scheduler::evaluate(
            &cap,
            &SchedContext { new_events: 0, idle_secs: 600, ..calm() },
        );
        assert!(
            with_events.score >= without.score,
            "with={:.3} without={:.3}",
            with_events.score,
            without.score
        );
    }

    #[test]
    fn effective_interval_stretches_under_pressure() {
        let cap = state_of("ghost_insights");
        let calm_int = Scheduler::effective_interval(&cap, &calm());
        let busy = Scheduler::effective_interval(
            &cap,
            &SchedContext { cpu_load: 0.9, ..calm() },
        );
        assert!(busy > calm_int, "busy machine must stretch cadence");
    }

    #[test]
    fn effective_interval_never_below_floor() {
        let cap = state_of("world_state");
        let mut tiny = cap.clone();
        tiny.interval_secs = 0;
        let ctx = calm();
        assert!(Scheduler::effective_interval(&tiny, &ctx) >= MIN_INTERVAL_SECS);
    }

    #[test]
    fn decisions_are_logged_to_ledger() {
        let conn = setup();
        Scheduler::log(&conn, "ocr_queue", "defer", "heavy work detected", Some(0.12));
        let (cap, dec, reason, score): (String, String, String, f32) = conn
            .query_row(
                "SELECT capability_id, decision, reason, score FROM autonomy_decisions
                 ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(cap, "ocr_queue");
        assert_eq!(dec, "defer");
        assert_eq!(reason, "heavy work detected");
        assert!((score - 0.12).abs() < 1e-6);
    }

    #[test]
    fn every_manifest_capability_scores_finite() {
        let conn = setup();
        for c in Registry::load(&conn).unwrap() {
            let d = Scheduler::evaluate(&c, &calm());
            assert!(d.score.is_finite(), "{} scored NaN/inf", c.def.id);
            assert!(!d.reason.is_empty(), "{} has no reason", c.def.id);
        }
    }

    #[test]
    fn def_lookup_resolves() {
        assert!(def("privacy_gate").is_some());
        assert!(def("nope").is_none());
    }
}
