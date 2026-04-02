use anyhow::{Result, anyhow};
use crate::internal::wechat::common::types::WechatKeys;
use crate::internal::wechat::common::extractor_trait::KeyExtractor;
use crate::internal::windows::winproc;
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::Foundation::BOOL;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_VM_READ, PROCESS_QUERY_INFORMATION};
use windows::Win32::System::Memory::{MEMORY_BASIC_INFORMATION, VirtualQueryEx, MEM_PRIVATE, PAGE_READWRITE, PAGE_WRITECOPY, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE};
use yara::Compiler;
use pbkdf2::pbkdf2_hmac_array;
use sha1::Sha1;
use std::path::PathBuf;
use hmac::Mac;
use regex::Regex;

pub struct WinV3Extractor;

#[derive(Clone, Debug)]
struct RegionInfo { base: usize, size: usize, _protect: u32, private_writable: bool, dll_writable: bool }

impl WinV3Extractor {
  unsafe fn enumerate_regions(h: windows::Win32::Foundation::HANDLE) -> Vec<MEMORY_BASIC_INFORMATION> {
    let mut regions = Vec::new();
    let mut mbi = MEMORY_BASIC_INFORMATION::default();
    let mut p: usize = 0x10000;
    while VirtualQueryEx(h, Some(p as _), &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) != 0 {
      regions.push(mbi.clone());
      let next = (mbi.BaseAddress as usize).saturating_add(mbi.RegionSize);
      p = if next <= p { p.saturating_add(0x1000) } else { next };
      if p > 0x7FFF_FFFF_FFFF { break; }
    }
    regions
  }

