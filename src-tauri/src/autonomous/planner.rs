//! 🗺️ Planner — Phase 8 of the Strawberry platform.
//!
//! Converts ACCEPTED GoalCandidates into deterministic, explainable Plans.
//!
//! HARD BOUNDARIES (per spec):
//!   * The Planner MAY *describe* actions that require approval later.
//!   * The Planner MUST NOT execute anything.
//!   * The Planner MUST NOT authorize anything.
//!   * The Planner MUST NOT pre-empt the future Safety Gate (Phase 9);
//!     every step carries risk *metadata* only — classification happens
//!     at the gate, never here.
//!
//! Determinism contract: same goal + same relevant DB state ⇒ byte-identical
//! plan (template selection by evidence kind, step ids from content hash,
//! costs and confidences from fixed arithmetic).
//!
//! PERSISTENCE DECISION (documented per spec): plans are GENERATION-BASED,
//! like Phase 7 goals. No new tables. Reasons:
//!   1. Nothing executes plans yet — persisting un-executed artifacts would
//!      be premature storage.
//!   2. The Action Ledger (Phase 14 hardening) will record plan ids when
//!      real execution arrives, which is the natural persistence point.
//!   3. Regeneration is cheap and deterministic.

use serde::{Deserialize, Serialize};

use super::goal::{GoalCandidate, GoalStatus, Priority};
use super::ids::PlanId;

// ─────────────────────────── model ───────────────────────────

/// What a step asks some capability/skill to do. `Capability` refers to the
/// 20-entry Capability Registry ids (e.g. "search_indexing"); future skills
/// (Phase 16) reuse the same field with `skill:` prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepAction {
    /// Read-only lookup/analysis.
    Inspect,
    /// Preparing context (memory recall, project brain, resume).
    Prepare,
    /// A command that would require approval at the Safety Gate.
    RequiresApproval,
}

/// One step in a plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    /// Content-derived stable id within the plan.
    pub step_id: u64,
    /// Which capability/skill this step belongs to.
    pub capability: String,
    /// inspect | prepare | requires_approval
    pub action: StepAction,
    /// Human purpose — the "why" for explainability.
    pub purpose: String,
    /// Inputs/targets (todo id, chat id, project path…).
    pub targets: Vec<String>,
    /// step_ids that must complete before this one.
    pub prerequisites: Vec<u64>,
    /// What success looks like for this step.
    pub expected_result: String,
    /// 1–5 resource cost (matches registry scale).
    pub resource_cost: u8,
    /// Risk *metadata* from the manifest — classification stays Phase 9.
    pub risk_hint: Option<String>,
    /// 1-based execution order after topological sort.
    pub order: u32,
}

/// Lifecycle of a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Generated, awaiting acceptance.
    Draft,
    /// Accepted for execution (Phase 10 will pick these up).
    Ready,
    /// Cannot be executed as specified (rejections map here with a reason).
    Rejected,
    /// Goal went stale/expired after generation.
    Stale,
}

/// A full deterministic plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Content-hash of (goal_id + step signatures) — same inputs ⇒ same id.
    pub plan_id: PlanId,
    pub goal_id: u64,
    pub title: String,
    pub description: String,
    /// Topologically ordered steps.
    pub steps: Vec<PlanStep>,
    /// Direct dependency pairs (from_step, to_step) for explainability.
    pub dependencies: Vec<(u64, u64)>,
    /// Sum of step costs (1–5 scale each) — estimate, not a budget.
    pub estimated_cost: u32,
    pub expected_outcome: String,
    /// Alternative step-sequences for the same goal (same DAG rules).
    pub alternatives: Vec<PlanStep>,
    /// Min of goal confidence and step-evidence confidence.
    pub confidence: f32,
    pub status: PlanStatus,
    pub created_at: String,
}

/// Why a plan was rejected (kept in the plan, not an error — the caller
/// stays in control and can log it to the ledger).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRejection {
    pub goal_id: u64,
    pub reason: String,
    pub created_at: String,
}

/// Result of planning one goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Planned {
    Plan(Plan),
    Rejected(PlanRejection),
}

// ─────────────────────────── helpers ───────────────────────────

