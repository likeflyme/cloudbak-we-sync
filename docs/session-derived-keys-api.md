# 会话派生密钥接口

本文档描述微信 v4 会话派生密钥的读取与增量追加接口。派生密钥用于 `wx_key` 为 `v2` 的会话按数据库文件 salt 解密新增 db 文件。

## 使用流程

1. 客户端调用读取接口，获取服务端已保存的派生密钥列表。
2. 客户端在本地获取当前微信 db 文件对应的派生密钥。
3. 客户端自行比对，把服务端缺少的派生密钥提交给追加接口。
4. 服务端保存新增派生密钥并刷新该会话客户端缓存。
5. 如需解析新增 db 文件，客户端再触发对应解析或重新解析流程。

派生密钥格式固定为 96 位十六进制字符串：前 64 位是派生文件密钥，后 32 位是数据库文件 salt。

## 鉴权

两个接口都需要登录态 Bearer Token：

```http
Authorization: Bearer <access_token>
```

接口按 `session_id` 校验会话归属，只允许读取或修改当前登录用户拥有的会话。

## 读取派生密钥

```http
GET /api/sys-session/{session_id}/derived-keys
```

### 响应示例

```json
{
  "session_id": 12,
  "keys": [
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  ],
  "total_count": 1
}
```

## 追加派生密钥

```http
POST /api/sys-session/derived-keys
Content-Type: application/json
```

### 请求体

```json
{
  "session_id": 12,
  "keys": [
    "ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccdddddddddddddddddddddddddddddddd"
  ]
}
```

服务端会对每个 key 去首尾空白并转为小写。已存在的 key 不会重复写入，会返回到 `skipped_keys`。

### 响应示例

```json
{
  "session_id": 12,
  "keys": [
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccdddddddddddddddddddddddddddddddd"
  ],
  "added_keys": [
    "ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccdddddddddddddddddddddddddddddddd"
  ],
  "skipped_keys": [],
  "added_count": 1,
  "skipped_count": 0,
  "total_count": 2
}
```

## 错误说明

- `401/403`：未登录或 Token 无效。
- `422`：请求体格式错误，或派生密钥不是 96 位十六进制字符串。
- `470`：会话不存在，或该会话不属于当前登录用户。

