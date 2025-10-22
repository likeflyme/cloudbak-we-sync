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
    let data_dir = String::from_utf8(data_dir_match.data.clone()).unwrap_or_default();
    if data_dir.is_empty() { return Err(anyhow!("empty data dir from memory")); }
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

  fn decrypt_and_extract_avatar_v3(data_dir: &str, account_name: &str, key_hex: &str) -> Option<String> {
    use std::path::PathBuf; use std::io::Read; use base64::Engine;
    const PAGE_SIZE: usize = 4096; const SALT_SIZE: usize = 16; const IV_SIZE: usize = 16; const HMAC_SHA1_SIZE: usize = 20; const AES_BLOCK_SIZE: usize = 16; const ROUND_COUNT: u32 = 64000; const KEY_SIZE: usize = 32;
    tracing::debug!(%data_dir, %account_name, key_hex_len=key_hex.len(), "v3 avatar: start decrypt/extract");
    let key_bytes = match hex::decode(key_hex) { Ok(v) if v.len()==KEY_SIZE => v, _ => { tracing::warn!("v3 avatar: key hex invalid len={}", key_hex.len()); return None } };
    tracing::debug!("v3 avatar: raw key bytes decoded len={}", key_bytes.len());
    let mut misc = PathBuf::from(data_dir); misc.push("Msg"); misc.push("Misc.db");
    tracing::debug!(path=%misc.display(), "v3 avatar: open Misc.db");
    let mut file = match std::fs::File::open(&misc) { Ok(f) => f, Err(e) => { tracing::warn!(error=%e,"v3 avatar: open Misc.db fail"); return None } };
    let mut file_bytes = Vec::new(); let _ = file.read_to_end(&mut file_bytes); let total_len = file_bytes.len();
    tracing::debug!(size=total_len, "v3 avatar: Misc.db size");
    if file_bytes.len() < PAGE_SIZE { tracing::warn!("v3 avatar: file too small < page_size"); return None; }
    let salt = file_bytes[..SALT_SIZE].to_vec(); let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3a).collect();
    tracing::debug!(salt_hex=%hex::encode(&salt), "v3 avatar: salt loaded");
    use pbkdf2::pbkdf2_hmac_array; use sha1::Sha1; let new_key = pbkdf2_hmac_array::<Sha1, KEY_SIZE>(&key_bytes, &salt, ROUND_COUNT); let mac_key = pbkdf2_hmac_array::<Sha1, KEY_SIZE>(&new_key, &mac_salt, 2);
    tracing::debug!(derived_key_head=%hex::encode(&new_key[..8]), mac_key_head=%hex::encode(&mac_key[..8]), "v3 avatar: keys derived (truncated)");
    use hmac::{Hmac, Mac}; type HmacSha1 = Hmac<Sha1>;
    let mut reserve = IV_SIZE + HMAC_SHA1_SIZE; reserve = if reserve % AES_BLOCK_SIZE == 0 { reserve } else { ((reserve / AES_BLOCK_SIZE)+1)*AES_BLOCK_SIZE }; tracing::debug!(reserve, "v3 avatar: reserve bytes");
    let total_pages = file_bytes.len() / PAGE_SIZE; let max_pages = total_pages.min(400); tracing::debug!(total_pages, max_pages, "v3 avatar: page calc");
    let mut plaintext_pages: Vec<Vec<u8>> = Vec::new();
    use aes::cipher::{BlockDecrypt, KeyInit}; use aes::Aes256; use aes::cipher::generic_array::GenericArray;
    let mut hmac_ok_pages = 0usize; let mut hmac_fail_pages = 0usize; let mut skip_alignment = 0usize; let mut skip_region = 0usize;
    for pi in 0..max_pages {
      let start = pi * PAGE_SIZE; let end = start + PAGE_SIZE; if end > file_bytes.len() { tracing::debug!(pi, "v3 avatar: page end overflow break"); break; }
      let page = &file_bytes[start..end];
      let hash_mac_start = PAGE_SIZE - reserve + IV_SIZE; let hash_mac_end = hash_mac_start + HMAC_SHA1_SIZE; if hash_mac_end > PAGE_SIZE { skip_region+=1; tracing::debug!(pi, "v3 avatar: mac region overflow skip"); continue; }
      let mut mac_le = <HmacSha1 as Mac>::new_from_slice(&mac_key).ok()?; let mut mac_be = <HmacSha1 as Mac>::new_from_slice(&mac_key).ok()?;
      let content_start = if pi == 0 { SALT_SIZE } else { 0 }; if hash_mac_start <= content_start || hash_mac_end > PAGE_SIZE { skip_alignment+=1; tracing::debug!(pi, hash_mac_start, content_start, "v3 avatar: alignment invalid skip"); continue; }
      let content_slice = &page[content_start..PAGE_SIZE - reserve + IV_SIZE];
      mac_le.update(content_slice); mac_be.update(content_slice);
      let pgno_le = ((pi as u32)+1).to_le_bytes(); let pgno_be = ((pi as u32)+1).to_be_bytes();
      mac_le.update(&pgno_le); mac_be.update(&pgno_be);
      let calc_le = mac_le.finalize().into_bytes(); let calc_be = mac_be.finalize().into_bytes();
      let stored = &page[hash_mac_start..hash_mac_end];
      let hmac_match_le = calc_le.as_slice() == stored; let hmac_match_be = calc_be.as_slice() == stored;
      if !(hmac_match_le || hmac_match_be) { hmac_fail_pages+=1; if pi < 10 { tracing::debug!(pi, stored=%hex::encode(stored), calc_le=%hex::encode(&calc_le[..]), calc_be=%hex::encode(&calc_be[..]), "v3 avatar: hmac mismatch"); } continue; } else { hmac_ok_pages+=1; tracing::debug!(pi, hmac_le=hmac_match_le, hmac_be=hmac_match_be, "v3 avatar: hmac ok"); }
      let iv_offset = PAGE_SIZE - reserve; let iv_end = iv_offset + IV_SIZE; if iv_end > PAGE_SIZE { skip_region+=1; tracing::debug!(pi, "v3 avatar: iv region overflow skip"); continue; } let iv = &page[iv_offset..iv_end];
      let cipher_region_end = PAGE_SIZE - reserve; let cipher_region = &page[content_start..cipher_region_end]; if cipher_region.is_empty() || cipher_region.len() % 16 != 0 { skip_alignment+=1; tracing::debug!(pi, cipher_len=cipher_region.len(), "v3 avatar: cipher len invalid skip"); continue; }
      let cipher = match Aes256::new_from_slice(&new_key) { Ok(c) => c, Err(e) => { tracing::debug!(pi, error=%e, "v3 avatar: aes init fail"); continue } };
      let mut prev = iv.to_vec(); let mut out = Vec::with_capacity(cipher_region.len());
      for block_bytes in cipher_region.chunks_exact(16) {
        let mut block = GenericArray::clone_from_slice(block_bytes); cipher.decrypt_block(&mut block); for i in 0..16 { block[i] ^= prev[i]; } out.extend_from_slice(&block); prev.copy_from_slice(block_bytes);
      }
      plaintext_pages.push(out);
    }
    tracing::debug!(hmac_ok_pages, hmac_fail_pages, skip_alignment, skip_region, decrypted_pages=plaintext_pages.len(), "v3 avatar: page decrypt summary");
    if plaintext_pages.is_empty() { tracing::warn!("v3 avatar: no decrypted pages, fallback raw heuristic"); return Self::heuristic_avatar_scan(&file_bytes, account_name); }
    let needle = account_name.as_bytes(); const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n"; const JPG_SIG: &[u8] = b"\xFF\xD8\xFF"; const GIF_SIG: &[u8] = b"GIF";
    let mut page_index = 0usize; for page in &plaintext_pages {
      let mut search_pos = 0usize; let mut local_hits=0usize; while let Some(pos) = memchr::memmem::find(&page[search_pos..], needle) {
        let gp = search_pos + pos; local_hits+=1; let slice = &page[gp..std::cmp::min(gp+120_000, page.len())];
        if let Some(p) = memchr::memmem::find(slice, PNG_SIG) { if let Some(iend) = memchr::memmem::find(&slice[p..], b"IEND\xAE\x42\x60\x82") { let endp = p + iend + 8; let img = &slice[p..endp]; let b64 = base64::engine::general_purpose::STANDARD.encode(img); tracing::info!(page=page_index, offset=gp, "v3 avatar: png found decrypted"); return Some(format!("data:image/png;base64,{}", b64)); } }
        if let Some(p) = memchr::memmem::find(slice, JPG_SIG) { if let Some(mut end_idx) = memchr::memmem::find(&slice[p+3..], b"\xFF\xD9") { end_idx += p+3+2; let img=&slice[p..end_idx]; let b64=base64::engine::general_purpose::STANDARD.encode(img); tracing::info!(page=page_index, offset=gp, "v3 avatar: jpg found decrypted"); return Some(format!("data:image/jpeg;base64,{}", b64)); } }
        if let Some(p) = memchr::memmem::find(slice, GIF_SIG) { if p+6 < slice.len() && (&slice[p..p+6]==b"GIF89a"||&slice[p..p+6]==b"GIF87a") { let tail=&slice[p..std::cmp::min(p+60_000, page.len())]; if let Some(re)=tail.iter().rposition(|&b| b==0x3B){ let img=&tail[..=re]; let b64=base64::engine::general_purpose::STANDARD.encode(img); tracing::info!(page=page_index, offset=gp, "v3 avatar: gif found decrypted"); return Some(format!("data:image/gif;base64,{}", b64)); } } }
        search_pos = gp + needle.len(); }
      tracing::debug!(page=page_index, account_hits=local_hits, "v3 avatar: page scan done no image"); page_index+=1; }
    tracing::warn!("v3 avatar: decrypted scan miss, fallback raw heuristic");
    let fallback = Self::heuristic_avatar_scan(&file_bytes, account_name);
    if fallback.is_some() { tracing::info!("v3 avatar: fallback heuristic found image"); } else { tracing::warn!("v3 avatar: fallback heuristic no image"); }
    fallback
  }

  // 原始未解密文件启发式扫描（供回退）
  fn heuristic_avatar_scan(file_bytes: &[u8], account_name: &str) -> Option<String> {
    use base64::Engine; let needle = account_name.as_bytes(); const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n"; const JPG_SIG: &[u8] = b"\xFF\xD8\xFF"; const GIF_SIG: &[u8] = b"GIF";
    let mut search_pos = 0usize; while let Some(pos) = memchr::memmem::find(&file_bytes[search_pos..], needle) {
      let gp = search_pos + pos; let slice = &file_bytes[gp..std::cmp::min(gp+180_000, file_bytes.len())];
      if let Some(p) = memchr::memmem::find(slice, PNG_SIG) { if let Some(iend) = memchr::memmem::find(&slice[p..], b"IEND\xAE\x42\x60\x82") { let endp=p+iend+8; let img=&slice[p..endp]; let b64=base64::engine::general_purpose::STANDARD.encode(img); return Some(format!("data:image/png;base64,{}", b64)); } }
      if let Some(p) = memchr::memmem::find(slice, JPG_SIG) { if let Some(mut e) = memchr::memmem::find(&slice[p+3..], b"\xFF\xD9") { e+=p+3+2; let img=&slice[p..e]; let b64=base64::engine::general_purpose::STANDARD.encode(img); return Some(format!("data:image/jpeg;base64,{}", b64)); } }
      if let Some(p) = memchr::memmem::find(slice, GIF_SIG) { if p+6 < slice.len() && (&slice[p..p+6]==b"GIF89a"||&slice[p..p+6]==b"GIF87a") { let tail=&slice[p..std::cmp::min(p+90_000, slice.len())]; if let Some(re)=tail.iter().rposition(|&b| b==0x3B){ let img=&tail[..=re]; let b64=base64::engine::general_purpose::STANDARD.encode(img); return Some(format!("data:image/gif;base64,{}", b64)); } } }
      search_pos = gp + needle.len(); }
    None
  }
}

