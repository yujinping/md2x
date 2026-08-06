## [unreleased]

### 🚀 Features

- *(release)* Auto-generate RELEASE_NOTES.md from changelog
## [0.1.2] - 2026-08-06

### ⚙️ Miscellaneous Tasks

- *(release)* Use gh CLI to create release and upload only installer artifacts
- Release v0.1.2
## [0.1.1] - 2026-08-06

### 🚀 Features

- 初始化 md2x workspace
- *(cli)* 增加 --format 参数支持 HTML 和 PNG 输出

### 🐛 Bug Fixes

- 修复模板 include_str 相对路径，适配独立仓库结构
- 取消忽略 templates 目录，修复 CI 缺少 mpe.html 模板
- 移除 *.html 忽略规则，提交 vite 入口 index.html
- *(release)* Include crate manifests in release commit and fix tag variable expansion

### 🚜 Refactor

- *(*)* 重命名项目为 md2x

### ⚙️ Miscellaneous Tasks

- 推送 main 时自动构建三平台产物
- Release v0.1.1
- Sync crate versions to 0.1.1
- Release v0.1.1
