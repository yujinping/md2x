# 架构说明

## 概述

md2x 采用三层的 Rust workspace 架构，核心逻辑与界面层完全分离。

```
┌──────────────────────────────────────────────────┐
│                  用户界面层                       │
│  ┌─────────────┐         ┌───────────────────┐   │
│  │ md2x-cli  │         │  md2x-gui       │   │
│  │ (CLAP 解析)  │         │  (Tauri 2 + Vue)  │   │
│  └──────┬──────┘         └────────┬──────────┘   │
│         │                         │              │
└─────────┼─────────────────────────┼──────────────┘
          │        依赖              │
          ▼                         ▼
┌───────────────────────────────────────────────┐
│             核心引擎层                          │
│         md2x-core (lib crate)               │
│                                               │
│  ┌──────────┐  ┌──────────┐  ┌─────────────┐  │
│  │converter │  │ template │  │   chrome    │  │
│  │comrak→HTML│  │ 渲染模板  │  │Chrome headless│  │
│  │图片嵌入   │  │ TOC 生成  │  │  PDF 生成   │  │
│  │Frontmatter│  │ 元数据卡片 │  │  系统打开   │  │
│  └──────────┘  └──────────┘  └─────────────┘  │
│                                               │
│  ┌─────────────────────────────────────────┐   │
│  │              error (MpeError)           │   │
│  └─────────────────────────────────────────┘   │
└───────────────────────────────────────────────┘
```

## Crate 依赖关系

- **md2x-core** — 零界面依赖，只依赖 `comrak`。提供 4 个 public module
- **md2x-cli** — 依赖 `md2x-core` + `clap`。纯 CLI 二进制
- **md2x-gui** — 依赖 `md2x-core` + `tauri 2` + `serde` + `tokio`。Tauri 桌面壳

## 核心模块职责

### converter
- `convert_markdown_to_html()` — comrak 转换 + tasklist `<p>` 清理
- `resolve_image_srcs()` — 图片 base64 嵌入（支持 Hugo 项目结构）
- `parse_front_matter()` / `strip_front_matter()` — YAML 元数据解析

### template
- `render_html_template_with_metadata()` — 注入 CSS/JS/TOC/元数据卡片
- `generate_toc()` — 从 HTML heading 自动生成带锚点的目录
- 模板资源通过 `include_str!` 编译期嵌入

### chrome
- `find_chrome()` — 跨平台查找 Chrome/Chromium/Edge
- `generate_pdf()` — 无头模式调用 `--print-to-pdf`
- `open_pdf()` — 系统默认打开

### error
- `MpeError` — 4 个变体，实现 `Display` + `Error` + `From<io::Error>`

## 模板资源路径

核心库中的 `template.rs` 使用 `include_str!` 编译期嵌入模板文件：

```
crates/md2x-core/src/template.rs
  → ../../../templates/mpe.html
  → ../../../templates/assets/*.css
  → ../../../templates/assets/*.js
```

## 双模式入口 (GUI)

`md2x-gui` 的 `main.rs` 支持三种启动方式：
1. `--pdf <file>` → CLI 模式，静默生成 PDF
2. `./md2x <file>` → GUI 模式，自动打开文件
3. 无参数 → 纯 GUI 模式