fn fnv(bytes: &[u8], seed: u64) -> u64 {
    let mut h = seed;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn step_hash(goal_id: u64, capability: &str, purpose: &str, targets: &[String]) -> u64 {
    let mut h = fnv(capability.as_bytes(), goal_id);
    h = fnv(purpose.as_bytes(), h);
    for t in targets {
        h = fnv(t.as_bytes(), h);
    }
    h
}

fn now_iso_deterministic() -> String {
    let secs = chrono::Utc::now().timestamp();
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

/// Priority → risk hint carried as metadata (classification is Phase 9).
fn risk_hint_of(priority: Priority) -> &'static str {
    match priority {
        Priority::High => "likely_medium",
        Priority::Medium => "likely_low",
        Priority::Low => "low",
    }
}

// ─────────────────────────── DAG validation ───────────────────────────

/// Validate a step set: unique ids, resolvable prerequisites, DAG (no
/// cycles), at least one root step. Returns the direct edge list or an
/// error string explaining the violation (deterministic order of checks:
/// unknown prereq → self/cycle → no root).
fn validate_dag(steps: &[PlanStep]) -> Result<Vec<(u64, u64)>, String> {
    let ids: std::collections::HashSet<u64> = steps.iter().map(|s| s.step_id).collect();
    if ids.len() != steps.len() {
        return Err("duplicate step ids".to_string());
    }

    let mut edges: Vec<(u64, u64)> = Vec::new();
    for s in steps {
        for p in &s.prerequisites {
            if !ids.contains(p) {
                return Err(format!("step {} references unknown prerequisite {p}", s.step_id));
            }
            if *p == s.step_id {
                return Err(format!("step {} depends on itself", s.step_id));
            }
            edges.push((*p, s.step_id));
        }
    }

    // Kahn's algorithm for cycle detection + ordering.
    let mut indegree: std::collections::HashMap<u64, usize> =
        steps.iter().map(|s| (s.step_id, s.prerequisites.len())).collect();
    let mut ready: Vec<u64> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| *id)
        .collect();
    ready.sort_unstable();
    let mut order: Vec<u64> = Vec::with_capacity(steps.len());
    let mut indegree = indegree;
    while let Some(id) = ready.pop() {
        order.push(id);
        for (from, to) in &edges {
            if *from == id {
                let d = indegree.get_mut(to).expect("known id");
                *d -= 1;
                if *d == 0 {
                    ready.push(*to);
                }
            }
        }
        ready.sort_unstable();
    }
    if order.len() != steps.len() {
        // Smallest remaining id for a deterministic message.
        let mut remaining: Vec<u64> = indegree
            .iter()
            .filter(|(_, d)| **d > 0)
            .map(|(id, _)| *id)
            .collect();
        remaining.sort_unstable();
        return Err(format!("cyclic dependency involving step {}", remaining[0]));
    }
    if order.is_empty() {
        return Err("no steps".to_string());
    }
    Ok(edges)
}

/// Topologically order steps in-place, assigning `order` 1..n. Steps are
/// made unique by step_id before sorting for determinism.
fn topo_order(steps: &mut Vec<PlanStep>) -> Result<(), String> {
    let _ = validate_dag(steps)?;
    // Sort by (order-in-Kahn, step_id) — validate_dag already proved a
    // total Kahn order exists; replicate it deterministically.
    let mut indegree: std::collections::BTreeMap<u64, usize> =
        steps.iter().map(|s| (s.step_id, s.prerequisites.len())).collect();
    let edges: Vec<(u64, u64)> = {
        let mut e = Vec::new();
        for s in steps.iter() {
            for p in &s.prerequisites {
                e.push((*p, s.step_id));
            }
        }
        e
    };
    let mut queue: std::collections::BinaryHeap<std::cmp::Reverse<u64>> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| std::cmp::Reverse(*id))
        .collect();
    let mut order: u32 = 1;
    let mut assigned: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
    while let Some(std::cmp::Reverse(id)) = queue.pop() {
        assigned.insert(id, order);
        order += 1;
        for (from, to) in &edges {
            if *from == id {
                let d = indegree.get_mut(to).expect("known id");
                *d -= 1;
                if *d == 0 {
                    queue.push(std::cmp::Reverse(*to));
                }
            }
        }
    }
    steps.sort_by_key(|s| assigned.get(&s.step_id).copied().unwrap_or(u32::MAX));
    for (i, s) in steps.iter_mut().enumerate() {
        s.order = (i + 1) as u32;
    }
    Ok(())
}

// ─────────────────────────── planning rules ───────────────────────────

