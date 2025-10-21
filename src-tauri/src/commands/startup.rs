use tauri::State;
use std::sync::atomic::{AtomicBool, Ordering};

// 仅保留自启启动标记查询（插件启用状态由前端直接调用 plugin:autostart 指令）。

pub struct LaunchState(pub AtomicBool);

#[tauri::command]
pub fn was_auto_launched(state: State<'_, LaunchState>) -> Result<bool, String> {
    let flag = state.0.load(Ordering::Relaxed);
    tracing::debug!(auto_launched = flag, "was_auto_launched");
    Ok(flag)
}
