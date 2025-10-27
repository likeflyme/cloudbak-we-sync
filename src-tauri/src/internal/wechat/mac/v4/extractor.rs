use anyhow::{Result, anyhow};
use crate::internal::wechat::common::types::WechatKeys;
use crate::internal::wechat::common::extractor_trait::KeyExtractor;
use crate::internal::wechat::mac::glance::read_process_memory;
use crate::internal::windows::validator::{DBValidator, ImgKeyValidator};
use sysinfo::System;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

pub struct MacV4Extractor;

const MAX_WORKERS: usize = 8;
const MIN_CHUNK_SIZE: usize = 1 * 1024 * 1024; // 1MB
const CHUNK_MULTIPLIER: usize = 2; // workers * multiplier chunks
const OVERLAP: usize = 1024; // boundary overlap

// From chatlog V4KeyPatterns & V4ImgKeyPatterns (simplified)
const V4_DATA_PATTERNS: &[(&[u8], &[isize])] = &[(b"\x20fts5(%\x00", &[16, -80, 64])]; // pattern: 20 66 74 73 35 28 25 00
const V4_IMG_PAT_ZERO16: &[u8] = &[0u8;16];
const V4_IMG_PATTERNS: &[(&[u8], &[isize])] = &[(V4_IMG_PAT_ZERO16, &[-32])];

impl MacV4Extractor {
  #[cfg(target_os = "macos")]
  fn find_wechat_pid() -> Option<u32> {
    let mut sys = System::new_all(); sys.refresh_processes();
    for (pid, proc_) in sys.processes() {
      let name = proc_.name().to_ascii_lowercase();
      if name.contains("wechat") { return Some((*pid).as_u32()); }
    }
    None
  }
  #[cfg(not(target_os = "macos"))]
  fn find_wechat_pid() -> Option<u32> { None }

  #[cfg(target_os = "macos")]
  fn scan_data_key(memory: &[u8], validator: &DBValidator) -> Option<String> {
    for (pat, offs) in V4_DATA_PATTERNS {
      let mut end = memory.len();
      while let Some(idx) = memchr::memmem::rfind(&memory[..end], pat) {
        for &off in *offs {
          let pos = idx as isize + off; if pos < 0 { continue; }
          let p = pos as usize; if p+32 > memory.len() { continue; }
          let key_data = &memory[p..p+32];
          if validator.validate_db_key(key_data) { return Some(hex::encode(key_data)); }
        }
        if idx == 0 { break; }
        end = idx;
      }
    }
    None
  }
  #[cfg(not(target_os = "macos"))]
  fn scan_data_key(_m: &[u8], _v: &DBValidator) -> Option<String> { None }

  #[cfg(target_os = "macos")]
  fn scan_img_key(memory: &[u8], validator: &ImgKeyValidator) -> Option<String> {
    for (pat, offs) in V4_IMG_PATTERNS {
      let mut end = memory.len();
      while let Some(mut idx) = memchr::memmem::rfind(&memory[..end], pat) {
        // align backwards to last non-zero before pattern block (similar heuristic)
        if idx > 0 {
          let slice = &memory[..idx];
          let nz = slice.iter().rposition(|&b| b != 0).unwrap_or(0);
          idx = nz;
        }
        for &off in *offs {
          let pos = idx as isize + off; if pos < 0 { continue; }
          let p = pos as usize; if p+16 > memory.len() { continue; }
          let key_data = &memory[p..p+16];
          if key_data == V4_IMG_PAT_ZERO16 { continue; }
          if validator.validate_img_key(key_data) { return Some(hex::encode(key_data)); }
        }
        if idx == 0 { break; }
        end = idx;
      }
    }
    None
  }
  #[cfg(not(target_os = "macos"))]
  fn scan_img_key(_m: &[u8], _v: &ImgKeyValidator) -> Option<String> { None }
}