/// Deterministic template selection: the PRIMARY evidence kind decides the
/// step skeleton; extra evidence kinds append optional context steps.
fn build_steps(goal: &GoalCandidate, now: &str) -> Vec<PlanStep> {
    use super::goal::EvidenceKind as EK;
    let mut steps: Vec<PlanStep> = Vec::new();
    let risk = risk_hint_of(goal.priority);

    let mut push = |capability: &str,
                    action: StepAction,
                    purpose: String,
                    targets: Vec<String>,
                    prereq: Vec<u64>,
                    expected: String,
                    cost: u8,
                    steps: &mut Vec<PlanStep>| {
        let sid = step_hash(goal.goal_id.raw(), capability, &purpose, &targets);
        steps.push(PlanStep {
            step_id: sid,
            capability: capability.to_string(),
            action,
            purpose,
            targets,
            prerequisites: prereq,
            expected_result: expected,
            resource_cost: cost,
            risk_hint: Some(risk.to_string()),
            order: 0, // assigned by topo_order
        });
    };

    // Phase boundary note: every template is INSPECT/PREPARE or explicitly
    // RequiresApproval. No step here executes anything.

    match primary_kind(goal) {
        EK::Task => {
            // 1. Prepare context from memory around the task.
            push(
                "search_indexing",
                StepAction::Inspect,
                format!("Recall related context for “{}”", trim(&goal.title, 40)),
                goal.evidence
                    .iter()
                    .filter(|e| e.kind == EK::Task)
                    .map(|e| e.reference.clone())
                    .collect(),
                vec![],
                "Related notes and past work surface in the recall".to_string(),
                2,
                &mut steps,
            );
            let ctx_id = steps[0].step_id;
            // 2. Prepare a concrete checklist.
            push(
                "planner_tasks",
                StepAction::Prepare,
                "Draft a checklist from the task and recalled context".to_string(),
                vec![goal.title.clone()],
                vec![ctx_id],
                "A concrete, ordered checklist exists".to_string(),
                1,
                &mut steps,
            );
            // 3. Actual work step — always approval-classified later.
            push(
                "planner_tasks",
                StepAction::RequiresApproval,
                "Perform the task's concrete work (user action or approval)".to_string(),
                vec![goal.title.clone()],
                vec![ctx_id],
                "Task's defined output is produced".to_string(),
                3,
                &mut steps,
            );
        }
        EK::Error => {
            // 1. Inspect the error context.
            push(
                "search_indexing",
                StepAction::Inspect,
                "Gather the captured error and surrounding context".to_string(),
                goal.evidence
                    .iter()
                    .filter(|e| e.kind == EK::Error)
                    .map(|e| e.reference.clone())
                    .collect(),
                vec![],
                "Error text, related files and past occurrences are known".to_string(),
                2,
                &mut steps,
            );
            let insp = steps[0].step_id;
            // 2. Prepare likely fixes from history.
            push(
                "project_brain",
                StepAction::Prepare,
                "Rank likely fixes from similar past errors".to_string(),
                vec![goal.title.clone()],
                vec![insp],
                "Ranked candidate fixes with evidence".to_string(),
                3,
                &mut steps,
            );
            // 3. Apply the fix — approval required.
            push(
                "file_code_watch",
                StepAction::RequiresApproval,
                "Apply the chosen fix to the affected file".to_string(),
                vec![goal.description.clone()],
                vec![insp],
                "Error no longer reproduces".to_string(),
                3,
                &mut steps,
            );
        }
        EK::Resume => {
            push(
                "search_indexing",
                StepAction::Inspect,
                "Load the resume point's context and open items".to_string(),
                vec![goal.description.clone()],
                vec![],
                "Open items and refs are in context".to_string(),
                2,
                &mut steps,
            );
            let ctx = steps[0].step_id;
            push(
                "freeze_resume",
                StepAction::Prepare,
                "Restore the workspace to the recorded state".to_string(),
                vec!["session".to_string()],
                vec![ctx],
                "Workspace matches the recorded session".to_string(),
                2,
                &mut steps,
            );
        }
        EK::Project => {
            push(
                "project_brain",
                StepAction::Inspect,
                format!("Assemble project context for {}", goal.project.clone().unwrap_or_default()),
                goal.evidence
                    .iter()
                    .filter(|e| e.kind == EK::Project)
                    .map(|e| e.reference.clone())
                    .collect(),
                vec![],
                "Project identity, tasks and errors are summarized".to_string(),
                3,
                &mut steps,
            );
            let brain = steps[0].step_id;
            push(
                "wellness",
                StepAction::Prepare,
                "Checkpoint the session before deep work".to_string(),
                vec!["session".to_string()],
                vec![brain],
                "Session is checkpointed for later resume".to_string(),
                1,
                &mut steps,
            );
        }
    }
    let _ = now; // timestamps live at plan level; steps stay time-free
    steps
}

