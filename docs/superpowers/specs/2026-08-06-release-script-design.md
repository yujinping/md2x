# release.sh 结构化与自动检测设计

日期：2026-08-06

## 目标

将 `release.sh` 结构化，使其对任意 Rust 项目（单 crate 或 workspace）开箱即用、零配置：

- 自动检测远程仓库信息（REPO_URL、项目名）
- 自动检测版本文件（Cargo.toml、tauri.conf.json）
- 自动检测默认分支
- 保留现有发版流程（bump → CHANGELOG → RELEASE_NOTES → 提交 → tag → 推送）
- 全局安装到 PATH，任意项目根目录直接执行，项目内不复制多份脚本

## 范围

- 仅支持 Rust 项目（依赖 `cargo metadata`）
- 远程平台仅支持 GitHub（HTTPS 与 SSH 地址均可解析）
- 不支持非 Rust 项目，不支持 GitLab / Gitee 等平台的发布链接拼接
- 脚本单一副本维护在 md2x 仓库，通过符号链接安装到 PATH，其他项目不复制

## 安装与使用

- 在脚本所在目录执行 `release.sh --install`：
  - 在 `~/.local/bin/` 创建符号链接 `release -> <脚本路径>`（目录不存在则创建，并提示将 `~/.local/bin` 加入 PATH）
  - 对符号链接赋可执行权限
- 安装后在任意项目根目录执行 `release v0.1.2`（等价于原来的 `./release.sh v0.1.2`）
- 升级：重跑 `release --install` 即可（符号链接指向不变，内容即时生效）
- 脚本源码与文档保留在 md2x 仓库（唯一“家”），日常使用走全局命令

## 架构

单文件 bash 脚本，逻辑函数化：

```
release.sh
├── 检测函数
│   ├── detect_repo()            # 解析 git remote → REPO_URL / 项目名
│   ├── detect_version_files()   # cargo metadata + find → 版本文件列表
│   └── detect_default_branch()  # origin/HEAD → 默认分支（回退 main）
├── 前置检查（git-cliff / 分支 / dirty / 本地与远程 tag）
├── 预览 → dry-run → 确认
├── bump（内嵌 python 读版本文件列表；cargo 自动刷新 Cargo.lock）
├── CHANGELOG + RELEASE_NOTES 生成
└── 提交 / tag / push
```

## 组件设计

### 项目定位

- 所有检测（git remote、cargo metadata、find tauri.conf.json、分支、tag）基于**执行时的当前目录（cwd）**，与脚本所在位置无关
- 在非项目目录或没有 origin 的目录执行 → 明确报错退出

### detect_repo()

- 读取 `git remote get-url origin`
- 支持两种格式：
  - HTTPS：`https://github.com/owner/repo.git`
  - SSH：`git@github.com:owner/repo.git`
- 提取 owner / repo（去掉 `.git` 后缀）
- 输出：
  - `REPO_URL=https://github.com/owner/repo`（发布链接前缀）
  - 项目名 = repo（用于文案“将发布 <项目名>”）
- 远程缺失或格式无法解析 → 报错退出，提示先配置 origin

### detect_version_files()

- 运行 `cargo metadata --no-deps --format-version 1`，解析 JSON：
  - workspace 根 manifest（若存在 `[workspace.package] version` 则纳入）
  - 所有 member 的 manifest_path（去重）
- `find . -name tauri.conf.json`（排除 `.git`、`target`、`node_modules`）追加到列表
- tauri 配置找不到 → 跳过，不报错
- 输出版本文件列表，供 bump 与 `git add` 共用

### detect_default_branch()

- 优先 `git symbolic-ref --short refs/remotes/origin/HEAD` 解析默认分支
- 失败时回退 `main`
- 前置检查：当前分支必须等于默认分支

### bump（内嵌 python）

- 对版本文件列表中的每个 Cargo.toml：替换第一个 `version = "x.y.z"`
- `version = { workspace = true }` 继承写法不匹配，自动跳过（继承根版本无需改）
- 对 `tauri.conf.json`：替换第一个 `"version": "..."`
- 任一文件找不到可替换版本字段 → 报错并列出该文件，退出
- 全部替换完成后运行 `cargo metadata --no-deps --format-version 1`，让 cargo 自动刷新 `Cargo.lock`

## 数据流

1. 检测：仓库、版本文件、默认分支
2. 前置检查：git-cliff 存在、当前分支、工作区干净（允许未提交的 RELEASE_NOTES.md）、本地/远程 tag 不存在
3. 预览（含 dry-run）→ 用户确认
4. bump 版本（含 Cargo.lock 刷新）
5. git-cliff 生成 CHANGELOG.md
6. 自动生成 / 更新 RELEASE_NOTES.md（从 CHANGELOG unreleased 段落提取；手动维护的不覆盖）
7. 提交 `chore: release <tag>`、打 tag、推送 main 与 tag

## 错误处理

| 场景 | 行为 |
|------|------|
| 不是 git 仓库 / 无 origin / remote 无法解析 | 报错退出，提示配置 origin |
| `cargo metadata` 失败 | 报错退出（脚本定位为 Rust 项目专用） |
| 无 tauri.conf.json | 跳过，不报错 |
| 版本文件找不到可替换字段 | 报错并列出文件，退出 |
| 版本号格式非法 | 报错退出（现有行为） |
| tag 已存在（本地或远程） | 报错退出（现有行为） |

## 测试矩阵（沙盒验证）

- workspace 多 crate + tauri（模拟 md2x 结构）
- 单 crate、无 tauri
- SSH remote 与 HTTPS remote 各一次
- 无 origin 的报错路径
- `version = { workspace = true }` 继承场景确认不被误改
- Cargo.lock 自动刷新验证
- 在其他 Rust 项目目录（沙盒模拟独立仓库）执行，确认检测基于 cwd 而非脚本位置
- 在非项目目录执行 → 报错路径
- `--install` 符号链接创建与命令可执行性

## 明确不做（YAGNI）

- 不支持配置文件（.release.toml 等），检测失败即报错
- 不拆分多文件脚本
- 不支持非 GitHub 平台发布链接
