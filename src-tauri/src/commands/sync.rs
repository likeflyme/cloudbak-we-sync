use std::{collections::{HashMap, HashSet}, path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}}};
use std::error::Error as StdError;
use crate::commands::auth::AuthState;
use anyhow::Result;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use walkdir::WalkDir;

// --- Auto Sync global auth cache (per session) ---
#[derive(Clone, Debug)]
struct AutoUploadAuth {
    base_url: String,
    token: Option<String>,
}

// session_id -> auth
static AUTO_UPLOAD_AUTH: Lazy<Mutex<HashMap<i32, AutoUploadAuth>>> = Lazy::new(|| Mutex::new(HashMap::new()));

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

#[derive(Clone, Debug, Deserialize)]
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

#[allow(dead_code)]
fn _fetch_remote_map_blocking(client: &reqwest::blocking::Client, base_url: &str, sys_session_id: i32) -> Result<HashMap<String, RemoteEntry>> {
    #[allow(dead_code)]
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

// 新增：仅获取指定目录（sub_path）的远程列表，recursive=false
fn fetch_remote_dir_blocking(
    client: &reqwest::blocking::Client,
    base_url: &str,
    sys_session_id: i32,
    sub_path: &str,
) -> Result<Vec<RemoteEntry>> {
    let url = format!(
        "{}/sync/list?sys_session_id={}&sub_path={}&recursive=false&include_hash=false",
        base_url.trim_end_matches('/'),
        sys_session_id,
        sub_path
    );
    tracing::trace!(session_id = sys_session_id, sub_path, %url, "fetch_remote_dir_blocking start");
    let resp = client.get(url).send()?;
    if !resp.status().is_success() {
        tracing::error!(session_id = sys_session_id, status = ?resp.status(), sub_path, "remote dir http error");
        anyhow::bail!("remote dir failed: {}", resp.status());
    }
    let items: Vec<RemoteEntry> = resp.json()?;
    tracing::trace!(session_id = sys_session_id, sub_path, count = items.len(), "remote dir fetched");
    Ok(items)
}

// 新增：增量构建远程文件映射，每次只请求一个目录的数据
fn build_remote_map_incremental(
    client: &reqwest::blocking::Client,
    base_url: &str,
    sys_session_id: i32,
) -> Result<HashMap<String, RemoteEntry>> {
    let mut map: HashMap<String, RemoteEntry> = HashMap::new();
    let mut stack: Vec<String> = vec![String::from("")]; // 根目录用空字符串表示
    let mut visited: HashSet<String> = HashSet::new(); // 记录已列过的目录

    while let Some(dir) = stack.pop() {
        // 若已列过该目录，跳过
        if !visited.insert(dir.clone()) {
            continue;
        }
        let entries = fetch_remote_dir_blocking(client, base_url, sys_session_id, &dir)?;
        for it in entries.into_iter() {
            // 记录当前目录项（文件或子目录）
            map.insert(it.path.clone(), it.clone());
            // 将子目录入栈，但避免重复入栈（已经访问过的不会再次请求）
            if it.is_dir {
                if !visited.contains(&it.path) {
                    stack.push(it.path.clone());
                }
            }
        }
        // 可选：轻微节流，降低服务端压力
        // std::thread::sleep(std::time::Duration::from_millis(2));
    }
    Ok(map)
}

fn upload_one_blocking(client: &reqwest::blocking::Client, base_url: &str, sys_session_id: i32, root: &Path, file_path: &Path, is_auto: bool) -> Result<()> {
    let dest_path = normalize_rel(root, file_path);
    let url = api_url(base_url, "sync/upload");
    tracing::debug!(
        session_id = sys_session_id,
        is_auto,
        base_url = %base_url,
        url = %url,
        file = %dest_path,
        "upload url resolved"
    );
    let local_mtime_ms = std::fs::metadata(file_path).ok().map(|m| file_mtime_millis(&m)).unwrap_or(0);
    let file_size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);
    tracing::trace!(session_id = sys_session_id, file = %dest_path, size = file_size, mtime = local_mtime_ms, is_auto, "upload_one_blocking start");
    let file_name = Path::new(&dest_path).file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string();
    let file_handle = std::fs::File::open(file_path)?;
    let part = reqwest::blocking::multipart::Part::reader_with_length(file_handle, file_size)
        .file_name(file_name);
    let mut form = reqwest::blocking::multipart::Form::new()
        .text("dest_path", dest_path.clone())
        .text("sys_session_id", sys_session_id.to_string())
        .text("overwrite", "true")
        .text("client_mtime", local_mtime_ms.to_string())
        .part("file", part);
    if is_auto { form = form.text("is_auto", "true"); }

    let resp = match client.post(url).multipart(form).send() {
        Ok(r) => r,
        Err(e) => {
            // Expand the error chain for diagnostics (connect timeout/refused/reset, TLS, proxy, etc.)
            let mut chain_parts: Vec<String> = Vec::new();
            chain_parts.push(e.to_string());
            let mut src = <reqwest::Error as std::error::Error>::source(&e);
            while let Some(s) = src {
                chain_parts.push(s.to_string());
                src = std::error::Error::source(s);
            }
            let chain = chain_parts.join(" | ");

            tracing::error!(
                session_id = sys_session_id,
                file = %dest_path,
                err = %e,
                chain = %chain,
                is_auto,
                "upload send error"
            );
            return Err(e.into());
        }
    };
    if !resp.status().is_success() {
        tracing::warn!(
            session_id = sys_session_id,
            file = %dest_path,
            status = ?resp.status(),
            base_url = %base_url,
            is_auto,
            "upload failed status"
        );
        anyhow::bail!("upload failed: {}", resp.status());
    }
    tracing::trace!(session_id = sys_session_id, file = %dest_path, is_auto, "upload success");
    Ok(())
}

