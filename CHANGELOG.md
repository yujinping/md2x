## [unreleased]

### 🚀 Features

- *(frontend)* 新增前进后退导航并修复相对链接空白
- *(mermaid)* 将 Mermaid 图表渲染为内嵌 SVG

### 🐛 Bug Fixes

- 修复链接解析与内部跳转问题

### ⚙️ Miscellaneous Tasks

- *(ci)* 升级 Actions 依赖与 pnpm 版本
- *(release)* 升级版本至 0.3.0
## [0.2.0] - 2026-08-08

### 🚀 Features

- *(core)* Docx 打包骨架（任务 1/13）
- *(core)* Docx 段落与行内文本渲染（任务 2/13）
- *(core)* Docx 标题样式与导航层级（任务 3/13）
- *(core)* Docx 列表与任务列表渲染（任务 4/13）
- *(core)* Docx 代码块底纹与 syntect 高亮（任务 5/13）
- *(core)* Docx 引用块与分割线（任务 6/13）
- *(core)* Docx 表格渲染（任务 7/13）
- *(core)* Docx 图片嵌入（任务 8/13）
- *(core)* Docx 入口集成与 frontmatter 处理（任务 9/13）
- *(cli)* 支持 --format docx（任务 10/13）
- *(gui)* 新增导出 html/pdf/docx 命令（任务 11/13）
- *(gui)* 前端导出菜单（HTML/PDF/DOCX）（任务 12/13）
- *(core)* Docx 字体按生成平台选择（macOS 苹方/Windows 雅黑/Linux Noto），观感对齐 HTML
- *(core)* 短代码块使用圆角文本框（圆角+内边距），长代码块保留段落底纹防跨页截断
- *(core)* 代码块改为无边框表格承载——底纹与页面左对齐，单元格边距提供四边内部内边距

### 🐛 Bug Fixes

- *(core)* 标题字号与行距对齐 HTML，支持 SVG 图片嵌入（修复 2 个反馈问题）
- *(core)* Hugo 静态图片路径（/images/xxx）解析与 HTML 一致（修复图片未嵌入）
- *(core)* 列表项/引用块内的图片正确嵌入（修复 docx 仍缺图）
- GIF 尺寸判断清理与文件分块读取越界保护

### 📚 Documentation

- 更新 README 支持 docx 输出（任务 13/13）

### ⚙️ Miscellaneous Tasks

- Release v0.2.0

### ◀️ Revert

- *(core)* 撤销代码块圆角文本框，保留段落底纹与文字内边距
## [0.1.7] - 2026-08-07

### 🚀 Features

- 初始化 md2x workspace
- *(cli)* 增加 --format 参数支持 HTML 和 PNG 输出
- *(release)* Auto-generate RELEASE_NOTES.md from changelog
- *(ci)* 支持macOS universal构建并移除AppImage和MSI

### 🐛 Bug Fixes

- 修复模板 include_str 相对路径，适配独立仓库结构
- 取消忽略 templates 目录，修复 CI 缺少 mpe.html 模板
- 移除 *.html 忽略规则，提交 vite 入口 index.html
- *(release)* Include crate manifests in release commit and fix tag variable expansion

### 🚜 Refactor

- *(*)* 重命名项目为 md2x
- *(release)* 结构化改造，自动检测仓库/版本文件/分支，支持 --install 全局安装
- *(ci)* 将macOS通用构建拆分为独立步骤

### 📚 Documentation

- Release 脚本结构化设计规格
- 规格补充全局安装与 cwd 检测设计
- Release 脚本结构化实现计划
- *(README)* 添加系统要求说明与macOS最低版本配置

### 🎨 Styling

- *(release)* 统一变量引用使用大括号形式

### ⚙️ Miscellaneous Tasks

- 推送 main 时自动构建三平台产物
- Release v0.1.1
- Sync crate versions to 0.1.1
- Release v0.1.1
- *(release)* Use gh CLI to create release and upload only installer artifacts
- Release v0.1.2
- Release v0.1.3
- *(ci)* 移除主干推送触发，仅保留标签触发
- Release v0.1.5
- Release v0.1.6
- *(ci)* 更新CLI产物名称从md2x-cli为md2x
- Release v0.1.7