impl KeyExtractor for WinV3Extractor {
  fn detect(&self) -> Result<bool> { Ok(!winproc::find_wechat_v3_processes()?.is_empty()) }
  fn extract(&self) -> Result<WechatKeys> {
    let procs = winproc::find_wechat_v3_processes()?; if procs.is_empty() { return Err(anyhow!("未找到微信 v3 进程")); }
    let selected = procs.into_iter().next().unwrap();
    let pid = selected.pid as u32;
    let (account_name, data_dir, data_key_hex) = Self::extract_data_key(pid)?;
    let full_version = selected.full_version.clone().unwrap_or_default();
    let client_version = full_version.split('.').next().map(|maj| if maj == "3" { "v3" } else { "unknown" }).unwrap_or("v3").to_string();
    let mut avatar_base64 = None;
    if let (Some(ref dir), Some(ref acct), Some(ref key_hex)) = (&data_dir, &account_name, &data_key_hex) {
      avatar_base64 = Self::decrypt_and_extract_avatar_v3(dir, acct, key_hex);
      if avatar_base64.is_none() { tracing::debug!("v3 avatar decrypt scan failed"); }
    }
    Ok(WechatKeys { ok: true, data_key: data_key_hex, image_key: None, xor_key: None, client_type: "win".into(), client_version, account_name, data_dir, method: Some("memory-v3".into()), pid: Some(pid), avatar_base64 })
  }
}
