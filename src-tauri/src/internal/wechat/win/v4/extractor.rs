use anyhow::{Result, anyhow};
use crate::internal::windows::winproc;
use crate::internal::wechat::common::types::WechatKeys;
use crate::internal::wechat::common::extractor_trait::KeyExtractor;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct WinV4Extractor;

impl KeyExtractor for WinV4Extractor {
  fn detect(&self) -> Result<bool> {
    // 只要找到 v4 进程即认为可用
    let procs = winproc::find_wechat_v4_processes()?;
    Ok(!procs.is_empty())
  }
  fn extract_db_keys(&self, data_dir: Option<&str>) -> Result<WechatKeys> {
    let procs = winproc::find_wechat_v4_processes()?;
    if procs.is_empty() { return Err(anyhow!("未找到微信 v4 进程")); }
    let selected = procs.iter().find(|p| p.status == "online" && p.data_dir.is_some()).cloned()
      .unwrap_or_else(|| procs.into_iter().next().unwrap());
    let pid = selected.pid as u32;
    let account_name = selected.account_name.clone();
    // 优先使用前端传入的 data_dir，其次使用进程检测到的
    let effective_dir = data_dir.map(|s| s.to_string()).or_else(|| selected.data_dir.clone());
    let full_version = selected.full_version.clone().unwrap_or_default();
    let client_version = full_version.split('.').next().map(|maj| if maj == "4" { "v4" } else { "unknown" }).unwrap_or("unknown").to_string();
    // 通过扫描进程内存中的 x'<96 hex chars>' 模式提取数据库密钥
    let db_keys = Self::scan_db_keys_from_memory(pid)?;
    // Heuristic avatar extraction (contact image) from data_dir if present
    let mut avatar_base64: Option<String> = None;
    if let (Some(ref dir), Some(ref acct)) = (&effective_dir, &account_name) {
      use std::io::Read; use base64::Engine;
      let mut misc = PathBuf::from(dir); misc.push("db_storage"); misc.push("contact"); misc.push("contact.db");
      if let Ok(mut f) = std::fs::File::open(&misc) { let mut buf=Vec::new(); let _=f.read_to_end(&mut buf);
        let needle = acct.as_bytes(); const PNG_SIG:&[u8]=b"\x89PNG\r\n\x1a\n"; const JPG_SIG:&[u8]=b"\xFF\xD8\xFF"; const GIF_SIG:&[u8]=b"GIF";
        let mut pos0=0usize; let mut found: Option<Vec<u8>>=None; while let Some(pos)=memchr::memmem::find(&buf[pos0..], needle){ let gp=pos0+pos; let slice=&buf[gp..std::cmp::min(gp+180_000, buf.len())];
          if let Some(p)=memchr::memmem::find(slice, PNG_SIG){ if let Some(iend)=memchr::memmem::find(&slice[p..], b"IEND\xAE\x42\x60\x82"){ let end=p+iend+8; found=Some(slice[p..end].to_vec()); break; } }
          if found.is_none(){ if let Some(p)=memchr::memmem::find(slice, JPG_SIG){ if let Some(mut e)=memchr::memmem::find(&slice[p+3..], b"\xFF\xD9"){ e+=p+3+2; found=Some(slice[p..e].to_vec()); break; } } }
          if found.is_none(){ if let Some(p)=memchr::memmem::find(slice, GIF_SIG){ if p+6<slice.len() && (&slice[p..p+6]==b"GIF89a"||&slice[p..p+6]==b"GIF87a"){ let tail=&slice[p..std::cmp::min(p+90_000,slice.len())]; if let Some(re)=tail.iter().rposition(|&b| b==0x3B){ found=Some(tail[..=re].to_vec()); break; } } } }
          pos0 = gp + needle.len(); }
        if let Some(img)=found { let mime = if img.starts_with(PNG_SIG){"image/png"} else if img.starts_with(JPG_SIG){"image/jpeg"} else if img.starts_with(GIF_SIG){"image/gif"} else {"application/octet-stream"}; let b64 = base64::engine::general_purpose::STANDARD.encode(&img); avatar_base64=Some(format!("data:{};base64,{}", mime, b64)); }
      }
    }
    Ok(WechatKeys {
      ok: true,
      data_key: None,
      db_keys,
      image_key: None,
      xor_key: None,
      client_type: "win".into(),
      client_version,
      account_name,
      data_dir: effective_dir,
      method: Some("primary".into()),
      pid: Some(pid),
      avatar_base64,
    })
  }
  fn extract_img_keys(&self, data_dir: Option<&str>) -> Result<WechatKeys> {
    let procs = winproc::find_wechat_v4_processes()?;
    if procs.is_empty() { return Err(anyhow!("未找到微信 v4 进程")); }
    let selected = procs.iter().find(|p| p.status == "online" && p.data_dir.is_some()).cloned()
      .unwrap_or_else(|| procs.into_iter().next().unwrap());
    let pid = selected.pid as u32;
    let account_name = selected.account_name.clone();
    let effective_dir = data_dir.map(|s| s.to_string()).or_else(|| selected.data_dir.clone());
    let full_version = selected.full_version.clone().unwrap_or_default();
    let client_version = full_version.split('.').next().map(|maj| if maj == "4" { "v4" } else { "unknown" }).unwrap_or("unknown").to_string();

    // --- 新的图片密钥提取逻辑 (参考 Python find_image_key.py) ---
    // 1. 从 data_dir 推导 attach_dir, 找到 V2 .dat 文件的 AES 密文块
    let (ciphertext, xor_key) = if let Some(ref dir) = effective_dir {
      // data_dir 指向账号目录 (如 wxid_xxx), attach_dir = {account_dir}/msg/attach
      let attach_dir = Path::new(dir).join("msg").join("attach");
      let ct = Self::find_v2_ciphertext(&attach_dir);
      let xor = Self::find_xor_key(&attach_dir);
      (ct, xor)
    } else {
      (None, None)
    };

    // 2. 扫描进程内存寻找 AES key (16/32 字符 alphanumeric ASCII)
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
      client_type: "win".into(),
      client_version,
      account_name,
      data_dir: effective_dir,
      method: Some("primary".into()),
      pid: Some(pid),
      avatar_base64: None,
    })
  }

  fn detect_data_dirs(&self) -> Result<Vec<String>> {
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    if appdata.is_empty() {
      return Ok(vec![]);
    }
    let config_dir = Path::new(&appdata).join("Tencent").join("xwechat").join("config");
    if !config_dir.is_dir() {
      return Ok(vec![]);
    }

    // 从 ini 文件中找到有效的目录路径
    let mut data_roots: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&config_dir) {
      for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("ini") {
          continue;
        }
        // 尝试以 utf-8 和 gbk 读取
        let content = Self::read_ini_content(&path);
        if let Some(text) = content {
          let trimmed = text.trim().to_string();
          // 跳过包含换行符或 NUL 的内容
          if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains('\r') || trimmed.contains('\0') {
            continue;
          }
          let dir = Path::new(&trimmed);
          if dir.is_dir() {
            data_roots.push(dir.to_path_buf());
          }
        }
      }
    }

    // 在每个根目录下搜索 xwechat_files\*\db_storage
    let mut seen: HashSet<String> = HashSet::new();
    let mut candidates: Vec<String> = Vec::new();
    for root in &data_roots {
      let xwechat_files = root.join("xwechat_files");
      if !xwechat_files.is_dir() {
        continue;
      }
      if let Ok(entries) = std::fs::read_dir(&xwechat_files) {
        for entry in entries.flatten() {
          let account_dir = entry.path();
          let db_storage = account_dir.join("db_storage");
          if db_storage.is_dir() {
            let normalized = account_dir.to_string_lossy().to_lowercase().replace('/', "\\");
            if !seen.contains(&normalized) {
              seen.insert(normalized);
              // 返回账号目录（不含 \db_storage），与进程检测 try_infer_data_dir 保持一致
              candidates.push(account_dir.to_string_lossy().to_string());
            }
          }
        }
      }
    }

    Ok(candidates)
  }
}