// 读取本地解析开关（来自 plugin-store 写入的 settings.json）
fn get_local_parse_enabled() -> bool {
    if let Ok(base) = crate::internal::app_paths::app_data_dir() {
        let path = base.join("settings.json");
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                return v.get("local_parse_enabled").and_then(|b| b.as_bool()).unwrap_or(false);
            }
        }
    }
    false
}

// 上传指定目标路径（用于上传解析后的 db）
fn upload_with_dest_blocking(
    client: &reqwest::blocking::Client,
    base_url: &str,
    sys_session_id: i32,
    src_file_path: &Path,
    dest_path: &str,
    is_auto: bool,
) -> Result<()> {
    let url = api_url(base_url, "sync/upload");
    let local_mtime_ms = std::fs::metadata(src_file_path).ok().map(|m| file_mtime_millis(&m)).unwrap_or(0);
    let file_size = std::fs::metadata(src_file_path).map(|m| m.len()).unwrap_or(0);
    tracing::trace!(session_id = sys_session_id, file = %dest_path, size = file_size, mtime = local_mtime_ms, is_auto, "upload_with_dest_blocking start");
    let file_name = Path::new(dest_path).file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string();
    let file_handle = std::fs::File::open(src_file_path)?;
    let part = reqwest::blocking::multipart::Part::reader_with_length(file_handle, file_size)
        .file_name(file_name);
    let mut form = reqwest::blocking::multipart::Form::new()
        .text("dest_path", dest_path.to_string())
        .text("sys_session_id", sys_session_id.to_string())
        .text("overwrite", "true")
        .text("client_mtime", local_mtime_ms.to_string())
        .part("file", part);
    if is_auto { form = form.text("is_auto", "true"); }
    let resp = client.post(url).multipart(form).send()?;
    if !resp.status().is_success() {
        tracing::warn!(session_id = sys_session_id, file = %dest_path, status = ?resp.status(), is_auto, "upload failed status");
        anyhow::bail!("upload failed: {}", resp.status());
    }
    tracing::trace!(session_id = sys_session_id, file = %dest_path, is_auto, "upload success");
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

// --- Auto Sync Upload Queue ---
#[derive(Clone, Debug)]
struct AutoUploadJob {
    user_id: i32,
    session_id: i32,
    root: PathBuf,
    rel: String,
    full_path: PathBuf,
}

static AUTO_UPLOAD_QUEUE: Lazy<std::sync::mpsc::SyncSender<AutoUploadJob>> = Lazy::new(|| {
    let (tx, rx) = std::sync::mpsc::sync_channel::<AutoUploadJob>(4096);
    let rx = Arc::new(std::sync::Mutex::new(rx));

    let concurrency: usize = std::env::var("WESYNC_AUTO_UPLOAD_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0 && n <= 16)
        .unwrap_or(2);

    // Spawn background workers to consume queue
    for worker_id in 0..concurrency {
        let rx = Arc::clone(&rx);
        std::thread::spawn(move || {
            loop {
                let job = {
                    let guard = rx.lock().expect("auto-upload queue mutex poisoned");
                    guard.recv()
                };
                let Ok(job) = job else { break; };

                 // file might be deleted shortly after event
                 let meta = match std::fs::metadata(&job.full_path) {
                     Ok(m) => m,
                     Err(e) => {
                         tracing::debug!(
                             session_id = job.session_id,
                             user_id = job.user_id,
                             file = %job.rel,
                             worker = worker_id,
                             error = %e,
                             "auto-upload skip: metadata failed"
                         );
                         continue;
                     }
                 };

                 let size = meta.len();
                 let mtime_ms = file_mtime_millis(&meta);

                // lookup cached base_url/token
                let auth = AUTO_UPLOAD_AUTH.lock().get(&job.session_id).cloned();
                let Some(auth) = auth else {
                    tracing::warn!(session_id = job.session_id, user_id = job.user_id, worker = worker_id, "auto-upload: missing cached auth");
                    continue;
                };

                let base_url = auth.base_url.clone();
                let token_opt: Option<String> = auth.token.clone();

                tracing::debug!(
                    session_id = job.session_id,
                    user_id = job.user_id,
                    worker = worker_id,
                    base_url = %base_url,
                    token_present = token_opt.is_some(),
                    "auto-upload using cached auth"
                );

                 // Validate base_url to avoid "relative URL without a base"
                 if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
                     tracing::warn!(session_id = job.session_id, user_id = job.user_id, worker = worker_id, base_url = %base_url, "auto-upload: invalid endpoint");
                     continue;
                 }

                 // Build a blocking client; apply Authorization header if present
                 let mut client_builder = reqwest::blocking::Client::builder();
                // Keep uploads resilient on slow networks / large files.
                // NOTE: These should align with manual sync client's settings.
                use std::time::Duration;
                client_builder = client_builder
                    .connect_timeout(Duration::from_secs(10))
                    .tcp_keepalive(Duration::from_secs(30))
                    .timeout(Duration::from_secs(1800));
                 if let Some(t) = token_opt {
                     let mut headers = reqwest::header::HeaderMap::new();
                     if let Ok(hval) = reqwest::header::HeaderValue::from_str(t.as_str()) {
                         headers.insert(reqwest::header::AUTHORIZATION, hval);
                     }
                     client_builder = client_builder.default_headers(headers);
                 }
                 let client = match client_builder.build() {
                     Ok(c) => c,
                     Err(e) => {
                         tracing::warn!(session_id = job.session_id, user_id = job.user_id, worker = worker_id, error = %e, "auto-upload: build http client failed");
                         continue;
                     }
                 };

                 tracing::info!(
                     session_id = job.session_id,
                     user_id = job.user_id,
                     worker = worker_id,
                     file = %job.rel,
                     size,
                     mtime = mtime_ms,
                     "auto-upload start"
                 );

                 if let Err(e) = upload_one_blocking(&client, &auth.base_url, job.session_id, &job.root, &job.full_path, true) {
                     tracing::warn!(session_id = job.session_id, user_id = job.user_id, worker = worker_id, file = %job.rel, error = %e, "auto-upload failed");
                 } else {
                     tracing::debug!(session_id = job.session_id, user_id = job.user_id, worker = worker_id, file = %job.rel, "auto-upload done");
                 }
            }
        });
    }

    tx
});

// --- Auto Sync Watcher ---
fn spawn_watcher(session_id: i32, user_id: i32, root: PathBuf, base_url: String, token: Option<String>, cancel: Arc<AtomicBool>) -> Result<()> {
    use notify::{RecommendedWatcher, RecursiveMode, Watcher};
    if !root.exists() { anyhow::bail!("wx_dir not found"); }

    // IMPORTANT: normalize to origin only (no /api or /app) to avoid /api/api/*
    let base_url = normalize_api_base(&base_url);

    tracing::info!(session_id, user_id, path = %root.display(), base_url = %base_url, "spawn_watcher start");

    // Cache base_url/token for this session; workers will use it.
    let token_for_cache = token.clone();
    {
        let mut auth_map = AUTO_UPLOAD_AUTH.lock();
        auth_map.insert(session_id, AutoUploadAuth { base_url: base_url.clone(), token: token_for_cache });
    }

    std::thread::spawn(move || {
        use std::collections::BTreeMap;
        use std::time::{Duration, Instant};
        let mut last_enqueue: BTreeMap<String, Instant> = BTreeMap::new();

        let filter_str = load_session_config(session_id, user_id).and_then(|c| c.sync_filters).unwrap_or_default();
        tracing::debug!(session_id, user_id, filters_len = filter_str.len(), "watcher loaded filters");

        let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher: RecommendedWatcher = RecommendedWatcher::new(move |res| { let _ = tx.send(res); }, notify::Config::default()).expect("watcher");
        watcher.watch(&root, RecursiveMode::Recursive).expect("watch start");
        tracing::info!(session_id, "watcher active");

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

                // debounce enqueue per file
                let now = Instant::now();
                let overdue = last_enqueue.get(&rel).map(|t| now.duration_since(*t) > Duration::from_millis(800)).unwrap_or(true);
                if !overdue { continue; }
                last_enqueue.insert(rel.clone(), now);

                let job = AutoUploadJob {
                    user_id,
                    session_id,
                    root: root.clone(),
                    rel: rel.clone(),
                    full_path: path.clone(),
                };

                match AUTO_UPLOAD_QUEUE.try_send(job) {
                    Ok(_) => tracing::trace!(session_id, file = %rel, "auto-sync enqueued changed file"),
                    Err(e) => tracing::warn!(session_id, file = %rel, error = %e, "auto-sync queue full, drop event"),
                }
            }
        }

        tracing::info!(session_id, "watcher thread exiting");

        // Remove auth cache when watcher exits
        {
            let mut auth_map = AUTO_UPLOAD_AUTH.lock();
            auth_map.remove(&session_id);
        }

        let mut active = ACTIVE_AUTO_SESSIONS.lock();
        active.remove(&session_id);
    });

    Ok(())
}

