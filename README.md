# md2x

md2x 是一个将 Markdown 文件转换为精美 PDF 的工具，同时提供命令行（CLI）和桌面应用（GUI）两种使用方式。

## 项目结构

```
md2pdf-project/
├── Cargo.toml                 # 工作区根配置
├── crates/
│   ├── md2pdf-core/           # 核心库：Markdown→HTML→PDF 转换引擎
│   ├── md2pdf-cli/            # CLI 二进制入口
│   └── md2pdf-gui/            # Tauri 2 桌面应用后端
├── frontend/                  # Vue 3 + Pinia + Tailwind 前端
├── templates/                 # HTML 模板和前端资源（CSS/JS）
├── docs/                      # 文档
└── tests/                     # 集成测试
```

## 快速开始

### CLI 模式

```bash
cargo run -p md2pdf-cli -- README.md          # 生成 README.pdf
cargo run -p md2pdf-cli -- README.md --preview # 生成并打开预览
```

### GUI 模式

```bash
cd frontend && npm install && npm run build
cd .. && cargo run -p md2pdf-gui
```

## 工作流

```
Markdown → comrak → HTML → 模板渲染 → Chrome headless → PDF
```

## 许可证

MIT
