mod brief;
mod commands;
mod db;
mod error;
mod state;
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
            commands::get_app_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Chat Memory Tree");
}
