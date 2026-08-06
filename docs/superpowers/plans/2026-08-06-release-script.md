# release 脚本结构化与全局安装实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 将 `release.sh` 重构为对任意 Rust 项目自动检测、可全局安装（`release --install`）的单文件发版工具。

**架构：** 单文件 bash，逻辑函数化（`detect_repo` / `detect_default_branch` / `detect_version_files` + 内嵌 python bump）。所有检测基于执行时 cwd；`--install` 通过符号链接安装到 `~/.local/bin/release`。

**技术栈：** bash 4+（`mapfile`、`BASH_REMATCH`）、python3、cargo、git、git-cliff。

---

## 文件结构

- 修改：`release.sh` — 唯一实现文件，重构为函数化 + 自检测 + `--install`
- 修改：`README.md` — 增加全局安装与使用说明（任务 6）
- 验证：沙盒临时目录（`mktemp -d`），不新增正式测试文件

## 任务 1：骨架 + detect_repo + detect_default_branch

**文件：**
- 修改：`release.sh`

### 步骤 1.1：验证当前脚本没有自动检测（失败确认）

运行：
```bash
TMPD=$(mktemp -d) && cd "$TMPD" && git init -q -b main && git remote add origin https://github.com/user/demo.git && /Users/yujinping/data/workspace/rust-projects/md2x/release.sh --dry-run v0.1.2 2>&1 | grep "demo"
```
预期：无输出（当前脚本写死 md2x，不识别 demo 仓库）。

### 步骤 1.2：实现函数骨架与 detect_repo / detect_default_branch

在 `release.sh` 头部（`set -euo pipefail` 之后、原 `REPO_URL=` 处）替换为：

```bash
# ---- 安装模式：符号链接到 ~/.local/bin/release ----
if [ "${1:-}" = "--install" ]; then
    INSTALL_DIR="${HOME}/.local/bin"
    mkdir -p "$INSTALL_DIR"
    REAL="$(python3 -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$0")"
    ln -sf "$REAL" "$INSTALL_DIR/release"
    chmod +x "$INSTALL_DIR/release"
    echo "已安装 release -> $INSTALL_DIR/release"
    echo "请确保 $INSTALL_DIR 已加入 PATH"
    exit 0
fi

DRY=0
if [ "${1:-}" = "--dry-run" ]; then
    DRY=1
    shift
fi

if [ $# -ne 1 ]; then
    echo "用法: release [--install] [--dry-run] <版本号>"
    echo "  例: release 0.1.2   或   release v0.1.2"
    exit 1
fi

VER="${1#v}"
TAG="v$VER"

if ! echo "$VER" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "错误: 版本号格式应为 x.y.z，收到 '$VER'"
    exit 1
fi

# ---- 检测：远程仓库 ----
detect_repo() {
    local url
    url="$(git remote get-url origin 2>/dev/null || true)"
    [ -n "$url" ] || { echo "错误: 未找到 origin remote，请先 git remote add origin <url>"; exit 1; }
    local owner repo
    if [[ "$url" =~ ^https?://[^/]+/([^/]+)/([^/]+?)(\.git)?/?$ ]]; then
        owner="${BASH_REMATCH[1]}"
        repo="${BASH_REMATCH[2]}"
    elif [[ "$url" =~ ^git@[^:]+:([^/]+)/([^/]+?)(\.git)?$ ]]; then
        owner="${BASH_REMATCH[1]}"
        repo="${BASH_REMATCH[2]}"
    else
        echo "错误: 无法解析 origin URL: $url（仅支持 GitHub HTTPS/SSH）"
        exit 1
    fi
    REPO_URL="https://github.com/${owner}/${repo}"
    PROJECT_NAME="$repo"
}
detect_repo

# ---- 检测：默认分支 ----
detect_default_branch() {
    DEFAULT_BRANCH="$(git symbolic-ref --short refs/remotes/origin/HEAD 2>/dev/null | sed 's#^origin/##' || true)"
    [ -n "${DEFAULT_BRANCH:-}" ] || DEFAULT_BRANCH="main"
}
detect_default_branch
```