  unsafe fn enumerate_wechatwin_writable(pid: u32) -> Result<Vec<(usize, usize)>> {
    // Snapshot modules and find WeChatWin.dll base ranges (there can be multiple segments)
    let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid)?;
    if snap.is_invalid() { return Err(anyhow!("CreateToolhelp32Snapshot failed")); }
    let mut me32 = MODULEENTRY32W { dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32, ..Default::default() };
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    if Module32FirstW(snap, &mut me32).is_ok() {
      loop {
        let name = {
          let raw = &me32.szModule;
          let len = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
          String::from_utf16_lossy(&raw[..len]).to_ascii_lowercase()
        };
        if name.contains("wechatwin.dll") {
          let base = me32.modBaseAddr as usize;
          let size = me32.modBaseSize as usize;
          ranges.push((base, size));
        }
        if Module32NextW(snap, &mut me32).is_err() { break; }
      }
    }
    if ranges.is_empty() { return Err(anyhow!("WeChatWin.dll module not found")); }
    Ok(ranges)
  }

  fn trim_trailing_separators(mut s: String) -> String {
    while s.ends_with('\\') || s.ends_with('/') { s.pop(); }
    s
  }

  fn filter_phone_and_datadir(pid: u32, dll_writable_regions: &[RegionInfo], private_regions: &[RegionInfo]) -> Result<(usize, usize, usize, String)> {
    let compiler = Compiler::new().map_err(|e| anyhow!("yara init fail: {e}"))?;
    let compiler = compiler.add_rules_str(
      "\nrule GetPhoneTypeStringOffset_v3 { strings: $a = \"iphone\\x00\" ascii fullword nocase $b = \"android\\x00\" ascii fullword nocase $c = \"OHOS\\x00\" ascii fullword nocase condition: any of them }\nrule GetDataDir_v3 { strings: $a = /([a-zA-Z]:\\\\|\\\\\\\\)([^\\\\:]{1,100}?\\\\){0,10}?WeChat Files\\\\[0-9a-zA-Z_-]{6,20}?\\\\/ condition: $a }\n"
    ).map_err(|e| anyhow!("yara add rules fail: {e}"))?;
    let rules = compiler.compile_rules().map_err(|e| anyhow!("yara compile fail: {e}"))?;
    let results = rules.scan_process(pid, 0).map_err(|e| anyhow!("yara scan fail: {e}"))?;
    // phone type: restrict to dll writable region bases
    let phone_match = results.iter().find(|r| r.identifier == "GetPhoneTypeStringOffset_v3")
      .and_then(|r| r.strings.first())
      .and_then(|s| s.matches.iter().find(|m| dll_writable_regions.iter().any(|ri| {
        let mb = m.base as usize; mb >= ri.base && mb < ri.base + ri.size
      })))
      .ok_or_else(|| anyhow!("phone type string not found in dll writable regions"))?;
    let phone_region_base = phone_match.base as usize; // region base used for key scan limit in original impl
    let phone_addr = (phone_match.base + phone_match.offset) as usize;
    let phone_len_addr = phone_addr.checked_add(16).ok_or_else(|| anyhow!("phone_len_addr overflow"))?;
    let phone_len: usize = unsafe { let mut v=0usize; let h = OpenProcess(PROCESS_VM_READ|PROCESS_QUERY_INFORMATION, BOOL(0), pid)?; let _=ReadProcessMemory(h, phone_len_addr as _, &mut v as *mut _ as _, std::mem::size_of::<usize>(), None); v };
    let _phone_len = phone_len.clamp(1,128);
    // data dir: prefer private writable regions, not necessarily dll. If none, fallback to any match.
    let data_dir_match_opt = results.iter().find(|r| r.identifier == "GetDataDir_v3")
      .and_then(|r| r.strings.first())
      .map(|s| {
        // first try private regions
        s.matches.iter().find(|m| private_regions.iter().any(|ri| {
          let mb = m.base as usize; mb >= ri.base && mb < ri.base + ri.size
        })).or_else(|| s.matches.first())
      }).flatten();
    let data_dir_match = data_dir_match_opt.ok_or_else(|| anyhow!("data dir not found in memory"))?;
    let mut data_dir = String::from_utf8(data_dir_match.data.clone()).unwrap_or_default();
    if data_dir.is_empty() { return Err(anyhow!("empty data dir from memory")); }
    // trim trailing separators extracted by yara pattern (often ends with a backslash)
    data_dir = Self::trim_trailing_separators(data_dir);
    Ok((phone_addr, phone_len_addr, phone_region_base, data_dir))
  }

  fn scan_account_name(pid: u32, phone_addr: usize) -> Result<(String, usize)> {
    let align = 2 * std::mem::size_of::<usize>();
    let mut cursor = phone_addr.saturating_sub(align);
    let lower = phone_addr.saturating_sub(align * 20);
    let mut account_name: Option<String> = None; let mut account_addr = cursor;
    let re = Regex::new(r"^[a-zA-Z0-9_]+$").unwrap(); let mut hits=0;
    unsafe {
      while cursor >= lower {
        let len_addr = cursor.checked_add(16).ok_or_else(|| anyhow!("len addr overflow"))?;
        let mut name_len=0usize; let h=OpenProcess(PROCESS_VM_READ|PROCESS_QUERY_INFORMATION, BOOL(0), pid)?; let _=ReadProcessMemory(h, len_addr as _, &mut name_len as *mut _ as _, std::mem::size_of::<usize>(), None);
        if name_len>0 && name_len <= 20 {
          let mut buf=vec![0u8; name_len]; let _=ReadProcessMemory(h, cursor as _, buf.as_mut_ptr() as _, name_len, None);
          if let Some(z)=buf.iter().position(|&c| c==0){ buf.truncate(z); }
          let s=String::from_utf8_lossy(&buf).to_string();
          if re.is_match(&s) && s.len()>=6 && s.len() <= 20 { account_name=Some(s); account_addr=cursor; hits+=1; if hits==2 { break; } }
        }
        cursor = cursor.saturating_sub(align);
      }
    }
    account_name.ok_or_else(|| anyhow!("account name not found")).map(|v| (v, account_addr))
  }

  fn find_key(pid: u32, phone_region_base: usize, account_addr: usize, page: &[u8;4096], private_regions: &[RegionInfo]) -> Result<Option<String>> {
    const SALT_SIZE: usize = 16; const KEY_SIZE: usize = 32; const IV_SIZE: usize = 16; const HMAC_SHA1_SIZE: usize = 20; const AES_BLOCK_SIZE: usize = 16; const PAGE_SIZE: usize = 4096; const ROUND_COUNT: u32 = 64000;
    let align = 2 * std::mem::size_of::<usize>();
    let mut key_cursor = account_addr.saturating_sub(align);
    let base_limit = phone_region_base; // now use region base
    unsafe {
      let h = OpenProcess(PROCESS_VM_READ|PROCESS_QUERY_INFORMATION, BOOL(0), pid)?;
      while key_cursor >= base_limit {
        let mut ptr_candidate=0usize; let _=ReadProcessMemory(h, key_cursor as _, &mut ptr_candidate as *mut _ as _, std::mem::size_of::<usize>(), None);
        if ptr_candidate>0x10000 {
          let in_region = private_regions.iter().any(|r| ptr_candidate >= r.base && ptr_candidate + KEY_SIZE <= r.base + r.size);
          if in_region {
            let mut key_bytes=[0u8;KEY_SIZE]; let mut read=0usize; let _=ReadProcessMemory(h, ptr_candidate as _, key_bytes.as_mut_ptr() as _, KEY_SIZE, Some(&mut read));
            if read>=KEY_SIZE && key_bytes.iter().filter(|&&b| b==0).count()<5 {
              let salt = page[..SALT_SIZE].to_vec(); let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3a).collect();
              let new_key = pbkdf2_hmac_array::<Sha1, KEY_SIZE>(&key_bytes, &salt, ROUND_COUNT);
              let mac_key = pbkdf2_hmac_array::<Sha1, KEY_SIZE>(&new_key, &mac_salt, 2);
              let mut reserve = IV_SIZE + HMAC_SHA1_SIZE; reserve = if reserve % AES_BLOCK_SIZE == 0 { reserve } else { ((reserve / AES_BLOCK_SIZE)+1)*AES_BLOCK_SIZE };
              use hmac::Hmac; type HamcSha1 = Hmac<Sha1>; let mut mac = HamcSha1::new_from_slice(&mac_key).map_err(|e| anyhow!("hmac init fail {e}"))?;
              mac.update(&page[SALT_SIZE..PAGE_SIZE - reserve + IV_SIZE]); mac.update(&(1u32.to_le_bytes()));
              let hash_mac = mac.finalize().into_bytes(); let hash_mac_start = PAGE_SIZE - reserve + IV_SIZE; let hash_mac_end = hash_mac_start + hash_mac.len();
              if hash_mac_end <= PAGE_SIZE && hash_mac.as_slice() == &page[hash_mac_start..hash_mac_end] { tracing::debug!("v3 key found ptr=0x{:x}", ptr_candidate); return Ok(Some(hex::encode(key_bytes))); }
            }
          }
        }
        key_cursor = key_cursor.saturating_sub(align);
      }
    }
    Ok(None)
  }

  fn extract_data_key(pid: u32) -> Result<(Option<String>, Option<String>, Option<String>)> {
    unsafe {
      let h = OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, BOOL(0), pid)?; if h.is_invalid() { return Err(anyhow!("OpenProcess v3 failed")); }
      let raw_regions = Self::enumerate_regions(h);
      // Build RegionInfo list
      let dll_ranges = Self::enumerate_wechatwin_writable(pid).unwrap_or_default();
      let mut regions: Vec<RegionInfo> = Vec::new();
      for r in &raw_regions {
        let base = r.BaseAddress as usize; let size = r.RegionSize; let prot = r.Protect.0;
        let writable_flags = prot & (PAGE_READWRITE.0 | PAGE_WRITECOPY.0 | PAGE_EXECUTE_READWRITE.0 | PAGE_EXECUTE_WRITECOPY.0) != 0;
        let private_writable = writable_flags && r.Type.0 == MEM_PRIVATE.0;
        let dll_writable = dll_ranges.iter().any(|(b,s)| base >= *b && base < *b + *s) && writable_flags;
        regions.push(RegionInfo { base, size, _protect: prot, private_writable, dll_writable });
      }
      let dll_writable_regions: Vec<RegionInfo> = regions.iter().filter(|ri| ri.dll_writable).cloned().collect();
      let private_regions: Vec<RegionInfo> = regions.iter().filter(|ri| ri.private_writable).cloned().collect();
      if dll_writable_regions.is_empty() { tracing::warn!("v3 no dll writable regions (phone string filtering may fail)"); }
      if private_regions.is_empty() { tracing::warn!("v3 no private writable regions (key scan may fail)"); }
      let (phone_addr, _phone_len_addr, phone_region_base, data_dir) = Self::filter_phone_and_datadir(pid, &dll_writable_regions, &private_regions)?;
      tracing::debug!("v3 phone_addr=0x{:x} phone_region_base=0x{:x} data_dir={}", phone_addr, phone_region_base, data_dir);
      // Account name scan
      let (account_name, account_addr) = Self::scan_account_name(pid, phone_addr)?;
      tracing::debug!("v3 account_name={} account_addr=0x{:x}", account_name, account_addr);
      // Read Misc.db first page
      use std::io::Read; let mut db_path = PathBuf::from(&data_dir); db_path.push("Msg"); db_path.push("Misc.db");
      let mut page = [0u8;4096]; if let Ok(mut f) = std::fs::File::open(&db_path) { let _=f.read(&mut page); } else { tracing::warn!("v3 Misc.db not found {}", db_path.display()); return Ok((Some(account_name), Some(data_dir), None)); }
      let data_key = Self::find_key(pid, phone_region_base, account_addr, &page, &private_regions)?;
      Ok((Some(account_name), Some(data_dir), data_key))
    }
  }

  // 已弃用: decrypt_and_extract_avatar_v3 / heuristic_avatar_scan / scan_image_in_slice
}

