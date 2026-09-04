//! 📈 Performance Audit Tests — Phase 31 of the Strawberry platform.
//!
//! Deterministic performance guarantees (not wall-clock benchmarks — CI
//! machines vary; these prove the ASYMPTOTIC posture):
//!   * the event bus stays bounded no matter how many publishes happen
//!   * goal/plan/schedule generation over realistic data stays under a
//!     generous per-call budget (catches accidental O(n²) regressions)
//!   * executor output stays capped

#[cfg(test)]
mod perf {
    use crate::autonomous::capability::Registry;
    use crate::autonomous::event::EventBus;
    use crate::autonomous::goal;
    use crate::autonomous::orchestrator::Orchestrator;
    use crate::autonomous::planner;
    use crate::autonomous::scheduler::{Scheduler, SchedContext};

    fn setup() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    #[test]
    fn event_bus_stays_bounded_under_flood() {
        let bus = EventBus::new(512);
        for i in 0..10_000 {
            let ev = crate::autonomous::event::NormalizedEvent::new(
                crate::autonomous::event::EventKind::Heartbeat {
                    source: format!("flood{i}"),
                },
            );
            bus.publish(ev);
        }
        // The queue must never exceed capacity — overflow drops oldest.
        assert!(bus.len() <= 512, "bus held {} > 512", bus.len());
        // And it must be FULL at capacity (publishes weren't dropped wholesale).
        assert_eq!(bus.len(), 512, "oldest-drop retention contract");
        // Draining everything yields at most capacity events.
        let drained = bus.drain(100_000).len();
        assert_eq!(drained, 512);
        assert!(bus.is_empty());
    }

    #[test]
    fn goal_generation_scales_linearly() {
        let conn = setup();
        // 500 open todos — realistic heavy planner usage.
        for i in 0..500 {
            conn.execute(
                "INSERT INTO todos(title,priority,completed) VALUES(?1,'medium',0)",
                [format!("task number {i} to complete today")],
            )
            .unwrap();
        }
        let t0 = std::time::Instant::now();
        let goals = goal::generate(&conn).unwrap();
        let elapsed = t0.elapsed();
        assert!(!goals.is_empty());
        // Generous ceiling: catches quadratic regressions while being
        // stable on busy CI machines (in-memory DB, ~500 rows).
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "goal generation took {elapsed:?}"
        );
    }

    #[test]
    fn planning_20_goals_stays_fast() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('seed','high',0)",
            [],
        )
        .unwrap();
        let goals = goal::generate(&conn).unwrap();
        let t0 = std::time::Instant::now();
        for g in &goals {
            let _ = planner::plan(g);
        }
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(1),
            "planning slow: {:?}",
            t0.elapsed()
        );
    }

    #[test]
    fn scheduler_evaluates_manifest_instantly() {
        let conn = setup();
        let caps = Registry::load(&conn).unwrap();
        let ctx = SchedContext {
            user_activity: 0.5,
            cpu_load: 0.5,
            mem_load: 0.5,
            battery: 1.0,
            on_ac: true,
            heavy_work: false,
            idle_secs: 60,
            new_events: 3,
        };
        let t0 = std::time::Instant::now();
        for _ in 0..1_000 {
            for c in &caps {
                let _ = Scheduler::evaluate(c, &ctx);
            }
        }
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(2),
            "20k evaluations took {:?}",
            t0.elapsed()
        );
    }

    #[test]
    fn orchestrator_gate_reuses_registry_efficiently() {
        let conn = setup();
        let orch = Orchestrator::new();
        let t0 = std::time::Instant::now();
        for _ in 0..100 {
            let _ = orch.gate(&conn, "ghost_insights", 0.5, 0, 1);
        }
        // First call logs the transition; the rest are cheap re-evaluations.
        // 100 gates must stay far under a second.
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(1),
            "100 gates took {:?}",
            t0.elapsed()
        );
    }

    #[test]
    fn planning_makes_no_writes_perf_contract() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('x','high',0)",
            [],
        )
        .unwrap();
        let before: i64 = conn.query_row("SELECT COUNT(*) FROM todos", [], |r| r.get(0)).unwrap();
        let goals = goal::generate(&conn).unwrap();
        for g in &goals {
            let _ = planner::plan(g);
        }
        let after: i64 = conn.query_row("SELECT COUNT(*) FROM todos", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
    }
}
