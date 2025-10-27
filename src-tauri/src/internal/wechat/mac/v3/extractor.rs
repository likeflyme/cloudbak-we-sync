use anyhow::{Result, anyhow};
use crate::internal::wechat::common::types::WechatKeys;
use crate::internal::wechat::common::extractor_trait::KeyExtractor;
use crate::internal::wechat::mac::glance::read_process_memory;
use crate::internal::windows::validator::DBValidatorV3;
use sysinfo::System;

pub struct MacV3Extractor;

// Simplified pattern (from chatlog V3KeyPatterns): rtree_i32 with offset 24 -> 32 bytes key
const V3_PATTERN: &[u8] = b"rtree_i32"; // hex: 72 74 72 65 65 5f 69 33 32
const V3_OFFSETS: &[isize] = &[24];

impl MacV3Extractor {
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
  fn scan_key(memory: &[u8], validator: &DBValidatorV3) -> Option<String> {
    let mut search_end = memory.len();
    while let Some(idx) = memchr::memmem::rfind(&memory[..search_end], V3_PATTERN) {
      for &off in V3_OFFSETS {
        let key_pos = idx as isize + off;
        if key_pos < 0 { continue; }
        let kp = key_pos as usize;
        if kp + 32 > memory.len() { continue; }
        let key_data = &memory[kp..kp+32];
        if validator.validate_db_key_v3(key_data) { return Some(hex::encode(key_data)); }
      }
      if idx == 0 { break; }
      search_end = idx;
    }
    None
  }
  #[cfg(not(target_os = "macos"))]
  fn scan_key(_m: &[u8], _v: &DBValidatorV3) -> Option<String> { None }
}

impl KeyExtractor for MacV3Extractor {
  fn detect(&self) -> Result<bool> { Ok(Self::find_wechat_pid().is_some()) }
  fn extract(&self) -> Result<WechatKeys> {
    #[cfg(not(target_os = "macos"))]
    { return Err(anyhow!("macOS only")); }
    #[cfg(target_os = "macos")]
    {
      let pid = Self::find_wechat_pid().ok_or_else(|| anyhow!("未找到 macOS 微信进程"))?;
      // Heuristic data_dir: WeChat sandbox path
      let home = std::env::var("HOME").unwrap_or_default();
      let data_dir = format!("{home}/Library/Containers/com.tencent.xinWeChat/Data/Library/Application Support/com.tencent.xinWeChat");
      let validator = DBValidatorV3::new(&data_dir).map_err(|e| anyhow!("初始化 v3 验证器失败: {e}"))?;
      let memory = read_process_memory(pid)?;
      let data_key = Self::scan_key(&memory, &validator);
      if data_key.is_none() { return Err(anyhow!("未找到 mac v3 数据密钥")); }
      Ok(WechatKeys { ok: true, data_key, image_key: None, xor_key: None, client_type: "mac".into(), client_version: "v3".into(), account_name: None, data_dir: Some(data_dir), method: Some("memory-scan".into()), pid: Some(pid), avatar_base64: None })
    }
  }
}
