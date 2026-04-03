use anyhow::{Result, anyhow};
use crate::internal::wechat::common::types::WechatKeys;
use crate::internal::wechat::common::extractor_trait::KeyExtractor;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

/// macOS WeChat v4 (Weixin) process info
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MacWeChatProcess {
    pid: u32,
    exe_path: String,
    status: String,
    data_dir: Option<String>,
    account_name: Option<String>,
}

pub struct MacV4Extractor;

// ──────────────────────── KeyExtractor trait impl ────────────────────────

impl KeyExtractor for MacV4Extractor {
    fn detect(&self) -> Result<bool> {
        let procs = Self::find_wechat_v4_processes()?;
        Ok(!procs.is_empty())
    }

    fn extract_db_keys(&self, data_dir: Option<&str>) -> Result<WechatKeys> {
        let procs = Self::find_wechat_v4_processes()?;
        if procs.is_empty() {
            return Err(anyhow!("未找到微信 v4 进程 (macOS)"));
        }
        let selected = procs
            .iter()
            .find(|p| p.status == "online" && p.data_dir.is_some())
            .cloned()
            .unwrap_or_else(|| procs.into_iter().next().unwrap());

        let pid = selected.pid;
        let account_name = selected.account_name.clone();
        let effective_dir = data_dir
            .map(|s| s.to_string())
            .or_else(|| selected.data_dir.clone());

        // Scan process memory for x'<96 hex chars>' db key pattern
        let db_keys = Self::scan_db_keys_from_memory(pid)?;

        // Heuristic avatar extraction from contact.db (same logic as Windows)
        let avatar_base64 = Self::extract_avatar(&effective_dir, &account_name);

        Ok(WechatKeys {
            ok: true,
            data_key: None,
            db_keys,
            image_key: None,
            xor_key: None,
            client_type: "mac".into(),
            client_version: "v4".into(),
            account_name,
            data_dir: effective_dir,
            method: Some("primary".into()),
            pid: Some(pid),
            avatar_base64,
        })
    }

    fn extract_img_keys(&self, data_dir: Option<&str>) -> Result<WechatKeys> {
        let procs = Self::find_wechat_v4_processes()?;
        if procs.is_empty() {
            return Err(anyhow!("未找到微信 v4 进程 (macOS)"));
        }
        let selected = procs
            .iter()
            .find(|p| p.status == "online" && p.data_dir.is_some())
            .cloned()
            .unwrap_or_else(|| procs.into_iter().next().unwrap());

        let pid = selected.pid;
        let account_name = selected.account_name.clone();
        let effective_dir = data_dir
            .map(|s| s.to_string())
            .or_else(|| selected.data_dir.clone());

        // 1. Find V2 .dat ciphertext & xor key from data_dir
        let (ciphertext, xor_key) = if let Some(ref dir) = effective_dir {
            let attach_dir = Path::new(dir).join("msg").join("attach");
            (Self::find_v2_ciphertext(&attach_dir), Self::find_xor_key(&attach_dir))
        } else {
            (None, None)
        };

        // 2. Scan process memory for AES key
        let image_key = if let Some(ref ct) = ciphertext {
            Self::scan_aes_key_from_memory(pid, ct)?
        } else {
            tracing::warn!("No V2 .dat ciphertext found, cannot scan for AES image key");
            None
        };

        let xor_str = xor_key.map(|k| format!("0x{:02x}", k));

        Ok(WechatKeys {
            ok: true,
            data_key: None,
            db_keys: vec![],
            image_key,
            xor_key: xor_str,
            client_type: "mac".into(),
            client_version: "v4".into(),
            account_name,
            data_dir: effective_dir,
            method: Some("primary".into()),
            pid: Some(pid),
            avatar_base64: None,
        })
    }

