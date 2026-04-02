use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WechatKeys {
  pub ok: bool,
  pub data_key: Option<String>,
  pub db_keys: Vec<String>,       // 数据库密钥列表（从进程内存扫描 x'...' 模式得到）
  pub image_key: Option<String>,
  pub xor_key: Option<String>,
  pub client_type: String,      // win | mac
  pub client_version: String,   // v3 | v4 | unknown
  pub account_name: Option<String>,
  pub data_dir: Option<String>,
  pub method: Option<String>,   // primary | fallback
  pub pid: Option<u32>,
  pub avatar_base64: Option<String>, // added: inline avatar data URL/base64
}

impl WechatKeys {
  pub fn fail(msg: &str) -> serde_json::Value { serde_json::json!({"ok": false, "error": msg }) }
  pub fn to_json(&self) -> serde_json::Value {
    serde_json::json!({
      "ok": self.ok,
      "dataKey": self.data_key,
      "dbKeys": self.db_keys,
      "imageKey": self.image_key,
      "xorKey": self.xor_key,
      "clientType": self.client_type,
      "clientVersion": self.client_version,
      "accountName": self.account_name,
      "dataDir": self.data_dir,
      "method": self.method,
      "pid": self.pid,
      "avatar": self.avatar_base64,
    })
  }
}