fn normalize_api_base(base_url: &str) -> String {
    // base_url from settings should be server origin, but can include /api or /app due to older configs.
    // Normalize to origin (no trailing /api or /app), no trailing '/'.
    let mut trimmed = base_url.trim();
    // strip query/fragment if someone pasted full url
    if let Some(i) = trimmed.find('#') { trimmed = &trimmed[..i]; }
    if let Some(i) = trimmed.find('?') { trimmed = &trimmed[..i]; }
    let trimmed = trimmed.trim_end_matches('/');
    let trimmed = trimmed.trim_end_matches("/api").trim_end_matches("/app");
    trimmed.to_string()
}

fn api_url(origin: &str, path: &str) -> String {
    // Defensively normalize again in case callers pass legacy values.
    let origin = normalize_api_base(origin);
    let origin = origin.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{}/api/{}", origin, path)
}

#[derive(Debug, Clone, Deserialize)]
struct ScanResponse {
    ok: bool,
    #[serde(flatten)]
    extra: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct CheckScanResponse {
    end: bool,
}

#[derive(Debug, Clone)]
struct RemoteDbEntry {
    path: String,
    is_dir: bool,
    size: u64,
    mtime_ms: i64,
}

fn cache_sync_db_path(user_id: i32, sys_session_id: i32) -> Result<PathBuf> {
    let base = crate::internal::app_paths::app_data_dir()?;
    // cache under per-user folder to avoid collisions
    let dir = base
        .join("users")
        .join(user_id.to_string())
        .join("cache")
        .join("syncdb");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("sync_{}.db", sys_session_id)))
}

