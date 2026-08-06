## [unreleased]

### 🚀 Features

- 初始化 md2x workspace

### 🐛 Bug Fixes

- 修复模板 include_str 相对路径，适配独立仓库结构
- 取消忽略 templates 目录，修复 CI 缺少 mpe.html 模板
- 移除 *.html 忽略规则，提交 vite 入口 index.html

### 🚜 Refactor

- *(*)* 重命名项目为 md2x

### ⚙️ Miscellaneous Tasks

- 推送 main 时自动构建三平台产物