同时删除原 `REPO_URL="https://github.com/yujinping/md2x"` 行，并把分支检查改为：

```bash
[ "$(git branch --show-current)" = "$DEFAULT_BRANCH" ] || { echo "错误: 请先切换到 $DEFAULT_BRANCH 分支"; exit 1; }
```

### 步骤 1.3：验证 HTTPS 与 SSH remote 检测

运行：
```bash
TMPD=$(mktemp -d) && cd "$TMPD" && git init -q -b main && git remote add origin https://github.com/user/demo.git && /Users/yujinping/data/workspace/rust-projects/md2x/release.sh --dry-run v0.1.2 2>&1 | grep -E "将发布 demo v0.1.2"
TMPD=$(mktemp -d) && cd "$TMPD" && git init -q -b main && git remote add origin git@github.com:user/demo.git && /Users/yujinping/data/workspace/rust-projects/md2x/release.sh --dry-run v0.1.2 2>&1 | grep -E "将发布 demo v0.1.2"
```
预期：两行都输出 `将发布 demo v0.1.2`。

无 origin 报错验证：
```bash
TMPD=$(mktemp -d) && cd "$TMPD" && git init -q -b main && /Users/yujinping/data/workspace/rust-projects/md2x/release.sh --dry-run v0.1.2 2>&1 | grep "未找到 origin remote"
```
预期：输出 `错误: 未找到 origin remote，请先 git remote add origin <url>`。

### 步骤 1.4：Commit

```bash
git add release.sh
git commit -m "refactor(release): 增加仓库与默认分支自动检测"
```

## 任务 2：detect_version_files

**文件：**
- 修改：`release.sh`

### 步骤 2.1：实现 detect_version_files

在任务 1 的 `detect_default_branch` 之后追加：

```bash
# ---- 检测：版本文件（cargo metadata + tauri.conf.json）----
detect_version_files() {
    local meta
    meta="$(cargo metadata --no-deps --format-version 1 2>/dev/null)" \
        || { echo "错误: cargo metadata 失败，请确认在 Rust 项目根目录执行"; exit 1; }
    mapfile -t VERSION_FILES < <(python3 - "$meta" <<'PY'
import json, sys
meta = json.loads(sys.argv[1])
files = set()
if meta.get("workspace_root"):
    files.add(meta["workspace_root"] + "/Cargo.toml")
for p in meta.get("packages", []):
    files.add(p["manifest_path"])
print("\n".join(sorted(files)))
PY
)
    while IFS= read -r f; do
        VERSION_FILES+=("$f")
    done < <(find . -name tauri.conf.json -not -path './.git/*' -not -path './target/*' -not -path './node_modules/*' 2>/dev/null || true)
}
detect_version_files
```

并把预览第 1 行改为显示检测数量：

```bash
echo "  1. bump ${#VERSION_FILES[@]} 个版本文件 ->  $VER"
```

### 步骤 2.2：验证 workspace 与单 crate 检测

workspace（3 crate + tauri）：
```bash
TMPD=$(mktemp -d) && cd "$TMPD" && git init -q -b main && git remote add origin https://github.com/user/demo.git && mkdir -p crates/a crates/b crates/c && printf '[workspace]\nmembers = ["crates/a", "crates/b", "crates/c"]\n\n[workspace.package]\nversion = "0.1.1"\n' > Cargo.toml && for c in a b c; do printf '[package]\nname = "%s"\nversion = "0.1.1"\n' "$c" > "crates/$c/Cargo.toml"; done && printf '{\n  "version": "0.1.1"\n}\n' > crates/c/tauri.conf.json && /Users/yujinping/data/workspace/rust-projects/md2x/release.sh --dry-run v0.1.2 2>&1 | grep "bump 5 个版本文件"
```
预期：`1. bump 5 个版本文件 -> 0.1.2`（根 Cargo.toml + 3 crate + tauri）。

