# md2x

md2x 是一个将 Markdown 文件转换为精美文档的工具，支持 PDF、HTML、PNG、DOCX 等输出格式，同时提供命令行（CLI）和桌面应用（GUI）两种使用方式。输出格式采用可扩展架构，新增格式只需扩展格式枚举与转换分支。

## 项目结构

```
md2x-project/
├── Cargo.toml                 # 工作区根配置
├── crates/
│   ├── md2x-core/           # 核心库：Markdown→HTML→PDF 转换引擎
│   ├── md2x-cli/            # CLI 二进制入口
│   └── md2x-gui/            # Tauri 2 桌面应用后端
├── frontend/                  # Vue 3 + Pinia + Tailwind 前端
├── templates/                 # 内置 HTML 模板、示例模板包与前端资源（CSS/JS）
└── docs/                      # 文档
```

## 系统要求

- **macOS**：macOS 11.0（Big Sur）或更高版本；安装包为 Universal 格式，Intel 与 Apple Silicon 通用
- **Windows**：Windows 10 或更高版本（NSIS 安装包）
- **Linux**：基于 deb 的发行版（如 Ubuntu 22.04+），安装时需自动拉取 WebKitGTK 4.1 等依赖

> 提示：PDF / PNG 输出依赖本机已安装的 Chrome、Chromium 或 Edge 浏览器。

## 快速开始

### CLI 模式

```bash
cargo run -p md2x-cli -- README.md                # 默认生成 README.pdf
cargo run -p md2x-cli -- README.md --format html  # 生成 README.html
cargo run -p md2x-cli -- README.md --format png   # 生成 README.png（1920x1080 截图）
cargo run -p md2x-cli -- README.md --format docx  # 生成 README.docx（可编辑 Word 文档，不依赖 Office）
cargo run -p md2x-cli -- README.md --preview      # 生成 PDF 并用默认应用打开
cargo run -p md2x-cli -- README.md --format html --template my-theme/  # 指定模板包目录
```

### 全局安装（推荐）

首次安装：在 md2x 仓库根目录执行 `./release.sh --install`，会在 `~/.local/bin/` 创建 `release` 命令的符号链接（请确保 `~/.local/bin` 已加入 PATH）。

之后可在任意 Rust 项目根目录直接使用：

```bash
release v0.1.2              # 发版：bump 版本 -> 生成 CHANGELOG/RELEASE_NOTES -> 提交 -> tag -> 推送
release --dry-run v0.1.2    # 预览将执行的操作
```

脚本自动检测当前项目的远程仓库（GitHub HTTPS/SSH）、版本文件（cargo workspace / 单 crate / tauri.conf.json）与默认分支，零配置即可在任意 Rust 项目复用。

### GUI 模式

```bash
cd frontend && pnpm install && pnpm build
cd .. && cargo run -p md2x-gui
```

桌面应用支持：

- **单文件预览**：打开 / 拖入 `.md` 文件，HTML 即时预览，可导出 PDF、HTML、DOCX
- **文件夹文件树**：`打开文件夹`（或 `Cmd/Ctrl+Shift+O`）后，左侧显示该目录及其子目录内全部 `.md` 文件，支持展开 / 收起与关闭；可直接把文件夹拖入窗口打开
- **默认视图**：可在设置中配置启动视图（单文件视图 / 文件夹树视图 / 自动）
- **HTML 模板**：预览与导出使用同一套模板。模板以「目录包」形式组织，放入模板目录（设置 → 导出 → 打开模板目录）后自动扫描加载，卡片式选择并记住所选模版；CLI 用 `--template <目录|文件>` 指定
- **外部图表引擎**：```d2 / ```dot / ```graphviz 代码块由外部 CLI（[D2](https://d2lang.com) / [Graphviz](https://graphviz.org)）在转换期烘焙为内嵌 SVG，设置面板可查看安装状态，装上即用；未安装时保留代码块不报错
- **默认打开程序**：设置中一键把 `.md` 关联到 md2x（Windows 写注册表、macOS 写 LaunchServices、Linux 使用 xdg-mime），双击 Markdown 文件即可用 md2x 打开

### 自定义 HTML 模板

模板以**目录包**（与 AI Skill 类似：目录 + 固定入口文件 + 元数据 + 缩略图）为单位，
放在约定目录内即被自动扫描，无需注册：

```
<templates>/                    # 默认 <app_data_dir>/templates，可用 MD2X_TEMPLATE_DIR 覆盖
└── my-theme/                   # 目录名即模板 id
    ├── template.html           # 必需：入口页面，必须含 {{BODY}}
    ├── template.json           # 可选：元数据
    └── preview.png             # 可选：缩略图（显示在设置面板的模板卡片上）
```

`template.json` 的字段全部可省略，未知字段忽略：

```json
{
  "name": "论文体",
  "description": "学术排版，窄栏长文",
  "version": "1.0.0",
  "author": "your name",
  "preview": "preview.png"
}
```

缩略图按约定名顺序探测 `preview.png` / `preview.jpg` / `preview.jpeg` / `preview.webp` / `preview.svg`，
也可用 `preview` 字段指定任意相对路径；缺失时列表显示占位图，不影响使用。
旧版把 `<id>.html` 直接放在模板根目录的方式仍然可用（无元数据、无缩略图）。

占位符契约：

| 占位符 | 必需 | 说明 |
|---|---|---|
| `{{BODY}}` | ✅ | 正文落点，缺失则模板不被识别 |
| `{{TITLE}}` | | 文档标题 |
| `{{TOC}}` | | 侧栏目录（嵌套 `li`，需自行包 `ul`） |
| `{{PRINT_TOC}}` | | 打印目录页（仅 @media print 可见） |
| `{{DOC_TREE}}` | | 聚合导出的文档树 |
| `{{SKILL_METADATA}}` | | SKILL.md 元数据卡片 |
| `{{THEME_LOCK}}` | | 主题档位（auto/light/dark） |
| `{{WIDTH_VARS}}` | | 全宽模式 CSS |
| `{{GITHUB_MD_CSS}}` 等 | | 内置样式与脚本，按需取用 |

模板自备样式与交互脚本；引用外部 CSS/JS 会破坏自包含性，建议内联。
模板目录内没有任何模板时，应用会自动生成 `sample/` 示例包（`templates/sample/` 亦随仓库提供），
可直接参照它改写。首次打开模板目录还会生成一份说明 README。

## 工作流

```
Markdown → comrak → HTML → 模板渲染
  ├─ PDF：Chrome headless（--print-to-pdf）
  ├─ HTML：直接写出渲染结果
  ├─ PNG：Chrome headless 截图（--screenshot）
  └─ DOCX：comrak AST → OOXML（zip+XML），纯 Rust 生成，无需 Office/WPS

> mermaid 图表在转换期经 headless Chrome 一次性烘焙为内嵌 SVG，布局引擎默认
> ELK（分层布局，边交叉更少）；ELK 不可用时自动回落 dagre，单个图渲染失败
> 保留原始代码块，不中断导出。
>
> d2 / dot 图表依赖外部 CLI（d2、graphviz），在 PATH 中检出即自动启用；
> 未安装或渲染失败同样降级保留代码块。GUI 设置面板展示各 CLI 的安装状态。

> DOCX 输出的标题使用 Word 标题样式，打开后左侧导航窗格即可看到目录；正文排版
> （字体、字号、行距、表格、代码高亮、图片等）与 HTML 预览保持一致。
```

## 许可证

MIT
