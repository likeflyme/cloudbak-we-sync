pub mod wechat;

use tauri::generate_handler;

pub fn all_handlers() -> impl Fn(tauri::ipc::Invoke) -> bool {
    generate_handler![
        wechat::extract_wechat_keys,
        wechat::load_avatar
    ]
}