单 crate 无 tauri：
```bash
TMPD=$(mktemp -d) && cd "$TMPD" && git init -q -b main && git remote add origin https://github.com/user/demo.git && printf '[package]\nname = "demo"\nversion = "0.1.1"\n' > Cargo.toml && /Users/yujinping/data/workspace/rust-projects/md2x/release.sh --dry-run v0.1.2 2>&1 | grep "bump 1 个版本文件"
```
预期：`1. bump 1 个版本文件 -> 0.1.2`。

### 步骤 2.3：Commit

```bash
git add release.sh
git commit -m "refactor(release): 自动检测版本文件列表"
```

## 任务 3：bump 重构（读 VERSION_FILES + cargo 刷新 Cargo.lock）

**文件：**
- 修改：`release.sh`

### 步骤 3.1：替换 bump 的 python 段

将原 bump 段（`python3 - "$VER" <<'PY' ... PY`）整体替换为：

```bash
# ---- bump 版本（版本文件列表 + cargo 刷新 Cargo.lock）----
python3 - "$VER" "${VERSION_FILES[@]}" <<'PY'
import re, sys
ver = sys.argv[1]
files = sys.argv[2:]
ok = True
for path in files:
    with open(path, encoding="utf-8") as fh:
        s = fh.read()
    if path.endswith(".json"):
        pattern, repl = re.compile(r'^  "version": "[^"]+"', re.M), f'  "version": "{ver}"'
    else:
        pattern, repl = re.compile(r'^version = "[^"]+"', re.M), f'version = "{ver}"'
    new_s, n = pattern.subn(repl, s, count=1)
    if n == 0:
        print(f"跳过 {path}（无独立版本字段，可能为 workspace 继承）")
        continue
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(new_s)
    print(f"bump {path} -> {ver}")
sys.exit(0 if ok else 1)
PY

# 让 cargo 自动同步 Cargo.lock
cargo metadata --no-deps --format-version 1 >/dev/null
echo "已同步 Cargo.lock"
```

把 `git add` 行改为使用检测列表：

```bash
git add Cargo.lock CHANGELOG.md "${VERSION_FILES[@]}"
```

### 步骤 3.2：验证 workspace 完整 bump + 继承跳过 + lock 刷新

```bash
TMPD=$(mktemp -d) && cd "$TMPD" && git init -q -b main && git remote add origin https://github.com/user/demo.git && mkdir -p crates/a crates/b && printf '[workspace]\nmembers = ["crates/a", "crates/b"]\n\n[workspace.package]\nversion = "0.1.1"\n' > Cargo.toml && printf '[[package]]\nname = "a"\nversion = "0.1.1"\n\n[[package]]\nname = "b"\nversion = "0.1.1"\n' > Cargo.lock && printf '[package]\nname = "a"\nversion = { workspace = true }\n' > crates/a/Cargo.toml && printf '[package]\nname = "b"\nversion = "0.1.1"\n' > crates/b/Cargo.toml && git add -A && git -c user.name=t -c user.email=t@t commit -qm init && printf 'y\n' | /Users/yujinping/data/workspace/rust-projects/md2x/release.sh v0.1.2 >/dev/null 2>&1; rg '0\.1\.2' Cargo.toml crates/b/Cargo.toml Cargo.lock | wc -l && rg 'workspace = true' crates/a/Cargo.toml
```
预期：`0.1.2` 出现 4 处（根 + crates/b + lock 2 处），crates/a 仍是 `version = { workspace = true }`（未被误改）。

### 步骤 3.3：Commit

```bash
git add release.sh
git commit -m "refactor(release): bump 基于检测列表并让 cargo 刷新 Cargo.lock"
```

## 任务 4：流程接线（文案 / 预览 / git add）

**文件：**
- 修改：`release.sh`

### 步骤 4.1：把剩余硬编码改为检测结果

- `echo "将发布 md2x $TAG"` → `echo "将发布 $PROJECT_NAME $TAG"`
- 末尾 `echo "已发布 ${TAG}：${REPO_URL}/releases/tag/${TAG}"` 保持（REPO_URL 已自动检测）

### 步骤 4.2：验证文案与完整流程

