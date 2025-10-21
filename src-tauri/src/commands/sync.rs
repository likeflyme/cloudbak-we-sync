use std::{collections::{HashMap, HashSet}, path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}}};
use crate::commands::auth::AuthState;
use anyhow::Result;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Default)]
pub struct SyncStatus {
    pub state: String,           // idle|running|stopped|done|error
    pub scanned: u64,
    pub to_upload: u64,
    pub uploaded: u64,
    pub skipped: u64,
    pub failed: u64,
    pub current: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionConfig {
    pub sync_filters: Option<String>,
    pub session_info: Option<serde_json::Value>,
    pub auto_sync: Option<bool>,
}

struct Task {
    cancel: AtomicBool,
    status: Mutex<SyncStatus>,
}

impl Task {
    fn new() -> Arc<Self> {
        Arc::new(Task { cancel: AtomicBool::new(false), status: Mutex::new(SyncStatus { state: "running".into(), ..Default::default() }) })
    }
}

static TASKS: Lazy<Mutex<HashMap<String, Arc<Task>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

// watchers for auto sync (keyed by session id string)
struct WatchHandle {
    cancel: Arc<AtomicBool>,
    // Keep the watcher thread alive by holding JoinHandle if needed
}

static WATCHERS: Lazy<Mutex<HashMap<String, WatchHandle>>> = Lazy::new(|| Mutex::new(HashMap::new()));
// 新增：跟踪已经启动的 session watcher，防止重复
static ACTIVE_AUTO_SESSIONS: Lazy<Mutex<HashSet<i32>>> = Lazy::new(|| Mutex::new(HashSet::new()));

#[derive(Debug, Deserialize)]
struct RemoteEntry {
    path: String,
    is_dir: bool,
    size: Option<u64>,
    // server may return seconds or milliseconds; accept float and normalize later
    mtime: Option<f64>,
}

fn normalize_rel<R: AsRef<Path>, F: AsRef<Path>>(root: R, full: F) -> String {
    let root = root.as_ref();
    let full = full.as_ref();
    let rel = full.strip_prefix(root).unwrap_or(full);
    let s = rel.to_string_lossy().replace('\\', "/");
    let s = if s.starts_with('/') { s.trim_start_matches('/').to_string() } else { s };
    if s.is_empty() { ".".into() } else { s }
}

fn file_mtime_millis(meta: &std::fs::Metadata) -> i64 {
    use std::time::UNIX_EPOCH;
    match meta.modified() {
        Ok(m) => {
            let d = m.duration_since(UNIX_EPOCH).unwrap_or_default();
            let ms = d.as_secs() as i64 * 1000 + (d.subsec_nanos() as i64 / 1_000_000);
            ms
        }
        Err(_) => 0,
    }
}

fn normalize_remote_mtime_to_ms(m: Option<f64>) -> Option<i64> {
    m.map(|v| {
        if v >= 1.0e11 { // looks like milliseconds
            v.round() as i64
        } else { // seconds -> to ms
            (v * 1000.0).round() as i64
        }
    })
}

fn same_mtime(local_ms: i64, remote_ms_opt: Option<i64>) -> bool {
    if let Some(remote_ms) = remote_ms_opt {
        let diff = (local_ms - remote_ms).abs();
        diff <= 999 // tolerate up to ~1s difference due to FS precision
    } else {
        false
    }
}

fn fetch_remote_map_blocking(client: &reqwest::blocking::Client, base_url: &str, sys_session_id: i32) -> Result<HashMap<String, RemoteEntry>> {
    let url = format!("{}/sync/list?sys_session_id={}&sub_path=&recursive=true&include_hash=false", base_url.trim_end_matches('/'), sys_session_id);
    tracing::debug!(session_id = sys_session_id, %url, "fetch_remote_map_blocking start");
    let resp = client.get(url).send()?;
    if !resp.status().is_success() {
        tracing::error!(session_id = sys_session_id, status = ?resp.status(), "remote list http error");
        anyhow::bail!("remote list failed: {}", resp.status());
    }
    let items: Vec<RemoteEntry> = resp.json()?;
    tracing::debug!(session_id = sys_session_id, count = items.len(), "remote list fetched");
    let mut map = HashMap::new();
    for it in items.into_iter() {
        map.insert(it.path.clone(), it);
    }
    Ok(map)
}

fn upload_one_blocking(client: &reqwest::blocking::Client, base_url: &str, sys_session_id: i32, root: &Path, file_path: &Path) -> Result<()> {
    let dest_path = normalize_rel(root, file_path);
    let url = format!("{}/sync/upload", base_url.trim_end_matches('/'));
    let file_bytes = std::fs::read(file_path)?;
    let local_mtime_ms = std::fs::metadata(file_path).ok().map(|m| file_mtime_millis(&m)).unwrap_or(0);
    tracing::trace!(session_id = sys_session_id, file = %dest_path, size = file_bytes.len(), mtime = local_mtime_ms, "upload_one_blocking start");
    let part = reqwest::blocking::multipart::Part::bytes(file_bytes).file_name(Path::new(&dest_path).file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string());
    let form = reqwest::blocking::multipart::Form::new()
        .text("dest_path", dest_path.clone())
        .text("sys_session_id", sys_session_id.to_string())
        .text("overwrite", "true")
        .text("client_mtime", local_mtime_ms.to_string())
        .part("file", part);
    let resp = client.post(url).multipart(form).send()?;
    if !resp.status().is_success() {
        tracing::warn!(session_id = sys_session_id, file = %dest_path, status = ?resp.status(), "upload failed status");
        anyhow::bail!("upload failed: {}", resp.status());
    }
    tracing::trace!(session_id = sys_session_id, file = %dest_path, "upload success");
    Ok(())
}

fn load_session_config(session_id: i32, user_id: i32) -> Option<SessionConfig> {
    let base = crate::internal::app_paths::app_data_dir().ok()?;
    let path = base.join("users").join(user_id.to_string()).join("sessions").join(format!("{}.json", session_id));
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn save_session_config_inner(session_id: i32, user_id: i32, cfg: &SessionConfig) -> Result<()> {
    let base = crate::internal::app_paths::app_data_dir()?;
    let dir = base.join("users").join(user_id.to_string()).join("sessions");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", session_id));
    let data = serde_json::to_vec_pretty(cfg)?;
    std::fs::write(path, data)?;
    Ok(())
}

// Basic wildcard matcher supporting '*' (any sequence) and '?' (single character)
fn wildcard_match(pat: &str, text: &str) -> bool {
    let (p, t) = (pat.as_bytes(), text.as_bytes());
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut match_i) = (None, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1; ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            match_i = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            match_i += 1;
            ti = match_i;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' { pi += 1; }
    pi == p.len()
}

// Exclusion filters: return true if the path should be excluded by provided patterns
fn should_exclude(rel_path: &str, filters: &str) -> bool {
    let norm = rel_path.replace('\\', "/");
    let patterns = filters
        .split(|c| c == '\n' || c == ';' || c == ',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if patterns.is_empty() { return false; }

    for raw in patterns {
        let mut p = raw.replace('\\', "/");
        // normalize leading ./
        if p.starts_with("./") { p = p[2..].to_string(); }

        // '*' alone excludes everything
        if p == "*" { return true; }

        // directory patterns: 'dir/' should exclude dir and its children
        if p.ends_with('/') {
            if norm.starts_with(&p) { return true; }
            // also allow missing trailing slash in path comparisons
            let without = p.trim_end_matches('/');
            if !without.is_empty() && (norm == without || norm.starts_with(&format!("{}/", without))) {
                return true;
            }
            continue;
        }

        // support simple globs like 'dir/*', '*.log', 'foo?.txt'
        if p.contains('*') || p.contains('?') {
            if wildcard_match(&p, &norm) { return true; }
            continue;
        }

        // plain file/dir name: exact match or prefix as directory
        if norm == p || norm.starts_with(&format!("{}/", p)) { return true; }
    }
    false
}

// --- Auto Sync Watcher ---
fn spawn_watcher(session_id: i32, user_id: i32, root: PathBuf, base_url: String, token: Option<String>, cancel: Arc<AtomicBool>) -> Result<()> {
    use notify::{RecommendedWatcher, RecursiveMode, Watcher};
    if !root.exists() { anyhow::bail!("wx_dir not found"); }
    tracing::info!(session_id, user_id, path = %root.display(), "spawn_watcher start");

    std::thread::spawn(move || {
        let client = {
            let mut builder = reqwest::blocking::Client::builder();
            if let Some(t) = token.clone() {
                use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
                let mut headers = HeaderMap::new();
                if let Ok(hval) = HeaderValue::from_str(&t) { headers.insert(AUTHORIZATION, hval); }
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                builder = builder.default_headers(headers);
            }
            builder.build().unwrap()
        };

        use std::collections::BTreeMap;
        use std::time::{Duration, Instant};
        let mut last_upload: BTreeMap<String, Instant> = BTreeMap::new();

        let filter_str = load_session_config(session_id, user_id).and_then(|c| c.sync_filters).unwrap_or_default();
        tracing::debug!(session_id, user_id, filters_len = filter_str.len(), "watcher loaded filters");

        let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher: RecommendedWatcher = RecommendedWatcher::new(move |res| { let _ = tx.send(res); }, notify::Config::default()).expect("watcher");
        watcher.watch(&root, RecursiveMode::Recursive).expect("watch start");
        tracing::info!(session_id, "watcher active");

        // watcher loop
        while !cancel.load(Ordering::Relaxed) {
            let Ok(res) = rx.recv_timeout(Duration::from_millis(500)) else { continue; };
            let evt = match res { Ok(e) => e, Err(err) => { tracing::warn!(session_id, error = %err, "watch event error"); continue } };
            let kind = evt.kind;
            let relevant = matches!(kind, notify::event::EventKind::Create(_) | notify::event::EventKind::Modify(_));
            if !relevant { continue; }
            for path in evt.paths.into_iter() {
                if cancel.load(Ordering::Relaxed) { break; }
                if path.is_dir() { continue; }
                let rel = normalize_rel(&root, &path);
                if rel == "." { continue; }
                if !filter_str.is_empty() && should_exclude(&rel, &filter_str) { continue; }
                let now = Instant::now();
                let overdue = last_upload.get(&rel).map(|t| now.duration_since(*t) > Duration::from_millis(800)).unwrap_or(true);
                if !overdue { continue; }
                last_upload.insert(rel.clone(), now);
                let _ = upload_one_blocking(&client, &base_url, session_id, &root, &path);
                tracing::trace!(session_id, file = %rel, "auto-sync uploaded changed file");
            }
        }
        tracing::info!(session_id, "watcher thread exiting");
        // 线程退出时清理活跃集合（如果未显式 stop 也能被重启）
        let mut active = ACTIVE_AUTO_SESSIONS.lock();
        active.remove(&session_id);
        // 不移除 WATCHERS：stop_auto_sync 已负责；若线程自然退出，可在下一次启动前覆盖
    });

    Ok(())
}

#[tauri::command]
pub async fn start_sync(
    sys_session_id: i32,
    user_id: i32,
    wx_dir: String,
    base_url: String,
    token: Option<String>,
    full: Option<bool>,
    auth_state: tauri::State<'_, AuthState>,
) -> Result<String, String> {
    tracing::info!(session_id = sys_session_id, user_id, %wx_dir, %base_url, full = ?full, "start_sync invoked");
     let token = token.or_else(|| auth_state.0.lock().as_ref().and_then(|c| c.token.clone()));
     let root = PathBuf::from(wx_dir);
     if !root.exists() {
        tracing::warn!(?root, "wx_dir not found");
         return Err("wx_dir not found".into());
     }
     let task_id = sys_session_id.to_string();

     // cancel existing task if any
     if let Some(old) = TASKS.lock().remove(&task_id) { old.cancel.store(true, Ordering::Relaxed); }
    tracing::debug!(session_id = sys_session_id, "previous task (if any) cancelled");

     let task = Task::new();
     TASKS.lock().insert(task_id.clone(), task.clone());
    tracing::debug!(task_id = %task_id, "sync task created");

     // Move heavy/blocking network work entirely into a dedicated OS thread
     std::thread::spawn(move || {
        tracing::info!(session_id = sys_session_id, "sync thread started");
         // Build a blocking client in this thread to avoid interacting with the Tokio runtime
         let client = {
             let mut builder = reqwest::blocking::Client::builder();
             if let Some(t) = token.clone() {
                 use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
                 let mut headers = HeaderMap::new();
                 if let Ok(hval) = HeaderValue::from_str(&t) { headers.insert(AUTHORIZATION, hval); }
                 headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                 builder = builder.default_headers(headers);
             }
             // Safe to unwrap here; if it fails, record error below
             match builder.build() {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = %e, "http client build error");
                     let mut s = task.status.lock();
                     s.state = "error".into();
                     s.message = Some(format!("http client build error: {}", e));
                     return;
                }
             }
         };

         {
             let mut status = task.status.lock();
             status.state = "running".into();
             status.message = Some("scanning".into());
         }

         let remote_map = match fetch_remote_map_blocking(&client, &base_url, sys_session_id) {
             Ok(m) => m,
             Err(e) => {
                tracing::error!(error = %e, session_id = sys_session_id, "remote list error");
                 let mut s = task.status.lock();
                 s.state = "error".into();
                 s.message = Some(format!("remote list error: {}", e));
                 return;
             }
         };

         let mut to_upload: Vec<PathBuf> = Vec::new();
         // load filters if any (exclusion rules)
         let cfg = load_session_config(sys_session_id, user_id);
         let filter_str = cfg.and_then(|c| c.sync_filters).unwrap_or_default();
    tracing::debug!(session_id = sys_session_id, filters_len = filter_str.len(), "begin scanning files");
         for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
             if task.cancel.load(Ordering::Relaxed) { break; }
             let path = entry.path();
             if path.is_dir() { continue; }
             let rel = normalize_rel(&root, path);
             if rel == "." { continue; }
             // exclusion check: if matches any rule -> skip
             if !filter_str.is_empty() && should_exclude(&rel, &filter_str) {
                 let mut s = task.status.lock();
                 s.skipped += 1;
                 continue;
             }
             let meta = match std::fs::metadata(path) { Ok(m) => m, Err(_) => continue };
             let size = meta.len();
             let mtime_ms = file_mtime_millis(&meta);
             let should_upload = match remote_map.get(&rel) {
                 None => true,
                 Some(rem) => {
                     if rem.is_dir { true } else {
                         let remote_ms = normalize_remote_mtime_to_ms(rem.mtime);
                         rem.size.unwrap_or(0) != size || !same_mtime(mtime_ms, remote_ms)
                     }
                 }
             };
             {
                 let mut s = task.status.lock();
                 s.scanned += 1;
             }
             if should_upload || full.unwrap_or(false) {
                 to_upload.push(path.to_path_buf());
             } else {
                 let mut s2 = task.status.lock();
                 s2.skipped += 1;
             }
         }

         {
             let mut s = task.status.lock();
             s.to_upload = to_upload.len() as u64;
             s.message = Some("uploading".into());
         }
    tracing::info!(session_id = sys_session_id, to_upload = to_upload.len(), "upload phase start");

         for file in to_upload {
             if task.cancel.load(Ordering::Relaxed) {
                 let mut s = task.status.lock();
                 s.state = "stopped".into();
                 s.message = Some("stopped by user".into());
                tracing::info!(session_id = sys_session_id, "sync cancelled by user");
                 break;
             }
             let rel = normalize_rel(&root, &file);
             {
                 let mut s = task.status.lock();
                 s.current = Some(rel.clone());
             }
             match upload_one_blocking(&client, &base_url, sys_session_id, &root, &file) {
                 Ok(_) => {
                     let mut s = task.status.lock();
                     s.uploaded += 1;
                    tracing::debug!(session_id = sys_session_id, file = %rel, "uploaded");
                 }
                 Err(e) => {
                     let mut s = task.status.lock();
                     s.failed += 1;
                     s.message = Some(format!("upload failed: {}", e));
                    tracing::warn!(session_id = sys_session_id, file = %rel, error = %e, "upload failed");
                 }
             }
         }

         if !task.cancel.load(Ordering::Relaxed) {
             let mut s = task.status.lock();
             if s.state != "stopped" && s.state != "error" {
                 s.state = "done".into();
                 s.message = Some("completed".into());
                tracing::info!(session_id = sys_session_id, "sync completed");
             }
         }
     });

