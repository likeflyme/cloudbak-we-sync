mod internal;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// New: expose a command to extract WeChat v4 keys on Windows
#[tauri::command]
#[cfg(target_os = "windows")]
fn extract_wechat_keys(data_dir: Option<String>) -> Result<serde_json::Value, String> {
    use crate::internal::windows::{winproc, memory, dat2img};
    use anyhow::Result;

    fn inner(mut data_dir: Option<String>) -> Result<serde_json::Value> {
        let mut procs = winproc::find_wechat_v4_processes()?;
        if procs.is_empty() {
            return Ok(serde_json::json!({
                "ok": false,
                "error": "No WeChat v4 process found"
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

        let (data_key_hex, img_key_hex) = memory::extract_keys_windows(&selected)?;
        let mut xor_key: Option<u8> = None;
        if let Some(dir) = selected.data_dir.as_deref() {
            xor_key = dat2img::scan_and_set_xor_key(dir)?;
        }

        // client type/version
        let client_type = "win"; // current project only supports Windows extraction
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
        }))
    }

    inner(data_dir).map_err(|e| e.to_string())
}

// Non-Windows stub
#[tauri::command]
#[cfg(not(target_os = "windows"))]
fn extract_wechat_keys(_data_dir: Option<String>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": false, "error": "Windows only" }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, extract_wechat_keys])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