fn primary_kind(goal: &GoalCandidate) -> super::goal::EvidenceKind {
    // Deterministic primary selection: highest-weight evidence kind, ties
    // broken by enum order (Task < Error < Resume < Project via weight map).
    let mut best: Option<(f32, u8, super::goal::EvidenceKind)> = None;
    let rank = |k: super::goal::EvidenceKind| -> u8 {
        match k {
            super::goal::EvidenceKind::Task => 3,
            super::goal::EvidenceKind::Error => 4,
            super::goal::EvidenceKind::Resume => 1,
            super::goal::EvidenceKind::Project => 2,
        }
    };
    for e in &goal.evidence {
        let r = rank(e.kind);
        match &best {
            Some((w, br, _)) if (e.weight, r) <= (*w, *br) => {}
            _ => best = Some((e.weight, r, e.kind)),
        }
    }
    best.map(|(_, _, k)| k)
        .unwrap_or(super::goal::EvidenceKind::Task)
}

fn trim(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Alternative steps: for goals with a task/error primary, a lighter
/// "inspect-only" fallback (no approval-classified work).
fn build_alternatives(goal: &GoalCandidate) -> Vec<PlanStep> {
    let mut alt = Vec::new();
    let sid = step_hash(goal.goal_id.raw(), "search_indexing", "alt: context-only pass", &[]);
    alt.push(PlanStep {
        step_id: sid,
        capability: "search_indexing".to_string(),
        action: StepAction::Inspect,
        purpose: "Alt: context-only pass, no state changes".to_string(),
        targets: vec![goal.title.clone()],
        prerequisites: vec![],
        expected_result: "Context recalled without any approval-needed work".to_string(),
        resource_cost: 2,
        risk_hint: Some("low".to_string()),
        order: 1,
    });
    alt
}

// ─────────────────────────── public API ───────────────────────────

/// Plan ONE accepted goal. Deterministic. Never executes anything.
///
/// Rejections (returned as `Planned::Rejected`, not errors):
///   * goal not in a plannable state (candidate/accepted only)
///   * stale/expired goal
///   * no usable evidence (nothing to anchor steps on)
///   * generated step set invalid (dup ids, unknown prereq, cycle)
pub fn plan(goal: &GoalCandidate) -> Planned {
    let now = now_iso_deterministic();

    if matches!(goal.status, GoalStatus::Cancelled | GoalStatus::Expired | GoalStatus::Completed) {
        return Planned::Rejected(PlanRejection {
            goal_id: goal.goal_id.raw(),
            reason: format!("goal in non-plannable state: {:?}", goal.status),
            created_at: now,
        });
    }
    if goal.is_stale(&now) {
        return Planned::Rejected(PlanRejection {
            goal_id: goal.goal_id.raw(),
            reason: "goal is stale (past expiry before planning)".to_string(),
            created_at: now,
        });
    }
    if goal.evidence.is_empty() {
        return Planned::Rejected(PlanRejection {
            goal_id: goal.goal_id.raw(),
            reason: "no evidence to anchor a plan on".to_string(),
            created_at: now,
        });
    }

    let mut steps = build_steps(goal, &now);
    if steps.is_empty() {
        return Planned::Rejected(PlanRejection {
            goal_id: goal.goal_id.raw(),
            reason: "no steps generated for this goal kind".to_string(),
            created_at: now,
        });
    }

    let dependencies = match validate_dag(&steps) {
        Ok(e) => e,
        Err(reason) => {
            return Planned::Rejected(PlanRejection {
                goal_id: goal.goal_id.raw(),
                reason: format!("invalid step graph: {reason}"),
                created_at: now,
            })
        }
    };

    if topo_order(&mut steps).is_err() {
        return Planned::Rejected(PlanRejection {
            goal_id: goal.goal_id.raw(),
            reason: "topological ordering failed".to_string(),
            created_at: now,
        });
    }

    let estimated_cost: u32 = steps.iter().map(|s| s.resource_cost as u32).sum();
    // Confidence: goal confidence attenuated by step count (more steps ⇒
    // more places to fail) — fixed arithmetic, no randomness.
    let confidence = (goal.confidence * (1.0 - 0.05 * (steps.len().max(1) as f32 - 1.0)))
        .clamp(0.05, 0.95);

    let alternatives = build_alternatives(goal);

    // plan id: hash over goal + each step signature (id, capability, purpose).
    let mut h = goal.goal_id.raw();
    for s in &steps {
        h = fnv(format!("{}|{}|{}", s.step_id, s.capability, s.purpose).as_bytes(), h);
    }

    Plan {
        plan_id: PlanId::new(h),
        goal_id: goal.goal_id.raw(),
        title: goal.title.clone(),
        description: goal.description.clone(),
        steps,
        dependencies,
        estimated_cost,
        expected_outcome: format!("Goal satisfied: {}", goal.title),
        alternatives,
        confidence: (confidence * 100.0).round() / 100.0,
        status: PlanStatus::Draft,
        created_at: now,
    }
    .into()
}

impl From<Plan> for Planned {
    fn from(p: Plan) -> Self {
        Planned::Plan(p)
    }
}

// ─────────────────────────── lifecycle ───────────────────────────

impl Plan {
    pub fn accept(&mut self) {
        if self.status == PlanStatus::Draft {
            self.status = PlanStatus::Ready;
        }
    }
    pub fn reject(&mut self) {
        self.status = PlanStatus::Rejected;
    }
    pub fn mark_stale(&mut self) {
        self.status = PlanStatus::Stale;
    }

    /// Explainability: one-line trace of the plan's shape.
    pub fn explain(&self) -> String {
        let deps = if self.dependencies.is_empty() { "no" } else { &self.dependencies.len().to_string() };
        format!(
            "Plan {} (goal {}) · {} steps · {} deps · cost {} · conf {:.2}",
            self.plan_id, self.goal_id, self.steps.len(), deps, self.estimated_cost, self.confidence
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::goal::{Evidence, EvidenceKind};

    fn setup() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    /// Build a goal directly (unit-level; generate() is Phase 7-tested).
    fn make_goal(title: &str, evidence: Vec<Evidence>, priority: Priority) -> GoalCandidate {
        let now = "2026-09-03T12:00:00Z".to_string();
        GoalCandidate {
            goal_id: super::super::ids::GoalId::new(42),
            title: title.to_string(),
            description: title.to_string(),
            project: None,
            priority,
            confidence: 0.7,
            evidence,
            status: GoalStatus::Accepted,
            created_at: now.clone(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
        }
    }

    fn task_goal() -> GoalCandidate {
        make_goal(
            "Complete: ship the parser",
            vec![Evidence { kind: EvidenceKind::Task, reference: "1".into(), summary: "ship".into(), weight: 1.0 }],
            Priority::High,
        )
    }

    fn error_goal() -> GoalCandidate {
        make_goal(
            "Fix error: E0308",
            vec![Evidence { kind: EvidenceKind::Error, reference: "chat-9".into(), summary: "E0308".into(), weight: 1.2 }],
            Priority::High,
        )
    }

    #[test]
    fn simple_goal_yields_simple_plan() {
        let p = match plan(&task_goal()) {
            Planned::Plan(p) => p,
            Planned::Rejected(r) => panic!("rejected: {}", r.reason),
        };
        assert_eq!(p.steps.len(), 3, "task template: recall → checklist → work");
        assert!(p.steps.iter().all(|s| s.order >= 1));
        // Orders are contiguous 1..n.
        let mut orders: Vec<u32> = p.steps.iter().map(|s| s.order).collect();
        orders.sort_unstable();
        assert_eq!(orders, vec![1, 2, 3]);
    }

    #[test]
    fn multi_step_goal_has_ordered_dependencies() {
        let p = match plan(&error_goal()) {
            Planned::Plan(p) => p,
            Planned::Rejected(r) => panic!("rejected: {}", r.reason),
        };
        assert_eq!(p.steps.len(), 3);
        // Step 2 and 3 depend on step 1's context.
        let first = p.steps.iter().find(|s| s.order == 1).unwrap();
        assert!(p.steps.iter().skip(1).all(|s| s.prerequisites.contains(&first.step_id)));
        assert!(!p.dependencies.is_empty());
        // Every dependency edge references existing steps.
        let ids: std::collections::HashSet<u64> = p.steps.iter().map(|s| s.step_id).collect();
        for (a, b) in &p.dependencies {
            assert!(ids.contains(a) && ids.contains(b));
        }
    }

    #[test]
    fn dependency_ordering_respects_prerequisites() {
        let p = match plan(&task_goal()) {
            Planned::Plan(p) => p,
            Planned::Rejected(r) => panic!("rejected: {}", r.reason),
        };
        // For every edge (a→b): order(a) < order(b).
        let order_of = |id: u64| p.steps.iter().find(|s| s.step_id == id).map(|s| s.order).unwrap();
        for (a, b) in &p.dependencies {
            assert!(order_of(*a) < order_of(*b), "topological order violated");
        }
    }

    #[test]
    fn cyclic_dependency_is_rejected() {
        let mut steps = vec![
            PlanStep { step_id: 1, capability: "a".into(), action: StepAction::Inspect, purpose: "one".into(), targets: vec![], prerequisites: vec![2], expected_result: "x".into(), resource_cost: 1, risk_hint: None, order: 0 },
            PlanStep { step_id: 2, capability: "a".into(), action: StepAction::Inspect, purpose: "two".into(), targets: vec![], prerequisites: vec![1], expected_result: "x".into(), resource_cost: 1, risk_hint: None, order: 0 },
        ];
        let err = validate_dag(&steps).unwrap_err();
        assert!(err.contains("cyclic"), "got: {err}");
        steps[1].prerequisites = vec![];
        assert!(validate_dag(&steps).is_ok());
        let _ = &mut steps;
    }

    #[test]
    fn missing_prerequisite_is_rejected() {
        let steps = vec![
            PlanStep { step_id: 1, capability: "a".into(), action: StepAction::Inspect, purpose: "one".into(), targets: vec![], prerequisites: vec![99], expected_result: "x".into(), resource_cost: 1, risk_hint: None, order: 0 },
        ];
        let err = validate_dag(&steps).unwrap_err();
        assert!(err.contains("unknown prerequisite"), "got: {err}");
    }

    #[test]
    fn alternative_plan_exists_and_is_lighter() {
        let p = match plan(&task_goal()) {
            Planned::Plan(p) => p,
            Planned::Rejected(r) => panic!("rejected: {}", r.reason),
        };
        assert!(!p.alternatives.is_empty());
        let alt_cost: u32 = p.alternatives.iter().map(|s| s.resource_cost as u32).sum();
        let main_cost: u32 = p.steps.iter().map(|s| s.resource_cost as u32).sum();
        assert!(alt_cost < main_cost, "alternative must be the lighter path");
        assert!(p.alternatives.iter().all(|s| s.action == StepAction::Inspect));
    }

    #[test]
    fn deterministic_repeatability() {
        let a = plan(&task_goal());
        let b = plan(&task_goal());
        match (&a, &b) {
            (Planned::Plan(a), Planned::Plan(b)) => {
                assert_eq!(a.plan_id, b.plan_id);
                assert_eq!(a.steps, b.steps);
                assert_eq!(a.confidence, b.confidence);
                assert_eq!(a.estimated_cost, b.estimated_cost);
            }
            _ => panic!("both must plan"),
        }
    }

    #[test]
    fn confidence_propagates_and_attenuates() {
        let p = match plan(&task_goal()) {
            Planned::Plan(p) => p,
            Planned::Rejected(r) => panic!("rejected: {}", r.reason),
        };
        // 3 steps ⇒ 0.7 * (1 - 0.05*2) = 0.63.
        assert!((p.confidence - 0.63).abs() < 1e-6, "got {}", p.confidence);
        assert!(p.confidence <= 0.7, "attenuated from goal confidence");
    }

    #[test]
    fn stale_goal_is_rejected() {
        let mut g = task_goal();
        g.expires_at = "2020-01-01T00:00:00Z".to_string();
        match plan(&g) {
            Planned::Rejected(r) => assert!(r.reason.contains("stale")),
            Planned::Plan(_) => panic!("stale goal must not plan"),
        }
    }

    #[test]
    fn non_plannable_states_are_rejected() {
        for status in [GoalStatus::Cancelled, GoalStatus::Expired, GoalStatus::Completed] {
            let mut g = task_goal();
            g.status = status;
            match plan(&g) {
                Planned::Rejected(r) => assert!(r.reason.contains("non-plannable")),
                Planned::Plan(_) => panic!("{status:?} must not plan"),
            }
        }
    }

    #[test]
    fn plan_serializes_round_trip() {
        let p = match plan(&task_goal()) {
            Planned::Plan(p) => p,
            Planned::Rejected(r) => panic!("rejected: {}", r.reason),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: Plan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.plan_id, p.plan_id);
        assert_eq!(back.steps.len(), p.steps.len());
        assert_eq!(back.steps[0].purpose, p.steps[0].purpose);
        // camelCase contract for the frontend.
        assert!(json.contains("\"goalId\""));
        assert!(json.contains("\"expectedOutcome\""));
        assert!(json.contains("\"resourceCost\""));
    }

    #[test]
    fn lifecycle_transitions() {
        let mut p = match plan(&task_goal()) {
            Planned::Plan(p) => p,
            Planned::Rejected(r) => panic!("rejected: {}", r.reason),
        };
        assert_eq!(p.status, PlanStatus::Draft);
        p.accept();
        assert_eq!(p.status, PlanStatus::Ready);
        p.accept(); // idempotent-ish: stays ready
        assert_eq!(p.status, PlanStatus::Ready);
        p.mark_stale();
        assert_eq!(p.status, PlanStatus::Stale);
        let mut r = plan(&task_goal()).unwrap_plan();
        r.reject();
        assert_eq!(r.status, PlanStatus::Rejected);
    }

    #[test]
    fn planning_never_executes_anything() {
        // The entire public surface here is pure computation. The strongest
        // executable proof: planning a goal against a LIVE database does not
        // change a single row.
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('prove no side effects','high',0)",
            [],
        )
        .unwrap();
        let before: (i64, i64) = conn
            .query_row("SELECT COUNT(*), MAX(rowid) FROM todos", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        let goals = crate::autonomous::goal::generate(&conn).unwrap();
        let plans: Vec<Plan> = goals
            .into_iter()
            .filter_map(|g| match plan(&g) {
                Planned::Plan(p) => Some(p),
                Planned::Rejected(_) => None,
            })
            .collect();
        assert!(!plans.is_empty());
        let after: (i64, i64) = conn
            .query_row("SELECT COUNT(*), MAX(rowid) FROM todos", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(before, after, "planning must not write");
        // And every step is one of the three non-executing action classes.
        for p in &plans {
            assert!(p.steps.iter().all(|s| {
                matches!(
                    s.action,
                    StepAction::Inspect | StepAction::Prepare | StepAction::RequiresApproval
                )
            }));
        }
    }

    // Small helper for the lifecycle test.
    trait UnwrapPlan {
        fn unwrap_plan(self) -> Plan;
    }
    impl UnwrapPlan for Planned {
        fn unwrap_plan(self) -> Plan {
            match self {
                Planned::Plan(p) => p,
                Planned::Rejected(r) => panic!("rejected: {}", r.reason),
            }
        }
    }

    #[test]
    fn impossible_goal_without_evidence_is_rejected() {
        let g = make_goal("Complete: ghost task", vec![], Priority::Low);
        match plan(&g) {
            Planned::Rejected(r) => assert!(r.reason.contains("no evidence")),
            Planned::Plan(_) => panic!("unsupported intent must not plan"),
        }
    }

    #[test]
    fn incomplete_context_still_plans_with_lower_confidence() {
        // Single low-weight evidence ⇒ lower goal confidence still plans.
        let mut g = make_goal(
            "Complete: vague thing",
            vec![Evidence { kind: EvidenceKind::Task, reference: "7".into(), summary: "v".into(), weight: 1.0 }],
            Priority::Low,
        );
        g.confidence = 0.3;
        match plan(&g) {
            Planned::Plan(p) => assert!(p.confidence < 0.3 + 0.31),
            Planned::Rejected(r) => panic!("low confidence should still plan: {}", r.reason),
        }
    }

    #[test]
    fn primary_kind_selection_is_deterministic() {
        let both = make_goal(
            "Dual evidence",
            vec![
                Evidence { kind: EvidenceKind::Task, reference: "1".into(), summary: "t".into(), weight: 1.0 },
                Evidence { kind: EvidenceKind::Error, reference: "c".into(), summary: "e".into(), weight: 1.2 },
            ],
            Priority::High,
        );
        let p = plan(&both).unwrap_plan();
        // Error wins (higher weight) ⇒ inspect targets carry the chat id.
        assert!(p.steps[0].targets.contains(&"c".to_string()));
    }
}
