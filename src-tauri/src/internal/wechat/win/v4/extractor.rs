use anyhow::{Result, anyhow};
use crate::internal::windows::{winproc, memory, dat2img};
use crate::internal::wechat::common::types::WechatKeys;
use crate::internal::wechat::common::extractor_trait::KeyExtractor;

pub struct WinV4Extractor;

impl KeyExtractor for WinV4Extractor {
  fn detect(&self) -> Result<bool> {
    // 只要找到 v4 进程即认为可用
    let procs = winproc::find_wechat_v4_processes()?;
    Ok(!procs.is_empty())
  }
  fn extract(&self) -> Result<WechatKeys> {
    let procs = winproc::find_wechat_v4_processes()?;
    if procs.is_empty() { return Err(anyhow!("未找到微信 v4 进程")); }
    // 选在线 + 有 data_dir 的，否则取第一个并尝试补 data_dir
    let selected = procs.iter().find(|p| p.status == "online" && p.data_dir.is_some()).cloned()
      .unwrap_or_else(|| procs.into_iter().next().unwrap());
    let pid = selected.pid as u32;
    let account_name = selected.account_name.clone();
    let data_dir = selected.data_dir.clone();
    let full_version = selected.full_version.clone().unwrap_or_default();
    let client_version = full_version.split('.').next().map(|maj| if maj == "4" { "v4" } else { "unknown" }).unwrap_or("unknown").to_string();
    let (data_key_hex, img_key_hex) = memory::extract_keys_windows(&selected)?;
    let xor = match &data_dir { Some(d) => dat2img::scan_and_set_xor_key(d).ok().flatten().map(|v| v.to_string()), None => None };
    // Heuristic avatar extraction (contact image) from data_dir if present
    let mut avatar_base64: Option<String> = None;
    if let (Some(ref dir), Some(ref acct)) = (&data_dir, &account_name) {
      use std::path::PathBuf; use std::io::Read; use base64::Engine;
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
      data_key: data_key_hex,
      image_key: img_key_hex,
      xor_key: xor,
      client_type: "win".into(),
      client_version,
      account_name,
      data_dir,
      method: Some("primary".into()),
      pid: Some(pid),
      avatar_base64,
    })
  }
}
