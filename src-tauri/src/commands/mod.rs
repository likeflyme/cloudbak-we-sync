pub mod wechat;
pub mod sync;
pub mod fs;
pub mod startup;
pub mod auth;

use tauri::generate_handler;

pub fn all_handlers() -> impl Fn(tauri::ipc::Invoke) -> bool {
    generate_handler![
        wechat::extract_wechat_keys,
        wechat::load_avatar,
        sync::start_sync,
        sync::stop_sync,
        sync::get_sync_status,
        sync::save_session_filters,
        sync::delete_session_config,
        sync::get_session_filters,
        sync::start_auto_sync,
        sync::stop_auto_sync,
        sync::save_session_info,
        sync::get_auto_sync_state,
        sync::init_user_auto_sync,
        fs::open_in_os,
        startup::was_auto_launched,
        auth::set_auth_context,
        auth::clear_auth_context,
        auth::get_current_user_id,
        auth::get_auth_context,
    ]
}