     Ok(task_id)
}

#[tauri::command]
pub async fn stop_sync(task_id: String) -> Result<(), String> {
    if let Some(task) = TASKS.lock().get(&task_id).cloned() {
        task.cancel.store(true, Ordering::Relaxed);
        Ok(())
    } else {
        Err("task not found".into())
    }
}

#[tauri::command]
pub async fn get_sync_status(task_id: String) -> Result<SyncStatus, String> {
    if let Some(task) = TASKS.lock().get(&task_id) {
        Ok(task.status.lock().clone())
    } else {
        Ok(SyncStatus { state: "idle".into(), ..Default::default() })
    }
}

#[tauri::command]
pub async fn save_session_filters(session_id: i32, user_id: i32, sync_filters: String) -> Result<(), String> {
    let mut cfg = load_session_config(session_id, user_id).unwrap_or_default();
    cfg.sync_filters = if sync_filters.is_empty() { None } else { Some(sync_filters) };
    save_session_config_inner(session_id, user_id, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_session_config(session_id: i32, user_id: i32) -> Result<(), String> {
    let base = crate::internal::app_paths::app_data_dir().map_err(|e| e.to_string())?;
    let path = base.join("users").join(user_id.to_string()).join("sessions").join(format!("{}.json", session_id));
    match std::fs::remove_file(&path) {
        Ok(_) => Ok(()),
        Err(e) => {
            // ignore if not found
            if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e.to_string()) }
        }
    }
}

