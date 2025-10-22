pub mod common;
pub mod win;
pub mod mac;

use anyhow::{Result, anyhow};
use common::types::WechatKeys;
use crate::internal::wechat::common::extractor_trait::KeyExtractor;
use win::v4::extractor::WinV4Extractor;
use win::v3::extractor::WinV3Extractor;
use mac::v4::extractor::MacV4Extractor;
use mac::v3::extractor::MacV3Extractor;

#[derive(Debug, Clone)]
pub enum WxPlatform { Win, Mac }

#[derive(Debug, Clone)]
pub enum WxMajor { V4, V3, Unknown }

fn detect_platform() -> WxPlatform { if cfg!(target_os = "macos") { WxPlatform::Mac } else { WxPlatform::Win } }

fn choose_extractors(p: &WxPlatform) -> Vec<Box<dyn KeyExtractor>> {
  match p {
    WxPlatform::Win => vec![Box::new(WinV4Extractor), Box::new(WinV3Extractor)],
    WxPlatform::Mac => vec![Box::new(MacV4Extractor), Box::new(MacV3Extractor)],
  }
}

pub fn extract_any() -> Result<WechatKeys> {
  let plat = detect_platform();
  for ext in choose_extractors(&plat) {
    if ext.detect()? {
      match ext.extract() {
        Ok(k) => return Ok(k),
        Err(e) => { tracing::warn!(error = %e, "extractor failed, try next"); }
      }
    }
  }
  Err(anyhow!("未找到可用的微信进程或提取器未实现"))
}