    fn detect_data_dirs(&self) -> Result<Vec<String>> {
        let home = Self::resolve_home();
        let base = Path::new(&home)
            .join("Library")
            .join("Containers")
            .join("com.tencent.xinWeChat")
            .join("Data")
            .join("Documents")
            .join("xwechat_files");

        tracing::info!("macOS detect_data_dirs: scanning {}", base.display());

        if !base.is_dir() {
            return Ok(vec![]);
        }

        let mut seen: HashSet<String> = HashSet::new();
        let mut candidates: Vec<String> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let account_dir = entry.path();
                let db_storage = account_dir.join("db_storage");
                if db_storage.is_dir() {
                    let normalized = account_dir.to_string_lossy().to_string();
                    if !seen.contains(&normalized) {
                        seen.insert(normalized.clone());
                        candidates.push(normalized);
                    }
                }
            }
        }

        tracing::info!("macOS detect_data_dirs: found {} candidates", candidates.len());
        Ok(candidates)
    }
}

// ──────────────────────── Private helpers ────────────────────────

#[allow(dead_code)]
impl MacV4Extractor {
    /// Resolve real user HOME, handling sudo which may change HOME to /var/root
    fn resolve_home() -> String {
        #[cfg(target_os = "macos")]
        {
            use std::ffi::CStr;
            // Check SUDO_USER first
            if let Ok(sudo_user) = std::env::var("SUDO_USER") {
                if !sudo_user.is_empty() {
                    unsafe {
                        let c_name = std::ffi::CString::new(sudo_user.as_str()).ok();
                        if let Some(ref name) = c_name {
                            let pw = libc::getpwnam(name.as_ptr());
                            if !pw.is_null() {
                                let dir = (*pw).pw_dir;
                                if !dir.is_null() {
                                    if let Ok(s) = CStr::from_ptr(dir).to_str() {
                                        return s.to_string();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
    }

    /// Check if a process name matches known macOS WeChat names.
    /// macOS WeChat may appear as "WeChat" (older/current) or "Weixin" (newer v4).
    fn is_wechat_process_name(name: &str) -> bool {
        name.eq_ignore_ascii_case("WeChat") || name.eq_ignore_ascii_case("Weixin")
    }

    /// Find running WeChat v4 processes on macOS.
    /// Strategy:
    ///   1. Use sysinfo to enumerate all processes and match by name.
    ///   2. If sysinfo finds nothing, fall back to `pgrep -x WeChat` / `pgrep -x Weixin`.
    fn find_wechat_v4_processes() -> Result<Vec<MacWeChatProcess>> {
        // --- Phase 1: sysinfo scan ---
        let mut sys = System::new();
        sys.refresh_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
        );

        let mut results = Vec::new();
        let total = sys.processes().len();
        tracing::info!(total, "macOS find_wechat_v4_processes: sysinfo scanning");

        for (_pid, p) in sys.processes() {
            let name = p.name();
            if !Self::is_wechat_process_name(name) {
                continue;
            }
            let cmdline_parts: Vec<&str> = p.cmd().iter().map(|s| s.as_str()).collect();
            let cmdline = cmdline_parts.join(" ");
            tracing::info!(pid = p.pid().as_u32(), name, ?cmdline, "found WeChat process via sysinfo");

            // Skip helper subprocesses (same heuristic as Windows)
            if cmdline.contains("--type=") {
                tracing::debug!(
                    pid = p.pid().as_u32(),
                    "skipping WeChat subprocess (has --type=)"
                );
                continue;
            }

            let exe_path = p
                .exe()
                .map(|pp| pp.to_string_lossy().to_string())
                .unwrap_or_default();

            let (data_dir, account_name) = Self::try_infer_data_dir();
            let status = if data_dir.is_some() { "online" } else { "offline" }.to_string();
            results.push(MacWeChatProcess {
                pid: p.pid().as_u32(),
                exe_path,
                status,
                data_dir,
                account_name,
            });
        }

        // --- Phase 2: pgrep fallback ---
        if results.is_empty() {
            tracing::info!("sysinfo found nothing, trying pgrep fallback");
            let mut seen_pids: std::collections::HashSet<u32> = std::collections::HashSet::new();
            // Try both names: "WeChat" and "Weixin"
            for proc_name in &["WeChat", "Weixin"] {
                if let Some(pid) = Self::pgrep_find(proc_name) {
                    if seen_pids.insert(pid) {
                        tracing::info!(pid, proc_name, "found WeChat process via pgrep");
                        let (data_dir, account_name) = Self::try_infer_data_dir();
                        let status = if data_dir.is_some() { "online" } else { "offline" }.to_string();
                        results.push(MacWeChatProcess {
                            pid,
                            exe_path: String::new(),
                            status,
                            data_dir,
                            account_name,
                        });
                    }
                }
            }
        }

        tracing::info!(count = results.len(), "macOS find_wechat_v4_processes: done");
        Ok(results)
    }

    /// Use `pgrep -x <name>` to find a process PID (returns first match).
    fn pgrep_find(name: &str) -> Option<u32> {
        use std::process::Command;
        let output = Command::new("pgrep")
            .args(["-x", name])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        // pgrep may return multiple PIDs (one per line); take the first
        stdout
            .lines()
            .next()
            .and_then(|line| line.trim().parse::<u32>().ok())
    }

    /// Infer data_dir from macOS standard location
    fn try_infer_data_dir() -> (Option<String>, Option<String>) {
        let home = Self::resolve_home();
        let base = Path::new(&home)
            .join("Library")
            .join("Containers")
            .join("com.tencent.xinWeChat")
            .join("Data")
            .join("Documents")
            .join("xwechat_files");

        if !base.is_dir() {
            return (None, None);
        }

        // Walk account directories, pick the one with db_storage/session/session.db
        if let Ok(entries) = std::fs::read_dir(&base) {
            // Collect and sort by modification time (newest first)
            let mut dirs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort_by(|a, b| {
                let ma = a.metadata().and_then(|m| m.modified()).ok();
                let mb = b.metadata().and_then(|m| m.modified()).ok();
                mb.cmp(&ma)
            });

            for account_dir in dirs {
                let session_db = account_dir
                    .join("db_storage")
                    .join("session")
                    .join("session.db");
                if session_db.exists() {
                    let account_name = account_dir
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    return (
                        Some(account_dir.to_string_lossy().to_string()),
                        Some(account_name),
                    );
                }
                // Fallback: just check for db_storage existence
                let db_storage = account_dir.join("db_storage");
                if db_storage.is_dir() {
                    let account_name = account_dir
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    return (
                        Some(account_dir.to_string_lossy().to_string()),
                        Some(account_name),
                    );
                }
            }
        }

        (None, None)
    }

    /// Extract avatar from contact.db (same heuristic as Windows v4)
    fn extract_avatar(
        effective_dir: &Option<String>,
        account_name: &Option<String>,
    ) -> Option<String> {
        use base64::Engine;
        use std::io::Read;

        let dir = effective_dir.as_ref()?;
        let acct = account_name.as_ref()?;

        let mut contact_db = PathBuf::from(dir);
        contact_db.push("db_storage");
        contact_db.push("contact");
        contact_db.push("contact.db");

        let mut f = std::fs::File::open(&contact_db).ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;

        let needle = acct.as_bytes();
        const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
        const JPG_SIG: &[u8] = b"\xFF\xD8\xFF";
        const GIF_SIG: &[u8] = b"GIF";

        let mut pos0 = 0usize;
        let mut found: Option<Vec<u8>> = None;

        while let Some(pos) = memchr::memmem::find(&buf[pos0..], needle) {
            let gp = pos0 + pos;
            let slice = &buf[gp..std::cmp::min(gp + 180_000, buf.len())];

            if let Some(p) = memchr::memmem::find(slice, PNG_SIG) {
                if let Some(iend) = memchr::memmem::find(&slice[p..], b"IEND\xAE\x42\x60\x82") {
                    let end = p + iend + 8;
                    found = Some(slice[p..end].to_vec());
                    break;
                }
            }
            if found.is_none() {
                if let Some(p) = memchr::memmem::find(slice, JPG_SIG) {
                    if let Some(mut e) = memchr::memmem::find(&slice[p + 3..], b"\xFF\xD9") {
                        e += p + 3 + 2;
                        found = Some(slice[p..e].to_vec());
                        break;
                    }
                }
            }
            if found.is_none() {
                if let Some(p) = memchr::memmem::find(slice, GIF_SIG) {
                    if p + 6 < slice.len()
                        && (&slice[p..p + 6] == b"GIF89a" || &slice[p..p + 6] == b"GIF87a")
                    {
                        let tail = &slice[p..std::cmp::min(p + 90_000, slice.len())];
                        if let Some(re) = tail.iter().rposition(|&b| b == 0x3B) {
                            found = Some(tail[..=re].to_vec());
                            break;
                        }
                    }
                }
            }
            pos0 = gp + needle.len();
        }

        let img = found?;
        let mime = if img.starts_with(PNG_SIG) {
            "image/png"
        } else if img.starts_with(JPG_SIG) {
            "image/jpeg"
        } else if img.starts_with(GIF_SIG) {
            "image/gif"
        } else {
            "application/octet-stream"
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&img);
        Some(format!("data:{};base64,{}", mime, b64))
    }

    // ──────────── V2 .dat ciphertext & XOR key (same as Windows) ────────────

    /// Find first 16-byte AES ciphertext block from a V2 .dat thumbnail file
    /// V2 structure: [6B magic: 07 08 'V' '2' 08 07] [4B aes_size LE] [4B xor_size LE] [1B padding] [aes_data...]
    fn find_v2_ciphertext(attach_dir: &Path) -> Option<[u8; 16]> {
        use std::io::Read;
        const V2_MAGIC: &[u8; 6] = b"\x07\x08V2\x08\x07";

        let mut dat_files: Vec<PathBuf> = Vec::new();
        for entry in walkdir::WalkDir::new(attach_dir).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if name.ends_with("_t.dat") {
                dat_files.push(entry.path().to_path_buf());
            }
            if dat_files.len() >= 200 {
                break;
            }
        }
        dat_files.sort_by(|a, b| {
            let ma = a.metadata().and_then(|m| m.modified()).ok();
            let mb = b.metadata().and_then(|m| m.modified()).ok();
            mb.cmp(&ma)
        });

        for f in dat_files.iter().take(100) {
            if let Ok(mut fp) = std::fs::File::open(f) {
                let mut header = [0u8; 31];
                if fp.read_exact(&mut header).is_ok() && &header[..6] == V2_MAGIC {
                    let mut blk = [0u8; 16];
                    blk.copy_from_slice(&header[15..31]);
                    tracing::info!("V2 ciphertext from: {}", f.display());
                    return Some(blk);
                }
            }
        }
        None
    }

    /// Derive XOR key from V2 thumbnail file tails (JPEG ends with FF D9)
    fn find_xor_key(attach_dir: &Path) -> Option<u8> {
        use std::collections::HashMap;
        use std::io::{Read, Seek, SeekFrom};
        const V2_MAGIC: &[u8; 6] = b"\x07\x08V2\x08\x07";

        let mut dat_files: Vec<PathBuf> = Vec::new();
        for entry in walkdir::WalkDir::new(attach_dir).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if name.ends_with("_t.dat") {
                dat_files.push(entry.path().to_path_buf());
            }
            if dat_files.len() >= 100 {
                break;
            }
        }
        dat_files.sort_by(|a, b| {
            let ma = a.metadata().and_then(|m| m.modified()).ok();
            let mb = b.metadata().and_then(|m| m.modified()).ok();
            mb.cmp(&ma)
        });

        let mut tail_counts: HashMap<(u8, u8), usize> = HashMap::new();
        for f in dat_files.iter().take(32) {
            if let Ok(mut fp) = std::fs::File::open(f) {
                let mut head = [0u8; 6];
                if fp.read_exact(&mut head).is_err() || &head != V2_MAGIC {
                    continue;
                }
                let sz = match fp.metadata() {
                    Ok(m) => m.len(),
                    Err(_) => continue,
                };
                if sz < 8 {
                    continue;
                }
                if fp.seek(SeekFrom::End(-2)).is_err() {
                    continue;
                }
                let mut tail = [0u8; 2];
                if fp.read_exact(&mut tail).is_ok() {
                    *tail_counts.entry((tail[0], tail[1])).or_insert(0) += 1;
                }
            }
        }
        if tail_counts.is_empty() {
            return None;
        }
        let &(x, y) = tail_counts
            .iter()
            .max_by_key(|(_, v)| *v)
            .map(|(k, _)| k)?;
        let xor_key = x ^ 0xFF;
        let check = y ^ 0xD9;
        if xor_key == check {
            tracing::info!("XOR key verified: 0x{:02x}", xor_key);
        } else {
            tracing::warn!(
                "XOR key mismatch: 0x{:02x} vs 0x{:02x}, using best guess 0x{:02x}",
                xor_key,
                check,
                xor_key
            );
        }
        Some(xor_key)
    }

    /// Try AES-ECB decryption of ciphertext block with given key; check for image signatures
    fn try_aes_key(key: &[u8], ciphertext: &[u8; 16]) -> bool {
        use aes::cipher::{BlockDecrypt, KeyInit};
        use aes::Aes128;
        if key.len() < 16 {
            return false;
        }
        let k = aes::cipher::generic_array::GenericArray::from_slice(&key[..16]);
        let cipher = Aes128::new(k);
        let mut blk = aes::cipher::generic_array::GenericArray::from(*ciphertext);
        cipher.decrypt_block(&mut blk);
        let dec = blk.as_slice();
        // JPEG: FF D8 FF, PNG: 89 50 4E 47, WEBP: RIFF, WXGF: wxgf, GIF: GIF
        dec.starts_with(b"\xFF\xD8\xFF")
            || dec.starts_with(&[0x89, 0x50, 0x4E, 0x47])
            || dec.starts_with(b"RIFF")
            || dec.starts_with(b"wxgf")
            || dec.starts_with(b"GIF")
    }

    // ──────────── macOS process memory scanning via Mach APIs ────────────

    /// Scan process memory for x'<96 hex chars>' pattern (db keys)
    #[cfg(target_os = "macos")]
    fn scan_db_keys_from_memory(pid: u32) -> Result<Vec<String>> {
        const PREFIX: &[u8] = b"x'";
        const HEX_LEN: usize = 96;
        const MATCH_TOTAL: usize = 2 + HEX_LEN + 1; // x' + 96 hex + '

        fn is_hex_char(b: u8) -> bool {
            matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')
        }

        let regions = Self::enumerate_memory_regions(pid)?;
        tracing::info!(
            "scan_db_keys: {} regions, total {:.0} MB",
            regions.len(),
            regions.iter().map(|r| r.1 as f64).sum::<f64>() / 1024.0 / 1024.0
        );

        let mut results: HashSet<String> = HashSet::new();

        for (base_addr, region_size) in &regions {
            let buf = match Self::read_process_memory(pid, *base_addr, *region_size) {
                Some(b) => b,
                None => continue,
            };

            let mut search_pos = 0usize;
            while search_pos + MATCH_TOTAL <= buf.len() {
                if let Some(pos) = memchr::memmem::find(&buf[search_pos..], PREFIX) {
                    let abs_pos = search_pos + pos;
                    if abs_pos + MATCH_TOTAL <= buf.len() {
                        let hex_start = abs_pos + 2;
                        let hex_end = hex_start + HEX_LEN;
                        let closing = hex_end;
                        if buf[closing] == b'\''
                            && buf[hex_start..hex_end].iter().all(|&b| is_hex_char(b))
                        {
                            let hex_str = std::str::from_utf8(&buf[hex_start..hex_end])
                                .unwrap_or_default()
                                .to_lowercase();
                            results.insert(hex_str);
                        }
                        search_pos = abs_pos + 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        tracing::info!("scan_db_keys_from_memory found {} unique keys", results.len());
        Ok(results.into_iter().collect())
    }

    #[cfg(not(target_os = "macos"))]
    fn scan_db_keys_from_memory(_pid: u32) -> Result<Vec<String>> {
        Ok(vec![])
    }

    /// Scan process memory for AES image key (16/32 char alphanumeric)
    #[cfg(target_os = "macos")]
    fn scan_aes_key_from_memory(pid: u32, ciphertext: &[u8; 16]) -> Result<Option<String>> {
        #[inline]
        fn is_alnum(b: u8) -> bool {
            matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z')
        }

        let regions = Self::enumerate_memory_regions(pid)?;
        let total_mb = regions.iter().map(|r| r.1 as f64).sum::<f64>() / 1024.0 / 1024.0;
        tracing::info!(
            "scan_aes_key: {} regions ({:.0} MB)",
            regions.len(),
            total_mb
        );

        let mut candidates_32 = 0usize;
        let mut candidates_16 = 0usize;

        for (base_addr, region_size) in &regions {
            let buf = match Self::read_process_memory(pid, *base_addr, *region_size) {
                Some(b) if b.len() >= 32 => b,
                _ => continue,
            };

            let mut pos = 0usize;
            while pos < buf.len() {
                if !is_alnum(buf[pos]) {
                    pos += 1;
                    continue;
                }
                // Left boundary check
                if pos > 0 && is_alnum(buf[pos - 1]) {
                    while pos < buf.len() && is_alnum(buf[pos]) {
                        pos += 1;
                    }
                    continue;
                }
                let start = pos;
                while pos < buf.len() && is_alnum(buf[pos]) {
                    pos += 1;
                }
                let run_len = pos - start;

                if run_len == 32 {
                    candidates_32 += 1;
                    if MacV4Extractor::try_aes_key(&buf[start..start + 16], ciphertext) {
                        let key_str = std::str::from_utf8(&buf[start..start + 16])
                            .unwrap_or_default()
                            .to_string();
                        tracing::info!("Found AES key (32-char, first 16): {}", &key_str);
                        return Ok(Some(key_str));
                    }
                } else if run_len == 16 {
                    candidates_16 += 1;
                    if MacV4Extractor::try_aes_key(&buf[start..start + 16], ciphertext) {
                        let key_str = std::str::from_utf8(&buf[start..start + 16])
                            .unwrap_or_default()
                            .to_string();
                        tracing::info!("Found AES key (16-char): {}", &key_str);
                        return Ok(Some(key_str));
                    }
                }
            }
        }

        tracing::debug!(
            "tested {} x 32-char + {} x 16-char candidates",
            candidates_32,
            candidates_16
        );
        tracing::warn!("AES image key not found in process memory");
        Ok(None)
    }

    #[cfg(not(target_os = "macos"))]
    fn scan_aes_key_from_memory(_pid: u32, _ciphertext: &[u8; 16]) -> Result<Option<String>> {
        Ok(None)
    }

    // ──────────── macOS Mach VM helpers ────────────

    /// Enumerate readable memory regions of a process via mach_vm_region
    #[cfg(target_os = "macos")]
    fn enumerate_memory_regions(pid: u32) -> Result<Vec<(u64, usize)>> {
        use std::mem;

        // Mach kernel types & functions
        extern "C" {
            fn task_for_pid(
                target_tport: u32,
                pid: i32,
                task: *mut u32,
            ) -> i32;
            fn mach_task_self() -> u32;
            fn mach_vm_region(
                target_task: u32,
                address: *mut u64,
                size: *mut u64,
                flavor: i32,
                info: *mut i32,
                info_cnt: *mut u32,
                object_name: *mut u32,
            ) -> i32;
        }

        // VM_REGION_BASIC_INFO_64 flavor
        const VM_REGION_BASIC_INFO_64: i32 = 9;
        const VM_REGION_BASIC_INFO_COUNT_64: u32 = 9;
        // VM protection bits
        const VM_PROT_READ: i32 = 0x01;

        // vm_region_basic_info_64 struct (9 x i32 = 36 bytes)
        #[repr(C)]
        #[derive(Default)]
        struct VmRegionBasicInfo64 {
            protection: i32,
            max_protection: i32,
            inheritance: i32,
            shared: i32,
            reserved: i32,
            offset: i64, // actually u64, but passed as i32 array
            behavior: i32,
            user_wired_count: u16,
            _pad: u16, // alignment padding
        }

        unsafe {
            let mut task: u32 = 0;
            let kr = task_for_pid(mach_task_self(), pid as i32, &mut task);
            if kr != 0 {
                return Err(anyhow!(
                    "task_for_pid failed (kern_return={}). 需要 root 权限或 taskgated 授权。\
                    尝试: sudo 运行, 或在系统偏好设置中授权开发者工具。",
                    kr
                ));
            }

            let mut regions: Vec<(u64, usize)> = Vec::new();
            let mut address: u64 = 0;
            let max_region_size: usize = 512 * 1024 * 1024; // skip huge regions

            loop {
                let mut size: u64 = 0;
                let mut info: [i32; 9] = [0; 9];
                let mut info_count: u32 = VM_REGION_BASIC_INFO_COUNT_64;
                let mut object_name: u32 = 0;

                let kr = mach_vm_region(
                    task,
                    &mut address,
                    &mut size,
                    VM_REGION_BASIC_INFO_64,
                    info.as_mut_ptr(),
                    &mut info_count,
                    &mut object_name,
                );

                if kr != 0 {
                    break; // No more regions
                }

                let protection = info[0]; // first field is protection

                // Only include readable regions within size limit
                if (protection & VM_PROT_READ) != 0 && (size as usize) <= max_region_size {
                    regions.push((address, size as usize));
                }

                address = address.saturating_add(size);
                if size == 0 {
                    break;
                }
            }

            Ok(regions)
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn enumerate_memory_regions(_pid: u32) -> Result<Vec<(u64, usize)>> {
        Ok(vec![])
    }

    /// Read a chunk of process memory via mach_vm_read_overwrite
    #[cfg(target_os = "macos")]
    fn read_process_memory(pid: u32, address: u64, size: usize) -> Option<Vec<u8>> {
        extern "C" {
            fn task_for_pid(target_tport: u32, pid: i32, task: *mut u32) -> i32;
            fn mach_task_self() -> u32;
            fn mach_vm_read_overwrite(
                target_task: u32,
                address: u64,
                size: u64,
                data: u64,
                out_size: *mut u64,
            ) -> i32;
        }

        unsafe {
            let mut task: u32 = 0;
            let kr = task_for_pid(mach_task_self(), pid as i32, &mut task);
            if kr != 0 {
                return None;
            }

            // Read in chunks of 16 MB to avoid issues with large regions
            const CHUNK_SIZE: usize = 16 * 1024 * 1024;
            let mut result = Vec::with_capacity(size);
            let mut offset: u64 = 0;

            while (offset as usize) < size {
                let to_read = std::cmp::min(CHUNK_SIZE, size - offset as usize) as u64;
                let mut buf = vec![0u8; to_read as usize];
                let mut out_size: u64 = 0;

                let kr = mach_vm_read_overwrite(
                    task,
                    address + offset,
                    to_read,
                    buf.as_mut_ptr() as u64,
                    &mut out_size,
                );

                if kr != 0 || out_size == 0 {
                    // If we already read something, return what we have
                    if !result.is_empty() {
                        return Some(result);
                    }
                    return None;
                }

                buf.truncate(out_size as usize);
                result.extend_from_slice(&buf);
                offset += out_size;
            }

            Some(result)
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn read_process_memory(_pid: u32, _address: u64, _size: usize) -> Option<Vec<u8>> {
        None
    }
}
