# Strawberry Phase 0 Architecture Audit

**Date:** 2026-08-31 · **Scope:** full repository audit + design for a Canonical Event Foundation · **Task:** Phase 0 (audit + design ONLY — no implementation)

Every claim in this report is labeled:

- **FACT** — directly verified from repository code (file path given).
- **INFERENCE** — conclusion derived from verified code, stated as reasoning not observation.
- **RECOMMENDATION** — proposed future architecture. Nothing in this category exists yet.
- **UNKNOWN** — cannot be verified from the current repository.

---

## 1. Executive Summary

**Verdict (INFERENCE):** Strawberry is a healthy, genuinely local-first desktop app with an unusually clean data spine (15-version migration chain, FTS5 with triggers, shared deterministic core crate) but a fragmented "event" landscape: **four disconnected event vocabularies, three time-block models, three workspace-snapshot systems, and one schema duplicated across two processes.** Nothing here needs to be thrown away. What is missing is *one thin unification layer* — a Canonical Event Foundation that **absorbs** the existing vocabularies rather than replacing them — plus a short list of safety/correctness fixes that should land *before* any major feature.

**Key numbers (FACT):**

| Metric | Value |
|---|---|
| Registered Tauri commands | **109** (`src-tauri/src/lib.rs`, counted twice via `generate_handler` block) |
| App Rust LOC / files | ~12,677 / 52 files |
| Migration SQL | 567 lines, 15 versions (001–015) |
| `strawberry-core` shared crate | 3,095 LOC (path dep of app **and** daemon) |
| Clipboard daemon (`capture-daemon`) | 1,334 LOC, binary `strawberry-daemon` v0.3.0 |
| Frontend TS/TSX | ~9,140 LOC across ~40 files, **0 tests** |
| Background threads running | 3 (autonomy, ghost, wellness) + 1 latent never-started (screen capture) |
| Event vocabularies | **4** (autonomy bus, ghost, ambient, wellness emits) — zero unification |
| Total DB tables | 31 app + 1 runtime FTS + 6 daemon-side (5 duplicated + embeddings) |

**The single most important architectural finding (INFERENCE):** Strawberry already has all the *ingredients* of an event-driven personal memory system — an in-memory event bus (`src-tauri/src/autonomous/event.rs`), a persisted event log (`ghost_events`, migration 014), a generic event fabric (`ambient_events`, migration 009), world state (`autonomous/world_state.rs`), an insight engine (`ghost/insights.rs`), and a scheduler precedent. They were built by different hands at different times and **none of them talk to each other.** The autonomy bus's only publisher is the frontend demo panel; ghost's only writer is one command; ambient's relation table is dead. A Canonical Event Foundation is therefore **needed — but as a UNIFIER of these four, not a fifth competitor.**

**Top risks (RED — details in §11, §12, §20):**

1. The clipboard daemon has **no blocklist/redaction of any kind** — it persists *all* clipboard text (including passwords, tokens, 2FA codes) into `chats`, `chat_fts`, and raw text files. (`capture-daemon/src/db.rs:175`, verified by grep: zero matches for blocklist/denylist/redact/secret handling).
2. Three overlapping workspace-snapshot systems (`work_snapshots` 006, `work_spaces` 008, `workspace_sessions` 011) with **two competing freeze UIs in the same Planner view**.
3. Screen capture is **unreachable**: `run_loop` (`src-tauri/src/screen/capture.rs:106`) is never spawned — `start_screen_capture` only creates tables — and its runtime table schema has **drifted** from migration 005.
4. Every background thread is killed abruptly at exit (`ShutdownFlags` are managed at `src-tauri/src/lib.rs:98-99` but never flipped — no exit hook exists in `.run()`), and DB writes buffered in WAL may be lost.
5. Ghost graph is wiped and fully rebuilt every 5 minutes with a quadratic pair-generation step silently truncated at 1000 rows, and `ghost_events` **grows unbounded** (the only prune path is a command the frontend never calls).
6. The daemon **bypasses the app's migration system** — it carries a copy of the 001 schema and its own `gen_id()`, on the *same* `app.db` file, creating a two-writer schema-authority split.

**Was a Canonical Event Foundation needed? Yes (INFERENCE)** — see §8–§10 for the evidence and §18 for the design-only specification. But the correct shape is: **one event spine, four adapters, zero new competing systems.**

---

## 2. Current Repository State

**Layout (FACT):**

```
chat-memory-tree/                 (branch: main, HEAD d7695a3 — clean at audit start)
├── src-tauri/          Tauri v2 app: Rust backend (52 files, ~12.7k LOC)
│   ├── migrations/      001–015 SQL, one transaction per run (567 lines)
│   └── src/            commands/ + domain modules (ghost, wellness, alpha, screen,
│                       autonomous, workspace, snapshot, ambient, …)
├── src/                React 18 + Zustand + Vite 5 (es2021) frontend, ~9.1k LOC
├── strawberry-core/    Shared deterministic crate (3,095 LOC) — path dep of BOTH app & daemon
├── capture-daemon/     Separate clipboard-capture binary `strawberry-daemon` v0.3.0 (1,334 LOC)
├── scripts/            Dev scripts
└── docs/               (this report)
```

**Runtime model (FACT):**

- DB: `~/.local/share/com.local.chatmemorytree/app.db` — SQLite, WAL mode, `foreign_keys=ON`, `busy_timeout=2000ms`. **The same file is opened by the app AND the daemon.**
- App state: `AppState { conn: Mutex<Connection> }` — every DB command serializes on this one mutex (`src-tauri/src/state.rs`). The ghost thread and wellness tick deliberately open their *own* connections (`open_db_for`) to avoid freezing every command.
- Async: **no tokio, no channels anywhere** (no `std::mpsc`, no `crossbeam`, no `notify` — verified by grep). All background coordination is polling threads + `Arc<AtomicBool>` + `Arc<Mutex<Vec<_>>>`.
- Command pattern: `#[tauri::command] pub async fn … -> Cmd<T>` where `Cmd<T> = Result<T, String>`; DB work via `blocking(state, closure)` off the main thread — *except* the ambient commands, which are synchronous and hold the AppState mutex on the main thread (FACT, `src-tauri/src/commands/ambient.rs`).

**Background threads (FACT):**

| Thread | Spawned at | Cadence | Notes |
|---|---|---|---|
| Autonomy supervisor | `lib.rs` | 5s cycles (2s while `recent_errors` non-empty), 200ms steps | Only adaptive poller in the codebase |
| Ghost cycle | `lib.rs` | 5 min | Own DB connection |
| Wellness tick | `lib.rs` | 60s | Own DB connection |
| Screen capture `run_loop` | **never** | (30s design) | `capture.rs:106` has no spawner; `lib.rs:40` manages `CaptureHandle::default()` |

**Shutdown (FACT):** `ShutdownFlags` are created "so they can be flipped on app exit" (`lib.rs:98-99`) but no exit hook exists — flags are never flipped and all threads die with the process.

**Build/test state (FACT, from the most recent full run):** `npm run check:ts` clean · `npm run build` ✓ · `cargo check` 0 errors · `cargo test` 46 passed / 0 failed / 2 ignored.

**Documented constraints (FACT):** `src-tauri/src/autonomous/mod.rs` header: "NO LLM / NO API key / NO cloud / NO Python / NO OLLAMA / Rust-native / Event-driven / Deterministic / Safety-gated / Locally persistent". README claims "zero cloud, zero API keys, zero telemetry… No network calls in core features" — **overstated (FACT):** two opt-in network modules exist (alpha hunter, news briefing), gated by `app_meta` flags (§12).

---

## 3. Current Architecture Map

```ascii
                         ┌──────────────────────────────────────────────┐
                         │                SQLite app.db                   │
                         │  (single file, TWO writers: app + daemon)     │
                         │  31 tables · 15 migrations · WAL · FTS5       │
                         └───────▲──────────────────▲───────────────────┘
                                 │ Mutex<Connection>│ own connections
   ┌─────────────────────────────┴───┐   ┌──────────┴──────────────┐
   │        Tauri app (Rust)          │   │  Background threads      │
   │  AppState ── 109 #[tauri::cmd]   │   │  autonomy 5s/2s          │
   │  commands/: roots folders chats  │   │  ghost     5min         │
   │  search planner handoff resume    │   │  wellness  60s          │
   │  tabs screen story news alpha     │   │  capture   NEVER STARTED│
   │  ghost workspace ambient health  │   └─────────────────────────┘
   │  inbox news alpha wellness        │
   └───────▲──────────────────────────┘
           │ invoke() camelCase
   ┌───────┴──────────────────────────┐
   │  React frontend (src/)           │   NO router (view state in React)
   │  AppLayout 9 views · Zustand     │   NO tests
   │  store · call() error-normalizer │   localStorage: theme, wellness payload,
   └──────────────────────────────────┘            CAL_REM:<…> (unbounded)

   ┌──────────────────────────────────┐      ┌────────────────────────────┐
   │  capture-daemon (separate bin)    │      │  strawberry-core            │
   │  clipboard watch (Wayland/native) │      │  deterministic shared crate  │
   │  own 001-schema copy + own IDs   │      │  path dep of app AND daemon │
   │  Ollama embeddings (opt-in)      │      │  "no AI, no network, no      │
   │  writes chats/chat_fts/raw files │      │   clocks, no randomness"     │
   └──────────────────────────────────┘      └────────────────────────────┘
```

