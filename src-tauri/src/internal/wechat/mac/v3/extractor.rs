use anyhow::{Result, anyhow};
use crate::internal::wechat::common::types::WechatKeys;
use crate::internal::wechat::common::extractor_trait::KeyExtractor;

pub struct MacV3Extractor;

impl KeyExtractor for MacV3Extractor {
  fn detect(&self) -> Result<bool> { Ok(false) } // 占位
  fn extract_db_keys(&self, data_dir: Option<&str>) -> Result<WechatKeys> { Err(anyhow!("mac v3 数据库密钥提取未实现")) }
  fn extract_img_keys(&self, data_dir: Option<&str>) -> Result<WechatKeys> { Err(anyhow!("mac v3 图片密钥提取未实现")) }
}