#[tauri::command]
pub async fn get_session_filters(session_id: i32, user_id: i32) -> Result<String, String> {
    let cfg = load_session_config(session_id, user_id);
    Ok(cfg.and_then(|c| c.sync_filters).unwrap_or_default())
}

#[tauri::command]
pub async fn save_session_info(session_id: i32, user_id: i32, info: serde_json::Value) -> Result<(), String> {
    let mut cfg = load_session_config(session_id, user_id).unwrap_or_default();
    cfg.session_info = Some(info);
    save_session_config_inner(session_id, user_id, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_auto_sync(sys_session_id: i32, user_id: i32, wx_dir: String, base_url: String, token: Option<String>) -> Result<(), String> {
     let root = PathBuf::from(&wx_dir);
     if !root.exists() { return Err("本机未找到会话目录，无法开启自动同步".into()); }
     {
         let active = ACTIVE_AUTO_SESSIONS.lock();
         if active.contains(&sys_session_id) {
             tracing::info!(session_id = sys_session_id, user_id, "auto sync watcher already active, skip");
             return Ok(());
         }
     }
     let _ = stop_auto_sync(sys_session_id, user_id).await; // 保留原逻辑以防残留
    tracing::info!(session_id = sys_session_id, user_id, %wx_dir, "start_auto_sync");
     let cancel = Arc::new(AtomicBool::new(false));
     let handle = WatchHandle { cancel: cancel.clone() };
     WATCHERS.lock().insert(sys_session_id.to_string(), handle);
     spawn_watcher(sys_session_id, user_id, root, base_url, token, cancel).map_err(|e| e.to_string())?;
     {
         let mut active = ACTIVE_AUTO_SESSIONS.lock();
         active.insert(sys_session_id);
     }
     let mut cfg = load_session_config(sys_session_id, user_id).unwrap_or_default();
     cfg.auto_sync = Some(true);
     save_session_config_inner(sys_session_id, user_id, &cfg).map_err(|e| e.to_string())?;
     Ok(())
}

#[tauri::command]
pub async fn stop_auto_sync(sys_session_id: i32, user_id: i32) -> Result<(), String> {
     if let Some(w) = WATCHERS.lock().remove(&sys_session_id.to_string()) { w.cancel.store(true, Ordering::Relaxed); }
     {
         let mut active = ACTIVE_AUTO_SESSIONS.lock();
         active.remove(&sys_session_id);
     }
    tracing::info!(session_id = sys_session_id, user_id, "stop_auto_sync");
     let mut cfg = load_session_config(sys_session_id, user_id).unwrap_or_default();
     cfg.auto_sync = Some(false);
     save_session_config_inner(sys_session_id, user_id, &cfg).map_err(|e| e.to_string())?;
     Ok(())
}

#[tauri::command]
pub async fn get_auto_sync_state(session_id: i32, user_id: i32) -> Result<bool, String> {
    let cfg = load_session_config(session_id, user_id);
    Ok(cfg.and_then(|c| c.auto_sync).unwrap_or(false))
}

#[tauri::command]
pub async fn init_user_auto_sync(user_id: i32, base_url: String, token: Option<String>) -> Result<u32, String> {
     use std::fs;
     let base = crate::internal::app_paths::app_data_dir().map_err(|e| e.to_string())?;
     let sess_dir = base.join("users").join(user_id.to_string()).join("sessions");
     if !sess_dir.exists() { return Ok(0); }
     let mut started: u32 = 0;
    tracing::info!(user_id, path = %sess_dir.display(), "init_user_auto_sync scanning");
     for entry in fs::read_dir(&sess_dir).map_err(|e| e.to_string())? {
         let entry = match entry { Ok(e) => e, Err(_) => continue };
         let path = entry.path();
         if !path.is_file() { continue; }
         if path.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
         let file_stem = match path.file_stem().and_then(|s| s.to_str()) { Some(s) => s, None => continue };
         let session_id: i32 = match file_stem.parse() { Ok(v) => v, Err(_) => continue };
         let cfg = match load_session_config(session_id, user_id) { Some(c) => c, None => continue };
         if !cfg.auto_sync.unwrap_or(false) { continue; }
         // 已有 watcher 则跳过
         {
             let active = ACTIVE_AUTO_SESSIONS.lock();
             if active.contains(&session_id) {
                 tracing::info!(session_id, user_id, "watcher already active, skip in init");
                 continue;
             }
         }
         if let Some(info) = cfg.session_info.as_ref() {
             if let Some(wx_dir_val) = info.get("wx_dir").and_then(|v| v.as_str()) {
                 if !wx_dir_val.is_empty() && Path::new(wx_dir_val).exists() {
                     let cancel = Arc::new(AtomicBool::new(false));
                     let handle = WatchHandle { cancel: cancel.clone() };
                     WATCHERS.lock().insert(session_id.to_string(), handle);
                     if let Err(e) = spawn_watcher(session_id, user_id, PathBuf::from(wx_dir_val), base_url.clone(), token.clone(), cancel) {
                        tracing::error!(session_id, error = %e, "auto sync watcher start failed");
                     } else {
                        let mut active = ACTIVE_AUTO_SESSIONS.lock();
                        active.insert(session_id);
                        tracing::info!(session_id, "auto sync watcher started");
                        started += 1;
                     }
                 }
             }
         }
     }
    tracing::info!(user_id, started, "init_user_auto_sync done");
     Ok(started)
}