fn trigger_remote_scan_blocking(
    client: &reqwest::blocking::Client,
    base_url: &str,
    sys_session_id: i32,
) -> Result<()> {
    let url = format!("{}?sys_session_id={}", api_url(base_url, "sync/scan"), sys_session_id);
    tracing::info!(session_id = sys_session_id, %url, "trigger remote scan");
    let resp = client.post(url).send()?;
    if !resp.status().is_success() {
        anyhow::bail!("scan failed: {}", resp.status());
    }
    // best-effort parse; server returns {ok: true, ...}
    let _ = resp.json::<ScanResponse>();
    Ok(())
}

fn wait_remote_scan_end_blocking(
    client: &reqwest::blocking::Client,
    base_url: &str,
    sys_session_id: i32,
    cancel: &AtomicBool,
) -> Result<()> {
    use std::time::{Duration, Instant};

    let url = format!("{}?sys_session_id={}", api_url(base_url, "sync/check_scan"), sys_session_id);

    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("cancelled");
        }

        let resp = client.post(url.clone()).send()?;
        if !resp.status().is_success() {
            anyhow::bail!("check_scan failed: {}", resp.status());
        }
        let body: CheckScanResponse = resp.json()?;
        if body.end {
            tracing::info!(session_id = sys_session_id, elapsed_ms = started.elapsed().as_millis() as u64, "remote scan finished");
            return Ok(());
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}

