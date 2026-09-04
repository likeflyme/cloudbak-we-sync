# AGENTS.md

本文件用于指导 Codex 或其他自动化编码代理在本仓库中工作。除非用户明确给出更具体的指令，否则优先遵守这里的约定。

## 项目概览

`we-sync` 是一个基于 Tauri 2 + Vue 3 + TypeScript 的桌面应用，核心功能围绕微信数据目录检测、密钥提取、会话配置、文件同步、自动同步和应用更新。

主要代码位置：

- `src/`：前端 Vue 应用。
- `src/api/`：前端请求封装。
- `src/components/`：通用和会话相关组件。
- `src/views/`：页面级 Vue 组件。
- `src/router/`：前端路由。
- `src-tauri/src/`：Tauri/Rust 后端。
- `src-tauri/src/commands/`：暴露给前端的 Tauri commands。
- `src-tauri/src/internal/wechat/`：微信数据提取逻辑，按平台和微信版本拆分。
- `src-tauri/src/internal/windows/`：Windows 专用内存扫描、数据库解密、头像和图片处理逻辑。
- `src-tauri/tauri.conf.json`：Tauri 构建、窗口、安全范围、更新器配置。

## 常用命令

在仓库根目录运行：

```powershell
npm run dev
npm run build
npm run tauri dev
npm run tauri build
```

Rust 检查通常在 `src-tauri` 下运行：

```powershell
cd src-tauri
cargo check
cargo test
```

修改前端后至少运行：

```powershell
npm run build
```

修改 Rust/Tauri 后至少运行：

```powershell
cd src-tauri
cargo check
```

涉及微信提取、同步、自动同步、文件上传、平台条件编译或 Tauri command 签名时，优先同时运行前端构建和 Rust 检查。

## 开发约定

- 前端使用 Vue 3 `<script setup>`、TypeScript 和 Vite，保持现有组件拆分风格。
- 新增 Tauri command 时，需要在 `src-tauri/src/commands/` 中实现，并确保 `commands::all_handlers()` 已注册。
- 前端调用 Tauri command 时，优先复用 `src/models/` 中的类型；缺类型时补充类型，不要把复杂对象长期保留为 `any`。
- 不要把微信目录、数据库密钥、token、用户路径、上传目标等敏感值硬编码进源码或日志。
- 日志使用 Rust 侧现有 `tracing` 体系，避免在高频扫描、上传循环里输出过量 `info` 日志；高频细节优先用 `debug` 或 `trace`。
- 跨平台代码必须保留 `#[cfg(target_os = "...")]` 边界。Windows 专用逻辑不要泄漏到 macOS 通用路径。
- 处理文件路径时使用 `Path` / `PathBuf` 和现有路径工具，避免手拼分隔符；展示或上传相对路径时再统一转换为 `/`。
- 同步逻辑涉及本地文件、远端索引、分块上传、mtime 精度容忍和取消状态，修改时要保持幂等和可恢复，不要只验证成功路径。
- 自动同步 watcher 必须可停止、避免重复启动，并清理对应的认证缓存或状态。

## 数据和安全边界

本项目会读取本机微信数据目录，并可能处理数据库密钥、头像、图片、聊天数据库和同步 token。编码代理在操作时必须遵守：

- 不要上传、复制、打印或提交真实微信数据库、聊天记录、密钥、token、用户目录截图或日志。
- 不要把 `chatlog/`、`target/`、`dist/`、`node_modules/` 中的生成物作为源码依据提交，除非用户明确要求处理这些产物。
- 排查问题时优先使用脱敏路径和错误摘要；需要引用日志时只摘录与故障定位直接相关的短片段。
- 修改 `src-tauri/tauri.conf.json` 的 `security.assetProtocol.scope`、更新器 endpoint、公钥或 bundle 配置时，要明确说明影响范围。

## 验证重点

不同改动的最低验证建议：

- UI/路由/类型改动：`npm run build`。
- Tauri command、Rust 类型、同步逻辑改动：`cargo check`。
- 微信平台提取逻辑改动：在对应平台验证 `detect_data_dirs`、`extract_wechat_db_keys`、`extract_wechat_img_keys` 的错误和成功返回结构。
- 上传/同步改动：验证小文件直接上传、大文件分块上传、过滤规则、取消任务和自动同步状态。
- 配置持久化改动：验证 `plugin-store` 和 `app_data_dir()/users/{user_id}/sessions/{session_id}.json` 的读写兼容。

## Git 和编辑规则

- 工作区可能已有用户改动。不要还原、覆盖或格式化无关文件。
- 优先做小范围补丁，避免顺手重构同步、密钥提取和上传流程。
- 不要提交生成目录：`dist/`、`target/`、`node_modules/`。
- 修改依赖时同步维护 `package-lock.json` 或 `Cargo.lock`，并说明为什么需要变更。
- 如果发现源码中已有乱码注释或历史遗留问题，除非它直接影响当前任务，不要大面积重写。

## 发布和版本

版本号同时存在于 `package.json`、`src-tauri/Cargo.toml` 和 `src-tauri/tauri.conf.json`。调整版本时保持三处一致，并确认 Tauri updater 产物仍匹配发布流程。