**The four event vocabularies, at a glance (FACT — full audit in §8):**

```
autonomous/event.rs      in-memory only    16 EventKind      1 consumer (autonomy)
ghost_events (014)       persisted         14 string types   1 writer (command)
ambient_events (009)     persisted         5 CHECK types    relations table DEAD
wellness emits           in-flight only    2 Tauri events   popup flow
   ── plus the naming hazard: the `events` table (010/015) is CALENDAR data ──
```

**What does NOT exist (FACT, verified by grep):** no tokio, no channels, no file watcher (`notify`), no OCR runtime (stub), no embedding runtime in the app (daemon only, opt-in), no HTTP server, no plugin system, no CI configuration.

---

## 4. Subsystem Inventory

Status markers: **EXISTS** (working) · **PARTIAL** (works but incomplete) · **LEGACY** (superseded) · **DUPLICATE** (overlaps another subsystem) · **PLACEHOLDER** (stub) · **UNUSED** (unreachable) · **UNKNOWN** (unverifiable).

| Subsystem | Location | Status | Evidence / notes |
|---|---|---|---|
| Roots/nodes/chats tree | `commands/roots.rs`, `folders.rs`, `chats.rs` | EXISTS | Core CRUD, UUID text PKs, `position` ordering |
| Chat brief + artifacts | `commands/brief.rs`, `chat_artifacts` (001, rebuilt 003) | EXISTS | Deterministic extraction lives in `strawberry-core` |
| Handoff packet | `commands/handoff.rs` | EXISTS | Slot model, budget report; 1 test |
| Chat resume points | `commands/resume.rs`, `chat_resume_points` (004), `resume_fts` | EXISTS | `cross_chat_suggestions` (004) is **DEAD** — zero code references (verified) |
| Full-text search | `chat_fts` runtime + 3 triggers; `commands/search.rs` | EXISTS | Fallback flag `app_meta.fts_enabled`; `tabs_fts`, `resume_fts`, `screen_fts` siblings |
| Tabs (browser history) | `commands/tabs.rs`, `tabs` (004) | PARTIAL | Stores **full URLs, no blocklist** (§12); 2 tests |
| Screen capture | `screen/`, `commands/screen.rs`, tables (005) | **UNUSED + DUPLICATE + PLACEHOLDER** | `run_loop` never spawned; `start_screen_capture` = table-create only (`commands/screen.rs:20`); `stop_screen_capture` = `// TODO` + `Ok(())`; runtime schema drifted from 005 (nullable width/height/byte_size, `embedding BLOB`) vs 005 (`NOT NULL`, "384-dim float32" comment); `embedding` column **never written** (`capture.rs:290` inserts 11 cols, excludes it); `ocr_text` always `None` (OCR stubbed); `xcap 0.9` declared in Cargo.toml but **unused** — tools are spectacle/grim/scrot/import + xdotool |
| Ambient memory | `commands/ambient.rs`, `ambient_events` (009) | PARTIAL | CLI capture ingestion + AST analysis; `ambient_relations` table **DEAD** (created 009, zero code) |
| Ghost (graph + insights) | `ghost/` (tracker, graph, insights, attention), `ghost_events` etc. (014) | PARTIAL | Full wipe-and-rebuild each cycle; see §13; `commands/ghost.rs:151-152` returns empty `Vec::new()` placeholder for graph query |
| Alpha hunter | `alpha/mod.rs` (591 lines), `alpha_candidates` (012) | EXISTS | **Only network module**; gated `app_meta.alpha_hunter_enabled` (`commands/alpha.rs:42`); 5 tests |
| Wellness | `wellness/`, tables (013) | PARTIAL | `wellness_activity` table is **write-only** (no readers); `popup.rs show_popup` likely dead — frontend renders its own popup; `next_reminder_in_secs` never recomputed; no tests |
| Autonomy runtime | `autonomous/` (event, runtime, world_state) | PARTIAL | **In-memory only, no DB tables** — dies on exit; starts `Paused`; observation-only (16-arm `apply_event`); `WorldStateDiff` declared never constructed; only publisher is the **frontend demo panel** via `autonomy_publish`; 11 tests |
| Workspace freeze/resume v0.1 | `workspace/`, `workspace_sessions/items` (011) | EXISTS | qdbus6 KWin + journalctl + `/proc` mining + `ss -tlnp`; safe restore gates (§12) |
| Work snapshots (legacy) | `work_snapshots` (006) + ws commands | **DUPLICATE/LEGACY** | `FreezePanel` UI still live; `listWorkSnapshots` API unused by UI |
| Work spaces (legacy) | `work_spaces` (008) | **DUPLICATE/LEGACY** | Second snapshot-ish model |
| Browser/app snapshot | `snapshot/` (Firefox mozLz4, Chrome History) | EXISTS | Chrome DB copied to temp, opened read-only; clipboard head 140 chars |
| Story (git activity) | `commands/story.rs` | PARTIAL | `git log` only; errors silently ignored |
| Planner (todos/habits/focus) | `commands/planner.rs`, tables (002) | EXISTS | `schedule` table **orphaned**: `getSchedule`/`addEvent` exist in API but no UI calls them |
| Calendar | `commands/planner.rs`, `events` (010 + 015) | EXISTS | Live 17-column model, recurrence, reminders, FTS-less LIKE search; see §7 |
| Daily briefing | `commands/planner.rs` | PARTIAL | News section gated by `app_meta.news_enabled` — **a flag with no setter anywhere** (read at `planner.rs:768`, written by nothing) → unreachable in practice |
| News fetcher | `commands/news.rs` | EXISTS | hn.algolia.com, 8s timeout, errors→empty |
| Inbox | `commands/inbox.rs`, `InboxView` | EXISTS | Ingestion queue |
| Health/info | `commands/health.rs` | EXISTS | `du`/`df` shellouts |
| Clipboard daemon | `capture-daemon/` | PARTIAL + DUPLICATE | Duplicates 001 schema + own `gen_id()` on the **same DB file**; `embeddings` table created outside the migration system (`semantic.rs:35`); Ollama `nomic-embed-text` 768-dim vs 005's 384-dim comment; **no capture filtering (§12)**; rusqlite 0.32 (app: 0.31) |
| `strawberry-core` | `strawberry-core/` | EXISTS | Deterministic brief/extraction shared by both binaries |
| File store | `storage/files.rs` (77 lines) | EXISTS | `files/<root_id>/<node_id>/<chat_id>.raw.txt` + `.brief.md` + `.meta.json` |
| Frontend | `src/` | PARTIAL | 9 views, Zustand, no router, **0 tests**, 18 unused API bindings, duplicate freeze panels |

---

## 5. Data Ownership Map

**FACT — every table, its writers and readers (verified via grep of all SQL in app + daemon):**

| Table (migration) | Writers | Readers | Owner module | State |
|---|---|---|---|---|
| `schema_migrations` | migrations.rs | migrations.rs | db | OK |
| `roots`, `nodes`, `chats` (001) | app commands + **daemon** (`capture-daemon/src/db.rs`) | app, daemon, ghost | commands/chats | **dual-writer** |
| `chat_artifacts` (001/003) | app brief, daemon | resume, handoff, search | commands/brief | OK |
| `app_meta` (001) | alpha toggle, FTS fallback | gates (alpha, news, FTS) | db | OK |
| `todos`, `habits`, `habit_logs` (002) | planner commands | PlannerView, briefing | commands/planner | OK |
| `schedule`, `focus_sessions` (002) | `add_event`/focus cmds | `get_schedule` (**UI never calls**) | commands/planner | **orphaned UI** |
| `chat_resume_points` + `resume_fts` (004) | resume cmds | resume, briefing | commands/resume | OK |
| `cross_chat_suggestions` (004) | **nothing** | **nothing** | — | **DEAD** |
| `tabs` + `tabs_fts` (004) | tabs record cmd | GhostPanel co-access | commands/tabs | OK (privacy gap §12) |
| `screen_frames`, `screen_blocklist`, `screen_fts` (005) | **nobody** (capture never runs) | screens view (empty) | screen | **UNUSED** + drifted runtime copy |
| `work_snapshots` (006) | ws commands (FreezePanel) | listWorkSnapshots (unused by UI) | commands/workspace? | LEGACY/UNREACHABLE UI |
| `work_spaces` (008) | ws commands | — | — | LEGACY |
| `ambient_events` (009) | ambient capture cmd | ambient view, ghost | commands/ambient | PARTIAL |
| `ambient_relations` (009) | **nothing** | **nothing** | — | **DEAD** |
| `events`, `event_reminders` (010) | calendar cmds | CalendarView | commands/planner | OK — **name collision hazard** |
| `event_sources` (010) | **nothing** | **nothing** | — | **DEAD** |
| `workspace_sessions`, `workspace_items` (011) | workspace cmds | PreviousWorkPanel | workspace | OK |
| `workspace_restore_attempts` (011) | restore attempts | **nothing** | workspace | **WRITE-ONLY** |
| `alpha_candidates` (012) | alpha hunter | AlphaHunter.tsx | alpha | OK |
| `wellness_config`, `wellness_state` (013) | wellness tick + cmds | wellness UI | wellness | OK |
| `wellness_activity` (013) | tick | **nothing** | wellness | **WRITE-ONLY** |
| `ghost_events`, `ghost_graph_*`, `ghost_insights` (014) | ghost cmd + thread | GhostPanel | ghost | OK but unbounded (§13) |
| `events` v2 cols (015) | calendar cmds | CalendarView | commands/planner | OK |
| `chat_fts` (runtime) | triggers | search | db/migrations.rs | OK |
| `embeddings` (daemon, runtime) | daemon semantic | daemon `--search` | capture-daemon/semantic.rs | **outside migration system** |

