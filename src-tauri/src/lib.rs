mod internal;
use tauri_plugin_http;
mod commands;
use commands::auth::{AuthState};

use std::sync::atomic::AtomicBool;
use commands::startup::LaunchState;
use tauri::Manager; // bring trait for window access
use tauri::tray::{TrayIconBuilder, TrayIconEvent, MouseButton};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tracing_subscriber::prelude::*; // for .with chaining
use tauri::Emitter; // 添加事件发送


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let auto_flag_capture = std::env::args().any(|a| a == "--auto-launched")
        || std::env::var("WE_SYNC_AUTO_LAUNCHED").ok().is_some();

    let launch_state = LaunchState(AtomicBool::new(auto_flag_capture));

    tauri::Builder::default()
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .setup(|app| {
            // initialize logging once (before other setup ideally)
            let log_dir = crate::internal::app_paths::app_data_dir().map(|p| p.join("logs")).unwrap_or(std::env::temp_dir().join("we-sync-logs"));
            std::fs::create_dir_all(&log_dir).ok();
            let file_appender = tracing_appender::rolling::daily(&log_dir, "app.log");
            let (nb_writer, _guard) = tracing_appender::non_blocking(file_appender);
            let fmt_layer = tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_line_number(true)
                .with_file(true)
                .with_thread_ids(true)
                .with_writer(nb_writer);
            let console_layer = tracing_subscriber::fmt::layer().with_target(true).with_ansi(true);
            let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,we-sync=info"));
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .with(console_layer)
                .init();
            tracing::info!(path = %log_dir.display(), "logger initialized");

            // 构建托盘菜单 (Tauri v2 API)
            let show_item = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let mut builder = TrayIconBuilder::new()
                .tooltip("We Sync")
                .menu(&menu);
            if let Some(win_icon) = app.default_window_icon() { builder = builder.icon(win_icon.clone()); }

            let _ = builder
                .on_tray_icon_event(|tray, ev| {
                    match ev {
                        // 左键单击或双击：显示窗口
                        TrayIconEvent::Click { button: MouseButton::Left, .. } | TrayIconEvent::DoubleClick { .. } => {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); }
                            let _ = app.emit("tray-show", &serde_json::json!({"ts": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()}));
                        }
                        _ => {}
                    }
                })
                .on_menu_event(|app, item| {
                    match item.id().as_ref() {
                        "show" => { if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); } }
                        "quit" => { app.exit(0); }
                        _ => {}
                    }
                })
                .build(app);

            let auto_flag = app.state::<LaunchState>().0.load(std::sync::atomic::Ordering::Relaxed);
            if auto_flag { if let Some(w) = app.get_webview_window("main") { let _ = w.hide(); } }
            if let Ok(uid_str) = std::env::var("WE_SYNC_USER_ID") {
                if let Ok(uid) = uid_str.parse::<i32>() {
                    let base_url = std::env::var("WE_SYNC_ENDPOINT").unwrap_or_default();
                    let token = std::env::var("WE_SYNC_TOKEN").ok();
                    if !base_url.is_empty() {
                        tauri::async_runtime::spawn(async move {
                            let _ = crate::commands::sync::init_user_auto_sync(uid, base_url + "/api", token).await;
                        });
                    }
                }
            }
            // 尝试加载持久化的认证信息 (store 优先)
            if let Ok(Some(info)) = tauri::async_runtime::block_on(commands::auth::load_persisted_auth(app.state::<commands::auth::AuthState>(), app.handle().clone())) {
                let uid = info.user_id;
                let base_url = info.base_url.clone();
                let token = None; // token 在状态中
                if !base_url.is_empty() {
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::commands::sync::init_user_auto_sync(uid, base_url, token).await;
                    });
                }
            }
            Ok(())
        })
        .manage(AuthState(parking_lot::Mutex::new(None)))
        .manage(launch_state)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // 新增自启插件，传递自启标记参数
        .plugin(tauri_plugin_autostart::Builder::new().arg("--auto-launched").app_name("We Sync").build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(commands::all_handlers())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}