impl KeyExtractor for MacV4Extractor {
  fn detect(&self) -> Result<bool> { Ok(Self::find_wechat_pid().is_some()) }
  fn extract(&self) -> Result<WechatKeys> {
    #[cfg(not(target_os = "macos"))]
    { return Err(anyhow!("macOS only")); }
    #[cfg(target_os = "macos")]
    {
      let pid = Self::find_wechat_pid().ok_or_else(|| anyhow!("未找到 macOS 微信进程"))?;
      let home = std::env::var("HOME").unwrap_or_default();
      let data_dir = format!("{home}/Library/Containers/com.tencent.xinWeChat/Data/Library/Application Support/com.tencent.xinWeChat");
      let validator = DBValidator::new(&data_dir).map_err(|e| anyhow!("初始化 v4 验证器失败: {e}"))?;
      let img_validator = ImgKeyValidator::new(&data_dir).ok(); // non-critical
      let memory = read_process_memory(pid)?;

      let total = memory.len();
      if total <= MIN_CHUNK_SIZE {
        let dk = Self::scan_data_key(&memory, &validator);
        let ik = img_validator.as_ref().and_then(|v| Self::scan_img_key(&memory, v));
        if dk.is_none() && ik.is_none() { return Err(anyhow!("未找到 mac v4 密钥")); }
        return Ok(WechatKeys { ok: true, data_key: dk, image_key: ik, xor_key: None, client_type: "mac".into(), client_version: "v4".into(), account_name: None, data_dir: Some(data_dir), method: Some("memory-scan".into()), pid: Some(pid), avatar_base64: None });
      }

      let workers = std::cmp::min(MAX_WORKERS, std::cmp::max(2, 4)); // fixed worker count (avoid extra deps)
      let chunk_count = workers * CHUNK_MULTIPLIER;
      let mut ranges: Vec<(usize,usize)> = Vec::new();
      let chunk_size = total / chunk_count;
      for i in (0..chunk_count).rev() {
        let mut start = i * chunk_size; let mut end = (i+1) * chunk_size; if i == chunk_count-1 { end = total; }
        if i>0 { start = start.saturating_sub(OVERLAP); }
        ranges.push((start,end));
      }

      let data_key = Arc::new(parking_lot::Mutex::new(None::<String>));
      let img_key = Arc::new(parking_lot::Mutex::new(None::<String>));
      let cancel = Arc::new(AtomicBool::new(false));

      std::thread::scope(|s| {
        for (start,end) in ranges {
          let mem_slice = &memory[start..end];
          let validator_ref = &validator;
            let img_val_ref = img_validator.as_ref();
          let dk_cell = Arc::clone(&data_key); let ik_cell = Arc::clone(&img_key); let cancel_flag = Arc::clone(&cancel);
          s.spawn(move || {
            if cancel_flag.load(Ordering::Relaxed) { return; }
            if dk_cell.lock().is_none() {
              if let Some(dk_found) = MacV4Extractor::scan_data_key(mem_slice, validator_ref) {
                *dk_cell.lock() = Some(dk_found);
              }
            }
            if img_val_ref.is_some() && ik_cell.lock().is_none() {
              if let Some(ik_found) = MacV4Extractor::scan_img_key(mem_slice, img_val_ref.unwrap()) {
                *ik_cell.lock() = Some(ik_found);
              }
            }
            if dk_cell.lock().is_some() && (img_val_ref.is_none() || ik_cell.lock().is_some()) {
              cancel_flag.store(true, Ordering::Relaxed);
            }
          });
        }
      });

      let dk = data_key.lock().clone();
      let ik = img_key.lock().clone();
      if dk.is_none() && ik.is_none() { return Err(anyhow!("未找到 mac v4 密钥")); }
      Ok(WechatKeys { ok: true, data_key: dk, image_key: ik, xor_key: None, client_type: "mac".into(), client_version: "v4".into(), account_name: None, data_dir: Some(data_dir), method: Some("memory-scan".into()), pid: Some(pid), avatar_base64: None })
    }
  }
}
