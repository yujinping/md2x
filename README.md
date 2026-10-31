# md2x

md2x 是一个将 Markdown **与 DBML** 转换为精美文档的工具，支持 PDF、HTML、PNG、DOCX 等输出格式，同时提供命令行（CLI）和桌面应用（GUI）两种使用方式。输出格式采用可扩展架构，新增格式只需扩展格式枚举与转换分支；输入格式同样可扩展，见「DBML 数据库设计文件」与设置中的「渲染类型」。

## 项目结构

```
md2x-project/
├── Cargo.toml                 # 工作区根配置
├── crates/
│   ├── md2x-core/           # 核心库：Markdown / DBML → HTML → PDF 转换引擎
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
cargo run -p md2x-cli -- README.md --format html --template my-theme/ --var accent=#e11d48  # 覆盖模版变量
cargo run -p md2x-cli -- schema.dbml --format html  # DBML -> HTML（数据字典 + 可缩放 Schema 画布）
cargo run -p md2x-cli -- schema.dbml --format docx  # DBML -> DOCX（字段表 + 备注）
```

### DBML 数据库设计文件

md2x 原生识别 [DBML](https://dbml.dbdiagram.io)（Database Markup Language），把一份 `.dbml` 直接渲染成文档，产出**单个自包含 HTML**（CSS / JS / SVG 全部内联，零外部请求，可直接发出去或进 git）。

一份 DBML 会产出三个视图，正文内 Tab 切换：

| Tab | 内容 |
|---|---|
| **数据字典** | 每张表一张卡片，7 列字段表：字段 / 类型 / 约束（PK·UQ·NN·INC）/ 可空 / 默认值 / 外键 / 说明；表备注在卡片头，索引附在卡片尾 |
| **Schema 画布** | 表按关系自动布局的内联 SVG。表名下方带**表注释**（取自 `Note:` 或表头 `[comment: '...']`，超出预算截断为 `...`）；每个字段前标注 **PK / FK / UQ** 徽章（红 / 绿 / 紫）。支持滚轮缩放、拖拽平移、表名搜索、点击定位；「适应窗口」带 60% 可读下限，不把字缩成一团 |
| **引用关系** | 全部 `Ref` 的表格清单，含 ON DELETE / ON UPDATE |

配色跟随所选 HTML 模版：模板变量（`--accent`、`--bg`、`--fg`…）改一处，数据字典、画布、引用表同时变化，深浅色主题一并生效。

**连线路由**：画布上的外键连线是正交折线，只走两种通道——相邻列之间的走廊，以及画布上下为跨列边预留的横向总线。这两种通道在结构上都不经过任何表框，所以「线穿表」不会发生，而不是靠调参看起来没穿。同列两表之间的外键绕到列外走 U 形（同列时无法直连），同一条走廊里的多条边会横向错开几像素避免糊成一条。

**链路短路**：DBML → HTML 是直连的，不经过中间 Markdown；导出 DOCX 时在内存里转成数据字典式 Markdown 再交给 Word 生成器，**中间文件不落盘**。同一份源在 GUI 预览、`--format docx`、右键导出 Markdown 三条路径上产出一致。

**在 GUI 里用**：

- 拖入文件夹时自动过滤出可渲染文件，DBML 文件在文件树里用三层圆柱图标区分
- 点击即渲染，无需先转 Markdown
- **右键**可导出 Markdown / HTML / PDF / DOCX，且不必先打开该文件
- 设置 → **渲染类型** 里可手工增删扩展名映射（内置 `md` / `markdown` / `dbml`，可再加 `.txt`、`.sql` 等并指定用哪个内置渲染器）

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

- **单文件预览**：打开 / 拖入 `.md`、`.dbml` 等受支持文件，HTML 即时预览，可导出 PDF、HTML、DOCX、Markdown
- **文件夹文件树**：`打开文件夹`（或 `Cmd/Ctrl+Shift+O`）后，左侧显示该目录及其子目录内全部**可渲染文件**（按「渲染类型」配置过滤），支持展开 / 收起与关闭；可直接把文件夹拖入窗口打开
- **按类型渲染**：`.md` 按 Markdown 渲染，`.dbml` 直接渲染为数据字典 + Schema 画布（不中转 Markdown）；扩展名与渲染器的映射可在设置 → 渲染类型中增删
- **右键导出**：文件树中右键任意文件即可导出 Markdown / HTML / PDF / DOCX，无需先打开该文件
- **默认视图**：可在设置中配置启动视图（单文件视图 / 文件夹树视图 / 自动）
- **HTML 模板**：预览与导出使用同一套模板。模板以「目录包」形式组织，放入模板目录（设置 → 导出 → 打开模板目录）后自动扫描加载，卡片式选择并记住所选模版；模版可声明可调变量（配色/字号/字体），设置面板直接改、即时预览；CLI 用 `--template <目录|文件>` 指定，`--var key=value` 覆盖变量
- **导出预设**：把常用的「格式 + 模版 + 主题 + 全宽」存成一条（如「发周报」「交审 PDF」），之后从菜单 文件 → 导出 一键触发；预设存本机，模版变量按模版 id 跟随生效
- **外部图表引擎**：```d2 / ```dot / ```graphviz 代码块由外部 CLI（[D2](https://d2lang.com) / [Graphviz](https://graphviz.org)）在转换期烘焙为内嵌 SVG，设置面板可查看安装状态，装上即用；未安装时保留代码块不报错
- **默认打开程序**：设置中一键把 `.md` 关联到 md2x（Windows 写注册表、macOS 写 LaunchServices、Linux 使用 xdg-mime），双击 Markdown 文件即可用 md2x 打开

### 自带模板

首次运行时，三套风格互不重叠的自带模板会写入模板目录，**都是普通模板包**：
改了就生效，删了不影响其他模板，可以和自建模板一样被复制、改写、删掉。

| 目录 | 版式 | 适合 |
|---|---|---|
| `sample/` | 单栏窄幅 + 顶部目录框 | 长文阅读、打印 |
| `docs/` | 左侧固定目录 + 滚动高亮章节 + 代码高亮 | 手册、说明文档 |
| `paper/` | 衬线正文 + 居中书名 + 首行缩进 + 打印目录页 | 汇报、论文、PDF |

想改成自己的版式，最省事的路径是**复制一份再改**：设置 → 导出 → 选中模板 → 「复制为副本」，
会生成 `<id>-copy` 并在卡片上标出「副本」；原模板保持干净，方便对照。
已存在的目录永远不会被覆盖，删掉的也不会在下次启动被写回（判断依据是同目录下的
`.bundled-seeded` 记录文件，想恢复就删掉它再重启）。

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
| `{{VARS_CSS}}` | | 用户自定义变量的覆盖样式（没改过变量时为空） |
| `{{VAR_大写KEY}}` | | 某变量的当前值直填（如 `{{VAR_ACCENT}}`） |
| `{{GITHUB_MD_CSS}}` 等 | | 内置样式与脚本，按需取用 |

#### 可调变量（配色 / 字号 / 字体）

模版可在 `template.json` 里声明 `vars`，设置面板就会在模版卡片下方生成对应控件，
用户改完即时预览、导出同效，取值按模版 id 记在本机（应用数据目录，不在模版目录里）：

```json
{
  "vars": {
    "accent": { "label": "主色", "default": "#2f6feb", "type": "color", "hint": "标题与链接的强调色" },
    "doc_font_size": "15.5px"
  }
}
```

| 字段 | 说明 |
|---|---|
| key | 只能是 `[a-z0-9_]`，同时决定 CSS 变量名与占位符：`doc_font_size` → `--doc-font-size` / `{{VAR_DOC_FONT_SIZE}}` |
| `type` | `color`（取色器）或省略 / `text`（文本框） |
| `default` | 默认值；值写不写字符串形式都行（`"accent": "#2f6feb"` 是简写） |
| `hint` | 控件下方的一行说明，可省略 |

样式里只要在 `<head>` 末尾放一个 `<style>{{VARS_CSS}}</style>`，用户改过的项就会自动追加成
`:root` / `html[data-theme="dark"]` 的覆盖声明，模版自己的 CSS 照常写 `var(--accent)` 即可；
需要非 CSS 场景（署名、标题后缀）时用 `{{VAR_大写KEY}}` 直填。没有改动时两者都为空，
导出的 HTML 与手写的完全一致。颜色值渲染前会校验，非法值回落默认。
CLI 侧用 `--var accent=#e11d48 --var doc_font_size=18px` 覆盖。

模板自备样式与交互脚本；引用外部 CSS/JS 会破坏自包含性，建议内联。
首次打开模板目录还会生成一份说明 README，写明包结构与占位符契约。

### 导出预设

设置 → 导出 → 「保存当前为预设」把**格式 + 模版 + 主题 + 全宽**存成一条，
之后从菜单 **文件 → 导出** 直接点名字即可（保存对话框照常弹出，不会静默覆盖文件）。
设置面板里每条预设还能「应用」（只切设置、不导出）或删除。

- 预设只存这四个字段；模版的可调变量按模版 id 独立存档，切到某个模版就自动带上它那套取值，
  因此改一次配色，所有用到该模版的预设都会跟着变
- 预设在本机（`localStorage`），不进模版目录，复制模版给别人不会带走你的导出习惯
- 已删除的模版仍被预设引用时不会失效：后端加载模版失败会回退内置模版

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
