#!/usr/bin/env bash
# md2x 一键发版脚本：bump 版本 -> 生成 CHANGELOG -> 提交 -> 打 tag -> 推送（触发 CI 构建并自动发布 Release）
# 用法：./release.sh [--dry-run] <版本号>     例：./release.sh 0.1.2 或 ./release.sh v0.1.2
set -euo pipefail

REPO_URL="https://github.com/yujinping/md2x"

DRY=0
if [ "${1:-}" = "--dry-run" ]; then
    DRY=1
    shift
fi

if [ $# -ne 1 ]; then
    echo "用法: ./release.sh [--dry-run] <版本号>"
    echo "  例: ./release.sh 0.1.2   或   ./release.sh v0.1.2"
    exit 1
fi

VER="${1#v}"
TAG="v$VER"

if ! echo "$VER" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "错误: 版本号格式应为 x.y.z，收到 '$VER'"
    exit 1
fi

# ---- 前置检查 ----
command -v git-cliff >/dev/null 2>&1 || { echo "错误: 缺少 git-cliff，请先安装（cargo install git-cliff）"; exit 1; }
[ "$(git branch --show-current)" = "main" ] || { echo "错误: 请先切换到 main 分支"; exit 1; }
# 允许 RELEASE_NOTES.md 保持未提交（发版说明随本次发布一起提交）
DIRTY="$(git status --porcelain | grep -v 'M RELEASE_NOTES.md' || true)"
if [ -n "$DIRTY" ]; then
    echo "错误: 工作区有未提交改动，请先提交或 stash："
    echo "$DIRTY"
    exit 1
fi
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    echo "错误: 本地 tag $TAG 已存在"
    exit 1
fi
if git ls-remote --exit-code --tags origin "$TAG" >/dev/null 2>&1; then
    echo "错误: 远程已存在 tag $TAG（可先 git push origin :$TAG 删除）"
    exit 1
fi

# RELEASE_NOTES.md 若仍是模板占位则提醒
if [ -f RELEASE_NOTES.md ] && grep -q '（示例：' RELEASE_NOTES.md; then
    echo "提醒: RELEASE_NOTES.md 仍是模板占位，建议先编辑「本版本变更」小节"
fi

echo "================================================"
echo "将发布 md2x $TAG"
echo "  1. bump workspace / crates / tauri.conf.json / Cargo.lock  ->  $VER"
echo "  2. git-cliff 生成 CHANGELOG.md"
echo "  3. 提交 'chore: release $TAG' 并打 tag"
echo "  4. push main 与 tag（触发 CI 构建，tag 自动发布 GitHub Release）"
echo "================================================"
if [ "$DRY" = "1" ]; then
    echo "[dry-run] 未执行任何修改，以上为将执行的操作"
    exit 0
fi
read -rp "确认继续? [y/N] " ans
case "$ans" in
    y | Y) ;;
    *) echo "已取消"; exit 1 ;;
esac

# ---- bump 版本（workspace、三个 crate、tauri.conf.json 与 Cargo.lock）----
python3 - "$VER" <<'PY'
import re, sys
from pathlib import Path

ver = sys.argv[1]

def bump(path, pattern, repl):
    """将文件中第一个匹配的版本号替换为新版本，找不到则报错"""
    p = Path(path)
    if not p.exists():
        print(f"错误: 缺少文件 {path}")
        return False
    new_s, n = pattern.subn(repl, p.read_text(), count=1)
    if n == 0:
        print(f"错误: {path} 中未找到版本号")
        return False
    p.write_text(new_s)
    print(f"bump {path} -> {ver}")
    return True

ok = True

# 根 workspace 与三个 crate 的版本号（每个文件只含一个 version 字段）
for path in (
    "Cargo.toml",
    "crates/md2x-core/Cargo.toml",
    "crates/md2x-cli/Cargo.toml",
    "crates/md2x-gui/Cargo.toml",
):
    ok &= bump(path, re.compile(r'^version = "[^"]+"', re.M), f'version = "{ver}"')

# Tauri 应用版本（GUI 打包产物的版本）
ok &= bump(
    "crates/md2x-gui/tauri.conf.json",
    re.compile(r'^  "version": "[^"]+"', re.M),
    f'  "version": "{ver}"',
)

# Cargo.lock：仅更新本仓库三个包，避免误伤同版本号的第三方依赖
lock_path = Path("Cargo.lock")
if not lock_path.exists():
    print("错误: 缺少文件 Cargo.lock")
    ok = False
else:
    lock = lock_path.read_text()
    for pkg in ("md2x-core", "md2x-cli", "md2x-gui"):
        lock, n = re.subn(
            rf'(name = "{pkg}"\nversion = ")[^"]+(")',
            rf"\g<1>{ver}\g<2>",
            lock,
            count=1,
        )
        if n == 0:
            print(f"错误: Cargo.lock 中未找到 {pkg} 包")
            ok = False
    if ok:
        lock_path.write_text(lock)
        print(f"bump Cargo.lock（md2x-core / md2x-cli / md2x-gui）-> {ver}")

sys.exit(0 if ok else 1)
PY

# ---- 生成 CHANGELOG ----
git-cliff -o CHANGELOG.md
echo "已生成 CHANGELOG.md"

# ---- 提交 / 打 tag / 推送 ----
git add Cargo.toml Cargo.lock CHANGELOG.md \
    crates/md2x-core/Cargo.toml crates/md2x-cli/Cargo.toml crates/md2x-gui/Cargo.toml \
    crates/md2x-gui/tauri.conf.json
[ -f RELEASE_NOTES.md ] && git add RELEASE_NOTES.md
git commit -m "chore: release $TAG"
git tag "$TAG"

git push origin main
git push origin "$TAG"
echo "已发布 ${TAG}：${REPO_URL}/releases/tag/${TAG}"
