use serde::Serialize;
use parking_lot::Mutex;

#[derive(Clone, Debug)]
pub struct AuthContext {
    pub user_id: i32,
    pub token: Option<String>,
    pub base_url: String, // should already include trailing /api if desired
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
pub async fn clear_auth_context(state: tauri::State<'_, AuthState>) -> Result<(), String> {
    let existed = state.0.lock().take().is_some();
    tracing::info!(cleared = existed, "auth context cleared");
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
