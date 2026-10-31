## [1.4.2] - 2026-10-31

### 🐛 Bug Fixes

- *(dbml)* 大规模 schema 下画布不再退化成顶部梳状线团，59 表样本宽高比由 4.78:1 回到 1.47:1
- *(dbml)* 跨列连线优先穿中间列水平空档，总线边数由 52 降到 1
- *(dbml)* 空档穿越的绝对 y 随总线区平移，修复 49 条连线穿表
- *(dbml)* 无外键的孤立表独立成块，不再把层 0 撑高近三倍

### ⚙️ Miscellaneous Tasks

- 版本升至 1.4.2

## [1.4.1] - 2026-10-31

### 🐛 Bug Fixes

- *(dbml)* 画布连线改为正交走廊路由，不再穿过表节点
- *(dbml)* 同列两张表的外键不再横穿源表自身

### 🚀 Features

- *(dbml)* 画布表头显示表注释，按显示宽度截断为 `...`

### ⚙️ Miscellaneous Tasks

- 版本升至 1.4.1

## [1.4.0] - 2026-10-31

### 🚀 Features

- *(dbml)* 新增 DBML 解析与渲染，产出数据字典与可缩放画布
- *(dbml)* 画布为字段标注 PK / FK / UQ 徽章
- *(core)* 类型映射抽象层，按扩展名分派渲染器
- *(gui)* 文件树按类型过滤、按路径导出，dbml 登记为可打开类型
- *(ui)* 设置新增「渲染类型」选项卡，文件树区分 dbml 与右键导出
- *(cli)* 接入类型分派，dbml 不再被当作 Markdown 渲染

### 🐛 Bug Fixes

- *(dbml)* 多行 note 去掉公共缩进，DOCX 里不再原样输出星号与反引号
- *(dbml)* 数据字典表格不再被模版通用样式压扁
- *(dbml)* note 标题不再露出裸井号
- *(dbml)* TableGroup 分组在表定义之后声明时不再静默失效

### 📚 Documentation

- *(dbml)* 补充 DBML 章节与画布徽章说明

### 🧪 Testing

- *(dbml)* 39 个回归测试，每个对应一类真实踩过的坑
- *(gui)* DBML 渲染链路端到端覆盖

### ⚙️ Miscellaneous Tasks

- 版本升至 1.4.0

## [1.3.0] - 2026-10-30

### 🚀 Features

- *(export)* [**breaking**] 支持自定义导出顺序与排除项
- *(template)* 四套模版补上未打包文件的提示样式
- *(gui)* 文件树支持拖动排序与右键排除

### 📚 Documentation

- 将 v1.2.0 变更归入版本段落

### 🧪 Testing

- *(cdp)* 测试函数名改用 snake_case，clippy -D warnings 通过

### ⚙️ Miscellaneous Tasks

- Release v1.3.0
## [1.2.0] - 2026-10-30

### 🚀 Features

- *(template)* 模版包约定与设置面板卡片式模版选择
- *(template)* 新增两套自带模版，播种改为逐个模版幂等写入
- *(template)* 模版可调变量（配色 / 字号 / 字体），设置面板直接改
- *(export)* 导出预设，常用「格式 + 模版 + 主题 + 全宽」一键触发

### 🐛 Bug Fixes

- *(template)* 章节目录去掉全部展开/全部收起

### 📚 Documentation

- 将 v1.1.2 变更归入版本段落

### ⚙️ Miscellaneous Tasks

- Release v1.2.0
## [1.1.2] - 2026-10-29

### 🚀 Features

- *(gui)* 文件树右键支持复制仓库相对路径
- *(ui)* 设置对话框选项卡化、预览右键菜单与显示偏好持久化
- *(gui)* 菜单重构、文件菜单导出子菜单与窗口标题

### 🐛 Bug Fixes

- *(core)* 跨平台二进制查找与 file URL 处理
- *(template)* 全屏时侧栏与背景不再被裁到一个视口高
- *(release)* 版本文件探测不再因文件缺失让脚本静默退出

### 📚 Documentation

- 将 v1.0.0 变更归入版本段落

### ⚙️ Miscellaneous Tasks

- Upgrade Node.js version from 22 to 24
- *(release)* 发版脚本同步前端 package.json 版本号
- Release v1.1.2
## [1.0.0] - 2026-10-29