```bash
TMPD=$(mktemp -d) && cd "$TMPD" && git init -q -b main && git remote add origin https://github.com/user/demo.git && printf '[package]\nname = "demo"\nversion = "0.1.1"\n' > Cargo.toml && printf '[[package]]\nname = "demo"\nversion = "0.1.1"\n' > Cargo.lock && git add -A && git -c user.name=t -c user.email=t@t commit -qm init && /Users/yujinping/data/workspace/rust-projects/md2x/release.sh --dry-run v0.1.2 2>&1 | grep -E "将发布 demo|releases/tag/v0.1.2"
```
预期：输出 `将发布 demo v0.1.2` 且 dry-run 预览包含 `releases/tag/v0.1.2`。

### 步骤 4.3：Commit

```bash
git add release.sh
git commit -m "refactor(release): 文案与流程使用自动检测结果"
```

## 任务 5：--install 全局安装

**文件：**
- 修改：`release.sh`（任务 1 已含 `--install` 分支，此任务验证并补强）

### 步骤 5.1：验证符号链接安装（用临时 HOME，不动真实环境）

```bash
TMPH=$(mktemp -d) && HOME="$TMPH" /Users/yujinping/data/workspace/rust-projects/md2x/release.sh --install && ls -la "$TMPH/.local/bin/release" && "$TMPH/.local/bin/release" 2>&1 | head -2
```
预期：符号链接存在且指向真实脚本；无参数执行显示用法。

### 步骤 5.2：验证从其他项目目录通过 PATH 命令执行

```bash
TMPH=$(mktemp -d) && HOME="$TMPH" /Users/yujinping/data/workspace/rust-projects/md2x/release.sh --install >/dev/null && TMPD=$(mktemp -d) && cd "$TMPD" && git init -q -b main && git remote add origin https://github.com/user/other.git && "$TMPH/.local/bin/release" --dry-run v0.1.2 2>&1 | grep "将发布 other v0.1.2"
```
预期：输出 `将发布 other v0.1.2`（cwd 检测，与脚本位置无关）。

### 步骤 5.3：Commit

```bash
git add release.sh
git commit -m "feat(release): 支持 --install 全局安装"
```

## 任务 6：回归矩阵 + README

**文件：**
- 修改：`README.md`

### 步骤 6.1：README 增加安装与使用说明

在 README 的 CLI 模式小节后追加以下小节（注意内嵌的命令示例使用代码块格式）：

标题：`### 全局安装（推荐）`

正文：

- 首次安装：在 md2x 仓库根目录执行 `./release.sh --install`，会在 `~/.local/bin/` 创建 `release` 命令的符号链接；
- 使用：在任意 Rust 项目根目录执行 `release v0.1.2`（或 `release --dry-run v0.1.2` 预览）；
- 自动检测：远程仓库（GitHub HTTPS/SSH）、版本文件（cargo workspace / 单 crate / tauri.conf.json）、默认分支，零配置。

### 步骤 6.2：跑完整回归矩阵

依次验证（每个均预期输出对应文案，具体命令见任务 1/2/3/5）：
1. HTTPS remote 检测
2. SSH remote 检测
3. 无 origin 报错
4. workspace 多 crate + tauri（bump 5 个文件）
5. 单 crate 无 tauri（bump 1 个文件）
6. `workspace = true` 继承不被误改
7. Cargo.lock 自动刷新
8. 其他项目目录执行（cwd 检测）
9. 非项目目录执行 → cargo metadata 报错
10. `--install` 符号链接可执行

### 步骤 6.3：Commit

```bash
git add README.md
git commit -m "docs: 补充 release 全局安装说明"
```

---

## 自检记录

- 规格覆盖度：detect_repo（任务 1）、detect_default_branch（任务 1）、detect_version_files（任务 2）、bump + cargo 刷新（任务 3）、文案/流程接线（任务 4）、--install（任务 5）、测试矩阵（任务 6）均有对应任务。
- 占位符扫描：无 TODO / 待定。
- 类型一致性：`VERSION_FILES`、`REPO_URL`、`PROJECT_NAME`、`DEFAULT_BRANCH` 在任务间一致；bump python 签名 `python3 - "$VER" "${VERSION_FILES[@]}"` 与任务 3 实现一致。