**INFERENCE:** ownership is clean *within* the app; the two structural ownership problems are (a) the daemon writing `chats`/`chat_fts`/roots with its own schema copy and ID generator on the shared file, and (b) five tables that nothing reads or writes (dead/write-only) — schema the migration system must forever carry.

---

## 6. Real Data Flow Traces

**FACT — traced end-to-end for each mandated flow:**

**① Copy text (daemon)** — `capture-daemon/src/main.rs` loop → `clip.rs` (`wl_clipboard-rs` on Wayland / `arboard` native; change detection via `compute_bytes_sig` — a `DefaultHasher` over 64 sampled byte points) → `db::insert_capture` (`db.rs:175`) → INSERT into `chats` under stable root `🍓 Captures`, full text into `chat_fts` (trigger) and `<chat_id>.raw.txt` on disk. **No blocklist, no redaction — everything is stored (§12).** Interactive `--save-once` mode can prompt; image mode can be user-ignored (`main.rs:313`, `:357`), but there is no policy filter.

**② Screenshot** — **two flows, neither live in the app:** app-side `screen/capture.rs` has a complete pipeline (pHash via DCT, hamming dedupe, spectacle/grim/scrot/import, thumbnail, `files/screens/<date>/<ms>_<hash8>.jpg`) but `run_loop` is never spawned — `start_screen_capture` only ensures tables (`commands/screen.rs:20`). Daemon-side image capture exists but is user-confirmation-gated. **FACT: screenshots are never taken by the current app.**

**③ File change** — **does not exist.** No `notify`/inotify dependency, no watcher thread (verified). The closest artifacts are: `workspace/shell_cwds` mining `/proc/<pid>/cwd`, `snapshot/` reading browser session files on demand, and story's `git log` (read-only). Any "What changed in my files?" feature has **no current foundation** (§15).

**④ Git commit** — `commands/story.rs:40-89` runs `git log --pretty=%ad%x1f%s` with a 200-line cap and days clamp 1..=90; errors silently ignored. Read-only; git is invoked nowhere else (FACT).

**⑤ Task (todo)** — PlannerView → `planner` commands → `todos` (002) → read back by planner, daily briefing, and ghost insight kinds (velocity, achievement).

**⑥ Calendar event** — CalendarView → `create_calendar_event`/`update_calendar_event` → `events` (010/015, 17 columns mapped in `map_calendar_event_row`) → `event_reminders`; search via `LIKE`; reminders deduped in frontend localStorage `CAL_REM:<eventId>|<minutesBefore>|<occStartIso>` (**unbounded**, never pruned — FACT).

**⑦ Freeze** — *two competing flows*: (a) `FreezePanel` → ws commands → `work_snapshots` (006); (b) `PreviousWorkPanel` → workspace v0.1 commands → KWin script via `qdbus6` with `SBFRZ|` markers, `/proc` + `ss` harvesting, → `workspace_sessions/items` (011). Both panels render in the same PlannerView (FACT).

**⑧ Resume** — PreviousWorkPanel → `restore_workspace_session` → per-item strategies; safety gates: `OpenUrl` restricted to http/https, `RunTerminalCommand` requires `confirmed=true`, `pending_servers` never auto-executed (FACT, `workspace/`). Attempts recorded into `workspace_restore_attempts`, which nothing reads.

**⑨ Chat import (user-added chat)** — dialog → `commands/chats.rs` → `nodes`/`chats` rows → `storage::files::write_raw` (called exactly once per chat creation) → brief extraction via `strawberry-core` → `chat_artifacts` → `chat_fts` triggers.

**⑩ Search** — SearchBox → `search_chats` → `chat_fts` MATCH (+ `resume_fts`, `tabs_fts`, `screen_fts` for their domains; calendar uses `LIKE`). If trigger creation failed at startup, FTS is disabled and `app_meta.fts_enabled` records the degraded mode.

**⑪ Session begin/end** — no automatic lifecycle exists: `workspace_sessions` rows are created only by explicit user freeze; there is no "session started/ended" event anywhere. The autonomy `WorldState` tracks an in-memory `workflow_phase` that resets on every app restart (FACT).

**INFERENCE:** the flows that work (chat import, search, planner, calendar, ghost analysis) share the same spine: *command → AppState mutex → table → FTS/derived tables*. The flows that are broken or absent (screenshot, file change, session lifecycle) are exactly the ones a future memory system needs most — which is why §15's dependency matrix shows "capture/wiring" as the recurring missing layer.

---

## 7. Calendar Audit

*(Mandate: do NOT redesign Calendar. Answer: how do future systems integrate without creating duplicate event/task models?)*

**What exists (FACT):**

- Model: `events` table (010) + v2 columns (015: `recurrence`, `recurrence_end`, `color`) — 17 columns mapped in `map_calendar_event_row` (`commands/planner.rs`).
- API: `create_calendar_event`, `update_calendar_event`, `search_calendar_events` (LIKE-based), `list_event_reminders`, `delete_event` — registered among the 109 commands; consumed by `CalendarView.tsx`.
- Reminders: `event_reminders` (minutes-before offsets); triggered-state tracked client-side via `CAL_REM:` localStorage keys (backend `triggered` column has no setter — known gap from the Calendar task).
- `event_sources` (010) exists for import-source tracking but is **dead** — zero writers/readers.

**Integration answers (RECOMMENDATION):**

1. **Do not create a second event/task model.** Any future subsystem that needs "things happening over time" must reference `events` by `event_id`, or map *into* it via its own source adapter — never fork the schema.
2. **The canonical event spine should treat calendar rows as a SOURCE, not a peer.** A calendar adapter would translate `events` rows into canonical *observations* (e.g. `calendar.event.created`) read-only; write-back goes through the existing commands only. This keeps one writer per table (the Calendar module) and gives other systems a uniform read surface.
3. **Unify the three time-block models before building anything temporal (INFERENCE → RECOMMENDATION):** `schedule` (002, orphaned), `events` (live calendar), and `todos` with due dates. Recommendation: retire the `schedule` path (its API is UI-orphaned), keep `todos` as *task* model and `events` as *temporal occurrence* model, and let the canonical layer define the relationship (a todo *references* a deadline; it is not an event). This is consolidation of existing models, not new ones.
4. **Naming hazard to document now (FACT):** the table `events` is calendar data. The canonical event spine must NOT reuse that name — recommend `core_events` or `event_log` (§18) precisely to avoid colliding with `events`.
5. **Recurrence is the only genuinely hard part (FACT):** stored as a string enum + `recurrence_end`; expansion happens in the frontend (`CalendarView` greedy `assignLanes`, EXPAND_CAP 5000). Any integration that needs occurrences should ask Calendar for expanded occurrences via a future command rather than reimplementing expansion logic.

---

## 8. Existing Event Mechanisms

