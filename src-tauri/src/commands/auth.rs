use serde::Serialize;
use parking_lot::Mutex;
use tauri::Manager; // for path()
use tauri::Emitter; // 事件发送
use std::{fs, io::Write};
use tauri_plugin_store::StoreExt; // 引入扩展 trait

#[derive(Clone, Debug)]
pub struct AuthContext {
    pub user_id: i32,
    pub token: Option<String>,
    pub base_url: String,
}

pub struct AuthState(pub Mutex<Option<AuthContext>>);

#[derive(Serialize)]
pub struct AuthInfo {
    pub user_id: i32,
    pub has_token: bool,
    pub base_url: String,
}

#[tauri::command]
pub async fn set_auth_context(state: tauri::State<'_, AuthState>, user_id: i32, token: Option<String>, base_url: String) -> Result<(), String> {
    if base_url.trim().is_empty() { return Err("base_url 不能为空".into()); }
    *state.0.lock() = Some(AuthContext { user_id, token: token.clone(), base_url: base_url.clone() });
    tracing::info!(user_id, has_token = token.is_some(), %base_url, "auth context set");
    Ok(())
}

#[tauri::command]
pub async fn clear_auth_context(state: tauri::State<'_, AuthState>, app: tauri::AppHandle) -> Result<(), String> {
    // 同步清理持久化数据，避免下次启动残留旧地址/用户
    clear_auth_persisted(&app);
    let existed = state.0.lock().take().is_some();
    tracing::info!(cleared = existed, store = STORE_NAME, "auth context cleared");
    Ok(())
}

#[tauri::command]
pub async fn get_current_user_id(state: tauri::State<'_, AuthState>) -> Result<Option<i32>, String> {
    let id = state.0.lock().as_ref().map(|c| c.user_id);
    tracing::debug!(user_id = ?id, "get_current_user_id");
    Ok(id)
}

#[tauri::command]
pub async fn get_auth_context(state: tauri::State<'_, AuthState>) -> Result<Option<AuthInfo>, String> {
    let ctx = state.0.lock();
    let info = ctx.as_ref().map(|c| AuthInfo { user_id: c.user_id, has_token: c.token.is_some(), base_url: c.base_url.clone() });
    tracing::debug!(has_ctx = info.is_some(), "get_auth_context");
    Ok(info)
}

const AUTH_FILE: &str = "auth.json"; // 纯文件备份
const STORE_NAME: &str = "auth.store.json"; // store 文件名

#[tauri::command]
pub async fn persist_auth(state: tauri::State<'_, AuthState>, app: tauri::AppHandle, user_id: i32, token: Option<String>, base_url: String) -> Result<(), String> {
    if base_url.trim().is_empty() { return Err("base_url 不能为空".into()); }
    *state.0.lock() = Some(AuthContext { user_id, token: token.clone(), base_url: base_url.clone() });

    // 写入备份文件
    let dir = app.path().app_config_dir().map_err(|e| format!("无法获取应用配置目录: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(AUTH_FILE);
    let data = serde_json::json!({ "user_id": user_id, "token": token, "base_url": base_url });
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(serde_json::to_string(&data).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;

    // 写入 store (统一读写)
    if let Ok(store) = app.store(STORE_NAME) { // 创建或加载
        store.set("user_id".to_string(), serde_json::json!(user_id));
        store.set("token".to_string(), serde_json::json!(token));
        store.set("base_url".to_string(), serde_json::json!(base_url));
        let _ = store.save();
    }

    tracing::info!(user_id, has_token = token.is_some(), %base_url, backup_path = %path.display(), store = STORE_NAME, "auth persisted");
    Ok(())
}

#[tauri::command]
pub async fn load_persisted_auth(state: tauri::State<'_, AuthState>, app: tauri::AppHandle) -> Result<Option<AuthInfo>, String> {
    // 优先从 store 读取
    if let Ok(store) = app.store(STORE_NAME) {
        let user_id = store.get("user_id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let base_url = store.get("base_url").map(|v| v.clone()).and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_default();
        let token = store.get("token").map(|v| v.clone()).and_then(|v| v.as_str().map(|s| s.to_string()));
        if user_id > 0 && !base_url.is_empty() {
            *state.0.lock() = Some(AuthContext { user_id, token: token.clone(), base_url: base_url.clone() });
            let info = AuthInfo { user_id, has_token: token.is_some(), base_url: base_url.clone() };
            let _ = app.emit("auth-restored", &info);
            tracing::info!(user_id, has_token = info.has_token, store = STORE_NAME, "auth loaded from store");
            return Ok(Some(info));
        }
    }

    // 回退读取备份文件
    let dir = app.path().app_config_dir().map_err(|e| format!("无法获取应用配置目录: {e}"))?;
    let path = dir.join(AUTH_FILE);
    if !path.exists() { return Ok(None); }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let user_id = v.get("user_id").and_then(|x| x.as_i64()).unwrap_or(-1) as i32;
    if user_id <= 0 { return Ok(None); }
    let token = v.get("token").and_then(|x| x.as_str()).map(|s| s.to_string());
    let base_url = v.get("base_url").and_then(|x| x.as_str()).unwrap_or("").to_string();
    if base_url.is_empty() { return Ok(None); }
    *state.0.lock() = Some(AuthContext { user_id, token: token.clone(), base_url: base_url.clone() });
    let info = AuthInfo { user_id, has_token: token.is_some(), base_url };
    let _ = app.emit("auth-restored", &info);
    tracing::info!(user_id, has_token = info.has_token, backup_file = %path.display(), "auth loaded from file");
    Ok(Some(info))
}

// 内部工具：清理持久化的鉴权数据（store 与备份文件）
fn clear_auth_persisted(app: &tauri::AppHandle) {
    // 清除 store
    if let Ok(store) = app.store(STORE_NAME) {
        store.clear();
        let _ = store.save();
    }
    // 清除备份文件
    if let Ok(dir) = app.path().app_config_dir() {
        let path = dir.join(AUTH_FILE);
        let _ = fs::remove_file(&path);
    }
}

#[tauri::command]
pub async fn clear_persisted_auth(state: tauri::State<'_, AuthState>, app: tauri::AppHandle) -> Result<(), String> {
    clear_auth_persisted(&app);
    state.0.lock().take();
    tracing::info!(store = STORE_NAME, "auth cleared");
    Ok(())
}
