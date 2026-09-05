mod alpha;
mod autonomous;
mod brief;
mod commands;
mod db;
mod docx;
mod error;
mod ghost;
mod intelligence;
mod memory;
mod project;
mod resume;
mod screen;
mod snapshot;
mod state;
mod tabs;
mod storage;
mod wellness;
mod workspace;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use autonomous::AutonomyRuntime;
use state::AppState;
use tauri::Manager;
use wellness::WellnessAgent;

// Re-export AutonomousWorker for visibility from lib.rs's thread.
use crate::autonomous::worker::AutonomousWorker;
use crate::autonomous::Orchestrator;

/// Holds the shutdown flags the background threads watch.
/// Flipped on app exit so threads stop cooperatively instead of being killed.
struct ShutdownFlags {
    autonomy: Arc<AtomicBool>,
    ghost: Arc<AtomicBool>,
    watcher: Arc<AtomicBool>,
    indexer: Arc<AtomicBool>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let st = AppState::init(data_dir).map_err(|e| -> Box<dyn std::error::Error> {
                e.into()
            })?;
            app.manage(Arc::new(st));
            app.manage(screen::capture::CaptureHandle::default());

            // Wellness agent: shared, mutex-protected, with shutdown signal.
            let agent = WellnessAgent::new(app.handle().clone());
            // Hydrate persisted global state (enabled / snoozed_until) before start.
            WellnessAgent::load_state_from_db(&agent, &app.handle());
            app.manage(agent.clone());
            WellnessAgent::start(agent);

            // 🤖 Spawn the Autonomous Runtime — observe → world state → goal → plan → safety → execute → verify → learn.
            // The runtime is started in Stopped/Paused mode by default; the user enables it via a Tauri command.
            // The background thread drives the full pipeline (Phase 23 — worker.rs).
            // On startup, restore world-state context from disk (Phase 27 — restart/persistence).
            // SAFETY: running mode is always downgraded to Paused on restore.
            let ghost_state_for_path = app.state::<Arc<AppState>>().inner().clone();
            let autonomy_db_path = ghost_state_for_path.db_path();
            let runtime_state_path = ghost_state_for_path.data_dir.join("runtime_state.json");
            let autonomy = AutonomyRuntime::restore(&runtime_state_path);
            let autonomy_for_thread = autonomy.clone();
            let autonomy_shutdown = Arc::new(AtomicBool::new(false));
            app.manage(autonomy);
            let autonomy_shutdown_thread = autonomy_shutdown.clone();