*(Mandate: do NOT create a new Event Bus if one exists. Determine what exists, whether they can be unified, what to retain, what's missing, and the minimal canonical abstraction.)*

**FACT — inventory of every event-adjacent mechanism:**

| # | Mechanism | Location | Storage | Types | Publishers | Consumers |
|---|---|---|---|---|---|---|
| 1 | Autonomy `EventBus` | `autonomous/event.rs` (159 lines) | in-memory `Arc<Mutex<Vec<NormalizedEvent>>>`, bound 512, drop-oldest `q.remove(0)`, `drain(n)` | 16 serde-tagged `EventKind` variants (`ActiveAppChanged`, `FileOpened`, `FileModified`, `ChatOpened`, `ChatCreated`, `FolderOpened`, `SearchExecuted`, `BuildStateChanged`, `TodoToggled`, `FocusSessionChanged`, `TabVisited`, `InboxAdded`, `ScreenCaptured`, `WellnessBreak`, `Heartbeat`, `ErrorObserved`) | **only the frontend** via `autonomy_publish` (AutonomyPanel demo buttons) — no backend module publishes | exactly one: `AutonomyRuntime::run_cycle` |
| 2 | Ghost event log | `ghost_events` (014), `ghost::tracker::record` | SQLite | 14 string `EventType` variants | `ghost_record_event` command only | ghost thread (graph/insights/attention); unknown type → **silent `Ok(0)`** (`commands/ghost.rs:56`) |
| 3 | Ambient events | `ambient_events` (009) | SQLite | 5 CHECK-constrained types | ambient capture command | ambient view, ghost |
| 4 | Wellness emits | `app.emit("wellness:popup")` (`wellness/mod.rs:159`), `"wellness:popup-shown"` (`popup.rs:35`) | in-flight | 2 | wellness tick | `src/main.tsx` listener → popup window |
| 5 | *Activity-shaped records* (event-like but not events) | `tabs` (004), `screen_frames` (005, unused), `work_snapshots` (006), `wellness_activity` (013, write-only), `focus_sessions`, `habit_logs` (002) | SQLite | row per activity | various | various |

**The four answers the mandate requires:**

**What exists (FACT):** four vocabularies with zero cross-references (verified — no module imports another's event types), plus six activity tables. There is no bus that anything else can subscribe to; there is no persisted bus at all.

**Can they be unified? (INFERENCE → RECOMMENDATION)** Yes, and they already want to be:
- The autonomy `EventKind` set is the best *semantic* vocabulary (16 kinds covering app/file/chat/screen/build/lifecycle) but the worst *plumbing* (in-memory, one consumer, frontend-published).
- `ghost_events` is the best *persistence* precedent (typed, timestamped, scored, has an insight consumer) but is ghost-private and stringly-typed.
- `ambient_events` is the best *generic fabric* (loose typed rows) but its relation table is dead, suggesting it never grew its intended graph role.
- Wellness emits are the only true *push* mechanism in the repo.

**What to retain (RECOMMENDATION):** all four. The bus keeps its role as the in-process spine (extended to multi-subscriber); `ghost_events` becomes one *adapter's* persistence target *or* is bridged read-only into the spine; ambient rows are bridged as a source adapter; the wellness `emit` pattern is the model for how the spine notifies the UI.

**What's missing (FACT):** persistence of bus events (dies on exit); multiple subscribers (drain-based single consumer); backend publishers (the bus is demo-driven); a common event identity/timestamp/provenance schema; retention/dedup; and any mechanism for the daemon or future agents to publish in.

**Minimal canonical abstraction (RECOMMENDATION — designed in §10/§18, implemented in a later phase):** one `core_events` table + one Rust `CanonicalEvent` type + four thin adapters over the existing vocabularies. NOT a new bus process, NOT a rewrite of `EventBus` — an extension of it to N subscribers and an optional persistence sink.

---

## 9. Event vs Memory Model

*(Mandate: define EVENT and MEMORY precisely; propose a relationship; the pipeline EVENT→OBSERVATION→MEMORY→INSIGHT→TASK/ACTION is a candidate, not automatically accepted.)*

**Definitions (RECOMMENDATION, grounded in what exists):**

- **EVENT — something that happened, at a time.** Immutable, past-tense, cheap, high-volume. Strawberry already has events in four dialects (§8) but no shared definition. An event never changes after creation.
- **MEMORY — a durable representation the system keeps to shape future behavior.** Curated, revisable, goal-laden, lower-volume. Strawberry's memories today are *implicit*: `chat_artifacts`, briefs, `ghost_insights`, `todos` — none named "memory," all functioning as one.

**The boundary in one line (INFERENCE):** *events are the raw material; memories are what the system chose to keep.* An event is never "wrong"; a memory can be (stale, misleading) and must therefore be revisable and expirable — events must not be.

**The candidate pipeline, evaluated against the codebase (INFERENCE → RECOMMENDATION):**

```
EVENT ──normalize──▶ OBSERVATION ──select/summarize──▶ MEMORY ──derive──▶ INSIGHT
                                                           │
                                        TASK / ACTION ◀────┘  (human- or agent-initiated)
```

Verdict: **accept the pipeline shape, reject collapsing its stages.** Concretely in Strawberry terms:

| Stage | Existing embodiment (FACT) | Must stay distinct because |
|---|---|---|
| EVENT | `EventKind` occurrences, clipboard captures, tabs visits | volume; immutability; retention by time |
| OBSERVATION | (mostly missing) — a normalized row the spine can reason about | filtering/normalization must be lossy-but-safe before storage decisions |
| MEMORY | `chat_artifacts`, briefs, `chat_resume_points` | revisable; referenced by ID; may be deleted by user choice |
| INSIGHT | `ghost_insights` (10 fixed-score kinds) | derived, regenerable, cheap to throw away — the ghost already DELETEs unseen ones each cycle |
| TASK / ACTION | `todos`; autonomy `CycleOutcome::Executed` | actionable, owned, has a lifecycle independent of the event that motivated it |

**RECOMMENDATION:** these five are *roles*, not five new tables. The canonical spine should make role an explicit field (§18) so an object can graduate (an observation promoted to memory) without a copy — but promotion must be a deliberate, logged transition, never a silent merge. **TASK/ACTION must NOT automatically become the same object as the event that triggered it** — the autonomy module's own gating (actions require safety gates) already implies this.

---

## 10. Canonical Event Foundation Proposal

*(Mandate: minimal + extensible; investigate — do not blindly use — a candidate field list; distinguish RAW EVENT / OBSERVATION / MEMORY / INSIGHT / TASK / ACTION.)*

**Core idea (RECOMMENDATION):** one event spine that unifies the four vocabularies. Nothing is replaced: the autonomy `EventBus` becomes the in-process hub (extended from 1 consumer to N subscribers and gaining an optional persistence sink); `ghost_events`, `ambient_events`, tabs, and capture flows become *source adapters*; wellness emits become the *UI-notification* pattern.

**Candidate field investigation (as mandated — each field judged, not assumed):**

| Candidate field | Verdict | Reasoning (grounded in FACTs of §8) |
|---|---|---|
| `event_id` | **ADOPT** | already exists twice in incompatible forms (`EventId` counter in `autonomous/event.rs` via `static EVENT_COUNTER: AtomicU64`; `gen_id()` in daemon). One UUID form. |
| `timestamp` | **ADOPT as `occurred_at_ms` (unix ms)** | the repo is split between RFC3339 TEXT (wellness — compared as *strings*, `wellness/mod.rs`) and unix-ms INTEGER (workspace, screen). Integer ms is the safer spine; adapters convert. |
| `event_type` | **ADOPT** (`string`, namespaced `source.kind`, e.g. `tabs.visited`) | unifies 16 EventKind + 14 ghost strings + 5 ambient CHECKs without a giant enum; keeps the bus's serde-tagged enum for in-process speed, with string form at the persistence boundary. |
| `source_type` / `source_id` | **ADOPT** | needed to name the four adapters + daemon + future agents; provenance is impossible without it. |
| `project_id` | **DEFER (nullable)** | no project concept exists today beyond `roots`. Map root→project later; don't block the spine on it. |
| `session_id` | **ADOPT (nullable)** | `workspace_sessions` (011) exists; no automatic session lifecycle exists (§6 ⑪). Nullable now, populated when a session concept lands. |
| `workspace_id` | **MERGE into session/actor context** | `work_spaces` (008) is legacy/duplicate; adding a second workspace identity now would be the exact duplication this audit forbids. |
| `actor` | **ADOPT (enum: `user` / `system` / `agent:<id>`)** | autonomy safety gates already require knowing who initiated an action (RunTerminalCommand `confirmed=true`). |
| `payload` / `reference` | **ADOPT both, split** | small inline payload JSON **plus** a typed reference to a domain row (`chat_id`, `event_id`(calendar), `frame_id`…). Pure-payload designs re-duplicate domain data; pure-reference designs make the log unreadable offline. |
| `provenance` | **ADOPT (minimal: chain: source → adapter version)** | the ghost's hand-rolled `build_tags_json_array` escaping bug (§12) proves payloads need discipline; provenance = how confidence is audited later. |
| `confidence` | **DEFER (nullable, default 1.0)** | nothing measures confidence today (ghost insight scores are *insight* scores, not event confidence). Keep the column, don't build machinery. |
| `privacy_level` | **ADOPT (`public`/`sensitive`/`secret`)** | the daemon's no-filtering privacy hole (§12) is precisely what this field must enable going forward. |
| `created_at` | **ADOPT separately from `occurred_at`** | capture lag matters (daemon batches; ghost cycles 5-min); also matches existing `created_at` conventions. |
| *(additions)* `role`, `dedupe_key`, `retention_class` | **ADOPT** | role implements §9's stages; dedupe_key prevents double-ingest when adapters replay (daemon + app both see a clipboard event); retention_class makes pruning declarative instead of the current "nothing is ever pruned." |

**The six object kinds (mandated distinction — these are ROLES on the spine, per §9):**

| Kind | Nature | Strawberry examples (FACT) | Storage policy (RECOMMENDATION) |
|---|---|---|---|
| RAW EVENT | immutable occurrence | clipboard capture, tab visit, publish to bus | append-only, time-retention |
| OBSERVATION | normalized/filtered raw | (missing today — adapters produce these) | append-only, dedupe by key |
| MEMORY | kept representation | chat brief, artifact, resume point | user-visible, revisable, deletable |
| INSIGHT | derived finding | ghost_insights rows | regenerable, cheap, score-bearing |
| TASK | actionable unit | `todos`, autonomy pending actions | lifecycle-owned, never auto-merged with events |
| ACTION | taken step | `CycleOutcome::Executed`, restore attempts | audited, actor-gated |

**What this proposal explicitly does NOT do (per the task's safety rules):** no new database, no second scheduler, no rewrite of `EventBus` internals beyond the subscriber extension, no change to existing tables, no migration edits. The spine lands as ONE new table + adapters in a later phase, behind the phase order in §19.

---

## 11. Duplication / Conflict Audit

*(Mandated format; GREEN = safe, YELLOW = consolidate later, RED = must address before major features.)*

| Area | Current implementations (FACT) | Conflict risk | Class | Recommendation |
|---|---|---|---|---|
| Event vocabularies (§8) | 4 disjoint vocabularies: `autonomous/event.rs` (16 kinds), ghost 14 strings, ambient 5 types, wellness emits | Every new feature must pick a dialect; insights can't see bus events; bus dies on exit | **RED** | Canonical spine unifying all four (§10/§18) — unifier, not replacement |
| Workspace snapshots | `work_snapshots` (006), `work_spaces` (008), `workspace_sessions/items` (011); **two freeze UIs in one view** (`FreezePanel` + `PreviousWorkPanel`, both in PlannerView) | User confusion ("which freeze?"), divergent restore semantics, three schemas to migrate forever | **RED** | Declare 011 the system; retire 006/008 UI paths, then tables |
| Time-block models | `schedule` (002, UI-orphaned), `events` (010/015 live calendar), `todos` (002) | Any temporal feature must choose among 3 models; schedule API is dead weight | **RED** | Retire `schedule` path; todos=task, events=occurrence (§7) |
| DB schema authority | app migrations (15 versions) vs daemon's own `ensure_schema` copy of 001 + out-of-migration `embeddings` table + own `gen_id()` — same file | Schema drift already happened (005's 384-dim comment vs daemon's 768-dim embeddings; screen table drift); daemon can break on app migration and vice versa | **RED** | Daemon must consume `strawberry-core` schema definitions (single source), or write through an app-owned path |
| Screen capture | migration 005 schema vs `commands/screen.rs:31-59` drifted runtime copy (nullable width/height/byte_size; `embedding BLOB` never written; `xcap` dep unused) | Table shape depends on which CREATE ran first; dead column; dead dependency | **RED** | Fix drift when wiring capture (P2, §19); drop xcap dep |
| Chat ID generation | app `new_uuid()` vs daemon `gen_id()` | Same table, two ID grammars — collision risk LOW (both unique-ish) but greppability/confusion HIGH | **YELLOW** | Unify via `strawberry-core` |
| Timestamp conventions | RFC3339 TEXT (wellness, chats) vs unix-ms INTEGER (workspace, screen, tabs) | Wellness compares RFC3339 as strings — correct only while all writers share one format (`wellness/mod.rs`) | **YELLOW** | Canonical spine standardizes ms; leave legacy tables alone |
| ID conventions | TEXT UUIDs (001 domain) vs INTEGER AUTOINCREMENT (screen 005, alpha 012) | Two idioms; cross-references get awkward | **YELLOW** | New tables pick TEXT UUID + ms; no backfill |
| Freeze UI duplication | `FreezePanel` vs `PreviousWorkPanel` (same view) | Two buttons, two mental models, two backends | **RED** (subset of workspace row) | Keep PreviousWorkPanel (011-based) |
| Dialogs | `Dialog.tsx` shared component vs several inline ad-hoc modals | Styling/behavior drift | **GREEN** | Migrate inline modals opportunistically |
| FTS strategies | `chat_fts` (trigger-maintained), `tabs_fts`, `resume_fts`, `screen_fts` | All follow the same pattern; consistent | **GREEN** | Reuse pattern for canonical event search |
| `chat_fts` triggers + daemon | app creates triggers at runtime; daemon writes `chats` assuming they exist | If app never ran, daemon writes unindexed rows; benign but real ordering coupling | **YELLOW** | Move FTS ensure into shared core schema |
| Brief/extraction logic | `strawberry-core` shared by app + daemon | This is *good* duplication-avoidance | **GREEN** | The model to follow elsewhere |
| React state | single Zustand store, no duplication | Low risk | **GREEN** | Keep |
| Error normalization | `call()` wrapper in `api.ts` vs 5 ambient/AST functions bypassing it | Inconsistent error surfaces | **YELLOW** | Route the 5 through `call()` |
| Dead/write-only tables | `cross_chat_suggestions` (004), `ambient_relations` (009), `event_sources` (010), `wellness_activity` (013), `workspace_restore_attempts` (011) | Schema bloat; migration surface grows forever; misleading to future readers | **YELLOW** | Document now; drop in a future migration (never edit existing ones) |

**INFERENCE:** the RED items are all the same disease — *organic growth without a spine* — which is why the fix is a unification layer plus deliberate retirement, not any rewrite.

---

## 12. Privacy / Security Audit

**CURRENT (FACT) vs RECOMMENDED FUTURE:**

| Area | Current state (verified) | Class | Recommended future |
|---|---|---|---|
| **Clipboard capture** | Daemon persists **all** clipboard text — including passwords, tokens, 2FA codes — to `chats`, `chat_fts`, and raw files. Grep of `capture-daemon/src` for blocklist/denylist/redact/secret handling: **zero hits**. Only interactive "Ignored by user" prompts exist (`main.rs:313`, `:357`) | **RED — highest priority** | Blocklist + secret-shape detection (repo already has a precedent: `screen_blocklist` table, 005) at the daemon boundary; `privacy_level` field on the spine (§18) |
| Browser URLs | `tabs.record()` stores full URLs, no blocklist (`commands/tabs.rs`) | RED | Same blocklist mechanism; consider title-only mode |
| Screen capture | `screen_blocklist` (005) exists — but capture never runs, so unexercised | YELLOW | Reuse the *table* when capture is wired |
| Local-only posture | Core is offline (verified: no network outside alpha/news); README's "no network calls in core features" is overstated but the exceptions are opt-in and gated | YELLOW (docs) | Fix README wording; keep gates |
| Alpha hunter network | Gated by `app_meta.alpha_hunter_enabled` (`commands/alpha.rs:42`); `verify` POSTs to user-specified `{base}/chat/completions` with the user's key, message "ping", `max_tokens:10`, key never stored | GREEN (by design, disclosed in UI) | Keep; document egress list |
| News briefing | Gated by `app_meta.news_enabled` — **a flag nothing can ever set** (read at `planner.rs:768`; no writer in Rust or TS). Dead gate ⇒ unreachable feature | GREEN (fail-closed) | Either wire a toggle or remove the section |
| Command execution (restore) | `RunTerminalCommand` requires `confirmed=true`; `OpenUrl` restricted to http/https; `pending_servers` never auto-executed | GREEN | Keep as the pattern for ALL agent-initiated actions |
| External process surface | git, du, df, qdbus6, journalctl, ss, grim, spectacle, xdotool, xdg-open, code, konsole, sh -c | YELLOW | Inventory + audit each; prefer libs where cheap |
| DB file perms | Standard SQLite in user data dir; no encryption | YELLOW (local-first tradeoff) | Document; optional SQLCipher someday — not a Phase 0/1 item |
| Injection risk | Ghost `build_tags_json_array` hand-rolls JSON string escaping for CSV `chats.tags` content — latent injection into anything that parses it as JSON | YELLOW | Use serde_json for the array; test with hostile tag strings |
| Silent failures | Unknown ghost event type → `Ok(0)` (`commands/ghost.rs:56`); story ignores git errors; `_ = conn.execute` patterns | YELLOW (correctness) | Errors should be errors |
| Frontend storage | `CAL_REM:<eventId>|<mins>|<occ>` localStorage keys grow unbounded, never pruned; wellness payload key cleared on mount (good) | YELLOW | Prune on calendar delete + size cap |
| RPC surface | 109 commands, no allowlist differentiation between UI-invoked and agent-invocable | YELLOW | Capability registry (§16) should classify commands by risk |

**UNKNOWN:** actual data at rest in the user's DB (not inspected — privacy); whether any compositor bypasses the KWin marker approach.

---

## 13. Performance Audit

**Polling/watching inventory (FACT):**

| Loop | Interval | Work per tick | Risk |
|---|---|---|---|
| Autonomy supervisor | 5s (2s after errors), 200ms steps | drain bus (empty in practice — no backend publishers), 16-arm match, WorldState update | LOW today; grows with feature count |
| Ghost cycle | 5 min | **full graph wipe-and-rebuild** (graph.rs, 356 lines), quadratic co_access pair generation silently truncated at `LIMIT 1000`, insights DELETE+regen | MEDIUM-HIGH; degrades with event count |
| Wellness tick | 60s | config/state reads, comparisons | LOW |
| Daemon clipboard loop | continuous | sig-based change detection (`compute_bytes_sig`, 64-point sampling) | LOW |
| Frontend timers | PlannerView 2×500ms; GhostPanel 30s; CalendarView 60s + 30s + 300ms debounce; AutonomyPanel 2s | render churn | LOW-MEDIUM |

**Structural risks (FACT + INFERENCE):**

| # | Risk | Evidence | Severity |
|---|---|---|---|
| 1 | Single `Mutex<Connection>` serializes ALL DB commands | `AppState` (state.rs); ambient commands additionally run on the main thread | MEDIUM — fine at personal-data scale; blocks under ghost thread contention (2s busy_timeout mitigates) |
| 2 | Ghost quadratic pair generation with silent truncation | `LIMIT 1000` in co_access | MEDIUM — results silently wrong at scale, not just slow |
| 3 | Unbounded tables: `ghost_events` (prune path `ghost_purge` never called by UI), `CAL_REM:` localStorage keys | `commands/ghost.rs:257`; frontend | MEDIUM — slow growth, permanent |
| 4 | WAL never checkpointed explicitly; abrupt process death on all threads (no exit hook) | §2 | LOW-MEDIUM (WAL recovery is designed for this, but flush-on-exit is trivially better) |
| 5 | `list_screens` ignores its limit/offset/filter args, hardcodes `LIMIT 100` | `commands/screen.rs` | LOW (table is empty anyway) |
| 6 | Frontend re-renders on 500ms timers regardless of data change | PlannerView | LOW |
| 7 | Process spawning per capture/restore (qdbus6, journalctl, ss, screenshot tools) | workspace/screen modules | LOW at current cadence; matters if wired to fast loops |

**RECOMMENDATION:** nothing here is urgent at current data volumes except #2 (silent wrongness) and the exit-hook fix (#4, correctness more than performance). The canonical spine's design (§18) deliberately avoids adding any new polling loop — it extends existing ones.

---

## 14. Testing / Reliability Audit

**Current coverage (FACT):** `cargo test` = 46 passed / 2 ignored, distributed:

| Module | Tests |
|---|---|
| `autonomous` (event + runtime) | 11 |
| `alpha` | 5 |
| `daemon` (db roundtrip, collision, clip sig) | ~5 |
| `db/migrations`, `state`, `handoff`, `tabs`, `snapshot`, `screen` | 1–2 each (screen's is trivial) |
| **Zero tests** in: roots, folders, chats, search, planner (todos/habits/briefing/calendar), resume, story, health, inbox, news, workspace, ghost, ambient, wellness commands | — |
| Frontend | **0 tests, no test runner configured, no CI for anything** |

**Reliability findings (FACT):** silent `Ok(0)` on unknown ghost types (`commands/ghost.rs:56`); story swallows git errors; `_ = conn.execute` patterns; RFC3339 compared as strings (correct only while format is uniform — fragile invariant); migration 8-before-7 execution order (`migrations.rs:116-138`, deliberate but undocumented in SQL — a trap for future migration authors); `news_enabled` dead gate (§12).

**Minimum regression suite (RECOMMENDATION — before Phase 1 changes anything):**

1. **Migration chain test:** fresh DB → run all migrations → assert all 31 tables + triggers exist → insert+read one row per table. (This catches the screen-schema drift class forever.)
2. **Daemon/app interop test:** daemon `insert_capture` → app-side search finds it via `chat_fts`; asserts trigger existence from the daemon path.
3. **FTS fallback test:** simulate trigger failure → assert `app_meta.fts_enabled` records degraded mode and search degrades gracefully.
4. **Round-trip per live domain:** todo, calendar event (with recurrence + reminder), chat import → brief → search → resume, workspace freeze → restore (mock the qdbus6 layer), ghost record → graph → insight.
5. **JS escaping test:** hostile tag strings through `build_tags_json_array`.
6. **Autonomy bus test (extends existing):** publish → two subscribers both receive (after multi-subscriber extension); persistence sink write/read.

---

## 15. Future Feature Dependency Matrix

*(Mandated format; "Existing Source" = verified current code.)*

| Future Feature | Needs | Existing Source | Missing Layer | Recommended Integration |
|---|---|---|---|---|
| **Unified Temporal Memory** | one event log + memory roles | `ghost_events`, `ambient_events`, `chat_resume_points`, bus | canonical spine (§18); retention; roles | Spine + adapters; ghost becomes a *consumer* |
| **Universal Search** | one query surface over heterogeneous stores | 4 FTS indexes + LIKE calendar + semantic (daemon) | federated query planner; screen capture not running (empty domain) | Extend `search_chats` pattern into a fan-out query command; never merge indexes |
| **Project Brain** | project identity + per-project aggregation | `roots` (de-facto projects) | project→root(s) mapping; cross-project joins | `project_id` on the spine (nullable, §10); root remains the UI-facing concept |
| **What Changed** | file/asset change events | **nothing** — no watcher (§6 ③) | file event source; capture wiring | `notify` crate source adapter → spine; ghost already has `FileModified` EventKind (unused) |
| **Intelligent Resume** | session lifecycle + state snapshots | workspace v0.1 (011), autonomy WorldState (in-memory) | session begin/end events; WorldState persistence | Spine `session.*` events; persist WorldState snapshots on session freeze |
| **Agent Task Graph** | tasks + dependencies + provenance | `todos`, autonomy pending actions | graph over tasks; dependency edges; actor/provenance | Tasks stay in `todos`; edges via spine references; **no second task table** |
| **Checkpoint System** | durable state snapshots at points in time | workspace sessions (011), `work_snapshots` (006, legacy) | checkpoint of *memory state*, not just windows | Reuse 011's session/attempt pattern; add memory snapshot as a spine-adjacent artifact |
| **Persistent Agent** | durable autonomy across restarts | autonomy runtime (excellent core, in-memory, starts Paused) | **persistence** (WorldState + queue), backend publishers, capability registry, governor, audit log | Extend `AutonomyRuntime` — see §16; do NOT build a second orchestrator |
| **Research Agent** | safe network + retrieval + citation | alpha (network precedent + gating), FTS, daemon semantic search | agent capability scoping; content store; provenance | Capability registry classifies `network.*`; alpha's gating pattern (`app_meta`) is the template |

**INFERENCE:** the recurring missing layer is either "the capture/wiring is absent" (What Changed, screen, sessions) or "there is no spine to hang it on" (everything else) — never "the domain logic must be rewritten."

---

## 16. Autonomous Agent Compatibility

*(Mandate: per component — already exists / partial / missing / reuse / dependency / risk; principle: ONE orchestrator / ONE event architecture / ONE adaptive scheduling architecture / MANY capability modules; do not recommend 20 independent background loops.)*

| Component | Status (FACT) | Reuse? | Dependencies | Risk |
|---|---|---|---|---|
| **Event Bus** | EXISTS (in-memory, 1 consumer, frontend-published, dies on exit) | **Yes — as the spine** | persistence sink, N subscribers, backend publishers | Medium: extending in place must not break `run_cycle` semantics (11 existing tests protect it) |
| **Capability Registry** | **MISSING** (109 commands are an untyped flat list; `open_workspace_item` defined but not even registered) | N/A | command inventory + risk classification | Low: additive; classify `RunTerminalCommand`-class as gated |
| **World State** | EXISTS (`WorldState`, 256 lines, version counter, RECENT_LIMIT=20) but **in-memory only** — dies on exit; starts Paused | **Yes** | persistence of snapshots | Low-Medium: add save/load behind existing methods; `WorldStateDiff` is dead code to either use or remove |
| **Memory Core** | PARTIAL — implicit memories (artifacts, briefs, resume points, insights) with no unifying role model | Yes, via roles (§9/§18) | spine + role field | Medium: must NOT become a second memory system — it is a *view* over existing stores |
| **Insight Engine** | EXISTS (ghost insights: 10 kinds, fixed scores, regen every 5 min) | **Yes** | read access to spine observations | Low: ghost is read-only over its inputs — ideal consumer |
| **Autonomy Engine** | EXISTS as observation-only runtime (16-arm `apply_event`, Paused start, safety-gated actions designed but only `Pending`/`Executed` outcomes produced today) | **Yes — THE one orchestrator** | capability registry, governor, audit log, persistence | **High if parallel**: any second orchestrator or per-feature background loops would violate the ONE-orchestrator principle; the 3 existing loops already cover supervisor/analysis/wellness cadences |
| **Resource Governor** | **MISSING** (no rate limits, no budget concept; autonomy's adaptive 5s→2s interval is the only adaptive behavior in the repo) | N/A | autonomy runtime hooks | Low: additive |
| **Privacy Filter** | **MISSING at capture** (§12: daemon stores everything); screen blocklist table exists unexercised | Reuse `screen_blocklist` *pattern* | spine `privacy_level` | **High until the daemon gap closes** — an agent reading memories inherits unfiltered secrets |
| **User Control** | PARTIAL — alpha has an on/off gate (`set_alpha_enabled`, persisted in `app_meta`); autonomy has mode (Paused start); no per-agent grants UI | Yes — `app_meta` gate pattern | capability registry | Low |
| **Audit Log** | **MISSING** — nothing records *who did what when* (workspace attempts are the closest, write-only) | N/A | spine persistence; actor field | Medium: must exist before any agent acts autonomously |

**Summary (INFERENCE):** the codebase already obeys "one orchestrator" *de facto* — one runtime, one bus, three loops with distinct jobs. The correct path is to grow `AutonomyRuntime` and the bus, add the four missing components (registry, governor, audit, persistence) as *modules of the existing runtime*, and wire every future agent as a **capability module** publishing through the ONE spine.

---

## 17. Minimal Target Architecture

```ascii
┌─────────────────────────── UI (React) ────────────────────────────┐
│  views (9) ── dialogs ── Zustand store ── call() error-normalizer │  [EXISTING / REUSE]
└──────────────────────────────┬───────────────────────────────────┘
                               │ 109 commands (EXTEND, never fork)
┌──────────────────────────────▼───────────────────────────────────┐
│                    Tauri app (Rust)                                │
│                                                                   │
│  ┌──────────── Capability Registry (NEW LAYER) ─────────────┐     │
│  │  risk-classified command/agent inventory, gates, audit    │     │
│  └────────────────────────────┬────────────────────────────┘     │
│                               │                                   │
│  ┌──── ONE event spine (EXTEND autonomous/event.rs) ────────┐    │
│  │  EventBus: N subscribers (was 1) + optional persistence  │     │
│  │  sink → core_events table (NEW LAYER)                    │     │
│  │  CanonicalEvent: id, occurred_at_ms, type, source,       │     │
│  │  actor, ref, payload, privacy_level, role, dedupe_key    │     │
│  └──┬─────────┬──────────┬──────────┬──────────┬───────────┘     │
│     │adapters (NEW LAYER, thin)                                   │
│  ┌──▼───┐ ┌──▼─────┐ ┌──▼──────┐ ┌──▼────────┐ ┌──▼────────────┐ │
│  │ ghost│ │ambient │ │ capture │ │ tabs/screen│ │ calendar (RO) │ │
│  │(014) │ │(009)   │ │ daemon  │ │ sources    │ │ adapter       │ │
│  │KEEP  │ │KEEP    │ │KEEP+FIX │ │ (wire/fix) │ │ no dup models │ │
│  └──────┘ └────────┘ └─────────┘ └────────────┘ └───────────────┘ │
│                                                                   │
│  ONE orchestrator: AutonomyRuntime [EXTEND]                      │
│    + WorldState persistence (EXTEND)   + governor/audit (NEW)     │
│  Consumers (EXISTING/REUSE): ghost insights · wellness · briefing │
│  Scheduler: the 3 existing threads [REUSE — no new loops]         │
└───────────────────────────────────────────────────────────────────┘
┌── Shared foundations ─────────────────────────────────────────────┐
│  SQLite app.db (EXISTING/REUSE) · migrations 001–016+ (EXTEND)     │
│  strawberry-core (REUSE — grows schema authority for daemon)      │
│  capture-daemon (KEEP + FIX: filtering, shared schema)            │
└───────────────────────────────────────────────────────────────────┘
  [DEFER]            embeddings-in-app · OCR · file watcher source ·
                     project concept · semantic cross-store ranking
  [REMOVE LATER]     work_snapshots(006) + work_spaces(008) UI paths ·
                     schedule(002) orphan path · dead tables (004
                     cross_chat_suggestions, 009 ambient_relations,
                     010 event_sources) · screen `embedding` column ·
                     xcap dependency · WorldStateDiff (or implement it)
```

Legend: **EXISTING** keep as-is · **REUSE** use without modification · **EXTEND** add to in place · **NEW LAYER** net-new but additive · **DEFER** not now · **REMOVE LATER** retire via future migration/UI removal (never by editing history).

---

## 18. Canonical Event Specification — DESIGN ONLY

*(18 mandated sub-items. Nothing below is implemented. All names provisional.)*

1. **Lifecycle:** append → (optional) role promotion (raw→observation→memory) via explicit logged transition → retention expiry → hard delete. Events themselves are immutable; only role/retention metadata changes.
2. **Identity:** UUIDv4 text PK (matches 001 conventions). No auto-increment (spans processes: app + daemon + future agents). `dedupe_key` (nullable, UNIQUE-when-present) = hash of source+source-id+occurred-window.
3. **Schema (one table, `core_events` — name chosen to avoid the `events` calendar collision, §7):** `id TEXT PK`, `occurred_at_ms INTEGER NOT NULL`, `created_at_ms INTEGER NOT NULL`, `event_type TEXT NOT NULL` (`source.kind` namespaced), `source_type TEXT NOT NULL`, `source_id TEXT`, `actor TEXT NOT NULL DEFAULT 'user'`, `role TEXT NOT NULL DEFAULT 'raw_event'`, `ref_table TEXT`, `ref_id TEXT`, `payload TEXT` (JSON, size-capped), `privacy_level TEXT NOT NULL DEFAULT 'sensitive'`, `project_id TEXT NULL`, `session_id TEXT NULL`, `provenance TEXT NULL`, `confidence REAL NULL`, `dedupe_key TEXT NULL`, `retention_class TEXT NOT NULL DEFAULT 'standard'`. Indexes: `(occurred_at_ms)`, `(event_type, occurred_at_ms)`, `(ref_table, ref_id)`.
4. **Source adapter interface:** trait `EventSource { fn name() -> &'static str; fn poll_and_publish(&mut self, bus: &EventBus) -> Result<usize>; }` — implemented first by bridges over ghost/ambient/tabs/capture/calendar-RO. Adapters are *the only* writers of observations besides direct commands.
5. **Normalization rules:** timestamps → unix ms at ingest; payloads must round-trip through serde_json (no hand-rolled escaping — directly addresses the `build_tags_json_array` finding); strings size-capped; enums serialized as lowercase kebab.
6. **Provenance:** minimal chain string `source@adapter_version` per event; adapters bump version on semantic change. Full chain-of-custody deferred.
7. **Privacy classification:** `public` / `sensitive` (default — personal but shareable with local agents) / `secret` (never surfaced to agents or insight generation; excluded from exports). Capture adapters default clipboard text to `secret` **unless** the daemon filtering fix (§12) has landed.
8. **Timestamps:** `occurred_at_ms` = when it happened; `created_at_ms` = when recorded. Never conflate (daemon batching and 5-min ghost cycles make the difference real).
9. **Project/session/workspace relationships:** nullable `project_id` (future mapping over roots), `session_id` (references `workspace_sessions`, 011). No `workspace_id` — 008 is legacy and must not gain a new identity (§10).
10. **Persistence strategy:** the EventBus gains an optional sink trait; the default sink writes observations to `core_events` on the ghost thread's *own connection* pattern (never the AppState mutex). Bus stays in-memory-fast; persistence is a subscriber, not the bus itself.
11. **Indexing:** start with the three B-tree indexes (sub-item 3); add FTS5 over `event_type || payload` only when a consumer needs text search (do not pre-build).
12. **Retention:** `retention_class` → policy table (e.g. `transient` 7d, `standard` 180d, `durable` keep). Pruning is one periodic statement inside the **existing** ghost or wellness thread cadence — no new loop.
13. **Dedup:** on insert, `INSERT OR IGNORE` keyed on `dedupe_key`; adapters replay-tolerant by design (the daemon + app can both observe one clipboard event).
14. **Error handling:** unknown event types are **errors at the adapter**, never silently dropped (fixes the `Ok(0)` class); sink failures degrade the bus to in-memory-only with a logged warning, never panic a thread.
15. **Versioning:** `schema_migrations` gains version 016 creating `core_events`. Event *format* versioning lives in `provenance` adapter versions, not extra columns.
16. **Backwards compatibility:** zero changes to existing tables. Ghost/ambient/tabs keep their tables and semantics; adapters *copy-forward* into the spine. Any eventual migration of ghost internals is a later, separate decision.
17. **Migration strategy:** standard numbered migration (016). The four adapters land in dependency order (§19). No data backfill in v0 — the spine starts empty and accumulates; ghost history remains authoritative for its own domain.
18. **Test strategy:** multi-subscriber bus test; sink write/read round-trip; dedupe test; hostile-payload escaping test; adapter contract test (each adapter: poll twice → same dedupe_key → one row); retention prune test; privacy-exclusion test (secret rows invisible to insight consumers).

---

## 19. Recommended Implementation Order

*(Derived from the codebase — capture/wiring gaps and safety holes gate everything else; the spine is only useful once its sources exist.)*

**Phase 1 — Safety & correctness fixes (small, surgical, no architecture):**
1. Exit hook: flip `ShutdownFlags` in a `.run()` exit hook (`lib.rs`) so threads flush and stop.
2. Ghost unknown event type → `Err`, not `Ok(0)` (`commands/ghost.rs:56`).
3. Replace hand-rolled JSON escaping with serde_json (ghost tags).
4. Wellness RFC3339 string compares → parsed comparisons.
5. Daemon capture filtering (blocklist + secret-shape detection) — the §12 RED item; needed *before* any agent ever reads memories.
6. `news_enabled`: wire a toggle or remove the dead gate.
7. Screen schema drift fix + drop unused `xcap` dependency (precondition for ever wiring capture).

**Phase 2 — Consolidation (retire duplicates before building on top):**
1. Choose workspace 011 as the system; remove `FreezePanel` from PlannerView (keep `PreviousWorkPanel`).
2. Retire the orphaned `schedule` API path from the UI layer (todos + events remain).
3. Daemon schema authority: move schema definitions into `strawberry-core` so both processes share one source of truth; align `gen_id()`/`new_uuid()`.
4. Route the 5 ambient functions through the `call()` error normalizer; prune `CAL_REM:` keys.
5. Land the minimum regression suite (§14) — this phase changes reachable behavior, so it goes first here and protects everything after.

**Phase 3 — Canonical Event Foundation (first architecture work):**
1. Migration 016: `core_events` table (spec §18).
2. Extend `EventBus`: subscriber registry (N consumers) + optional persistence sink.
3. Adapters, in order of value: capture/daemon bridge → ghost bridge → tabs → ambient → calendar read-only bridge.
4. Backend publishers: wire real sources (chat created, search executed, todo toggled) so the bus stops being frontend-demo-only.

**Phase 4 — Memory & search unification (only after 3):**
Universal search fan-out command; memory roles made user-visible; retention enforcement.

**Phase 5 — Autonomy extensions (only after 3 + Phase 1.5 governor prerequisites):**
WorldState persistence; capability registry (classify the 109 commands by risk); resource governor; audit log; then — only then — resume the autonomy core loop beyond observation.

**Explicitly deferred:** embeddings inside the app, OCR runtime, file-watcher source (candidate for early Phase 3.5 if "What Changed" is prioritized), project concept beyond nullable `project_id`, any second process/daemon/API.

---

## 20. GO / NO-GO Decisions

| Decision | Verdict | Rationale |
|---|---|---|
| Canonical Event Foundation | **GO** (Phase 3) | four vocabularies with zero unification is the single biggest structural blocker (§8, §11) |
| …as a *unifier* of existing systems | **GO — mandatory framing** | any fifth vocabulary or new bus would deepen the exact problem |
| Reuse `AutonomyRuntime` + `EventBus` as the ONE orchestrator/spine | **GO** | sound core, 11 tests, safety-gated design (§16) |
| Reuse ghost as the insight engine | **GO** | read-only consumer over spine inputs; fixes (quadratic, escaping, Ok(0)) are local |
| Reuse calendar `events` as THE temporal occurrence model | **GO** | no duplicate models; read-only bridge (§7) |
| Screen capture wiring | **GO (after Phase 1.7)** | complete pipeline exists; only spawner + drift fix missing |
| Daemon rewrite | **NO-GO** | fix filtering + schema authority in place; the daemon's architecture (separate process, opt-in embeddings) is correct |
| Any new background thread/loop | **NO-GO by default** | 3 loops already cover cadences; spine extends existing threads (§13, §16 principle) |
| `work_snapshots`/`work_spaces` consolidation | **GO (Phase 2)** | two live freeze UIs is a user-facing conflict |
| `schedule` retirement | **GO (Phase 2)** | UI-orphaned; three time models is one too many |
| Frontend rewrite / router | **NO-GO** | 9-view switcher is fine at this scale |
| Migrating ghost's tables into the spine | **DEFER** | adapters first; table retirement is a later, evidence-based call |
| Dead-table removal | **DEFER (documented now)** | never edit existing migrations; removal is a future migration + code sweep |
| New persistence layer / second database | **NO-GO** | existing SQLite + WAL + migrations are healthy (§2, §11) |
| Embeddings in-app, OCR, file watcher | **DEFER** | no consumer yet; Phase 4+ |

**RED FLAGS (would invalidate this plan):** a fifth event vocabulary appearing before Phase 3; a second scheduler; a second task table; any new "memory" table that duplicates `chat_artifacts`/briefs; per-feature background threads; touching migrations 001–015.

---

## 21. Risks and Unknowns

**Risks (INFERENCE):**
- The bus extension (Phase 3.2) touches the autonomy runtime that currently has the best test coverage — regression risk is real but contained by the existing 11 tests plus new subscriber tests.
- Retiring 006/008 paths removes working user-facing features (`FreezePanel`); must ship with the 011 path visibly covering the same need.
- Daemon filtering (Phase 1.5) risks false positives silently dropping wanted captures — needs a visible count of suppressed events, not just silence.
- Two-process DB writes continue until Phase 2.3; any new migration in between must be daemon-tested (the 005 drift shows the failure mode).
- UNKNOWN: how much data real users have (all performance analysis assumes personal-scale volumes).

**Unknowns (UNKNOWN):** Wayland/compositor variance beyond KWin; Chrome/Firefox profile format drift; whether `show_popup` in `wellness/popup.rs` has any caller path not visible to static analysis; actual contents at rest in `app.db`.

---

## 22. Files Inspected

**Read in full:** `src-tauri/src/lib.rs`; all `src-tauri/migrations/001–015.sql`; `capture-daemon/src/{main.rs,db.rs,semantic.rs,handoff.rs,clip.rs}`; `src-tauri/src/commands/{story.rs,screen.rs,news.rs}`; `src/main.tsx`; `src/components/Layout/AppLayout.tsx`; `src/lib/types.ts`.

**Read in substantive part:** `src-tauri/src/commands/{planner.rs,ghost.rs,alpha.rs,ambient.rs,tabs.rs}`; `src-tauri/src/{autonomous/event.rs,autonomous/runtime.rs,autonomous/world_state.rs,ghost/{tracker.rs,graph.rs,insights.rs,attention.rs},wellness/*,workspace/*,snapshot/*,screen/*,alpha/mod.rs,storage/files.rs,state.rs}`; `src/lib/api.ts`; `src/styles/global.css`; `capture-daemon/src/main.rs` (privacy grep pass).

**Verified via search/grep (no full read):** complete SQL table inventory (all CREATE TABLEs, app + daemon); `generate_handler` command list (109, counted twice); event-vocabulary cross-references (zero); tokio/channel/notify absence; daemon blocklist absence; `news_enabled` writer absence; dead/write-only table confirmation (`cross_chat_suggestions`, `ambient_relations`, `event_sources`, `wellness_activity`, `workspace_restore_attempts`); `prune_older_than` caller; `run_loop` spawner absence; `app_meta` key inventory.

**Agent-assisted deep reads (reports incorporated, spot-verified):** frontend `src/` (42 components/views incl. `CalendarView.tsx`, `PlannerView`, `GhostPanel`, `AutonomyPanel`, `WellnessCard.tsx`, `PreviousWorkPanel`, `FreezePanel`, `AlphaHunter.tsx`, `Dialog.tsx`, `HomeView`, `InboxView`, `ScreensView`, `AmbientMemoryView`, `DashboardView`, `SearchBox`); `src-tauri/src/commands/*` (all 20+ modules); `src-tauri/src/db/migrations.rs`.

**Not modified by this task (FACT):** no source file was changed; the only repository modification is this report file.

---

## 23. Appendix

### A. The 12 audit questions → where answered

| Question | Section |
|---|---|
| What exists today | §2–§4 |
| Who owns each piece of data | §5 |
| How data enters the system | §6 (flows ①②⑨), §4 daemon rows |
| Where data is stored | §5 table map, §2 DB facts |
| How data is queried | §6 ⑩, §7, §13 |
| How subsystems communicate | §8 (they mostly don't), §3 diagram |
| Where responsibility is duplicated | §11 |
| What should be reused | §20 GO/REUSE rows |
| What should be unified | §8, §10, §11 RED rows |
| Where the Canonical Event Model fits | §10, §17, §18 |
| What must stay separate | §9 (kinds), §7 (calendar), §16 |
| Safest architecture for autonomous agents | §16, §17, §19 Phase 5 |

### B. Naming hazards (FACT — read before touching anything)

| Name | Actually is | Trap |
|---|---|---|
| `events` table | **Calendar** data (010/015) | not an event bus; canonical spine must use a different name |
| `event_sources` table | dead (no writers/readers) | not a source-registry precedent |
| `screen_fts` | exists, empty (capture never runs) | wiring capture later will populate, not create |
| `Wellness` "popup" | two implementations: backend `popup.rs show_popup` (likely dead) + frontend window via `main.tsx` | check callers before editing either |
| `autonomy_publish` | frontend demo → bus | not a backend integration point yet |

### C. Convention drift cheat-sheet (FACT)

| Dimension | Convention A | Convention B | Where |
|---|---|---|---|
| IDs | TEXT UUID (`new_uuid()`) | INTEGER AUTOINCREMENT | 001 domain vs 005/012 |
| Timestamps | ISO-8601 / RFC3339 TEXT | unix-ms INTEGER | chats/wellness vs workspace/screen/tabs |
| Migration order | sequential | **8 runs before 7** (deliberate) | `db/migrations.rs:116-138` |
| rusqlite | 0.31 (app) | 0.32 (daemon) | Cargo.tomls |
| Embedding dims | "384-dim float32" (005 comment) | 768 (daemon Ollama) | `semantic.rs` |
| Error style | `Cmd<T>` string errors | silent `Ok(0)` / `_ =` | ghost.rs:56, story.rs |

### D. Verified command inventory snapshot

109 commands registered in `generate_handler` (`src-tauri/src/lib.rs`). Notable non-registrations (FACT): `open_workspace_item` defined but unregistered; `news::fetch_top_headlines` is a function, not a command (called internally by the briefing command); `RebuildGraphArgs` unused.

### E. External-process egress inventory (FACT)

git · du · df · qdbus6 · journalctl · ss · grim · spectacle · scrot · import · xdotool · xdg-open · code · konsole · sh -c. Network (all ureq, 8s, opt-in): hn.algolia.com (news + alpha) · reddit.com · openrouter.ai · api.github.com · huggingface.co · producthunt.com.

---

*End of report. This document is the only repository modification produced by the Phase 0 audit task; every item labeled RECOMMENDATION remains unimplemented by design.*
