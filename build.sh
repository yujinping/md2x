#!/usr/bin/env bash
#
# build.sh — md2x 构建/运行脚本
#
# 用法:
#   ./build.sh run      构建前端 + 后端，并启动 GUI（默认）
#   ./build.sh build    仅构建前端 + 后端，不启动
#   ./build.sh          等同于 ./build.sh run
#
set -euo pipefail

# 定位脚本所在目录（项目根）
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

FRONTEND_DIR="$SCRIPT_DIR/frontend"

command -v pnpm >/dev/null 2>&1 || { echo "✗ 未找到 pnpm，请先安装 (pnpm@11.x)"; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "✗ 未找到 cargo，请先安装 Rust 工具链"; exit 1; }

build_frontend() {
  echo "==> [1/2] 构建前端 (pnpm build) ..."
  (cd "$FRONTEND_DIR" && pnpm build)
  echo "    ✓ 前端构建完成 -> frontend/dist"
}

build_backend() {
  echo "==> [2/2] 构建后端 GUI (cargo build -p md2x-gui) ..."
  cargo build -p md2x-gui
  echo "    ✓ 后端构建完成"
}

cmd="${1:-run}"
case "$cmd" in
  run)
    build_frontend
    build_backend
    echo "==> 启动 GUI ..."
    exec cargo run -p md2x-gui
    ;;
  build)
    build_frontend
    build_backend
    echo "==> 构建全部完成。运行 ./build.sh run 启动 GUI。"
    ;;
  *)
    echo "用法: $0 {run|build}"
    exit 1
    ;;
esac