### 🚀 Features

- *(gui)* 渲染视图页内查找，⌘/Ctrl+F 高亮跳转
- *(template)* 可选 HTML 模板，预览与导出共用，支持外置模板
- *(mermaid)* 图表布局引擎切换为 ELK，分层布局减少边交叉
- *(diagram)* D2 / graphviz 外部 CLI 图表渲染
- *(gui)* 设置面板展示外部图表工具安装状态

### ⚡ Performance

- *(mermaid)* 内嵌资源 gzip 压缩，二进制瘦身 6.2MB

### 🎨 Styling

- 清理 clippy 告警（div_ceil、strip_prefix、冗余 clone 等）

### ⚙️ Miscellaneous Tasks

- CHANGELOG 版本段落间补空行
- Release v1.0.0
## [0.8.0] - 2026-10-06

### 🚀 Features

- *(aggregate)* 目录聚合导出，跨文档锚点与链接重写
- *(theme)* 导出与预览统一支持浅色/暗色双主题
- *(export)* PDF 书签树与 DOCX 目录域
- *(cli,gui)* 目录聚合导出支持 PDF/DOCX 格式
- *(ui)* 设置面板接入主题预览联动与无 JS 降级

### 📚 Documentation

- 将 0.7.0 变更归入版本段落
- 补充无 JS 降级说明

### ⚙️ Miscellaneous Tasks

- Release v0.8.0
## [0.7.0] - 2026-10-06

### 🚀 Features

- 目录支持层级折叠，导出 HTML 增加全屏阅读

### ⚙️ Miscellaneous Tasks

- Release v0.7.0
## [0.6.1] - 2026-09-09

### ⚙️ Miscellaneous Tasks

- *(md2x-gui)* 更新应用图标资源
- Release v0.6.1
## [0.6.0] - 2026-09-09

### 🚀 Features

- *(gui)* 支持文件夹文件树预览与默认程序关联

### ⚙️ Miscellaneous Tasks

- Add LICENSE.
- Release v0.6.0
## [0.2.3] - 2026-08-24

### ⚙️ Miscellaneous Tasks

- *(ci)* 新增 macOS 单架构 dmg 构建并优化产物命名
- Release v0.2.3
## [0.2.1] - 2026-08-24

### ⚙️ Miscellaneous Tasks

- *(icons)* 更新应用图标
- Release v0.2.1
## [0.4.1] - 2026-08-13

### ⚙️ Miscellaneous Tasks

- Release v0.4.1
## [0.4.0] - 2026-08-13

### 🚀 Features

- *(render)* 支持 HTML 全宽显示并修复侧栏收起占位

### 🐛 Bug Fixes

- *(fullwidth)* 修复全宽切换无效并改为前端注入样式

### ⚙️ Miscellaneous Tasks

- Release v0.4.0
## [0.3.1] - 2026-08-12

### 🚀 Features

- *(frontend)* 新增前进后退导航并修复相对链接空白
- *(mermaid)* 将 Mermaid 图表渲染为内嵌 SVG

### 🐛 Bug Fixes

- 修复链接解析与内部跳转问题

### ⚙️ Miscellaneous Tasks

- *(ci)* 升级 Actions 依赖与 pnpm 版本
- *(release)* 升级版本至 0.3.0
- Release v0.3.1
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

### ⚙️ Miscellaneous Tasks

- *(ci)* 更新CLI产物名称从md2x-cli为md2x
- Release v0.1.7
## [0.1.6] - 2026-08-06

### 📚 Documentation

- *(README)* 添加系统要求说明与macOS最低版本配置

### 🚜 Refactor

- *(ci)* 将macOS通用构建拆分为独立步骤

### ⚙️ Miscellaneous Tasks

- Release v0.1.6
## [0.1.5] - 2026-08-06

### 🚀 Features

- *(ci)* 支持macOS universal构建并移除AppImage和MSI

### 📚 Documentation

- Release 脚本结构化设计规格
- 规格补充全局安装与 cwd 检测设计
- Release 脚本结构化实现计划

### 🚜 Refactor

- *(release)* 结构化改造，自动检测仓库/版本文件/分支，支持 --install 全局安装

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
