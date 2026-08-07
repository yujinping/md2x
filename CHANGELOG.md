## [unreleased]

### ⚙️ Miscellaneous Tasks

- *(ci)* 更新CLI产物名称从md2x-cli为md2x
## [0.1.6] - 2026-08-06

### 🚜 Refactor

- *(ci)* 将macOS通用构建拆分为独立步骤

### 📚 Documentation

- *(README)* 添加系统要求说明与macOS最低版本配置

### ⚙️ Miscellaneous Tasks

- Release v0.1.6
## [0.1.5] - 2026-08-06

### 🚀 Features

- *(ci)* 支持macOS universal构建并移除AppImage和MSI

### 🚜 Refactor

- *(release)* 结构化改造，自动检测仓库/版本文件/分支，支持 --install 全局安装

### 📚 Documentation

- Release 脚本结构化设计规格
- 规格补充全局安装与 cwd 检测设计
- Release 脚本结构化实现计划

### 🎨 Styling

- *(release)* 统一变量引用使用大括号形式

### ⚙️ Miscellaneous Tasks

- *(ci)* 移除主干推送触发，仅保留标签触发
- Release v0.1.5
## [0.1.3] - 2026-08-06

### 🚀 Features

- *(release)* Auto-generate RELEASE_NOTES.md from changelog

### ⚙️ Miscellaneous Tasks

- Release v0.1.3
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
