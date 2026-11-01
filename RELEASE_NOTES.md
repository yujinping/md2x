# v1.6.0 发布说明

本版是底层升级与清理：Markdown 引擎 comrak 升至 0.59 并启用中文强调修复，bundle identifier 摆脱早期遗留包名。

## 🚀 Features

- **(core)** comrak 0.28 → 0.59，启用 CJK Friendly Emphasis：修复全角标点紧邻时「**粗体**」不生效的问题（HTML 与 DOCX 导出同步生效）；解析性能提升约 1.32 倍，并带入脚注递归 DoS 安全修复

## 🔧 Changed

- **bundle identifier** 由 `com.mpe.md2x` 改为 `top.yujinping.md2x`（项目早期遗留包名的清理）
  - 注意：应用内设置（主题/语言/编辑器模式）会因 WebView 存储分区变更重置一次
  - 旧的 `com.mpe.md2x` 数据目录成为死数据，可安全删除

## 📦 Dependencies

- Rust 侧兼容升级：tauri 2.12.5、tauri-build 2.7.4、tauri-plugin-dialog 2.8.4、tokio 1.54.1 等（172 个 crate 刷新）
- 前端兼容升级：vue 3.5.44、pinia 4.1.0、@tauri-apps/api 2.12.4 等
- vite 锁 `~8.1.5`、esbuild 锁 `~0.28.1`：esbuild 0.28.2 会导致 vitest 5.0.5 worker 启动超时，待上游修复后放开

## 🛠 Internal

- 发版脚本修复：CHANGELOG 改为打 tag 后生成，变更正确归入版本段落，不再需要手工补提交