            // Dedicated orchestrator for the autonomy thread (separate from ghost's).
            let autonomy_orch = Arc::new(Orchestrator::new());
            let autonomy_orch_for_thread = autonomy_orch.clone();
            let autonomy_db_path_thread = autonomy_db_path.clone();
            let runtime_state_path_thread = runtime_state_path.clone();
            std::thread::spawn(move || {
                let mut last_persist = std::time::Instant::now();
                while !autonomy_shutdown_thread.load(std::sync::atomic::Ordering::Relaxed) {
                    if autonomy_for_thread.mode() == autonomous::runtime::RuntimeMode::Running {
                        // 1. Drain events and update world state (Phase 1).
                        let _ = autonomy_for_thread.run_cycle(32);
                        // 2. Run the full autonomous worker (goal → plan → safety → execute → verify → replan).
                        let worker = AutonomousWorker::new(
                            &autonomy_db_path_thread,
                            &autonomy_shutdown_thread,
                            &autonomy_orch_for_thread,
                        );
                        let _outcome = worker.run_cycle(3, 32);

                        // 3. Persist state every 30s so we can restore after restart.
                        if last_persist.elapsed() > std::time::Duration::from_secs(30) {
                            let _ = autonomy_for_thread.persist(&runtime_state_path_thread);
                            last_persist = std::time::Instant::now();
                        }

                        let sleep = autonomy_for_thread.suggested_cycle_interval();
                        // Sleep in small steps so shutdown is responsive.
                        let mut slept = std::time::Duration::ZERO;
                        let step = std::time::Duration::from_millis(200);
                        while slept < sleep
                            && !autonomy_shutdown_thread
                                .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            std::thread::sleep(step);
                            slept += step;
                        }
                    } else {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
            });

            // 👻 Spawn the Ghost: rebuild graph + regenerate insights on the
            // CENTRAL scheduler's cadence (capability: ghost_insights).
            // Uses its own SQLite connection to avoid blocking the AppState
            // lock (which would freeze every other command for the duration
            // of the rebuild).
            let ghost_state = app.state::<Arc<AppState>>().inner().clone();
            let ghost_shutdown = Arc::new(AtomicBool::new(false));
            let ghost_db_path = ghost_state.db_path();
            let ghost_db_path_for_indexer = ghost_db_path.clone();
            let ghost_shutdown_thread = ghost_shutdown.clone();
            let ghost_orch = Arc::new(autonomous::Orchestrator::new());
            std::thread::spawn(move || {
                let orch = ghost_orch;
                while !ghost_shutdown_thread.load(std::sync::atomic::Ordering::Relaxed) {
                    // Adaptive interval from the scheduler (user override +
                    // system pressure aware), slept in 1-second steps so
                    // shutdown stays responsive.
                    let wait_secs = {
                        match rusqlite::Connection::open(&ghost_db_path) {
                            Ok(gc) => orch.effective_interval_secs(&gc, "ghost_insights", 0),
                            Err(_) => 300,
                        }
                    };
                    for _ in 0..wait_secs.max(1) {
                        if ghost_shutdown_thread.load(std::sync::atomic::Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                    // Scheduler gate: registry state + live system context.
                    let proceed = {
                        match rusqlite::Connection::open(&ghost_db_path) {
                            Ok(gc) => orch.gate(&gc, "ghost_insights", 0.6, 0, 3).proceed,
                            Err(_) => true,
                        }
                    };
                    if proceed {
                        let _ = ghost::run_cycle_offline(&ghost_db_path, &ghost_shutdown_thread);
                    }
                }
            });

            // 👁️ Spawn the persistent FileWatcher thread.
            //
            // This thread polls the SHARED `FileWatcherRunner` (held in
            // AppState) and publishes file events to the EventBus. The
            // bus is then drained by the file→memory indexer (below).
            //
            // Cooperative shutdown: the thread checks `watcher_shutdown`
            // every 200ms.
            let watcher_state = app.state::<Arc<AppState>>().inner().clone();
            let watcher_runner = watcher_state.watcher.clone();
            let watcher_shutdown = Arc::new(AtomicBool::new(false));
            let watcher_shutdown_thread = watcher_shutdown.clone();
            // Get the runtime's EventBus so we publish into the same bus
            // the autonomy runtime drains.
            let runtime_event_bus = {
                use crate::autonomous::runtime::AutonomyRuntime;
                let rt: tauri::State<'_, Arc<AutonomyRuntime>> = app.state();
                rt.bus()
            };
            std::thread::spawn(move || {
                let bus = runtime_event_bus;
                while !watcher_shutdown_thread.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = watcher_runner.tick(&bus);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            });

            // 📥 Spawn the file→memory indexer thread.
            //
            // Drains file events from the EventBus and writes them to the
            // `unified_memories` table as Document-kind memory records.
            // Privacy gate is applied inside the indexer.
            let indexer_db_path = ghost_db_path_for_indexer;
            let indexer_event_bus = {
                use crate::autonomous::runtime::AutonomyRuntime;
                let rt: tauri::State<'_, Arc<AutonomyRuntime>> = app.state();
                rt.bus()
            };
            let indexer_shutdown = Arc::new(AtomicBool::new(false));
            let indexer_shutdown_thread = indexer_shutdown.clone();
            std::thread::spawn(move || {
                use crate::autonomous::file_indexer;
                let bus = indexer_event_bus;
                let mut last_persist = std::time::Instant::now();
                while !indexer_shutdown_thread.load(std::sync::atomic::Ordering::Relaxed) {
                    let processed = match rusqlite::Connection::open(&indexer_db_path) {
                        Ok(conn) => file_indexer::process_bus_events(
                            &conn,
                            &bus,
                            &indexer_shutdown_thread,
                        ),
                        Err(_) => 0,
                    };
                    if processed == 0 {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    if last_persist.elapsed() > std::time::Duration::from_secs(60) {
                        last_persist = std::time::Instant::now();
                    }
                }
            });

            // Stash all shutdown flags in managed state so they can be
            // flipped on app exit.
            app.manage(ShutdownFlags {
                autonomy: autonomy_shutdown,
                ghost: ghost_shutdown,
                watcher: watcher_shutdown,
                indexer: indexer_shutdown,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::roots::get_roots,
            commands::roots::create_root,
            commands::roots::rename_root,
            commands::roots::delete_root,
            commands::roots::get_root_tree,
            commands::roots::get_children,
            commands::roots::get_node_path,
            commands::roots::get_breadcrumb,
            commands::folders::create_folder,
            commands::folders::rename_folder,
            commands::folders::delete_folder,
            commands::folders::move_folder,
            commands::chats::create_chat_from_text,
            commands::chats::import_chat_file_text,
            commands::chats::get_chat,
            commands::chats::get_chat_raw,
            commands::chats::delete_chat,
            commands::chats::move_chat,
            commands::chats::update_chat_metadata,
            commands::chats::regenerate_brief,
            commands::handoff::export_handoff,
            commands::handoff::handoff_from_text,
            commands::search::search_chats,
            commands::search::search_all,
            commands::dbview::get_db_overview,
            commands::docx::docx_list,
            commands::docx::docx_new,
            commands::docx::docx_open,
            commands::docx::docx_save,
            commands::docx::docx_delete,
            commands::docx::docx_parse_paste,
            commands::docx::docx_search,
            commands::docx::docx_export,
            commands::docx_link::docx_link_block_to_memory,
            commands::docx_link::docx_unlink_block_memory,
            commands::docx_link::docx_list_block_memories,
            commands::docx_link::docx_list_memory_blocks,
            commands::project::get_project_brain,
            commands::project::get_what_changed,
            commands::project::get_intelligent_resume,
            commands::planner::get_todos,
            commands::planner::add_todo,
            commands::planner::toggle_todo,
            commands::planner::delete_todo,
            commands::planner::get_habits,
            commands::planner::add_habit,
            commands::planner::toggle_habit_today,
            commands::planner::toggle_habit_date,
            commands::planner::log_focus_session,
            commands::planner::get_focus_stats,
            commands::planner::get_schedule,
            commands::planner::add_event,
            commands::planner::list_calendar_events,
            commands::planner::create_calendar_event,
            commands::planner::update_calendar_event,
            commands::planner::delete_calendar_event,
            commands::planner::list_event_reminders,
            commands::planner::search_calendar_events,
            commands::planner::get_daily_briefing,
            commands::tabs::record_tab_visit,
            commands::tabs::list_tab_groups,
            commands::tabs::find_tabs_for_topic,
            commands::resume::get_resume_suggestions,
            commands::resume::save_resume_point,
            commands::resume::dismiss_resume_point,
            commands::resume::get_day_summary,
            commands::story::export_my_story,
            commands::health::health_report,
            commands::health::ping,
            commands::alpha::scan_alpha,
            commands::alpha::list_alpha_candidates,
            commands::alpha::verify_alpha_candidate,
            commands::alpha::dismiss_alpha_candidate,
            commands::alpha::get_alpha_config,
            commands::alpha::get_alpha_enabled,
            commands::alpha::set_alpha_enabled,
            commands::inbox::get_inbox_items,
            commands::inbox::get_inbox_counts,
            commands::inbox::delete_inbox_item,
            commands::screen::start_screen_capture,
            commands::screen::stop_screen_capture,
            commands::screen::get_screen_config,
            commands::screen::update_screen_config,
            commands::screen::list_screens,
            commands::screen::search_screens,
            commands::screen::get_screen_frame,
            commands::screen::delete_screen_frame,
            commands::screen::add_screen_blocklist,
            commands::screen::remove_screen_blocklist,
            commands::screen::list_screen_blocklist,
            commands::workspace::capture_workspace_snapshot,
            commands::workspace::freeze_workspace,
            commands::workspace::list_workspace_sessions,
            commands::workspace::get_workspace_session,
            commands::workspace::resume_workspace_session,
            commands::workspace::retry_workspace_item,
            commands::workspace::delete_workspace_session,
            commands::snapshot::capture_work_snapshot,
            commands::snapshot::get_latest_work_snapshot,
            commands::snapshot::list_work_snapshots,
            commands::wellness::wellness_get_state,
            commands::wellness::wellness_set_enabled,
            commands::wellness::wellness_snooze,
            commands::wellness::wellness_get_config,
            commands::wellness::wellness_set_category,
            commands::wellness::wellness_record_activity,
            commands::wellness::wellness_dismiss,
            commands::wellness::wellness_test_popup,
            commands::ghost::ghost_record_event,
            commands::ghost::ghost_rebuild_graph,
            commands::ghost::ghost_regenerate_insights,
            commands::ghost::ghost_get_snapshot,
            commands::ghost::ghost_mark_seen,
            commands::ghost::ghost_purge,
            commands::autonomy::autonomy_get_state,
            commands::autonomy::autonomy_start,
            commands::autonomy::autonomy_pause,
            commands::autonomy::autonomy_resume,
            commands::autonomy::autonomy_shutdown,
            commands::autonomy::autonomy_run_cycle,
            commands::autonomy::autonomy_publish,
            commands::autonomy::list_capabilities,
            commands::autonomy::set_capability_enabled,
            commands::autonomy::set_capability_interval,
            commands::autonomy::get_capability_ledger,
            commands::autonomy::get_goal_candidates,
            commands::autonomy::get_plans,
            commands::ambient::record_ambient_event,
            commands::ambient::get_ambient_events,
            commands::ambient::analyze_code_ast,
            commands::ambient::get_ambient_stats,
            commands::ambient::generate_deterministic_report,
            commands::get_app_info,
            commands::intelligence::ai_get_status,
            commands::intelligence::ai_set_enabled,
            commands::intelligence::ai_configure_provider,
            commands::intelligence::ai_test_connection,
            commands::intelligence::ai_list_models,
            commands::intelligence::ai_remove_credential,
            commands::watcher::watcher_start,
            commands::watcher::watcher_stop,
            commands::watcher::watcher_list,
            commands::watcher::watcher_check_path,
            commands::memory::memory_create,
            commands::memory::memory_get,
            commands::memory::memory_delete,
            commands::memory::memory_update,
            commands::memory::memory_record_view,
            commands::memory::memory_record_copy,
            commands::memory::memory_record_use,
            commands::memory::memory_search,
            commands::memory::memory_create_relationship,
            commands::memory::memory_list_relationships,
            commands::memory::memory_count,
            commands::credentials::credential_create,
            commands::credentials::credential_get_metadata,
            commands::credentials::credential_search,
            commands::credentials::credential_reveal,
            commands::credentials::credential_update_secret,
            commands::credentials::credential_delete,
            commands::credentials::credential_secret_store_status,
            commands::images::image_register,
            commands::images::image_get,
            commands::images::image_list,
            commands::images::image_delete,
            commands::images::image_set_ocr_text,
            commands::images::image_mark_ocr_failed,
            commands::images::image_mark_ocr_unavailable,
            commands::images::image_ocr_run_next,
            commands::autonomy::autonomy_get_stats,
            commands::autonomy::autonomy_get_ledger,
            commands::autonomy::autonomy_get_goals,
        ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building Chat Memory Tree");

    // Graceful shutdown: flip all background-thread shutdown flags
    // so threads stop cooperatively before the process exits.
    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(flags) = app_handle.try_state::<ShutdownFlags>() {
                flags
                    .autonomy
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                flags
                    .ghost
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                flags
                    .watcher
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                flags
                    .indexer
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if let Some(agent) = app_handle.try_state::<Arc<std::sync::Mutex<WellnessAgent>>>() {
                WellnessAgent::signal_shutdown(agent.inner());
            }
        }
    });
}
