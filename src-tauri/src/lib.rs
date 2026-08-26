mod brief;
mod commands;
mod db;
mod error;
mod resume;
mod screen;
mod state;
mod tabs;
mod storage;

use std::sync::Arc;

use state::AppState;
use tauri::Manager;

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
            commands::planner::get_schedule,
            commands::planner::add_event,
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
            commands::get_app_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Chat Memory Tree");
}
