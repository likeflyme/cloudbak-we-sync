use anyhow::Result;
use super::types::WechatKeys;

pub trait KeyExtractor {
  fn detect(&self) -> Result<bool>;
  /// 提取数据库密钥（data_key）以及基础信息（pid, account_name, data_dir, client_type/version, avatar 等）
  fn extract_db_keys(&self, data_dir: Option<&str>) -> Result<WechatKeys>;
  /// 提取图片密钥（image_key, xor_key）以及基础信息
  fn extract_img_keys(&self, data_dir: Option<&str>) -> Result<WechatKeys>;
  /// 检测微信数据目录，可能返回多个或零个
  fn detect_data_dirs(&self) -> Result<Vec<String>> { Ok(vec![]) }
}
