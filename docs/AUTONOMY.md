# 🍓 Strawberry — Autonomy Stack (Phases 7–22)

Actual implementation state. Every claim here is backed by tests in
`src-tauri/src/autonomous/`. Nothing on this page is aspirational.

## Pipeline (implemented, tested end-to-end)

```
EVENT → PRIVACY FILTER → NORMALIZE → STORE → WORLD STATE
      → GOAL (7) → PLAN (8) → SAFETY (9) → EXECUTE (10) → VERIFY (11)
      → REPLAN (12) → LIFECYCLE (13) → LEDGER (14) → LEARN (17) → PREDICT (18)
```

AI (Ollama/BYOK) proposes only; every model output passes the Phase 15
validator; the deterministic core decides (Phase 19 gate).

## Modules

| Phase | Module | What it does | Authority |
|---|---|---|---|
| 7 | `goal.rs` | Evidence-backed GoalCandidates from todos/captured errors/resume/project brain. Content-hash ids, dedup+merge, negation conflicts, priority/confidence arithmetic, expiry. | Never invents intent; no execution. |
| 8 | `planner.rs` | Deterministic plan templates per goal kind; Kahn DAG validation (cycle/unknown-prereq/self rejection); topological step order; alternatives (inspect-only); confidence attenuation per extra step. | Does NOT authorize or execute. |
| 9 | `safety.rs` | The ONE authorization boundary. Fixed base-risk table + hard rules (forbidden always blocked; sensitive+external = forbidden; destructive/external/model = high; cautious escalates; blocked mode = reads only). `AuthorizedAction` is the only execution credential and cannot be minted from Blocked/NeedsApproval. | Overrides everything: model confidence, scheduler scores, priorities. |
| 10 | `executor.rs` | Runs ONLY AuthorizedActions through pluggable Effectors. Panic isolation (catch_unwind), global cancellation, timeout detection, bounded 16 KB output capture, full ActionRecord provenance (goal/plan/capability/reason/timestamps/exit/output). | Cannot run forbidden actions — structurally impossible. |
| 11 | `verifier.rs` | Expected vs actual from evidence (exit codes, output content, file existence/content). Success/Failure/Unknown with deterministic confidence; read-only (mtime-preserving). | Never mutates user state. |
| 12 | `replanner.rs` | Recovery ladder: attempt 1→alternative (if any), 2→replan, ≥3→escalate; exponential backoff (5s base, 2^attempt, 1 h cap); stale-goal abandon; Unknown-verification escalation; environmental vs logical failure split. | Never blind-retries; bounded attempts. |
| 13 | `lifecycle.rs` | One-goal-at-a-time driver connecting all phases with a full explainability trail; pause honored between every stage; work-step selection (context-only alternatives escalate honestly). | Reuses every component — no second orchestrator. |
| 14 | `ledger.rs` | Append-only `autonomy_decisions` (SQLite triggers reject UPDATE/DELETE), details-JSON provenance, `explain()` partitions: did / refused / needed-approval / failed / learned. | Autonomous code cannot erase audit history. |
| 15 | `ai_validation.rs` | Strict schemas for model goal/plan proposals; malformed → reject; forbidden action in a plan → reject; risk suggestions → advisory-only; confidence clamped; provider fallbacks are always "deterministic core continues". | Model output is untrusted input. |
| 16 | `skills.rs` | 6 skills (inspect_project, run_tests, inspect_git, summarize_changes, inspect_error, prepare_context) with the full contract: deps, permissions, risk, cost, privacy, input schema (strict, anti-smuggling), output schema, verify method. | Same gate, same executor, same ledger. |
| 17 | `learning.rs` | Patterns from local data only (repeated errors ≥3×, recurring projects, repeated workflow categories). Strict class separation OBSERVED FACT / INFERENCE / USER PREFERENCE. Privacy screen before persistence; retention (90 d inferences) + curation; forgetting deletes patterns, never facts. | Learning never touches safety policy (structural test). |
| 18 | `prediction.rs` | Next-action/unfinished-task/project-switch/resume-target/repeated-problem/context-needed with evidence, expiry, confidence floor 0.3. `prediction_grants_authority()` is hardwired false. | Prediction is never authorization. |
| 19 | `intelligence_gate.rs` | Privacy-aware provider routing over the ONE ProviderRouter: no-AI capabilities never dispatch; blocked prompts never reach any provider; sensitive prompts only redacted-local or CLOUD DENIED; clean prompts prefer local; cloud requires explicit capability opt-in. | Private data never silently reaches cloud. |
| 20 | `hardening_audits.rs` | Executable architecture guarantees: manifest uniqueness, safety-path totality, forbidden-battery (every actor/approval/mode), model-bypass rejection, panic isolation, bounded retries, ledger append-only, planner zero-authority. | Regression fence. |
| 21 | `priority.rs` | The 15-level precedence ladder (safety > privacy > user denial > instruction > … > convenience). Deterministic resolution; ambiguity ladder: low→safer choice, medium→defer, high→ask user, safety-ambiguous→block. | No dangerous conflict resolves randomly. |
| 22 | `intent.rs` | Explicit user intent registry: denial beats high-confidence goals and filters predictions; scoped, revocable preferences; instructions recorded but never authority. | Denial stops the affected action/goal. |

## Infrastructure already in place (earlier phases — unchanged)

- **Capability Registry** (`capability.rs`): 20-capability manifest, DB overrides.
- **Adaptive Scheduler** (`scheduler.rs`): run_score engine, hard gates (heavy work, battery, idle-layers), adaptive intervals.
- **Orchestrator** (`orchestrator.rs`): the ONE gate helper used by live loops (ghost/wellness), transition-only ledger logging.
- **SystemProbe** (`context.rs`): /proc-based CPU/mem/battery sampling, graceful degradation.
- **Privacy engine**: `strawberry-core` `PrivacyPolicy` — deterministic, tested; the app's chat-creation path and capture-daemon both screen every capture before storage.
- **EventBus**: bounded (512), oldest-drop, subscriber fan-out with drops.
- **Project Brain / What Changed / Intelligent Resume** (`src/project/`): read-only aggregation over existing storage.

## Known limitations

- Plans are generation-based; persistence lands when execution wiring (Phase 14 ledger fields) is consumed by the UI.
- Executor ships the harness + shell effector; file/git effectors arrive with future work needing them.
- Lifecycle drives one goal at a time; parallel arbitration is the Phase 21 priority ladder's job in a future phase.
- Predictions use simple deterministic detectors; richer inference is Phase 17 learning's next iteration.

## Validation commands

```
cargo check && cargo test            # 315 tests
npx tsc --noEmit                     # 0 errors
npx vite build                       # green
```