fn download_sync_db_blocking(
    client: &reqwest::blocking::Client,
    base_url: &str,
    sys_session_id: i32,
    target_path: &Path,
) -> Result<()> {
    let url = format!("{}?sys_session_id={}", api_url(base_url, "sync/download_sync_db"), sys_session_id);
    tracing::info!(session_id = sys_session_id, %url, path = %target_path.display(), "download sync.db");

    let mut resp = client.post(url).send()?;
    if !resp.status().is_success() {
        anyhow::bail!("download_sync_db failed: {}", resp.status());
    }

    let mut f = std::fs::File::create(target_path)?;
    std::io::copy(&mut resp, &mut f)?;
    Ok(())
}

fn remote_map_from_sync_db(db_path: &Path) -> Result<HashMap<String, RemoteEntry>> {
    // table schema:
    // files(path TEXT PRIMARY KEY, is_dir INTEGER NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL)
    let conn = rusqlite::Connection::open(db_path)?;
    let mut stmt = conn.prepare("SELECT path, is_dir, size, mtime FROM files")?;

    let rows = stmt.query_map([], |row| {
        let path: String = row.get(0)?;
        let is_dir_i: i64 = row.get(1)?;
        let size_i: i64 = row.get(2)?;
        let mtime_i: i64 = row.get(3)?;

        Ok(RemoteDbEntry {
            path,
            is_dir: is_dir_i != 0,
            size: size_i.max(0) as u64,
            mtime_ms: mtime_i,
        })
    })?;

    let mut map: HashMap<String, RemoteEntry> = HashMap::new();
    for r in rows {
        let r = r?;
        map.insert(
            r.path.clone(),
            RemoteEntry {
                path: r.path,
                is_dir: r.is_dir,
                size: Some(r.size),
                // store ms in f64 to keep using existing normalize_remote_mtime_to_ms logic
                mtime: Some(r.mtime_ms as f64),
            },
        );
    }

    Ok(map)
}

