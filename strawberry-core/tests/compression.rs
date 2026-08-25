//! End-to-end compression guarantees against a realistic noisy chat.
//!
//! The unit tests in `handoff.rs` use a dense sample where every line carries
//! signal. This fixture is the opposite and the honest case: a real session
//! full of AI narration, tool-approval banners, filler acknowledgements and
//! repeated build output. That is where compression earns its number.

use strawberry_core::rules::est_tokens;
use strawberry_core::{build_handoff_from_raw, render_handoff, DEFAULT_TOKEN_BUDGET};

const NOISY: &str = include_str!("fixtures/noisy_chat.txt");

#[test]
fn noisy_chat_compresses_substantially() {
    let p = build_handoff_from_raw("Compressor", Some("chat-1".into()), NOISY, DEFAULT_TOKEN_BUDGET);
    let rendered = render_handoff(&p);

    assert!(
        p.budget.reduction_pct >= 60,
        "expected >=60% reduction, got {}% ({} -> {} tok)\n{}",
        p.budget.reduction_pct,
        p.budget.original_tokens,
        p.budget.packet_tokens,
        rendered
    );
    assert_eq!(p.budget.packet_tokens, est_tokens(&rendered));
    assert!(p.budget.packet_tokens <= DEFAULT_TOKEN_BUDGET);
}

#[test]
fn task_lossless_every_critical_fact_survives() {
    let p = build_handoff_from_raw("Compressor", Some("chat-1".into()), NOISY, DEFAULT_TOKEN_BUDGET);
    let out = render_handoff(&p);

    // Rejected approaches with their reasons — the negative knowledge that
    // stops a fresh reader from re-proposing a dead end.
    assert!(out.contains("Wayland clipboard API"), "lost wayland rejection");
    assert!(out.contains("permission was denied"), "lost wayland reason");
    assert!(out.contains("sqlx"), "lost sqlx rejection");
    assert!(
        out.contains("async overhead") || out.contains("overhead"),
        "lost sqlx reason"
    );
    assert!(out.contains("100ms") || out.contains("polling"), "lost polling rejection");
    assert!(out.contains("CPU usage spiked") || out.contains("CPU"), "lost CPU reason");

    // Verbatim identifiers.
    assert!(out.contains("env:HERMES_CUSTOM_API_KEY"), "lost env var");
    assert!(out.contains("port:1420"), "lost port");
    assert!(out.contains("table:chat_artifacts"), "lost table");

    // Constraint and next steps.
    assert!(out.contains("offline") || out.contains("network"), "lost constraint");
    assert!(out.contains("hotkey"), "lost next step");

    // Pointer back to the untouched original.
    assert!(out.contains("chat-1"));
    assert!(out.contains("original retained"));
}

#[test]
fn noise_is_removed() {
    let p = build_handoff_from_raw("Compressor", None, NOISY, DEFAULT_TOKEN_BUDGET);
    let out = render_handoff(&p);

    for noise in [
        "Auto-approved by your global config",
        "Let me check the config file",
        "Let me now start implementing",
        "No problem! Let me continue",
        "thanks bhai",
        "I'll reply in Hinglish",
    ] {
        assert!(
            !out.contains(noise),
            "noise survived compression: {noise:?}\n{out}"
        );
    }
}

#[test]
fn tight_budget_still_keeps_goal_and_rejections() {
    let p = build_handoff_from_raw("Compressor", None, NOISY, 160);
    let out = render_handoff(&p);
    assert!(p.goal.is_some(), "goal dropped under tight budget");
    assert!(!p.rejected.is_empty(), "rejections dropped under tight budget");
    // Lowest-priority slot starves before the highest ones.
    assert!(p.constraints.len() <= p.rejected.len(), "{out}");
}

#[test]
fn deterministic_across_runs() {
    let a = build_handoff_from_raw("C", Some("x".into()), NOISY, 500);
    let b = build_handoff_from_raw("C", Some("x".into()), NOISY, 500);
    assert_eq!(render_handoff(&a), render_handoff(&b));
    assert_eq!(a, b);
}
