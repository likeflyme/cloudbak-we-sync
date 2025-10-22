use anyhow::Result;
use super::types::WechatKeys;

pub trait KeyExtractor {
  fn detect(&self) -> Result<bool>;
  fn extract(&self) -> Result<WechatKeys>;
}
