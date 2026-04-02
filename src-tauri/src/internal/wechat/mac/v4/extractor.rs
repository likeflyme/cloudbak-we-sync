use anyhow::{Result, anyhow};
use crate::internal::wechat::common::types::WechatKeys;
use crate::internal::wechat::common::extractor_trait::KeyExtractor;

pub struct MacV4Extractor;

impl KeyExtractor for MacV4Extractor {
  fn detect(&self) -> Result<bool> { Ok(false) } // 占位
  fn extract_db_keys(&self, data_dir: Option<&str>) -> Result<WechatKeys> { Err(anyhow!("mac v4 数据库密钥提取未实现")) }
  fn extract_img_keys(&self, data_dir: Option<&str>) -> Result<WechatKeys> { Err(anyhow!("mac v4 图片密钥提取未实现")) }
}
