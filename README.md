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
├── templates/                 # HTML 模板和前端资源（CSS/JS）
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

## 工作流

```
Markdown → comrak → HTML → 模板渲染
  ├─ PDF：Chrome headless（--print-to-pdf）
  ├─ HTML：直接写出渲染结果
  ├─ PNG：Chrome headless 截图（--screenshot）
  └─ DOCX：comrak AST → OOXML（zip+XML），纯 Rust 生成，无需 Office/WPS

> DOCX 输出的标题使用 Word 标题样式，打开后左侧导航窗格即可看到目录；正文排版
> （字体、字号、行距、表格、代码高亮、图片等）与 HTML 预览保持一致。
```

## 许可证

MIT
