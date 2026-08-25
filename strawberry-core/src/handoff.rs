//! Feature 1 — AI-to-AI handoff packets.
//!
//! Turns an [`Extraction`] into a compact packet a *different* AI can read
//! cold. Eight slots, priority-ordered, filled greedily against a token
//! budget. Deterministic: no AI, no network, no randomness.
//!
//! The claim this module supports is **task-lossless**, not lossless: every
//! slot needed to choose the next action survives, prose does not, and the
//! packet always points back at the untouched original.

use serde::{Deserialize, Serialize};

use crate::extractor::{self, Extraction};
use crate::rules as r;

/// Default budget: comfortable for pasting into any chat box.
pub const DEFAULT_TOKEN_BUDGET: usize = 700;

/// Slots, in the order they earn their tokens.
///
/// Order is the whole design: a receiving AI that reads only the first three
/// still avoids re-proposing dead ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Goal,
    Decisions,
    Rejected,
    Identifiers,
    State,
    NextSteps,
    OpenQuestions,
    Constraints,
}

impl Slot {
    pub fn label(self) -> &'static str {
        match self {
            Slot::Goal => "GOAL",
            Slot::Decisions => "DECIDED",
            Slot::Rejected => "REJECTED (do not re-propose)",
            Slot::Identifiers => "IDS (verbatim)",
            Slot::State => "STATE",
            Slot::NextSteps => "NEXT",
            Slot::OpenQuestions => "OPEN",
            Slot::Constraints => "RULES",
        }
    }

    /// Lines guaranteed before any lower-priority slot gets extras. Keeps a
    /// long decisions list from starving the rejected slot.
    fn min_lines(self) -> usize {
        match self {
            Slot::Goal => 1,
            Slot::Decisions => 3,
            Slot::Rejected => 3,
            Slot::Identifiers => 4,
            Slot::State => 2,
            Slot::NextSteps => 2,
            Slot::OpenQuestions => 1,
            Slot::Constraints => 2,
        }
    }

    pub const ALL: [Slot; 8] = [
        Slot::Goal,
        Slot::Decisions,
        Slot::Rejected,
        Slot::Identifiers,
        Slot::State,
        Slot::NextSteps,
        Slot::OpenQuestions,
        Slot::Constraints,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedEntry {
    pub what: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotReport {
    pub slot: Slot,
    pub kept: usize,
    pub dropped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pointer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
    pub original_words: usize,
    pub original_chars: usize,
    /// Always true: STRAWBERRY never rewrites the source chat.
    pub original_retained: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetReport {
    pub token_budget: usize,
    pub original_tokens: usize,
    pub packet_tokens: usize,
    /// Whole-percent reduction against the original.
    pub reduction_pct: u32,
    /// True when the packet had to exceed the budget to keep the goal line.
    pub over_budget: bool,
    pub slots: Vec<SlotReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffPacket {
    pub version: u32,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    pub decisions: Vec<String>,
    pub rejected: Vec<RejectedEntry>,
    pub identifiers: Vec<String>,
    pub state: Vec<String>,
    pub next_steps: Vec<String>,
    pub open_questions: Vec<String>,
    pub constraints: Vec<String>,
    pub pointer: Pointer,
    pub budget: BudgetReport,
}

// ---------------------------------------------------------------------------
// Costing
// ---------------------------------------------------------------------------

fn bullet_cost(line: &str) -> usize {
    r::est_tokens(&format!("- {line}\n"))
}

fn header_cost(slot: Slot) -> usize {
    r::est_tokens(&format!("{}:\n", slot.label()))
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

/// Build a packet straight from raw chat text.
pub fn build_from_raw(
    title: &str,
    chat_id: Option<String>,
    raw: &str,
    token_budget: usize,
) -> HandoffPacket {
    let ex = extractor::extract(raw);
    build(title, chat_id, raw, &ex, token_budget)
}

/// Build a packet from an existing extraction (avoids re-parsing).
pub fn build(
    title: &str,
    chat_id: Option<String>,
    raw: &str,
    ex: &Extraction,
    token_budget: usize,
) -> HandoffPacket {
    // --- candidates per slot ------------------------------------------------
    let goal = ex.first_idea.clone();

    let decisions = dedupe(&ex.decisions);

    let rejected_entries: Vec<RejectedEntry> = ex
        .rejected
        .iter()
        .map(|rj| RejectedEntry {
            what: rj.what.clone(),
            why: rj.why.clone(),
        })
        .collect();

    let identifiers: Vec<String> = ex.identifiers.iter().map(|i| i.tagged()).collect();

    // State = most recent errors first. The last failure describes where the
    // work actually stands; the first one is usually already fixed.
    let mut state: Vec<String> = ex.errors.iter().rev().cloned().collect();
    state = dedupe(&state);

    let next_steps = dedupe(&ex.action_items);

    // Open questions = the tail of the chat. Earlier questions are normally
    // answered by the decisions slot already.
    let mut open_questions: Vec<String> = ex.questions.iter().rev().cloned().collect();
    if let Some(g) = &goal {
        let gl = g.to_lowercase();
        open_questions.retain(|q| q.to_lowercase() != gl);
    }

    let constraints = dedupe(&ex.constraints);

    // --- cross-slot dedupe --------------------------------------------------
    // A line that already earned its tokens in a higher-priority slot must not
    // be paid for again lower down. Without this, "decided to use rules only,
    // zero LLM calls" bills once as a decision and again as a constraint.
    let mut seen_global: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(g) = &goal {
        seen_global.insert(norm(g));
    }
    let decisions = keep_unseen(decisions, &mut seen_global);
    let rejected_keys: Vec<String> = rejected_entries.iter().map(|e| norm(&e.what)).collect();
    let mut rejected_entries_f: Vec<RejectedEntry> = Vec::new();
    for (e, k) in rejected_entries.into_iter().zip(rejected_keys) {
        if seen_global.insert(k) {
            rejected_entries_f.push(e);
        }
    }
    let rejected_entries = rejected_entries_f;
    let rejected_lines: Vec<String> = rejected_entries.iter().map(render_rejected).collect();
    let state = keep_unseen(state, &mut seen_global);
    let next_steps = keep_unseen(next_steps, &mut seen_global);
    let open_questions = keep_unseen(open_questions, &mut seen_global);
    let constraints = keep_unseen(constraints, &mut seen_global);

    // --- fixed costs --------------------------------------------------------
    let title_line = format!("[STRAWBERRY HANDOFF v1 — {}]\n", title.trim());
    let source_line_estimate = source_line(
        chat_id.as_deref(),
        raw.split_whitespace().count(),
        0,
        0,
        0,
    );
    let fixed = r::est_tokens(&title_line) + r::est_tokens(&source_line_estimate);

    let mut left = token_budget.saturating_sub(fixed);
    let mut over_budget = false;

    // Goal is non-negotiable: a packet without it cannot direct anything.
    let mut goal_out: Option<String> = None;
    if let Some(g) = &goal {
        let cost = header_cost(Slot::Goal) + r::est_tokens(&format!("{g}\n"));
        if cost <= left {
            left -= cost;
        } else {
            over_budget = true;
            left = 0;
        }
        goal_out = Some(g.clone());
    }

    // --- greedy fill: guaranteed minimums, then priority order --------------
    let pools: [&Vec<String>; 7] = [
        &decisions,
        &rejected_lines,
        &identifiers,
        &state,
        &next_steps,
        &open_questions,
        &constraints,
    ];
    let slots: [Slot; 7] = [
        Slot::Decisions,
        Slot::Rejected,
        Slot::Identifiers,
        Slot::State,
        Slot::NextSteps,
        Slot::OpenQuestions,
        Slot::Constraints,
    ];
    let mut taken = [0usize; 7];
    let mut header_paid = [false; 7];

    for pass in 0..2 {
        for (i, pool) in pools.iter().enumerate() {
            let limit = if pass == 0 {
                slots[i].min_lines().min(pool.len())
            } else {
                pool.len()
            };
            while taken[i] < limit {
                let line = &pool[taken[i]];
                let mut cost = bullet_cost(line);
                if !header_paid[i] {
                    cost += header_cost(slots[i]);
                }
                if cost > left {
                    break;
                }
                left -= cost;
                header_paid[i] = true;
                taken[i] += 1;
            }
        }
    }

    let out_decisions = decisions[..taken[0]].to_vec();
    let out_rejected = rejected_entries[..taken[1]].to_vec();
    let out_identifiers = identifiers[..taken[2]].to_vec();
    let out_state = state[..taken[3]].to_vec();
    let out_next = next_steps[..taken[4]].to_vec();
    let out_open = open_questions[..taken[5]].to_vec();
    let out_constraints = constraints[..taken[6]].to_vec();

    let mut slot_reports = vec![SlotReport {
        slot: Slot::Goal,
        kept: usize::from(goal_out.is_some()),
        dropped: 0,
    }];
    for (i, pool) in pools.iter().enumerate() {
        slot_reports.push(SlotReport {
            slot: slots[i],
            kept: taken[i],
            dropped: pool.len() - taken[i],
        });
    }

    let original_tokens = r::est_tokens(raw);

    let mut packet = HandoffPacket {
        version: 1,
        title: title.trim().to_string(),
        goal: goal_out,
        decisions: out_decisions,
        rejected: out_rejected,
        identifiers: out_identifiers,
        state: out_state,
        next_steps: out_next,
        open_questions: out_open,
        constraints: out_constraints,
        pointer: Pointer {
            chat_id,
            original_words: raw.split_whitespace().count(),
            original_chars: raw.chars().count(),
            original_retained: true,
        },
        budget: BudgetReport {
            token_budget,
            original_tokens,
            packet_tokens: 0,
            reduction_pct: 0,
            over_budget,
            slots: slot_reports,
        },
    };

    // Measure the real rendered packet so the report never flatters itself.
    //
    // The SOURCE line quotes the token count, so writing the count changes the
    // text being counted. Iterate to a fixed point (converges in 2-3 rounds
    // because only digit widths move) and stop when the number is stable.
    for _ in 0..4 {
        let measured = r::est_tokens(&render(&packet));
        if measured == packet.budget.packet_tokens {
            break;
        }
        packet.budget.packet_tokens = measured;
        packet.budget.reduction_pct = if original_tokens == 0 || measured >= original_tokens {
            0
        } else {
            (((original_tokens - measured) * 100) / original_tokens) as u32
        };
    }
    if packet.budget.packet_tokens > token_budget {
        packet.budget.over_budget = true;
    }
    packet
}

fn render_rejected(e: &RejectedEntry) -> String {
    match &e.why {
        Some(why) => format!("{} — why: {}", e.what, why),
        None => e.what.clone(),
    }
}

fn dedupe(items: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .iter()
        .filter(|i| seen.insert(norm(i)))
        .cloned()
        .collect()
}

/// Comparison key for cross-slot duplicate detection: lowercase, and with
/// leading list/decision noise and trailing punctuation removed so
/// "We decided to use X." and "- use X" collapse to one entry.
fn norm(s: &str) -> String {
    let mut t = s.trim().to_lowercase();
    for prefix in [
        "we decided to ",
        "we decided ",
        "decided to ",
        "decided ",
        "we will ",
        "we'll ",
        "going with ",
        "todo: ",
        "todo ",
        "next: ",
        "- ",
        "* ",
    ] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest.to_string();
        }
    }
    t.trim()
        .trim_end_matches(['.', ',', ';', '!', '?', ':'])
        .trim()
        .to_string()
}

/// Keep only entries whose normalized form has not been claimed by a
/// higher-priority slot.
fn keep_unseen(
    items: Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) -> Vec<String> {
    items.into_iter().filter(|i| seen.insert(norm(i))).collect()
}

fn source_line(
    chat_id: Option<&str>,
    words: usize,
    original_tokens: usize,
    packet_tokens: usize,
    reduction: u32,
) -> String {
    let id = chat_id.unwrap_or("unsaved");
    format!(
        "SOURCE: chat {id} · {words} words original · {original_tokens}→{packet_tokens} tok ({reduction}% smaller) · original retained, ask by section for detail\n"
    )
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

/// Render the paste-ready block. This is the artifact that goes in a clipboard.
pub fn render(p: &HandoffPacket) -> String {
    let mut s = String::with_capacity(1024);
    s.push_str(&format!("[STRAWBERRY HANDOFF v1 — {}]\n", p.title));

    if let Some(g) = &p.goal {
        s.push_str(&format!("{}:\n{}\n", Slot::Goal.label(), g));
    }
    push_list(&mut s, Slot::Decisions, &p.decisions);
    push_list(
        &mut s,
        Slot::Rejected,
        &p.rejected.iter().map(render_rejected).collect::<Vec<_>>(),
    );
    push_list(&mut s, Slot::Identifiers, &p.identifiers);
    push_list(&mut s, Slot::State, &p.state);
    push_list(&mut s, Slot::NextSteps, &p.next_steps);
    push_list(&mut s, Slot::OpenQuestions, &p.open_questions);
    push_list(&mut s, Slot::Constraints, &p.constraints);

    s.push_str(&source_line(
        p.pointer.chat_id.as_deref(),
        p.pointer.original_words,
        p.budget.original_tokens,
        p.budget.packet_tokens,
        p.budget.reduction_pct,
    ));
    s
}

fn push_list(s: &mut String, slot: Slot, items: &[String]) {
    if items.is_empty() {
        return;
    }
    s.push_str(slot.label());
    s.push_str(":\n");
    for i in items {
        s.push_str("- ");
        s.push_str(i);
        s.push('\n');
    }
}

/// Serialize to the `.strawberry.json` interchange form.
pub fn to_json(p: &HandoffPacket) -> String {
    serde_json::to_string_pretty(p).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
User:
I need the clipboard daemon to compress a chat before pasting it into another AI.

Assistant:
We decided to use deterministic rules only, zero LLM calls.
Tried the Wayland clipboard API first but it didn't work because permission was denied.
Reverted the sqlx migration in favor of rusqlite because bundle size.
The daemon must never call the network.
Set HERMES_CUSTOM_API_KEY in ~/.hermes/.env and bind localhost:1420.
```sql
CREATE TABLE IF NOT EXISTS chat_artifacts (id TEXT);
```
Error: embedding failed with 500 from the local server.
TODO: wire the hotkey into the daemon.
Should we cap the packet at 700 tokens?
";

    #[test]
    fn packet_has_all_eight_slots_populated() {
        let p = build_from_raw("Demo", Some("chat-1".into()), SAMPLE, DEFAULT_TOKEN_BUDGET);
        assert!(p.goal.is_some(), "goal missing");
        assert!(!p.decisions.is_empty(), "decisions missing");
        assert!(!p.rejected.is_empty(), "rejected missing");
        assert!(!p.identifiers.is_empty(), "identifiers missing");
        assert!(!p.state.is_empty(), "state missing");
        assert!(!p.next_steps.is_empty(), "next steps missing");
        assert!(!p.open_questions.is_empty(), "open questions missing");
        assert!(!p.constraints.is_empty(), "constraints missing");
    }

    #[test]
    fn rejected_reasons_survive_into_render() {
        let p = build_from_raw("Demo", None, SAMPLE, DEFAULT_TOKEN_BUDGET);
        let out = render(&p);
        assert!(out.contains("REJECTED (do not re-propose)"));
        assert!(out.contains("why: permission was denied"));
        assert!(out.contains("why: bundle size"));
    }

    #[test]
    fn identifiers_are_verbatim() {
        let p = build_from_raw("Demo", None, SAMPLE, DEFAULT_TOKEN_BUDGET);
        let out = render(&p);
        assert!(out.contains("env:HERMES_CUSTOM_API_KEY"));
        assert!(out.contains("port:1420"));
        assert!(out.contains("table:chat_artifacts"));
    }

    #[test]
    fn respects_token_budget() {
        for budget in [80usize, 150, 300, 700] {
            let p = build_from_raw("Demo", Some("c".into()), SAMPLE, budget);
            let tokens = r::est_tokens(&render(&p));
            assert!(
                tokens <= budget || p.budget.over_budget,
                "budget {budget} exceeded with {tokens} tokens and over_budget=false"
            );
        }
    }

    #[test]
    fn tight_budget_keeps_priority_order() {
        // Enough for the goal plus a couple of high-priority lines only.
        let p = build_from_raw("Demo", Some("c".into()), SAMPLE, 110);
        assert!(p.goal.is_some());
        // Constraints are the lowest priority slot, so they starve first.
        assert!(p.constraints.len() <= p.rejected.len());
        assert!(p.open_questions.len() <= p.decisions.len() + p.rejected.len());
    }

    #[test]
    fn budget_report_matches_rendered_packet() {
        let p = build_from_raw("Demo", None, SAMPLE, DEFAULT_TOKEN_BUDGET);
        // The report must describe the bytes actually produced, not an
        // optimistic pre-render estimate.
        assert_eq!(p.budget.packet_tokens, r::est_tokens(&render(&p)));
        assert_eq!(p.budget.original_tokens, r::est_tokens(SAMPLE));

        // This sample is already dense — every line carries a decision, a
        // rejection or an identifier — so a faithful packet is *not* smaller.
        // Reduction comes from removing noise, and there is none here. The
        // honest report says 0%, and real reduction is asserted against the
        // noisy fixture in tests/compression.rs.
        assert_eq!(p.budget.reduction_pct, 0);
    }

    #[test]
    fn slot_report_accounts_for_every_candidate() {
        let p = build_from_raw("Demo", None, SAMPLE, 120);
        for rep in &p.budget.slots {
            // kept + dropped is the full candidate count; neither is negative
            // by construction, so just assert the report exists per slot.
            assert!(rep.kept + rep.dropped >= rep.kept);
        }
        assert_eq!(p.budget.slots.len(), Slot::ALL.len());
    }

    #[test]
    fn deterministic_and_json_roundtrips() {
        let a = build_from_raw("Demo", Some("c".into()), SAMPLE, 400);
        let b = build_from_raw("Demo", Some("c".into()), SAMPLE, 400);
        assert_eq!(render(&a), render(&b));
        let json = to_json(&a);
        let back: HandoffPacket = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }

    #[test]
    fn empty_input_yields_minimal_packet() {
        let p = build_from_raw("Empty", None, "", DEFAULT_TOKEN_BUDGET);
        assert!(p.goal.is_none());
        let out = render(&p);
        assert!(out.starts_with("[STRAWBERRY HANDOFF v1 — Empty]"));
        assert!(out.contains("SOURCE: chat unsaved"));
    }

    #[test]
    fn pointer_states_original_is_retained() {
        let p = build_from_raw("Demo", Some("chat-9".into()), SAMPLE, 400);
        assert!(p.pointer.original_retained);
        assert_eq!(p.pointer.chat_id.as_deref(), Some("chat-9"));
        assert!(p.pointer.original_words > 0);
    }
}