#[tauri::command]
pub async fn start_sync(
    sys_session_id: i32,
    wx_dir: String,
    full: Option<bool>,
    auth_state: tauri::State<'_, AuthState>,
    app: AppHandle,
) -> Result<String, String> {
    // Use ONLY values from store_utils; do not fall back to request args.
    let user_id = crate::common::store_utils::get_user_id(&app)
        .ok_or_else(|| "missing user_id in settings store".to_string())?;
    let base_url = crate::common::store_utils::get_endpoint(&app)
        .ok_or_else(|| "missing endpoint in settings store".to_string())?;

    // IMPORTANT: keep `base_url` as server ORIGIN (no /api and no /app).
    // Upload helpers already append `/api/sync/*`.
    let base_url = normalize_api_base(&base_url);

    let token = crate::common::store_utils::get_token(&app)
        .or_else(|| auth_state.0.lock().as_ref().and_then(|c| c.token.clone()));

    if token.is_none() {
        return Err("missing token in settings store".to_string());
    }

    tracing::info!(session_id = sys_session_id, user_id, %wx_dir, %base_url, full = ?full, "start_sync invoked");
    let root = PathBuf::from(wx_dir);
    if !root.exists() {
        tracing::warn!(?root, "wx_dir not found");
        return Err("wx_dir not found".into());
    }
    let task_id = sys_session_id.to_string();

    tracing::info!("task sync initialized");

    // cancel existing task if any
    if let Some(old) = TASKS.lock().remove(&task_id) { old.cancel.store(true, Ordering::Relaxed); }
    tracing::info!(session_id = sys_session_id, "previous task (if any) cancelled");

    let task = Task::new();
    TASKS.lock().insert(task_id.clone(), task.clone());
    tracing::info!(task_id = %task_id, "sync task created");

    // Move heavy/blocking network work entirely into a dedicated OS thread
    std::thread::spawn(move || {
        tracing::info!(session_id = sys_session_id, "sync thread started");

        // Build a blocking client in this thread to avoid interacting with the Tokio runtime
        let client = {
            let mut builder = reqwest::blocking::Client::builder();
            use std::time::Duration;
            if let Some(t) = token.clone() {
                use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
                let mut headers = HeaderMap::new();
                if let Ok(hval) = HeaderValue::from_str(&t) { headers.insert(AUTHORIZATION, hval); }
                builder = builder
                    .default_headers(headers)
                    .connect_timeout(Duration::from_secs(10))
                    .tcp_keepalive(Duration::from_secs(30))
                    .timeout(Duration::from_secs(1800));
            }
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
            status.message = Some("remote scanning".into());
        }

        // 1) trigger scan
        if let Err(e) = trigger_remote_scan_blocking(&client, &base_url, sys_session_id) {
            tracing::error!(error = %e, session_id = sys_session_id, "remote scan trigger error");
            let mut s = task.status.lock();
            s.state = "error".into();
            s.message = Some(format!("remote scan trigger error: {}", e));
            return;
        }

        // 2) poll check_scan every 1s
        if let Err(e) = wait_remote_scan_end_blocking(&client, &base_url, sys_session_id, &task.cancel) {
            // cancelled is treated as stopped
            if e.to_string().contains("cancelled") {
                let mut s = task.status.lock();
                s.state = "stopped".into();
                s.message = Some("stopped by user".into());
                return;
            }
            tracing::error!(error = %e, session_id = sys_session_id, "remote scan check error");
            let mut s = task.status.lock();
            s.state = "error".into();
            s.message = Some(format!("remote scan check error: {}", e));
            return;
        }

        // 3) download sync.db
        {
            let mut status = task.status.lock();
            status.message = Some("downloading sync db".into());
        }

        let sync_db_path = match cache_sync_db_path(user_id, sys_session_id) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(error = %e, session_id = sys_session_id, "prepare cache path failed");
                let mut s = task.status.lock();
                s.state = "error".into();
                s.message = Some(format!("prepare cache path failed: {}", e));
                return;
            }
        };

        if let Err(e) = download_sync_db_blocking(&client, &base_url, sys_session_id, &sync_db_path) {
            tracing::error!(error = %e, session_id = sys_session_id, "download sync db error");
            let mut s = task.status.lock();
            s.state = "error".into();
            s.message = Some(format!("download sync db error: {}", e));
            return;
        }

        // 4) load remote map from sqlite
        {
            let mut status = task.status.lock();
            status.message = Some("loading remote index".into());
        }

        let remote_map = match remote_map_from_sync_db(&sync_db_path) {
            Ok(m) => m,
            Err(e) => {
                tracing::error!(error = %e, session_id = sys_session_id, path = %sync_db_path.display(), "read sync db error");
                let mut s = task.status.lock();
                s.state = "error".into();
                s.message = Some(format!("read sync db error: {}", e));
                return;
            }
        };

        // best-effort cleanup: remove cached db after read
        let _ = std::fs::remove_file(&sync_db_path);

        // --- existing diff/upload logic below (unchanged) ---
        let mut to_upload: Vec<PathBuf> = Vec::new();
        // load filters if any (exclusion rules)
        let cfg = load_session_config(sys_session_id, user_id);
        let filter_str = cfg.and_then(|c| c.sync_filters).unwrap_or_default();
        tracing::info!(session_id = sys_session_id, filters_len = filter_str.len(), "begin scanning files");

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

         // 读取“本地解析”开关
         let local_parse_enabled = get_local_parse_enabled();
         // 尝试从会话配置读取 db 解密密钥（若存在）
         let session_cfg = load_session_config(sys_session_id, user_id);
         let db_key_hex = session_cfg.as_ref().and_then(|c| c.session_info.as_ref()).and_then(|info| info.get("db_key").and_then(|v| v.as_str())).map(|s| s.to_string());

         // 并发上传：使用固定大小的工作线程池
         let concurrency: usize = std::env::var("WESYNC_UPLOAD_CONCURRENCY")
             .ok()
             .and_then(|v| v.parse().ok())
             .filter(|&n| n > 0 && n <= 16)
             .unwrap_or(4);
         use std::sync::mpsc;
         let (tx, rx) = mpsc::channel::<PathBuf>();
         // 将 Receiver 包装为可在多线程共享的互斥体
         let rx_shared = std::sync::Arc::new(std::sync::Mutex::new(rx));

         // 为每个 worker 克隆必要数据
         let mut workers = Vec::with_capacity(concurrency);
         for i in 0..concurrency {
             let rx_shared = rx_shared.clone();
             let client = client.clone();
             let base_url = base_url.clone();
             let root = root.clone();
             let task = task.clone();
             let local_parse_enabled = local_parse_enabled;
             let db_key_hex = db_key_hex.clone();
             workers.push(std::thread::spawn(move || {
                 loop {
                     // 从共享的 Receiver 取任务（阻塞等待）
                     let file = {
                         match rx_shared.lock().unwrap().recv() {
                             Ok(f) => f,
                             Err(_) => break, // 发送端关闭
                         }
                     };
                     // 取消检查
                     if task.cancel.load(Ordering::Relaxed) { break; }
                     let rel = normalize_rel(&root, &file);
                     {
                         let mut s = task.status.lock();
                         s.current = Some(rel.clone());
                     }

                     // 若需要本地解析且为 .db 文件，尝试解密到缓存并上传解析后的文件
                     let mut decoded_uploaded = false;
                     if local_parse_enabled && file.extension().and_then(|e| e.to_str()) == Some("db") {
                         if let Some(db_key_hex) = db_key_hex.as_ref() {
                             if !db_key_hex.is_empty() {
                                 if let Ok(base) = crate::internal::app_paths::app_data_dir() {
                                     let cache_dir = base.join("cache").join("decoded").join(sys_session_id.to_string());
                                     let _ = std::fs::create_dir_all(&cache_dir);
                                     let orig_name = file.file_name().and_then(|s| s.to_str()).unwrap_or("file.db");
                                     let decoded_name = format!("decoded_{}", orig_name);
                                     let decoded_path = cache_dir.join(&decoded_name);
                                     tracing::info!(session_id = sys_session_id, file = %rel, decoded_file = %decoded_path.display(), "attempting db decrypt and upload");
                                     // 调用解密
                                     if let Err(e) = crate::internal::windows::db_decrypt::decrypt_db_file_v4(&file, db_key_hex, &decoded_path) {
                                         // 解析失败：打印失败日志与异常链
                                         let chain = e.to_string();
                                         tracing::error!(
                                             session_id = sys_session_id,
                                             file = %rel,
                                             err = %e,
                                             chain = %chain,
                                             saved_target = %decoded_path.display(),
                                             "db decrypt exception"
                                         );
                                         tracing::warn!(session_id = sys_session_id, file = %rel, "db decrypt failed, skip decoded upload");
                                     } else {
                                         // 解析成功后打印完整保存路径
                                         tracing::info!(session_id = sys_session_id, file = %rel, saved_decoded = %decoded_path.display(), "db decrypted and saved");
                                         // 解析后的上传目标路径：与原文件同目录，文件名前加 decoded_
                                         let parent_rel = Path::new(&rel).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| "".to_string());
                                         let dest_decoded = if parent_rel.is_empty() { decoded_name.clone() } else { format!("{}/{}", parent_rel, decoded_name) };
                                         tracing::info!(session_id = sys_session_id, dest = %dest_decoded, src = %decoded_path.display(), "uploading decoded db");
                                         match upload_with_dest_blocking(&client, &base_url, sys_session_id, &decoded_path, &dest_decoded, false) {
                                             Ok(_) => {
                                                 decoded_uploaded = true;
                                                 let mut s = task.status.lock();
                                                 s.uploaded += 1;
                                                 tracing::debug!(session_id = sys_session_id, worker = i, file = %dest_decoded, "uploaded decoded db");
                                             }
                                             Err(e) => {
                                                 let mut s = task.status.lock();
                                                 s.failed += 1;
                                                 s.message = Some(format!("upload decoded failed: {}", e));
                                                 tracing::warn!(session_id = sys_session_id, worker = i, file = %dest_decoded, error = %e, "upload decoded failed");
                                             }
                                         }
                                     }
                                 }
                             }
                         }
                     }

                     // 始终上传原始文件
                     match upload_one_blocking(&client, &base_url, sys_session_id, &root, &file, false) {
                         Ok(_) => {
                             let mut s = task.status.lock();
                             s.uploaded += 1;
                             // 可选：轻微节流，防止服务端压力过大
                             // std::thread::sleep(std::time::Duration::from_millis(2));
                             tracing::debug!(session_id = sys_session_id, worker = i, file = %rel, decoded_uploaded, "uploaded original");
                         }
                         Err(e) => {
                             let mut s = task.status.lock();
                             s.failed += 1;
                             s.message = Some(format!("upload failed: {}", e));
                             tracing::warn!(session_id = sys_session_id, worker = i, file = %rel, error = %e, "upload original failed");
                         }
                     }
                 }
                 tracing::trace!(session_id = sys_session_id, worker = i, "worker exit");
             }));
         }

         // 派发任务到队列
         for file in to_upload {
             if task.cancel.load(Ordering::Relaxed) {
                 let mut s = task.status.lock();
                 s.state = "stopped".into();
                 s.message = Some("stopped by user".into());
                 tracing::info!(session_id = sys_session_id, "sync cancelled by user");
                 break;
             }
             // 发送到队列（若接收端关闭则提前结束）
             if tx.send(file).is_err() { break; }
         }
         // 关闭发送端，使所有 worker 正常退出
         drop(tx);
         // 等待所有 worker 结束
         for h in workers {
             let _ = h.join();
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

     // IMPORTANT: normalize to origin only (no /api or /app)
     let base_url = normalize_api_base(&base_url);

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
    // best-effort: remove auth cache immediately; watcher thread also removes on exit
    {
        let mut auth_map = AUTO_UPLOAD_AUTH.lock();
        auth_map.remove(&sys_session_id);
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
pub async fn init_user_auto_sync(app: AppHandle) -> Result<u32, String> {
    let user_id = crate::common::store_utils::get_user_id(&app);
    let endpoint = crate::common::store_utils::get_endpoint(&app);
    let token = crate::common::store_utils::get_token(&app);
    if user_id.is_none() || endpoint.is_none() || token.is_none() {
        tracing::info!("init_user_auto_sync skipped: missing user_id/endpoint/token");
        return Ok(0);
    }
    let base_url = normalize_api_base(&endpoint.unwrap());
    use std::fs;
    let base = crate::internal::app_paths::app_data_dir().map_err(|e| e.to_string())?;
    let sess_dir = base.join("users").join(user_id.unwrap().to_string()).join("sessions");
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
        let cfg = match load_session_config(session_id, user_id.unwrap()) { Some(c) => c, None => continue };
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
                    if let Err(e) = spawn_watcher(session_id, user_id.unwrap(), PathBuf::from(wx_dir_val), base_url.clone(), token.clone(), cancel) {
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
