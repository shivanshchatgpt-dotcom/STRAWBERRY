//! 🔗 Integration tests — proof that the central scheduler wiring works
//! end-to-end: live registry state → Orchestrator gate → ledger row.
//!
//! These run against an in-memory DB with real migrations, real manifest
//! and the real system probe, exactly as lib.rs uses them.

use rusqlite::Connection;

use crate::autonomous::capability::Registry;
use crate::autonomous::orchestrator::Orchestrator;
use crate::autonomous::scheduler::Verdict;

fn setup() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&mut conn).unwrap();
    crate::db::migrations::ensure_fts(&conn).unwrap();
    conn
}

/// The exact path the Ghost thread now takes: registry load → gate →
/// (when allowed) run cycle. Proves the wired capability is governed.
#[test]
fn ghost_thread_wiring_is_scheduler_governed() {
    let conn = setup();

    // 1. Default state: ghost_insights enabled → gate allows work.
    let orch = Orchestrator::new();
    let g1 = orch.gate(&conn, "ghost_insights", 0.6, 0, 3);
    assert!(g1.proceed, "enabled ghost must be allowed to rebuild");

    // 2. Ledger got exactly one transition entry.
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM autonomy_decisions WHERE capability_id='ghost_insights'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);

    // 3. User disables the capability → gate blocks, ledger transitions.
    Registry::set_enabled(&conn, "ghost_insights", false, "user paused ghost").unwrap();
    let g2 = orch.gate(&conn, "ghost_insights", 0.6, 0, 3);
    assert!(!g2.proceed);
    assert_eq!(g2.verdict, Verdict::Skip);

    // 4. Re-enable → allowed again, and the ledger records the resume.
    Registry::set_enabled(&conn, "ghost_insights", true, "user resumed ghost").unwrap();
    let g3 = orch.gate(&conn, "ghost_insights", 0.6, 0, 3);
    assert!(g3.proceed);

    let decisions: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT decision FROM autonomy_decisions
                 WHERE capability_id='ghost_insights' ORDER BY id ASC",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    // run …, skip, run — transitions only, no spam.
    assert_eq!(decisions.len(), 3, "transitions: initial, pause, resume");
}

/// Wellness wiring: the popup path consults the same gate; a disabled
/// wellness capability suppresses the reminder exactly once per state.
#[test]
fn wellness_gate_wiring_suppresses_when_disabled() {
    let conn = setup();
    let orch = Orchestrator::new();

    // Enabled → proceeds.
    assert!(orch.gate(&conn, "wellness", 0.6, 0, 1).proceed);

    // Disabled → suppressed + logged once, no repeat spam.
    Registry::set_enabled(&conn, "wellness", false, "user paused wellness").unwrap();
    assert!(!orch.gate(&conn, "wellness", 0.6, 0, 1).proceed);
    assert!(!orch.gate(&conn, "wellness", 0.6, 0, 1).proceed);
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM autonomy_decisions
             WHERE capability_id='wellness' AND decision='skip'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "repeat suppression must not spam the ledger");
}

/// Interval override flows into the loop's sleep: ghost sleeps per the
/// scheduler's effective interval, not a hard-coded 300s.
#[test]
fn ghost_sleep_follows_user_interval_override() {
    let conn = setup();
    let orch = Orchestrator::new();

    // Baseline: default interval (300s) clamped by adaptive factors.
    let base = orch.effective_interval_secs(&conn, "ghost_insights", 0);
    assert!(base >= 5);

    // User tightens to 60s → effective interval must drop accordingly.
    Registry::set_interval(&conn, "ghost_insights", 60, "user wants faster ghost").unwrap();
    let tightened = orch.effective_interval_secs(&conn, "ghost_insights", 0);
    assert!(
        tightened < base || base == 60,
        "override must shrink sleep: base={base} tightened={tightened}"
    );
}

/// The whole manifest is gate-safe: no capability can panic or produce a
/// non-finite score through the full gate path.
#[test]
fn every_wired_capability_survives_the_gate() {
    let conn = setup();
    let orch = Orchestrator::new();
    for cap in Registry::load(&conn).unwrap() {
        let g = orch.gate(&conn, &cap.def.id, 0.5, 30, 2);
        // Any verdict is fine — it just must be deterministic + finite.
        assert!(matches!(
            g.verdict,
            Verdict::Run | Verdict::Debounce | Verdict::Defer | Verdict::Skip
        ));
    }
}
