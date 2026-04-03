// New: expose a command to extract WeChat v4 keys on Windows
#[cfg(target_os = "windows")]
use once_cell::sync::Lazy;
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "windows")]
pub static EXTRACT_CANCEL_FLAG: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

#[cfg(target_os = "windows")]
pub fn is_extract_cancelled() -> bool { EXTRACT_CANCEL_FLAG.load(Ordering::Relaxed) }

#[tauri::command]
#[cfg(target_os = "windows")]
pub async fn cancel_extract_wechat_keys() -> Result<(), String> {
    EXTRACT_CANCEL_FLAG.store(true, Ordering::Relaxed);
    tracing::info!("cancel_extract_wechat_keys invoked; flag set");
    Ok(())
}

#[tauri::command]
#[cfg(target_os = "windows")]
pub async fn extract_wechat_db_keys(data_dir: Option<String>) -> Result<serde_json::Value, String> {
    use crate::internal::wechat;
    // reset cancel flag at start
    EXTRACT_CANCEL_FLAG.store(false, Ordering::Relaxed);
    tracing::info!(?data_dir, "extract_wechat_db_keys invoked");
    let result = tokio::task::spawn_blocking(move || {
        wechat::extract_db_keys(data_dir.as_deref())
    }).await.map_err(|e| format!("task join error: {}", e))?;
    match result {
        Ok(keys) => Ok(keys.to_json()),
        Err(e) => Ok(crate::internal::wechat::common::types::WechatKeys::fail(&e.to_string()))
    }
}

#[tauri::command]
#[cfg(target_os = "windows")]
pub async fn extract_wechat_img_keys(data_dir: Option<String>) -> Result<serde_json::Value, String> {
    use crate::internal::wechat;
    // reset cancel flag at start
    EXTRACT_CANCEL_FLAG.store(false, Ordering::Relaxed);
    tracing::info!(?data_dir, "extract_wechat_img_keys invoked");
    let result = tokio::task::spawn_blocking(move || {
        tracing::info!("extract_wechat_img_keys: spawn_blocking started");
        let r = wechat::extract_img_keys(data_dir.as_deref());
        tracing::info!(?r, "extract_wechat_img_keys: spawn_blocking result");
        r
    }).await.map_err(|e| format!("task join error: {}", e))?;
    match result {
        Ok(keys) => {
            let json = keys.to_json();
            tracing::info!(?json, "extract_wechat_img_keys: returning ok");
            Ok(json)
        }
        Err(e) => {
            tracing::warn!(error = %e, "extract_wechat_img_keys: returning error");
            Ok(crate::internal::wechat::common::types::WechatKeys::fail(&e.to_string()))
        }
    }
}

#[tauri::command]
#[cfg(target_os = "windows")]
pub async fn extract_wechat_v3_avatar(wx_id: String, data_dir: String) -> Result<serde_json::Value, String> {
    use std::path::PathBuf;
    use base64::Engine;
    tracing::info!(%wx_id, %data_dir, "extract_wechat_v3_avatar invoked");
    let mut db_path = PathBuf::from(&data_dir);
    db_path.push("Msg");
    db_path.push("Misc.db");
    let file_bytes = match std::fs::read(&db_path) {
        Ok(b) => b,
        Err(e) => return Ok(serde_json::json!({"ok": false, "error": format!("read Misc.db failed: {}", e)})),
    };
    let needle = wx_id.as_bytes();
    let mut avatar_data: Option<Vec<u8>> = None;
    // Signatures moved to outer scope so we can reuse after loop
    const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const JPG_SIG: &[u8] = b"\xFF\xD8\xFF";
    const GIF_SIG: &[u8] = b"GIF"; // ASCII GIF87a / GIF89a header prefix
    // Heuristic scan: find usrName then search forward for image signatures within a window
    let mut search_pos = 0usize;
    while let Some(pos) = memchr::memmem::find(&file_bytes[search_pos..], needle) {
        let global_pos = search_pos + pos;
        let window_start = global_pos;
        let window_end = std::cmp::min(global_pos + 200_000, file_bytes.len());
        let slice = &file_bytes[window_start..window_end];
        if let Some(p) = memchr::memmem::find(slice, PNG_SIG) {
            if let Some(iend_rel) = memchr::memmem::find(&slice[p..], b"IEND\xAE\x42\x60\x82") {
                let end = p + iend_rel + 8;
                avatar_data = Some(slice[p..end].to_vec());
                break;
            }
        }
        if avatar_data.is_none() {
            if let Some(p) = memchr::memmem::find(slice, JPG_SIG) {
                if let Some(mut end_idx) = memchr::memmem::find(&slice[p+3..], b"\xFF\xD9") { end_idx += p+3+2; avatar_data = Some(slice[p..end_idx].to_vec()); break; }
            }
        }
        if avatar_data.is_none() {
            if let Some(p) = memchr::memmem::find(slice, GIF_SIG) {
                if p + 6 < slice.len() && ( &slice[p..p+6] == b"GIF89a" || &slice[p..p+6] == b"GIF87a") {
                    let gif_tail_slice = &slice[p..std::cmp::min(p+120_000, slice.len())];
                    if let Some(rel_end) = gif_tail_slice.iter().rposition(|&b| b==0x3B) { avatar_data = Some(gif_tail_slice[..=rel_end].to_vec()); break; }
                }
            }
        }
        search_pos = global_pos + needle.len();
    }
    if let Some(img) = avatar_data {
        let (mime, _) = if img.starts_with(PNG_SIG) { ("image/png", true) } else if img.starts_with(JPG_SIG) { ("image/jpeg", true) } else if img.starts_with(GIF_SIG) { ("image/gif", true) } else { ("application/octet-stream", false) };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&img);
        Ok(serde_json::json!({"ok": true, "avatar": format!("data:{};base64,{}", mime, b64) }))
    } else {
        Ok(serde_json::json!({"ok": false, "error": "avatar not found heuristically"}))
    }
}


// 非Windows不支持
#[tauri::command]
#[cfg(not(target_os = "windows"))]
pub async fn extract_wechat_db_keys(_data_dir: Option<String>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": false, "error": "Windows only" }))
}

#[tauri::command]
#[cfg(not(target_os = "windows"))]
pub async fn extract_wechat_img_keys(_data_dir: Option<String>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": false, "error": "Windows only" }))
}

#[tauri::command]
#[cfg(target_os = "windows")]
pub async fn detect_data_dirs() -> Result<serde_json::Value, String> {
    use crate::internal::wechat;
    tracing::info!("detect_data_dirs invoked");
    match wechat::detect_data_dirs() {
        Ok(dirs) => Ok(serde_json::json!({ "ok": true, "dirs": dirs })),
        Err(e) => Ok(serde_json::json!({ "ok": false, "error": e.to_string(), "dirs": [] })),
    }
}

#[tauri::command]
#[cfg(not(target_os = "windows"))]
pub async fn detect_data_dirs() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": false, "error": "Windows only", "dirs": [] }))
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