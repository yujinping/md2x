# 开发指南

## 环境要求

- **Rust** 1.75+ (edition 2021)
- **Node.js** 18+ (构建前端)
- **Chrome/Chromium/Edge** (PDF 生成引擎)
- **Tauri 系统依赖**：macOS 需 Xcode CLI Tools；Linux 需 `libwebkit2gtk-4.1-dev` 等

## 开发命令

### 编译全部

```bash
cargo build --workspace
```

### 运行 CLI

```bash
cargo run -p md2x-cli -- path/to/file.md
cargo run -p md2x-cli -- path/to/file.md --preview
```

### 运行 GUI（需要先构建前端）

```bash
# 终端 1：前端开发服务器
cd frontend && npm install && npm run dev

# 终端 2：Tauri 开发模式
cargo tauri dev --manifest-path crates/md2x-gui/Cargo.toml
```

### 运行测试

```bash
# 核心库单元测试
cargo test -p md2x-core

# 全部测试
cargo test --workspace
```

### 构建发布

```bash
# CLI
cargo build -p md2x-cli --release
# 产物: target/release/md2x

# GUI (Tauri 打包)
cargo tauri build --manifest-path crates/md2x-gui/Cargo.toml
# 产物: crates/md2x-gui/target/release/bundle/
```

## 前端开发

前端使用 Vue 3 + Pinia + Tailwind CSS v4 + Vite 8。

```bash
cd frontend
npm install        # 首次
npm run dev        # 开发服务器 (端口 1420)
npm run build      # 构建到 dist/
```

## 版本号管理

版本号在以下文件中同步维护：
- `crates/md2x-core/Cargo.toml`
- `crates/md2x-cli/Cargo.toml`
- `crates/md2x-gui/Cargo.toml`
- `crates/md2x-gui/tauri.conf.json`

可使用原有 `bump.js` 脚本（需复制到新位置后调整路径）。

## 代码风格

- Rust 使用 `cargo fmt` 格式化
- 测试使用 `#[cfg(test)]` 内联在源文件中
- 错误处理使用统一的 `MpeError` 类型
