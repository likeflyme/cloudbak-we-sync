use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_store::StoreBuilder;
use tauri::Wry;
use tauri::Manager;

pub fn get_settings_store(app: &AppHandle) -> Arc<tauri_plugin_store::Store<Wry>> {
    StoreBuilder::new(
        app,
        app.path()
            .app_data_dir()
            .expect("app_data_dir not found")
            .join("settings.json"),
    )
    .build()
    .expect("failed to build store")
}

pub fn get_token(app: &AppHandle) -> Option<String> {
    let store = get_settings_store(app);
    store
        .get("token")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

pub fn get_endpoint(app: &AppHandle) -> Option<String> {
    let store = get_settings_store(app);
    store
        .get("endpoint")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

pub fn get_user_id(app: &AppHandle) -> Option<i32> {
    let store = get_settings_store(app);
    store
        .get("user_info")
        .and_then(|v| v.get("id").and_then(|id| id.as_i64().map(|id| id as i32)))
}