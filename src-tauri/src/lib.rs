mod alpha;
mod autonomous;
mod brief;
mod commands;
mod db;
mod error;
mod ghost;
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

/// Holds the two shutdown flags the background threads watch.
struct ShutdownFlags {
    autonomy: Arc<AtomicBool>,
    ghost: Arc<AtomicBool>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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

            // 🤖 Spawn the Autonomous Runtime — observe → world state → (later: goal/plan/exec).
            // Phase 1 only observes and updates world state. The runtime is started
            // in Paused mode by default; the user can enable it via a Tauri command.
            let autonomy = AutonomyRuntime::new();
            let autonomy_for_thread = autonomy.clone();
            let autonomy_shutdown = Arc::new(AtomicBool::new(false));
            app.manage(autonomy);
            let autonomy_shutdown_thread = autonomy_shutdown.clone();
            std::thread::spawn(move || {
                while !autonomy_shutdown_thread.load(std::sync::atomic::Ordering::Relaxed) {
                    if autonomy_for_thread.mode() == autonomous::runtime::RuntimeMode::Running {
                        let _ = autonomy_for_thread.run_cycle(32);
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

            // 👻 Spawn the Ghost: every 5 minutes, rebuild graph + regenerate insights.
            // Uses its own SQLite connection to avoid blocking the AppState lock
            // (which would freeze every other command for the duration of the rebuild).
            let ghost_state = app.state::<Arc<AppState>>().inner().clone();
            let ghost_shutdown = Arc::new(AtomicBool::new(false));
            let ghost_db_path = ghost_state.db_path();
            let ghost_shutdown_thread = ghost_shutdown.clone();
            std::thread::spawn(move || {
                while !ghost_shutdown_thread.load(std::sync::atomic::Ordering::Relaxed) {
                    // Sleep 5 minutes in 1-second steps so shutdown is responsive.
                    for _ in 0..300 {
                        if ghost_shutdown_thread.load(std::sync::atomic::Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                    let _ = ghost::run_cycle_offline(&ghost_db_path, &ghost_shutdown_thread);
                }
            });

            // Stash both shutdown flags in managed state so they can be
            // flipped on app exit.
            app.manage(ShutdownFlags {
                autonomy: autonomy_shutdown,
                ghost: ghost_shutdown,
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
            commands::planner::delete_calendar_event,
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
            commands::ambient::record_ambient_event,
            commands::ambient::get_ambient_events,
            commands::ambient::analyze_code_ast,
            commands::ambient::get_ambient_stats,
            commands::ambient::generate_deterministic_report,
            commands::get_app_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Chat Memory Tree");
}