impl KeyExtractor for WinV3Extractor {
  fn detect(&self) -> Result<bool> { Ok(!winproc::find_wechat_v3_processes()?.is_empty()) }
  fn extract_db_keys(&self, _data_dir: Option<&str>) -> Result<WechatKeys> {
    let procs = winproc::find_wechat_v3_processes()?; if procs.is_empty() { return Err(anyhow!("未找到微信 v3 进程")); }
    let selected = procs.into_iter().next().unwrap();
    let pid = selected.pid as u32;
    let (account_name, data_dir, data_key_hex) = Self::extract_data_key(pid)?;
    let full_version = selected.full_version.clone().unwrap_or_default();
    let client_version = full_version.split('.').next().map(|maj| if maj == "3" { "v3" } else { "unknown" }).unwrap_or("v3").to_string();
    let avatar_base64 = None;
    Ok(WechatKeys { ok: true, data_key: data_key_hex, db_keys: vec![], image_key: None, xor_key: None, client_type: "win".into(), client_version, account_name, data_dir, method: Some("memory-v3".into()), pid: Some(pid), avatar_base64 })
  }
  fn extract_img_keys(&self, _data_dir: Option<&str>) -> Result<WechatKeys> {
    // v3 不支持图片密钥提取
    let procs = winproc::find_wechat_v3_processes()?; if procs.is_empty() { return Err(anyhow!("未找到微信 v3 进程")); }
    let selected = procs.into_iter().next().unwrap();
    let pid = selected.pid as u32;
    let full_version = selected.full_version.clone().unwrap_or_default();
    let client_version = full_version.split('.').next().map(|maj| if maj == "3" { "v3" } else { "unknown" }).unwrap_or("v3").to_string();
    Ok(WechatKeys { ok: true, data_key: None, db_keys: vec![], image_key: None, xor_key: None, client_type: "win".into(), client_version, account_name: None, data_dir: None, method: Some("memory-v3".into()), pid: Some(pid), avatar_base64: None })
  }
}