impl WinV4Extractor {
  /// 尝试以 utf-8 和 gbk 编码读取 ini 文件内容（前 1024 字节）
  fn read_ini_content(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let slice = &bytes[..bytes.len().min(1024)];
    // 先尝试 UTF-8
    if let Ok(s) = std::str::from_utf8(slice) {
      return Some(s.to_string());
    }
    // 再尝试 GBK
    use encoding_rs::GBK;
    let (cow, _, had_errors) = GBK.decode(slice);
    if !had_errors {
      return Some(cow.into_owned());
    }
    None
  }

  /// 扫描微信 v4 进程内存，查找 x'<96 hex chars>' 模式的数据库密钥
  #[cfg(target_os = "windows")]
  fn scan_db_keys_from_memory(pid: u32) -> Result<Vec<String>> {
    use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows::Win32::System::Memory::{MEM_COMMIT, MEMORY_BASIC_INFORMATION, VirtualQueryEx};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
    use windows::Win32::Foundation::BOOL;
    use std::collections::HashSet;
    use crate::commands::wechat::is_extract_cancelled;

    // x'<96 hex chars>' 的前缀 b"x'" 用于快速定位
    const PREFIX: &[u8] = b"x'";
    const HEX_LEN: usize = 96;
    // 总长度: x' + 96 hex + ' = 99
    const MATCH_TOTAL: usize = 2 + HEX_LEN + 1;

    fn is_hex_char(b: u8) -> bool {
      matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')
    }

    unsafe {
      let h = OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, BOOL(0), pid)
        .map_err(|e| anyhow!("OpenProcess failed: {}", e))?;

      let mut results: HashSet<String> = HashSet::new();
      let mut address: usize = 0;
      let max_addr: usize = 0x7FFF_FFFF_FFFF;

      while address < max_addr {
        if is_extract_cancelled() {
          tracing::info!("scan_db_keys_from_memory cancelled");
          break;
        }

        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let res = VirtualQueryEx(h, Some(address as _), &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>());
        if res == 0 { break; }

        let base = mbi.BaseAddress as usize;
        let size = mbi.RegionSize;

        // 只扫描已提交的内存区域
        if mbi.State == MEM_COMMIT && size > 0 && size <= 512 * 1024 * 1024 {
          // 分块读取，每块 16MB
          const CHUNK_SIZE: usize = 16 * 1024 * 1024;
          let mut offset = 0usize;
          while offset < size {
            if is_extract_cancelled() { break; }
            let to_read = std::cmp::min(CHUNK_SIZE, size - offset);
            let mut buf = vec![0u8; to_read];
            let mut bytes_read = 0usize;
            let ok = ReadProcessMemory(
              h, (base + offset) as _, buf.as_mut_ptr() as _, to_read, Some(&mut bytes_read)
            );
            if ok.is_ok() && bytes_read > 0 {
              buf.truncate(bytes_read);
              // 在 buf 中搜索所有 x'<96 hex>' 匹配
              let mut search_pos = 0usize;
              while search_pos + MATCH_TOTAL <= buf.len() {
                if let Some(pos) = memchr::memmem::find(&buf[search_pos..], PREFIX) {
                  let abs_pos = search_pos + pos;
                  if abs_pos + MATCH_TOTAL <= buf.len() {
                    let hex_start = abs_pos + 2;
                    let hex_end = hex_start + HEX_LEN;
                    let closing = hex_end;
                    if buf[closing] == b'\'' && buf[hex_start..hex_end].iter().all(|&b| is_hex_char(b)) {
                      let hex_str = std::str::from_utf8(&buf[hex_start..hex_end])
                        .unwrap_or_default()
                        .to_lowercase();
                      results.insert(hex_str);
                    }
                    search_pos = abs_pos + 1; // 继续搜索下一个
                  } else {
                    break; // 剩余数据不够一个完整匹配
                  }
                } else {
                  break; // 当前块中没有更多匹配
                }
              }
            }
            offset += to_read;
          }
        }

        let next = base.saturating_add(size);
        address = if next <= address { address.saturating_add(0x1000) } else { next };
      }

      tracing::info!("scan_db_keys_from_memory found {} unique keys", results.len());
      Ok(results.into_iter().collect())
    }
  }

  #[cfg(not(target_os = "windows"))]
  fn scan_db_keys_from_memory(_pid: u32) -> Result<Vec<String>> {
    Ok(vec![])
  }

  /// 从 V2 .dat 缩略图文件中提取第一个 16 字节 AES 密文块
  /// V2 文件结构: [6B magic: 07 08 'V' '2' 08 07] [4B aes_size LE] [4B xor_size LE] [1B padding] [aes_data...]
  /// 密文块位于 offset 15..31
  fn find_v2_ciphertext(attach_dir: &Path) -> Option<[u8; 16]> {
    use std::io::Read;
    const V2_MAGIC: &[u8; 6] = b"\x07\x08V2\x08\x07";

    // 递归搜索 *_t.dat (缩略图, 最可能是 JPEG)
    let mut dat_files: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(attach_dir).into_iter().flatten() {
      if !entry.file_type().is_file() { continue; }
      let name = entry.file_name().to_string_lossy();
      if name.ends_with("_t.dat") {
        dat_files.push(entry.path().to_path_buf());
      }
      if dat_files.len() >= 200 { break; }
    }
    // 按修改时间降序排列 (最新优先)
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

  /// 从 V2 缩略图文件末尾推导 XOR key (JPEG 结尾 FF D9)
  fn find_xor_key(attach_dir: &Path) -> Option<u8> {
    use std::io::{Read, Seek, SeekFrom};
    use std::collections::HashMap;
    const V2_MAGIC: &[u8; 6] = b"\x07\x08V2\x08\x07";

    let mut dat_files: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(attach_dir).into_iter().flatten() {
      if !entry.file_type().is_file() { continue; }
      let name = entry.file_name().to_string_lossy();
      if name.ends_with("_t.dat") {
        dat_files.push(entry.path().to_path_buf());
      }
      if dat_files.len() >= 100 { break; }
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
        if fp.read_exact(&mut head).is_err() || &head != V2_MAGIC { continue; }
        let sz = match fp.metadata() { Ok(m) => m.len(), Err(_) => continue };
        if sz < 8 { continue; }
        if fp.seek(SeekFrom::End(-2)).is_err() { continue; }
        let mut tail = [0u8; 2];
        if fp.read_exact(&mut tail).is_ok() {
          *tail_counts.entry((tail[0], tail[1])).or_insert(0) += 1;
        }
      }
    }
    if tail_counts.is_empty() { return None; }
    let &(x, y) = tail_counts.iter().max_by_key(|(_, v)| *v).map(|(k, _)| k)?;
    let xor_key = x ^ 0xFF;
    let check = y ^ 0xD9;
    if xor_key == check {
      tracing::info!("XOR key verified: 0x{:02x}", xor_key);
    } else {
      tracing::warn!("XOR key mismatch: 0x{:02x} vs 0x{:02x}, using best guess 0x{:02x}", xor_key, check, xor_key);
    }
    Some(xor_key)
  }

  /// 尝试用给定 key 解密 AES-ECB 密文块, 检查是否为已知图片格式
  fn try_aes_key(key: &[u8], ciphertext: &[u8; 16]) -> bool {
    use aes::cipher::{BlockDecrypt, KeyInit};
    use aes::Aes128;
    if key.len() < 16 { return false; }
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

  /// 扫描进程内存寻找 AES 图片密钥 (16/32 字符 alphanumeric ASCII)
  /// 分两阶段: 先扫 RW 区域, 再扫其余可读区域
  #[cfg(target_os = "windows")]
  fn scan_aes_key_from_memory(pid: u32, ciphertext: &[u8; 16]) -> Result<Option<String>> {
    use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows::Win32::System::Memory::MEM_COMMIT;
    use windows::Win32::System::Memory::MEMORY_BASIC_INFORMATION;
    use windows::Win32::System::Memory::VirtualQueryEx;
    use windows::Win32::System::Memory::PAGE_NOACCESS;
    use windows::Win32::System::Memory::PAGE_GUARD;
    use windows::Win32::System::Memory::PAGE_READWRITE;
    use windows::Win32::System::Memory::PAGE_WRITECOPY;
    use windows::Win32::System::Memory::PAGE_EXECUTE_READWRITE;
    use windows::Win32::System::Memory::PAGE_EXECUTE_WRITECOPY;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
    use windows::Win32::Foundation::BOOL;
    use crate::commands::wechat::is_extract_cancelled;

    #[inline]
    fn is_alnum(b: u8) -> bool {
      matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z')
    }

    #[inline]
    fn is_rw_protect(protect: u32) -> bool {
      let rw_flags = PAGE_READWRITE.0 | PAGE_WRITECOPY.0
        | PAGE_EXECUTE_READWRITE.0 | PAGE_EXECUTE_WRITECOPY.0;
      (protect & rw_flags) != 0
    }

    /// 扫描一组区域, 返回找到的 key 或 None
    unsafe fn scan_regions(
      h: windows::Win32::Foundation::HANDLE,
      regions: &[(usize, usize)],
      ciphertext: &[u8; 16],
    ) -> Result<Option<String>> {
      let mut candidates_32 = 0usize;
      let mut candidates_16 = 0usize;

      for (idx, &(base_addr, region_size)) in regions.iter().enumerate() {
        if is_extract_cancelled() { return Ok(None); }
        if idx % 100 == 0 {
          tracing::debug!("scanning region {}/{}", idx, regions.len());
        }

        let mut buf = vec![0u8; region_size];
        let mut bytes_read = 0usize;
        let ok = ReadProcessMemory(
          h, base_addr as _, buf.as_mut_ptr() as _, region_size, Some(&mut bytes_read),
        );
        if ok.is_err() || bytes_read < 32 { continue; }
        buf.truncate(bytes_read);

        // 搜索 32 字符 alphanumeric 候选 (前后非 alnum 边界)
        let mut pos = 0usize;
        while pos < buf.len() {
          if !is_alnum(buf[pos]) { pos += 1; continue; }
          // 确认左边界
          if pos > 0 && is_alnum(buf[pos - 1]) {
            // 在一个更长的 alnum run 中间, 跳到 run 结尾
            while pos < buf.len() && is_alnum(buf[pos]) { pos += 1; }
            continue;
          }
          // 测量连续 alnum 长度
          let start = pos;
          while pos < buf.len() && is_alnum(buf[pos]) { pos += 1; }
          let run_len = pos - start;

          if run_len == 32 {
            candidates_32 += 1;
            // 尝试前 16 字符作为 AES-128 key
            if WinV4Extractor::try_aes_key(&buf[start..start + 16], ciphertext) {
              let key_str = std::str::from_utf8(&buf[start..start + 16]).unwrap_or_default().to_string();
              tracing::info!("Found AES key (32-char, first 16): {}", &key_str);
              return Ok(Some(key_str));
            }
          } else if run_len == 16 {
            candidates_16 += 1;
            if WinV4Extractor::try_aes_key(&buf[start..start + 16], ciphertext) {
              let key_str = std::str::from_utf8(&buf[start..start + 16]).unwrap_or_default().to_string();
              tracing::info!("Found AES key (16-char): {}", &key_str);
              return Ok(Some(key_str));
            }
          }
          // run_len 不为 16 或 32 的跳过
        }
      }

      tracing::debug!("tested {} x 32-char + {} x 16-char candidates", candidates_32, candidates_16);
      Ok(None)
    }

    unsafe {
      let h = OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, BOOL(0), pid)
        .map_err(|e| anyhow!("OpenProcess failed: {}", e))?;

      // Phase 0: 枚举所有可用内存区域, 分为 RW 和其他
      let mut rw_regions: Vec<(usize, usize)> = Vec::new();
      let mut other_regions: Vec<(usize, usize)> = Vec::new();
      let mut address: usize = 0;
      let max_addr: usize = 0x7FFF_FFFF_FFFF;

      while address < max_addr {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let res = VirtualQueryEx(h, Some(address as _), &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>());
        if res == 0 { break; }
        let base = mbi.BaseAddress as usize;
        let size = mbi.RegionSize;
        if mbi.State == MEM_COMMIT
          && mbi.Protect.0 != PAGE_NOACCESS.0
          && (mbi.Protect.0 & PAGE_GUARD.0) == 0
          && size <= 50 * 1024 * 1024
        {
          if is_rw_protect(mbi.Protect.0) {
            rw_regions.push((base, size));
          } else {
            other_regions.push((base, size));
          }
        }
        let next = base.saturating_add(size);
        address = if next <= address { address.saturating_add(0x1000) } else { next };
      }

      let rw_mb = rw_regions.iter().map(|r| r.1).sum::<usize>() as f64 / 1024.0 / 1024.0;
      let all_mb = (rw_regions.iter().map(|r| r.1).sum::<usize>()
        + other_regions.iter().map(|r| r.1).sum::<usize>()) as f64 / 1024.0 / 1024.0;
      tracing::info!("RW regions: {} ({:.0} MB), total: {} ({:.0} MB)",
        rw_regions.len(), rw_mb, rw_regions.len() + other_regions.len(), all_mb);

      // Phase 1: 扫描 RW 区域 (key 字符串最可能在这里)
      tracing::info!("Phase 1: scanning RW memory regions");
      if let Some(key) = scan_regions(h, &rw_regions, ciphertext)? {
        return Ok(Some(key));
      }

      if is_extract_cancelled() {
        tracing::info!("scan_aes_key_from_memory cancelled");
        return Ok(None);
      }

      // Phase 2: 扫描其余可读区域
      tracing::info!("Phase 2: scanning remaining memory regions");
      if let Some(key) = scan_regions(h, &other_regions, ciphertext)? {
        return Ok(Some(key));
      }

      tracing::warn!("AES image key not found in process memory");
      Ok(None)
    }
  }

  #[cfg(not(target_os = "windows"))]
  fn scan_aes_key_from_memory(_pid: u32, _ciphertext: &[u8; 16]) -> Result<Option<String>> {
    Ok(None)
  }
}
