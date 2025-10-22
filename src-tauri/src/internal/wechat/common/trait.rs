use anyhow::Result;
use super::types::WechatKeys;

pub trait extractor_trait {
  fn detect(&self) -> Result<bool>;      // 是否适用当前环境
  fn extract(&self) -> Result<WechatKeys>; // 提取
}
