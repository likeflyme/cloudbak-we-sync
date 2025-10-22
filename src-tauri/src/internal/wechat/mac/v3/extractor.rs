use anyhow::{Result, anyhow};
use crate::internal::wechat::common::types::WechatKeys;
use crate::internal::wechat::common::extractor_trait::KeyExtractor;

pub struct MacV3Extractor;

impl KeyExtractor for MacV3Extractor {
  fn detect(&self) -> Result<bool> { Ok(false) } // 占位
  fn extract(&self) -> Result<WechatKeys> { Err(anyhow!("mac v3 提取未实现")) }
}
