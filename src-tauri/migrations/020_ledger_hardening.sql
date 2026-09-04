-- 📜 Phase 14 — Ledger hardening.
--
-- The `details` column already exists (migration 019, line 29) for full
-- provenance JSON (actor, goal, plan, risk, approval, result, verification).
--
-- This migration adds append-only enforcement: autonomous code must never
-- erase or rewrite its own audit history. UPDATE/DELETE raise ABORT.

CREATE TRIGGER IF NOT EXISTS trg_decisions_no_update
BEFORE UPDATE ON autonomy_decisions
BEGIN
    SELECT RAISE(ABORT, 'autonomy_decisions is append-only');
END;

CREATE TRIGGER IF NOT EXISTS trg_decisions_no_delete
BEFORE DELETE ON autonomy_decisions
BEGIN
    SELECT RAISE(ABORT, 'autonomy_decisions is append-only');
END;
