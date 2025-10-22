// New: expose a command to extract WeChat v4 keys on Windows
#[cfg(target_os = "windows")]
use once_cell::sync::Lazy;
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "windows")]
static EXTRACT_CANCEL_FLAG: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

#[tauri::command]
#[cfg(target_os = "windows")]
pub async fn cancel_extract_wechat_keys() -> Result<(), String> {
    EXTRACT_CANCEL_FLAG.store(true, Ordering::Relaxed);
    tracing::info!("cancel_extract_wechat_keys invoked; flag set");
    Ok(())
}

#[tauri::command]
#[cfg(target_os = "windows")]
pub async fn extract_wechat_keys(data_dir: Option<String>) -> Result<serde_json::Value, String> {
    use crate::internal::windows::{winproc, memory, dat2img};
    use anyhow::Result;
    tracing::info!(?data_dir, "extract_wechat_keys invoked");

    fn cancelled() -> bool { EXTRACT_CANCEL_FLAG.load(Ordering::Relaxed) }

    fn cancel_resp() -> serde_json::Value { serde_json::json!({"ok": false, "error": "用户已取消"}) }

    EXTRACT_CANCEL_FLAG.store(false, Ordering::Relaxed); // 开始前重置

    fn inner(mut data_dir: Option<String>) -> Result<serde_json::Value> {
        if cancelled() { return Ok(cancel_resp()); }
        let mut procs = winproc::find_wechat_v4_processes()?;
        tracing::debug!(count = procs.len(), "found wechat processes");
        if cancelled() { return Ok(cancel_resp()); }
        if procs.is_empty() {
            return Ok(serde_json::json!({
                "ok": false,
                "error": "没有找到微信进程"
            }));
        }
        if let Some(ref dir) = data_dir {
            for p in &mut procs { p.data_dir = Some(dir.clone()); }
        }
        let selected = procs
            .iter()
            .find(|p| p.status == "online" && p.data_dir.is_some())
            .cloned()
            .or_else(|| {
                let mut p = procs.into_iter().next().unwrap();
                if p.data_dir.is_none() { p.data_dir = data_dir.take(); }
                Some(p)
            })
            .unwrap();
        if cancelled() { return Ok(cancel_resp()); }
        tracing::info!(pid = selected.pid, status = %selected.status, data_dir = ?selected.data_dir, acct = ?selected.account_name, "wechat process selected");

        let (data_key_hex, img_key_hex) = memory::extract_keys_windows(&selected)?;
        if cancelled() { return Ok(cancel_resp()); }
        tracing::debug!(has_data_key = data_key_hex.is_some(), has_img_key = img_key_hex.is_some(), "keys extracted");
        let mut xor_key: Option<u8> = None;
        if let Some(dir) = selected.data_dir.as_deref() {
            if cancelled() { return Ok(cancel_resp()); }
            xor_key = dat2img::scan_and_set_xor_key(dir)?;
        }
        if cancelled() { return Ok(cancel_resp()); }
        let data_dir: Option<String> = selected.data_dir.clone();
        let wx_id: Option<String> = selected.account_name.clone();
        let mut head_img: Option<String> = None;
        if let (Some(data_dir), Some(data_key_hex), Some(wx_id)) = (data_dir, data_key_hex.clone(), wx_id) {
            if !cancelled() {
                use crate::internal::windows::avatar;
                head_img = avatar::extract_avatar_to_appdata(&data_dir, &data_key_hex, &wx_id);
            }
        }
        if cancelled() { return Ok(cancel_resp()); }
        tracing::debug!(has_head_img = head_img.is_some(), "avatar extraction done");
        let client_type = "win";
        let client_version = selected
            .full_version
            .as_ref()
            .and_then(|v| v.split('.').next())
            .map(|maj| if maj == "4" { "v4" } else if maj == "3" { "v3" } else { "unknown" })
            .unwrap_or("unknown");

        Ok(serde_json::json!({
            "ok": true,
            "pid": selected.pid,
            "fullVersion": selected.full_version,
            "dataDir": selected.data_dir,
            "accountName": selected.account_name,
            "dataKey": data_key_hex,
            "imageKey": img_key_hex,
            "xorKey": xor_key,
            "clientType": client_type,
            "clientVersion": client_version,
            "headImg": head_img
        }))
    }

    tauri::async_runtime::spawn_blocking(move || inner(data_dir))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}


// 非Windows不支持
#[tauri::command]
#[cfg(not(target_os = "windows"))]
pub async fn extract_wechat_keys(_data_dir: Option<String>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": false, "error": "Windows only" }))
}

#[tauri::command]
pub fn load_avatar(path: String) -> Result<String, String> {
    use base64::Engine; // bring trait into scope for encode()
    tracing::debug!(%path, "load_avatar invoked");
    fn detect_mime(bytes: &[u8], path: &str) -> &'static str {
        let lower = path.to_lowercase();
        if lower.ends_with(".png") {
            "image/png"
        } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            "image/jpeg"
        } else if lower.ends_with(".gif") {
            "image/gif"
        } else if lower.ends_with(".webp") {
            "image/webp"
        } else if bytes.len() >= 12 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
            "image/png"
        } else if bytes.len() >= 3 && &bytes[0..3] == [0xFF, 0xD8, 0xFF] {
            "image/jpeg"
        } else if bytes.len() >= 3 && &bytes[0..3] == b"GIF" {
            "image/gif"
        } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            "image/webp"
        } else {
            "application/octet-stream"
        }
    }

    let data = std::fs::read(&path).map_err(|e| {
        tracing::warn!(%path, error = %e, "avatar read failed");
        e.to_string()
    })?;
    let mime = detect_mime(&data, &path);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    tracing::debug!(%path, mime, size = data.len(), "avatar encoded");
    Ok(format!("data:{};base64,{}", mime, b64))
}