use md2x_core::aggregate;
use md2x_core::chrome;
use md2x_core::converter;
use md2x_core::error::MpeError;
use md2x_core::{ext_diagram, template};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;
use tauri::menu::{CheckMenuItem, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager, State};
use tokio::sync::oneshot;

/// 主题参数的解析辅助：前端传字符串，缺失或非法时回退到「跟随系统」。
mod theme_arg {
    use md2x_core::template::Theme;

    pub fn parse_opt(v: &Option<String>) -> Theme {
        match v {
            Some(s) => Theme::parse(s),
            None => Theme::Auto,
        }
    }
}

/// 约定模版目录：默认 `<app_data_dir>/templates`，可用环境变量 `MD2X_TEMPLATE_DIR` 覆盖。
///
/// 覆盖入口用于并行维护多套模版（例如设计评审时比对配色），也是自动化测试
/// 注入夹具的通道——不必触碰真实用户目录。目录不存在则创建。
fn templates_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Ok(custom) = std::env::var("MD2X_TEMPLATE_DIR") {
        if !custom.trim().is_empty() {
            let d = PathBuf::from(custom.trim());
            std::fs::create_dir_all(&d).map_err(|e| format!("创建模板目录失败: {e}"))?;
            return Ok(d);
        }
    }
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录: {e}"))?;
    let d = base.join("templates");
    std::fs::create_dir_all(&d).map_err(|e| format!("创建模板目录失败: {e}"))?;
    Ok(d)
}

/// 首次打开模板目录时写入的说明文件，向模板作者说明模版包约定与占位符契约。
// JSON 示例里含 `"#hex"`，会把 r#"…"# 的定界符提前闭合，故用 r##"…"##
const TEMPLATES_README: &str = r##"# md2x 模版目录

本目录下**每个子目录即一个模版包**，模版列表自动扫描，放入后点「刷新列表」即可选用；
选中结果会被记住，预览与导出共用同一个模版。

## 模版包结构

```
<templates>/
└── my-theme/              # 目录名即模版 id（选中后持久化的就是这个 id）
    ├── template.html      # 必需：入口页面，必须含 {{BODY}} 占位符
    ├── template.json      # 可选：元数据（缺省时展示名取目录名）
    └── preview.png        # 可选：缩略图，显示在设置面板的模版卡片上
```

`template.json` 全部字段可省略，未知字段会被忽略：

```json
{
  "name": "论文体",
  "description": "学术排版，窄栏长文",
  "version": "1.0.0",
  "author": "your name",
  "preview": "preview.png"
}
```

缩略图按约定名顺序探测：`preview.png` / `preview.jpg` / `preview.jpeg` /
`preview.webp` / `preview.svg`；也可用 `template.json` 的 `preview` 字段指定任意相对路径。
缺失缩略图时列表显示占位图，不影响使用。

## 可调变量（配色 / 字号 / 字体）

在 `template.json` 里声明 `vars`，模版就会在「设置 → 导出」下方多出一组控件，
用户改完的值保存在本机应用数据目录（不在本目录），预览与导出共用：

```json
{
  "vars": {
    "accent": {
      "label": "主色",
      "default": "#2f6feb",
      "type": "color",
      "hint": "标题与链接的强调色"
    },
    "doc_font_size": "15.5px"
  }
}
```

- `type` 取 `color`（取色器）或省略 / `text`（文本框）；也可整个写成字符串，等于只给默认值
- key 只能是 `[a-z0-9_]`，它同时决定 CSS 变量名与占位符：
  key `doc_font_size` → CSS 变量 `--doc-font-size`、占位符 `{{VAR_DOC_FONT_SIZE}}`
- 要在样式里生效有两种方式，任选：

```html
<!-- 1. 推荐：在 <head> 末尾放一个空样式块，用户改过的变量会自动追加成
     :root / html[data-theme="dark"] 的覆盖声明，模版自己的 CSS 用 var() 取值即可 -->
<style>{{VARS_CSS}}</style>

<!-- 2. 直接文本替换：适合非 CSS 场景（标题后缀、署名等） -->
<div class="brand">{{VAR_ACCENT}}</div>
```

没改过任何变量时 `{{VARS_CSS}}` 为空，导出的页面与手写的完全一致。
颜色值会在渲染前校验，非法值自动回落到默认，不会把样式写坏。

## 旧式单文件模版

直接放在本目录根下的 `<id>.html` 仍可用（无元数据、无缩略图），但不推荐新模版采用。

## 占位符契约

- `{{BODY}}`（必需）：正文落点；缺失该占位符的模版不会被识别
- `{{TITLE}}`：文档标题
- `{{TOC}}`：侧栏目录（嵌套 li，需自行包 ul）
- `{{PRINT_TOC}}`：打印目录页（仅 @media print 可见）
- `{{DOC_TREE}}`：聚合导出的文档树
- `{{SKILL_METADATA}}`：SKILL.md 元数据卡片
- `{{THEME_LOCK}}`：主题档位（auto/light/dark 字符串）
- `{{WIDTH_VARS}}`：全宽模式 CSS
- `{{VARS_CSS}}`：用户自定义变量的覆盖样式（未改过变量时为空）
- `{{VAR_大写KEY}}`：把某个变量的当前值直接填进来（如 `{{VAR_ACCENT}}`）
- `{{GITHUB_MD_CSS}}` / `{{ATOM_ONE_DARK_CSS}}` / `{{HIGHLIGHT_JS}}`：内置样式与脚本，按需取用

## 自带模版（可直接复制修改）

应用首次运行会把三套自带模版写进本目录，风格各不相同：

| 目录 | 风格 | 适合 |
|---|---|---|
| `sample/` | 单栏窄幅 + 顶部目录框 | 长文阅读、打印 |
| `docs/` | 左侧固定目录 + 滚动高亮 | 手册、说明文档 |
| `paper/` | 衬线正文 + 首行缩进 + 打印目录页 | 汇报、论文、PDF |

它们是**普通的模版包**：改了就生效，删了不会影响其他模版。
推荐做法是整份复制成 `<你的名字>/` 再改（设置面板的「复制为副本」可直接完成这一步），
原目录保持干净，方便对照。

已存在的目录永远不会被覆盖；删掉的也不会在下次启动时被写回——
判断依据是同目录下的 `.bundled-seeded` 记录文件，想恢复删掉它再重启应用即可。

## 注意

- 模版需自备样式与交互脚本；引用外部 CSS/JS 会破坏自包含性，建议内联
- 聚合导出的文档树、主题切换、全宽等交互需模版自行支持
"##;

/// 一份自带模版包：编译进二进制，首次运行写入模版目录，供用户复制后修改。
///
/// 刻意没有任何「自带」特权——写入后它就是普通模版包，走同一条扫描与渲染路径，
/// 用户改完立即生效，改坏了删掉即可。
struct BundledTemplate {
    /// 目录名，同时是模版 id
    id: &'static str,
    /// 入口 HTML（必须含 {{BODY}}）
    html: &'static str,
    /// 清单（名称/描述/缩略图字段）
    manifest: &'static str,
    /// 缩略图文件名，需与清单里的 `preview` 字段一致
    preview_name: &'static str,
    /// 缩略图内容
    preview: &'static str,
}

/// 三套风格互不重叠的自带模版：
/// 简约（单栏窄幅 + 顶部目录框）、技术文档（左侧固定目录 + 滚动高亮）、
/// 正式报告（衬线 + 首行缩进 + 打印目录页）。
const BUNDLED_TEMPLATES: &[BundledTemplate] = &[
    BundledTemplate {
        id: "sample",
        html: include_str!("../../../templates/sample/template.html"),
        manifest: include_str!("../../../templates/sample/template.json"),
        preview_name: "preview.svg",
        preview: include_str!("../../../templates/sample/preview.svg"),
    },
    BundledTemplate {
        id: "docs",
        html: include_str!("../../../templates/docs/template.html"),
        manifest: include_str!("../../../templates/docs/template.json"),
        preview_name: "preview.svg",
        preview: include_str!("../../../templates/docs/preview.svg"),
    },
    BundledTemplate {
        id: "paper",
        html: include_str!("../../../templates/paper/template.html"),
        manifest: include_str!("../../../templates/paper/template.json"),
        preview_name: "preview.svg",
        preview: include_str!("../../../templates/paper/preview.svg"),
    },
];

/// 播种记录文件名（隐藏文件，放在模版目录内）。
///
/// 只靠「目录是否存在」判断无法区分「从没写过」与「用户删掉了」，
/// 缺了这份记录，用户删掉不想要的自带模版后，下次启动又会被铺回来。
const SEED_STATE_FILE: &str = ".bundled-seeded";

/// 读取播种记录：一行一个 id，`#` 开头为注释
fn read_seed_state(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// 写回播种记录；带上恢复说明，避免用户看到陌生隐藏文件时无从下手
fn write_seed_state(path: &Path, ids: &[String]) {
    let mut out = String::from(
        "# md2x 自带模版播种记录（自动生成）：列在这里的模版不会再被自动写回。\n\
         # 想恢复某个自带模版：删掉本文件后重启应用即可重新铺开；\n\
         # 想保留自己的改动：把模版目录复制一份改名再改，原目录别动。\n",
    );
    for id in ids {
        out.push_str(id);
        out.push('\n');
    }
    let _ = std::fs::write(path, out);
}

/// 把缺失的自带模版逐个写入模版目录，已存在的一律跳过。
///
/// 两条硬约束：
/// - **目录已存在就不写**：里面可能是用户改过的版本，覆盖等于吞掉工作成果
/// - **删掉的不复活**：用户明确删掉的模版由播种记录记住，下次启动不再冒出来
fn seed_bundled_templates(dir: &Path) {
    let state_path = dir.join(SEED_STATE_FILE);
    let mut seeded = read_seed_state(&state_path);
    let mut dirty = false;
    for t in BUNDLED_TEMPLATES {
        let exists = dir.join(t.id).is_dir();
        let recorded = seeded.iter().any(|s| s == t.id);
        if exists || recorded {
            // 已存在（可能是用户改过的版本）或用户删过：都补记一笔，
            // 否则老版本种下的模版因为没有记录，删掉后仍会被写回来
            if !recorded {
                seeded.push(t.id.to_string());
                dirty = true;
            }
            continue;
        }
        let d = dir.join(t.id);
        if std::fs::create_dir_all(&d).is_err() {
            continue;
        }
        let _ = std::fs::write(d.join(template::TEMPLATE_HTML), t.html);
        let _ = std::fs::write(d.join(template::TEMPLATE_MANIFEST), t.manifest);
        let _ = std::fs::write(d.join(t.preview_name), t.preview);
        // 只记录真正落盘的：磁盘满等失败场景下次启动还能补写
        if d.join(template::TEMPLATE_HTML).is_file() {
            seeded.push(t.id.to_string());
            dirty = true;
        }
    }
    if dirty {
        write_seed_state(&state_path, &seeded);
    }
}

/// 确保模板目录存在、带说明文件与自带模版，返回目录路径。
fn ensure_templates_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let d = templates_dir(app)?;
    let readme = d.join("README.md");
    if !readme.exists() {
        let _ = std::fs::write(&readme, TEMPLATES_README);
    }
    seed_bundled_templates(&d);
    Ok(d)
}

/// 用户自定义模版变量的存档路径：`<app_data_dir>/template-vars.json`。
///
/// 刻意放在模版目录之外：模版目录是「作品」，可能是共享/可移植的一份；
/// 变量取值是「本机偏好」，混在一起会让用户复制模版给别人时把配色偏好也带走。
fn var_store_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录: {e}"))?;
    std::fs::create_dir_all(&base).map_err(|e| format!("创建应用数据目录失败: {e}"))?;
    Ok(base.join("template-vars.json"))
}

/// 读取变量存档：`{ "模版 id": { "变量 key": "取值" } }`。
///
/// 读取失败一律当作「没有任何自定义」——存档损坏不该让预览/导出失败，
/// 只是回到默认外观。
fn read_var_store(app: &tauri::AppHandle) -> HashMap<String, HashMap<String, String>> {
    match var_store_path(app) {
        Ok(p) => read_var_store_file(&p),
        Err(_) => HashMap::new(),
    }
}

/// 写回变量存档（空表也写，便于「重置后」不留残影）。
fn write_var_store(
    app: &tauri::AppHandle,
    store: &HashMap<String, HashMap<String, String>>,
) -> Result<(), String> {
    write_var_store_file(&var_store_path(app)?, store)
}

fn read_var_store_file(p: &Path) -> HashMap<String, HashMap<String, String>> {
    let Ok(raw) = std::fs::read_to_string(p) else {
        return HashMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_var_store_file(
    p: &Path,
    store: &HashMap<String, HashMap<String, String>>,
) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(store)
        .map_err(|e| format!("序列化模版变量失败: {e}"))?;
    std::fs::write(p, raw).map_err(|e| format!("写入模版变量失败: {e}"))
}

/// 按前端传入的模板 id 加载模板；目录不可用或 id 失效时回退内置。
///
/// 同时把该模版已保存的变量取值穿上——预览、PDF、导出、聚合全都走这里，
/// 因此自定义配色只需在这一处接一次，不会出现「预览生效、导出没生效」。
fn load_selected_template(
    app: &tauri::AppHandle,
    template_id: &Option<String>,
) -> template::HtmlTemplate {
    let id = template_id.clone().unwrap_or_default();
    if id.is_empty() || id == template::BUILTIN_TEMPLATE_ID {
        return template::HtmlTemplate::builtin();
    }
    match templates_dir(app) {
        Ok(dir) => {
            let tpl = template::load_template(&id, &dir);
            match read_var_store(app).remove(&id) {
                Some(values) => tpl.with_var_overrides(&values),
                None => tpl,
            }
        }
        Err(_) => template::HtmlTemplate::builtin(),
    }
}

/// 模版列表项（返回给设置面板的模版卡片）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TemplateInfo {
    /// 模版 id：内置为 "default"，外置为模版包目录名（旧式为文件名去扩展名）
    id: String,
    /// 展示名（内置项为兜底文案，前端按 id 本地化）
    name: String,
    /// 一句话描述
    description: String,
    /// 版本号（可能为空）
    version: String,
    /// 作者（可能为空）
    author: String,
    /// 是否内置模板
    builtin: bool,
    /// 是否为目录式模版包（false 表示旧式单文件模版）
    packaged: bool,
    /// 缩略图 data URL；空串表示无缩略图，由前端渲染占位图
    preview: String,
    /// 模版入口 HTML 路径（内置为空）
    path: String,
    /// 可调变量（含当前生效值）；空表示该模版没有可调项
    vars: Vec<TemplateVarInfo>,
}

/// 模版变量条目（返回给设置面板渲染控件）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TemplateVarInfo {
    /// 变量 key，改动后原样回传
    key: String,
    /// 控件标签
    label: String,
    /// 控件形态："color" / "text"
    kind: String,
    /// 默认值（前端用于占位提示与「重置」）
    default: String,
    /// 当前生效值（已含用户覆盖）
    value: String,
    /// 一行说明（可能为空）
    hint: String,
}

/// 文件树节点（目录/文件），递归描述文件夹内的 Markdown 文件
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TreeNode {
    name: String,
    /// 绝对路径，统一使用 / 分隔符（Windows 也兼容）
    path: String,
    is_dir: bool,
    /// 文件为空；目录为子节点列表（已剪除不含 .md 的空目录）
    children: Vec<TreeNode>,
}

struct CachedPreview {
    /// 生成时的文件 mtime
    file_mtime: SystemTime,
    /// 缓存的 PDF 文件路径
    pdf_path: PathBuf,
    /// 缓存的 HTML 文件路径
    html_path: PathBuf,
    /// 生成时的全宽开关，用于缓存命中判定
    full_width: bool,
    /// 生成时使用的模板 id：切换模板后旧缓存必须失效
    template_id: String,
}

struct AppState {
    current_file: Mutex<Option<PathBuf>>,
    /// 文件上次刷新时的 mtime，用于前端轮询检测变更
    last_known_mtime: Mutex<Option<SystemTime>>,
    /// 菜单「关于」点击标记，前端轮询消费
    show_about_flag: Mutex<bool>,
    /// 菜单「设置」点击标记，前端轮询消费
    settings_flag: Mutex<bool>,
    /// 当前打开的文件夹根目录（文件树视图），None 表示单文件视图
    folder_root: Mutex<Option<PathBuf>>,
    /// 顶级菜单「文件」/「视图」：切语言时要改标题，故需持有句柄
    file_menu: Mutex<Option<Submenu<tauri::Wry>>>,
    view_menu: Mutex<Option<Submenu<tauri::Wry>>>,
    about_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    settings_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    open_file_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    close_file_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    open_folder_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    close_folder_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    quit_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    /// 「显示目录树」勾选项：状态由前端文件树可见性决定，需双向同步
    toggle_tree_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    /// 「显示工具栏」勾选项：状态由前端 settings.toolbarVisible 决定，需双向同步
    toggle_toolbar_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    /// 「全宽显示」勾选项：状态由前端 settings.fullWidth 决定，需双向同步
    fullwidth_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    /// 「全屏」勾选项：勾选状态直接映射窗口全屏状态
    fullscreen_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    /// 最近一次同步给菜单的全屏状态，避免每次窗口尺寸变化都做一次跨线程设置
    fullscreen_state: Mutex<bool>,
    /// 菜单「打开文件」点击标记，前端轮询消费
    show_open_file_flag: Mutex<bool>,
    /// 菜单「关闭」点击标记，前端轮询消费
    close_file_flag: Mutex<bool>,
    /// 菜单「显示目录树」点击标记，前端轮询消费（真正切换与勾选由前端回写）
    toggle_tree_flag: Mutex<bool>,
    /// 菜单「显示工具栏」点击标记，前端轮询消费（真正切换与勾选由前端回写）
    toggle_toolbar_flag: Mutex<bool>,
    /// 菜单「全宽显示」点击标记，前端轮询消费（真正切换与勾选由前端回写）
    toggle_fullwidth_flag: Mutex<bool>,
    /// 菜单「打开文件夹」点击标记，前端轮询消费
    show_open_folder_flag: Mutex<bool>,
    /// 菜单「关闭文件夹」点击标记，前端轮询消费
    close_folder_flag: Mutex<bool>,
    /// 「导出 ▸」子菜单：切语言要改标题、无文件时要整组置灰，故需持有句柄
    export_menu: Mutex<Option<Submenu<tauri::Wry>>>,
    /// 「导出 ▸」各格式项：无文件/无文件夹时分别置灰
    export_folder_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    export_html_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    export_pdf_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    export_docx_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    /// 菜单「导出 ▸ X」请求的格式（html/pdf/docx/folder），前端轮询消费。
    /// 用 Option<String> 而不是四个 bool：一次点击只可能产生一个格式，
    /// 布尔组会让「同时置真」这种非法状态在类型上成立。
    /// 导出预设被点击时这里存放 `preset:<id>`，由前端查表得到完整参数
    export_request: Mutex<Option<String>>,
    /// 「导出 ▸」下动态挂载的预设项与它们上方的分隔线。
    /// 前端是预设的唯一来源（存 localStorage），每次变更整组重建：
    /// 保留这些句柄才能把上一批 remove 掉，否则改一次名就多一行。
    preset_items: Mutex<Vec<MenuItem<tauri::Wry>>>,
    preset_sep: Mutex<Option<PredefinedMenuItem<tauri::Wry>>>,
    /// 缓存的预览结果（PDF + HTML），文件没变时复用
    cached_preview: Mutex<Option<CachedPreview>>,
}

/// 拖拽事件通道：前端 async wait_for_drop 通过 oneshot 接收文件路径
struct DropChannel {
    tx: Mutex<Option<oneshot::Sender<String>>>,
}

#[derive(Serialize)]
struct PreviewResult {
    base64: String,
    temp_path: String,
    file_name: String,
}

pub fn run() {
    let f = std::env::var("MD2X_GUI_FILE").ok();
    let s = AppState {
        current_file: Mutex::new(f.as_ref().map(PathBuf::from)),
        last_known_mtime: Mutex::new(None),
        show_about_flag: Mutex::new(false),
        settings_flag: Mutex::new(false),
        folder_root: Mutex::new(
            std::env::var("MD2X_GUI_FOLDER")
                .ok()
                .map(PathBuf::from),
        ),
        about_item: Mutex::new(None),
        file_menu: Mutex::new(None),
        view_menu: Mutex::new(None),
        settings_item: Mutex::new(None),
        open_file_item: Mutex::new(None),
        close_file_item: Mutex::new(None),
        open_folder_item: Mutex::new(None),
        close_folder_item: Mutex::new(None),
        quit_item: Mutex::new(None),
        toggle_tree_item: Mutex::new(None),
        toggle_toolbar_item: Mutex::new(None),
        fullwidth_item: Mutex::new(None),
        fullscreen_item: Mutex::new(None),
        fullscreen_state: Mutex::new(false),
        show_open_file_flag: Mutex::new(false),
        close_file_flag: Mutex::new(false),
        toggle_tree_flag: Mutex::new(false),
        toggle_toolbar_flag: Mutex::new(false),
        toggle_fullwidth_flag: Mutex::new(false),
        show_open_folder_flag: Mutex::new(false),
        close_folder_flag: Mutex::new(false),
        export_menu: Mutex::new(None),
        export_folder_item: Mutex::new(None),
        export_html_item: Mutex::new(None),
        export_pdf_item: Mutex::new(None),
        export_docx_item: Mutex::new(None),
        export_request: Mutex::new(None),
        preset_items: Mutex::new(Vec::new()),
        preset_sep: Mutex::new(None),
        cached_preview: Mutex::new(None),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(s)
        .manage(DropChannel {
            tx: Mutex::new(None),
        })
        .on_window_event(|w, e| {
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = e {
                if let Some(p) = paths.first() {
                    let path = p.to_string_lossy().to_string();
                    // 通知前端等待的 wait_for_drop
                    if let Some(state) = w.try_state::<DropChannel>() {
                        if let Ok(mut guard) = state.tx.lock() {
                            if let Some(tx) = guard.take() {
                                let _ = tx.send(path);
                            }
                        }
                    }
                }
            }
            // 全屏状态也可能被系统操作改变（Esc、macOS 绿灯按钮），
            // 窗口尺寸随之变化时回读一次，保证菜单勾选与实际一致；
            // 变化时再广播出去，预览页的浮动按钮图标才不会和窗口状态脱节
            if let tauri::WindowEvent::Resized(_) = e {
                if let Ok(fs) = w.is_fullscreen() {
                    if sync_fullscreen_check(w.app_handle(), fs) {
                        let _ = w.app_handle().emit(FULLSCREEN_EVENT, fs);
                    }
                }
            }
        })
        .setup(|app| {
            // ===== 应用菜单：只保留「关于」「退出」 =====
            let about_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("about", "关于").build(app)?;
            let quit_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("quit", "退出")
                    .accelerator("CmdOrCtrl+Q")
                    .build(app)?;

            // ===== 文件菜单：打开 / 关闭 =====
            let open_file_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("open_file", "打开…")
                    .accelerator("CmdOrCtrl+O")
                    .build(app)?;
            let open_folder_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("open_folder", "打开文件夹…")
                    .accelerator("CmdOrCtrl+Shift+O")
                    .build(app)?;
            let close_file_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("close_file", "关闭")
                    .accelerator("CmdOrCtrl+W")
                    .build(app)?;
            let close_folder_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("close_folder", "关闭文件夹")
                    .accelerator("CmdOrCtrl+Shift+W")
                    .build(app)?;

            // ===== 文件 → 导出 子菜单 =====
            // 与工具栏导出按钮、预览区右键菜单同一组格式、同一条后端链路；
            // 不加加速键：导出是有副作用的写盘操作，误触就地弹保存框的体验更差
            let export_folder_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("export_folder", "文件夹 → 单个 HTML")
                    .build(app)?;
            let export_html_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("export_html", "HTML").build(app)?;
            let export_pdf_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("export_pdf", "PDF").build(app)?;
            let export_docx_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("export_docx", "Word（docx）").build(app)?;
            let export_menu: Submenu<tauri::Wry> = tauri::menu::SubmenuBuilder::new(app, "导出")
                .item(&export_folder_item)
                .separator()
                .item(&export_html_item)
                .item(&export_pdf_item)
                .item(&export_docx_item)
                .build()?;

            // ===== 视图菜单：工具栏 / 目录树 / 全宽 / 全屏 / 设置 =====
            // 工具栏勾选状态由前端（settings.toolbarVisible）回写，默认隐藏
            // （工具栏收起来后正文区域更大，纯阅读场景才是主用法）。
            // 刻意不设加速键：它与目录树的 ⌘B 属于同一层级的功能开关，
            // 再抢一个组合键反而会和常用编辑快捷键冲突
            let toggle_toolbar_item: CheckMenuItem<tauri::Wry> =
                tauri::menu::CheckMenuItemBuilder::with_id("toggle_toolbar", "显示工具栏")
                    .checked(false)
                    .build(app)?;
            // 目录树勾选状态由前端（文件树可见性）回写，这里给初始值 true（默认展开）
            let toggle_tree_item: CheckMenuItem<tauri::Wry> =
                tauri::menu::CheckMenuItemBuilder::with_id("toggle_tree", "显示目录树")
                    .checked(true)
                    .accelerator("CmdOrCtrl+B")
                    .build(app)?;
            // 全宽显示：与工具栏那个按钮同一份状态（settings.fullWidth），
            // 初始值由前端挂载时回写，这里先给 false
            let fullwidth_item: CheckMenuItem<tauri::Wry> =
                tauri::menu::CheckMenuItemBuilder::with_id("toggle_fullwidth", "全宽显示")
                    .checked(false)
                    .build(app)?;
            // 全屏加速键按平台区分：macOS 沿用系统惯例 ⌃⌘F，其余平台 F11
            let fullscreen_accel = if cfg!(target_os = "macos") {
                "CTRL+CMD+F"
            } else {
                "F11"
            };
            let fullscreen_item: CheckMenuItem<tauri::Wry> =
                tauri::menu::CheckMenuItemBuilder::with_id("fullscreen", "全屏")
                    .checked(false)
                    .accelerator(fullscreen_accel)
                    .build(app)?;
            let settings_item: MenuItem<tauri::Wry> =
                tauri::menu::MenuItemBuilder::with_id("settings", "设置…")
                    .accelerator("CmdOrCtrl+,")
                    .build(app)?;

            let app_menu = tauri::menu::SubmenuBuilder::new(app, "md2x")
                .item(&about_item)
                .separator()
                .item(&quit_item)
                .build()?;
            let file_menu: Submenu<tauri::Wry> = tauri::menu::SubmenuBuilder::new(app, "文件")
                .item(&open_file_item)
                .item(&open_folder_item)
                .separator()
                .item(&export_menu)
                .separator()
                .item(&close_file_item)
                .item(&close_folder_item)
                .build()?;
            let view_menu: Submenu<tauri::Wry> = tauri::menu::SubmenuBuilder::new(app, "视图")
                .item(&toggle_toolbar_item)
                .item(&toggle_tree_item)
                .item(&fullwidth_item)
                .item(&fullscreen_item)
                .separator()
                .item(&settings_item)
                .build()?;

            let menu = tauri::menu::MenuBuilder::new(app)
                .items(&[&app_menu, &file_menu, &view_menu])
                .build()?;
            app.set_menu(menu)?;
            // 保存菜单项引用供语言切换与状态同步
            if let Some(state) = app.try_state::<AppState>() {
                *state.about_item.lock().unwrap() = Some(about_item);
                *state.settings_item.lock().unwrap() = Some(settings_item);
                *state.open_file_item.lock().unwrap() = Some(open_file_item);
                *state.close_file_item.lock().unwrap() = Some(close_file_item);
                *state.open_folder_item.lock().unwrap() = Some(open_folder_item);
                *state.close_folder_item.lock().unwrap() = Some(close_folder_item);
                *state.quit_item.lock().unwrap() = Some(quit_item);
                *state.toggle_toolbar_item.lock().unwrap() = Some(toggle_toolbar_item);
                *state.toggle_tree_item.lock().unwrap() = Some(toggle_tree_item);
                *state.fullwidth_item.lock().unwrap() = Some(fullwidth_item);
                *state.fullscreen_item.lock().unwrap() = Some(fullscreen_item);
                *state.file_menu.lock().unwrap() = Some(file_menu);
                *state.view_menu.lock().unwrap() = Some(view_menu);
                *state.export_menu.lock().unwrap() = Some(export_menu);
                *state.export_folder_item.lock().unwrap() = Some(export_folder_item);
                *state.export_html_item.lock().unwrap() = Some(export_html_item);
                *state.export_pdf_item.lock().unwrap() = Some(export_pdf_item);
                *state.export_docx_item.lock().unwrap() = Some(export_docx_item);
            }
            // 启动参数可能直接带文件/文件夹（双击 .md、`md2x --folder`），
            // 此时窗口还停在配置里的默认标题，需要按实际打开内容纠正一次
            if let Some(state) = app.try_state::<AppState>() {
                if let Some(win) = app.get_webview_window("main") {
                    apply_window_title(&win, &state);
                }
            }
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "about" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut flag) = state.show_about_flag.lock() {
                        *flag = true;
                    }
                }
            }
            "settings" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut flag) = state.settings_flag.lock() {
                        *flag = true;
                    }
                }
            }
            "open_file" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut flag) = state.show_open_file_flag.lock() {
                        *flag = true;
                    }
                }
            }
            "close_file" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut flag) = state.close_file_flag.lock() {
                        *flag = true;
                    }
                }
            }
            "toggle_tree" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut flag) = state.toggle_tree_flag.lock() {
                        *flag = true;
                    }
                }
            }
            "toggle_toolbar" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut flag) = state.toggle_toolbar_flag.lock() {
                        *flag = true;
                    }
                }
            }
            "toggle_fullwidth" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut flag) = state.toggle_fullwidth_flag.lock() {
                        *flag = true;
                    }
                }
            }
            "fullscreen" => {
                // 全屏由后端直接翻转窗口状态，无需前端参与；
                // 勾选状态以窗口实际状态为准（Esc / 绿灯退出时也能纠正）
                if let Some(win) = app.get_webview_window("main") {
                    let next = !win.is_fullscreen().unwrap_or(false);
                    apply_fullscreen(&win, next);
                }
            }
            "open_folder" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut flag) = state.show_open_folder_flag.lock() {
                        *flag = true;
                    }
                }
            }
            "close_folder" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut flag) = state.close_folder_flag.lock() {
                        *flag = true;
                    }
                }
            }
            "quit" => {
                app.exit(0);
            }
            // 导出子菜单：只记录请求，真正的保存对话框与写盘都在前端
            // （与工具栏按钮、预览右键菜单共用同一实现，避免两条导出链路跑偏）。
            // 预设项走同一个通道，存 `preset:<id>`，参数由前端查本地预设表
            id => {
                let Some(req) = menu_export_request(id) else {
                    return;
                };
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut slot) = state.export_request.lock() {
                        *slot = Some(req);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            set_file,
            get_html,
            read_file_chunk,
            preview_pdf,
            save_pdf_as,
            export_html,
            export_folder_html,
            export_pdf,
            export_docx,
            get_file_name,
            get_file_path,
            wait_for_drop,
            check_file_changed,
            get_platform,
            get_app_info,
            check_show_about,
            check_show_settings,
            check_show_open_file,
            check_show_close_file,
            check_toggle_tree,
            check_toggle_toolbar,
            check_toggle_fullwidth,
            toggle_fullscreen,
            sync_menu_state,
            set_menu_language,
            open_folder,
            close_folder,
            clear_file,
            get_folder_path,
            path_kind,
            check_show_open_folder,
            check_show_close_folder,
            check_menu_export,
            set_export_presets,
            is_fullscreen,
            register_default_md_handler,
            list_templates,
            set_template_vars,
            open_templates_dir,
            duplicate_template,
            check_external_tools,
            repo_relative_path,
            copy_text,
        ])
        .build(tauri::generate_context!())
        .expect("error")
        .run(|handle, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = event {
                for url in urls {
                    if url.scheme() == "file" {
                        if let Ok(path) = url.to_file_path() {
                            let path_str = path.to_string_lossy().to_string();
                            let is_dir = path.is_dir();
                            // 文件记录为当前文件；文件夹记录为文件树根目录
                            if let Some(state) = handle.try_state::<AppState>() {
                                if is_dir {
                                    if let Ok(mut cur) = state.folder_root.lock() {
                                        *cur = Some(path);
                                    }
                                } else if let Ok(mut cur) = state.current_file.lock() {
                                    *cur = Some(path);
                                }
                            }
                            if let Some(state) = handle.try_state::<DropChannel>() {
                                if let Ok(mut guard) = state.tx.lock() {
                                    if let Some(tx) = guard.take() {
                                        let _ = tx.send(path_str);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (handle, event);
        });
}

/// CLI 模式：静默生成 PDF
pub fn generate_pdf_from_file(
    path: &Path,
    full_width: bool,
    tpl: Option<&template::HtmlTemplate>,
) -> Result<PathBuf, MpeError> {
    let md = std::fs::read_to_string(path)?;
    let is_skill = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n == "SKILL.md")
        .unwrap_or(false);
    let (metadata, body_md) = if is_skill {
        converter::parse_front_matter(&md)
    } else {
        (None, &md[..])
    };
    let h = converter::convert_markdown_to_html_with_mermaid(body_md)?;
    let h = converter::resolve_image_srcs(&h, path);
    let t = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");
    // PDF 走 @media print，模板在打印样式下恒为浅色（纸是白的），故此处不传主题
    let f = match tpl {
        Some(tpl) => template::render_html_with(tpl, &h, t, metadata.as_ref(), full_width, template::Theme::Auto),
        None => template::render_html_template_with_metadata(&h, t, metadata.as_ref(), full_width, template::Theme::Auto),
    };

    let d = std::env::temp_dir().join("rust-mpe-browser");
    std::fs::create_dir_all(&d)?;
    let n = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let hp = d.join(format!("{}.html", n));
    std::fs::write(&hp, &f)?;

    let pp = path.with_extension("pdf");
    chrome::generate_pdf(&hp.to_string_lossy(), &pp.to_string_lossy())?;
    let _ = std::fs::remove_file(&hp);
    Ok(pp)
}

// ==================== IPC 命令 ====================

/// 等待用户拖拽文件（异步，不轮询）
#[tauri::command]
async fn wait_for_drop(state: State<'_, DropChannel>) -> Result<String, String> {
    let (tx, rx) = oneshot::channel();
    {
        let mut guard = state.tx.lock().map_err(|e| e.to_string())?;
        *guard = Some(tx);
    }
    rx.await.map_err(|_| "操作已取消".to_string())
}

/// 读取文件分块（用于大文件流式加载）
#[tauri::command]
fn read_file_chunk(path: String, offset: usize) -> Result<(String, bool), String> {
    let data = std::fs::read(&path).map_err(|e| e.to_string())?;
    if offset >= data.len() {
        return Ok((String::new(), true));
    }
    let end = std::cmp::min(offset + 524288, data.len());
    Ok((
        String::from_utf8_lossy(&data[offset..end]).to_string(),
        end >= data.len(),
    ))
}

/// 设置当前文件，记录初始 mtime 供前端轮询检测变更
#[tauri::command]
fn set_file(path: String, s: State<AppState>, window: tauri::WebviewWindow) -> Result<(), String> {
    // 拖拽与 macOS 的「用 md2x 打开」都可能传 file URL：
    // Windows 下是 file:///C:/x，直接 strip_prefix 会得到 "/C:/x" 而判定不存在
    let p = match file_url_to_path(&path) {
        Some(p) => p,
        None => PathBuf::from(&path),
    };
    if !p.exists() {
        return Err(format!("文件不存在: {}", p.display()));
    }
    s.current_file
        .lock()
        .map_err(|e| e.to_string())?
        .replace(p.clone());

    // 记录当前 mtime
    let mtime = std::fs::metadata(&p).ok().and_then(|m| m.modified().ok());
    *s.last_known_mtime.lock().map_err(|e| e.to_string())? = mtime;

    // 清空预览缓存（文件已切换）
    *s.cached_preview.lock().map_err(|e| e.to_string())? = None;

    // 标题跟着当前文件走（放在最后：前面任何一步失败都不该改标题）
    apply_window_title(&window, &s);

    Ok(())
}

/// 获取当前运行平台（macos / windows / linux）
#[tauri::command]
fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

/// 获取应用信息（版本号、版权）
#[derive(Serialize)]
struct AppInfo {
    version: String,
    copyright: String,
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        copyright: "yujinping8620@qq.com".to_string(),
    }
}

/// 前端轮询：检查菜单「关于」是否被点击
#[tauri::command]
fn check_show_about(s: State<AppState>) -> bool {
    let mut guard = s.show_about_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 前端轮询：检查菜单「设置」是否被点击
#[tauri::command]
fn check_show_settings(s: State<AppState>) -> bool {
    let mut guard = s.settings_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 前端轮询：检查菜单「文件 → 打开…」是否被点击
#[tauri::command]
fn check_show_open_file(s: State<AppState>) -> bool {
    let mut guard = s.show_open_file_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 前端轮询：检查菜单「文件 → 关闭」是否被点击
#[tauri::command]
fn check_show_close_file(s: State<AppState>) -> bool {
    let mut guard = s.close_file_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 前端轮询：检查菜单「视图 → 显示目录树」是否被点击
#[tauri::command]
fn check_toggle_tree(s: State<AppState>) -> bool {
    let mut guard = s.toggle_tree_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 前端轮询：检查菜单「视图 → 显示工具栏」是否被点击
#[tauri::command]
fn check_toggle_toolbar(s: State<AppState>) -> bool {
    let mut guard = s.toggle_toolbar_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 前端轮询：检查菜单「视图 → 全宽显示」是否被点击
#[tauri::command]
fn check_toggle_fullwidth(s: State<AppState>) -> bool {
    let mut guard = s.toggle_fullwidth_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 菜单项 id → 导出格式串（前端 `export_<格式>` 命令的后缀）。
///
/// 单独抽成纯函数是为了能单测：id 与格式的对应关系一旦写错，
/// 「点了 PDF 却导出 docx」在界面上极难一眼发现，而重定向到不存在的
/// 命令还会静默失败成一条状态栏错误。
fn export_format_of(id: &str) -> Option<&'static str> {
    match id {
        "export_folder" => Some("folder"),
        "export_html" => Some("html"),
        "export_pdf" => Some("pdf"),
        "export_docx" => Some("docx"),
        _ => None,
    }
}

/// 前端轮询：检查菜单「文件 → 导出 ▸ X」是否被点击。
/// 返回格式字符串（html/pdf/docx/folder），或预设请求 `preset:<id>`；无请求时返回 None
#[tauri::command]
fn check_menu_export(s: State<AppState>) -> Option<String> {
    s.export_request.lock().ok().and_then(|mut req| req.take())
}

/// 导出预设菜单项 id 前缀：`preset:<预设 id>` 与固定格式项共用一条请求通道，
/// 前端据前缀决定是「直接导出该格式」还是「先套用预设再导出」。
const PRESET_ID_PREFIX: &str = "preset:";

/// 菜单项 id → 前端导出请求（`html` / `folder` / `preset:xxx`）。
///
/// 预设项与格式项混在同一段 `match` 兜底分支里，若不在这里统一收口，
/// 新增的预设项会被当成未知 id 静默丢弃——点了菜单什么都不发生。
fn menu_export_request(id: &str) -> Option<String> {
    if let Some(rest) = id.strip_prefix(PRESET_ID_PREFIX) {
        return if rest.is_empty() {
            None
        } else {
            Some(id.to_string())
        };
    }
    export_format_of(id).map(|s| s.to_string())
}

/// 前端同步过来的预设菜单项：只需要 id 与显示名。
/// 格式/模版/主题等参数留在前端——菜单只负责发出「哪个预设」这一个信号。
#[derive(Debug, Deserialize)]
struct ExportPresetMenuItem {
    id: String,
    name: String,
}

/// 按前端当前的预设列表重建「文件 → 导出」下的预设项。
///
/// 整组重建而不是增量更新：预设可改名、可删除、可调整顺序，
/// 增量对账的代码量更大且更容易残留旧行。
///
/// 注：菜单对象的创建与增删在 macOS 上要求主线程，tauri 内部已统一
/// 用 `run_on_main_thread` 派发，因此这里在命令线程里直接改是安全的；
/// 但也正因如此它进不了单测（Rust 测试跑在工作线程，muda 会直接 panic）。
#[tauri::command]
fn set_export_presets(
    app: tauri::AppHandle,
    presets: Vec<ExportPresetMenuItem>,
    s: State<AppState>,
) -> Result<(), String> {
    let menu = s.export_menu.lock().map_err(|e| e.to_string())?;
    let Some(ref submenu) = *menu else {
        return Ok(());
    };
    // 先摘掉上一批（含分隔线），否则改名会留下旧名字那一行
    {
        let mut items = s.preset_items.lock().map_err(|e| e.to_string())?;
        for it in items.drain(..) {
            submenu.remove(&it).ok();
        }
    }
    {
        let mut sep = s.preset_sep.lock().map_err(|e| e.to_string())?;
        if let Some(it) = sep.take() {
            submenu.remove(&it).ok();
        }
    }

    // 先建后挂：全部非法时直接返回，不留一条悬空分隔线
    let mut created: Vec<MenuItem<tauri::Wry>> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for p in &presets {
        let label = p.name.trim();
        if label.is_empty() || p.id.trim().is_empty() || seen.contains(&p.id.as_str()) {
            continue;
        }
        seen.push(&p.id);
        // 菜单宽度受名称长度影响，过长的名字截断而不是让整列变宽
        let label: String = label.chars().take(24).collect();
        let item: MenuItem<tauri::Wry> =
            tauri::menu::MenuItemBuilder::with_id(format!("{PRESET_ID_PREFIX}{}", p.id), label)
                .build(&app)
                .map_err(|e| format!("创建预设菜单项失败: {e}"))?;
        created.push(item);
    }
    if created.is_empty() {
        return Ok(());
    }

    let sep = PredefinedMenuItem::separator(&app).map_err(|e| format!("创建分隔线失败: {e}"))?;
    submenu.append(&sep).map_err(|e| e.to_string())?;
    for it in &created {
        submenu.append(it).map_err(|e| e.to_string())?;
    }
    *s.preset_items.lock().map_err(|e| e.to_string())? = created;
    *s.preset_sep.lock().map_err(|e| e.to_string())? = Some(sep);
    Ok(())
}

/// 前端回写菜单状态：目录树/工具栏可见性、全宽状态与勾选保持一致。
/// 前端是这些状态的唯一来源（工具栏按钮、全宽模式、打开文件夹、设置项都会改变它们），
/// 因此每次变化都需要把结果同步回菜单，否则勾选会与实际显示不符。
#[tauri::command]
fn sync_menu_state(
    tree_visible: bool,
    tree_enabled: bool,
    full_width: bool,
    toolbar_visible: bool,
    has_file: bool,
    has_folder: bool,
    s: State<AppState>,
) -> Result<(), String> {
    {
        let item = s.toggle_tree_item.lock().map_err(|e| e.to_string())?;
        if let Some(ref it) = *item {
            it.set_enabled(tree_enabled).ok();
            it.set_checked(tree_visible && tree_enabled).ok();
        }
    }
    {
        let item = s.fullwidth_item.lock().map_err(|e| e.to_string())?;
        if let Some(ref it) = *item {
            it.set_checked(full_width).ok();
        }
    }
    {
        let item = s.toggle_toolbar_item.lock().map_err(|e| e.to_string())?;
        if let Some(ref it) = *item {
            it.set_checked(toolbar_visible).ok();
        }
    }
    // 导出组：没有可导出的内容时整组置灰，比点了再报错更符合预期；
    // 「文件夹 → 单个 HTML」只在确实打开了文件夹时可用
    {
        let item = s.export_menu.lock().map_err(|e| e.to_string())?;
        if let Some(ref it) = *item {
            it.set_enabled(has_file || has_folder).ok();
        }
    }
    for (slot, enabled) in [
        (&s.export_html_item, has_file),
        (&s.export_pdf_item, has_file),
        (&s.export_docx_item, has_file),
        (&s.export_folder_item, has_folder),
    ] {
        let item = slot.lock().map_err(|e| e.to_string())?;
        if let Some(ref it) = *item {
            it.set_enabled(enabled).ok();
        }
    }
    Ok(())
}

/// 把全屏状态同步到菜单勾选项；仅在状态变化时写入，避免窗口拖拽缩放期间反复跨线程调用。
///
/// 返回是否发生了变化——调用方据此决定要不要再往前端广播一次，
/// 免得每次拖动窗口尺寸都发一条无意义的事件。
fn sync_fullscreen_check(app: &tauri::AppHandle, fullscreen: bool) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let changed = state
        .fullscreen_state
        .lock()
        .map(|mut cur| {
            let changed = *cur != fullscreen;
            *cur = fullscreen;
            changed
        })
        .unwrap_or(true);
    if !changed {
        return false;
    }
    // 先把菜单项取出（克隆），避免后续借用与 state 生命周期缠绕
    let item = state
        .fullscreen_item
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    if let Some(it) = item {
        it.set_checked(fullscreen).ok();
    }
    true
}

/// 全屏状态变化事件名：前端据此把状态回写给预览 iframe 里的浮动按钮
const FULLSCREEN_EVENT: &str = "md2x://fullscreen";

/// 切换窗口全屏并广播状态。
///
/// 菜单项、⌃⌘F 加速键、预览页浮动按钮三处入口共用，避免各写一遍取反/同步/广播；
/// 失败时返回原状态（`!next`），调用方无需再判断。
fn apply_fullscreen(window: &tauri::WebviewWindow, next: bool) -> bool {
    if window.set_fullscreen(next).is_err() {
        return !next;
    }
    sync_fullscreen_check(window.app_handle(), next);
    let _ = window.app_handle().emit(FULLSCREEN_EVENT, next);
    next
}

/// 预览页浮动按钮转交的全屏请求。
///
/// md2x 的预览跑在 `<iframe srcdoc>` 里，而 WebView 本身（macOS 12 的 WKWebView）
/// 没有 `Element.requestFullscreen`，iframe 内又受权限策略限制——所以「让文档全屏」
/// 这条路在应用内不可用。改为切换**整个窗口**的全屏，与菜单项、⌃⌘F 走同一条路径，
/// 勾选状态也由同一处同步。返回切换后的状态，前端据此回写 iframe 里的按钮图标。
#[tauri::command]
fn toggle_fullscreen(window: tauri::WebviewWindow) -> bool {
    let next = !window.is_fullscreen().unwrap_or(false);
    apply_fullscreen(&window, next)
}

/// 读取当前窗口全屏状态。
///
/// 前端启动时要按真实状态决定底部状态栏是否显示——窗口若是带着全屏配置启动，
/// 不会有任何 Resized 事件，光等 `md2x://fullscreen` 广播会漏掉初始态。
#[tauri::command]
fn is_fullscreen(window: tauri::WebviewWindow) -> bool {
    window.is_fullscreen().unwrap_or(false)
}

/// 前端轮询：检查菜单「打开文件夹」是否被点击
#[tauri::command]
fn check_show_open_folder(s: State<AppState>) -> bool {
    let mut guard = s.show_open_folder_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 前端轮询：检查菜单「关闭文件夹」是否被点击
#[tauri::command]
fn check_show_close_folder(s: State<AppState>) -> bool {
    let mut guard = s.close_folder_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 切换菜单文本语言
/// 标签集中在此处以「中文 / English」成对维护，新增菜单项只需追加一行 `set_text!`
#[tauri::command]
fn set_menu_language(lang: String, s: State<AppState>) -> Result<(), String> {
    let en = lang == "en";
    macro_rules! set_text {
        ($slot:expr, $zh:literal, $en_text:literal) => {
            if let Some(ref it) = *$slot {
                it.set_text(if en { $en_text } else { $zh }).ok();
            }
        };
    }

    let file_menu = s.file_menu.lock().map_err(|e| e.to_string())?;
    let view_menu = s.view_menu.lock().map_err(|e| e.to_string())?;
    let about = s.about_item.lock().map_err(|e| e.to_string())?;
    let quit = s.quit_item.lock().map_err(|e| e.to_string())?;
    let open_file = s.open_file_item.lock().map_err(|e| e.to_string())?;
    let close_file = s.close_file_item.lock().map_err(|e| e.to_string())?;
    let open_folder = s.open_folder_item.lock().map_err(|e| e.to_string())?;
    let close_folder = s.close_folder_item.lock().map_err(|e| e.to_string())?;
    let export_menu = s.export_menu.lock().map_err(|e| e.to_string())?;
    let export_folder = s.export_folder_item.lock().map_err(|e| e.to_string())?;
    let export_html = s.export_html_item.lock().map_err(|e| e.to_string())?;
    let export_pdf = s.export_pdf_item.lock().map_err(|e| e.to_string())?;
    let export_docx = s.export_docx_item.lock().map_err(|e| e.to_string())?;
    let toggle_tree = s.toggle_tree_item.lock().map_err(|e| e.to_string())?;
    let toggle_toolbar = s.toggle_toolbar_item.lock().map_err(|e| e.to_string())?;
    let fullwidth = s.fullwidth_item.lock().map_err(|e| e.to_string())?;
    let fullscreen = s.fullscreen_item.lock().map_err(|e| e.to_string())?;
    let settings = s.settings_item.lock().map_err(|e| e.to_string())?;

    // 顶级菜单
    set_text!(file_menu, "文件", "File");
    set_text!(view_menu, "视图", "View");
    // 应用菜单
    set_text!(about, "关于", "About");
    set_text!(quit, "退出", "Quit");
    // 文件菜单
    set_text!(open_file, "打开…", "Open…");
    set_text!(open_folder, "打开文件夹…", "Open Folder…");
    set_text!(close_file, "关闭", "Close");
    set_text!(close_folder, "关闭文件夹", "Close Folder");
    // 文件 → 导出
    set_text!(export_menu, "导出", "Export");
    set_text!(export_folder, "文件夹 → 单个 HTML", "Folder → Single HTML");
    set_text!(export_html, "HTML", "HTML");
    set_text!(export_pdf, "PDF", "PDF");
    set_text!(export_docx, "Word（docx）", "Word (DOCX)");
    // 视图菜单
    set_text!(toggle_toolbar, "显示工具栏", "Show Toolbar");
    set_text!(toggle_tree, "显示目录树", "Show Directory Tree");
    set_text!(fullwidth, "全宽显示", "Full Width");
    set_text!(fullscreen, "全屏", "Full Screen");
    set_text!(settings, "设置…", "Settings…");
    Ok(())
}

/// 前端轮询调用：检查当前文件 mtime 是否有变化
#[tauri::command]
fn check_file_changed(s: State<AppState>) -> Result<bool, String> {
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = match cur.as_ref() {
        Some(p) => p.clone(),
        None => return Ok(false),
    };
    drop(cur);

    let current_mtime = std::fs::metadata(&p).ok().and_then(|m| m.modified().ok());

    let mut stored = s.last_known_mtime.lock().map_err(|e| e.to_string())?;
    match (stored.as_ref(), current_mtime.as_ref()) {
        (Some(last), Some(cur)) if cur > last => {
            // 更新记录
            *stored = current_mtime;
            Ok(true)
        }
        (None, Some(_)) => {
            // 首次获取到 mtime
            *stored = current_mtime;
            Ok(false)
        }
        _ => Ok(false),
    }
}

/// 生成 HTML 预览（写入临时文件，返回路径）
/// 如果缓存命中且文件未变更，直接返回缓存的 HTML 路径
#[tauri::command]
fn get_html(
    theme: Option<String>,
    template_id: Option<String>,
    app: tauri::AppHandle,
    s: State<AppState>,
) -> Result<String, String> {
    // 全宽由前端在拿到 HTML 后注入 CSS 实现，后端始终渲染标准宽度
    let full_width = false;
    let theme = theme_arg::parse_opt(&theme);
    let tpl = load_selected_template(&app, &template_id);
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = cur.as_ref().ok_or_else(|| "No file".to_string())?.clone();
    let file_stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("preview")
        .to_string();
    drop(cur);

    // 获取当前文件 mtime
    let current_mtime = std::fs::metadata(&p).ok().and_then(|m| m.modified().ok());

    // 检查缓存：mtime 一致且 HTML 文件存在，且模板未切换
    if let Some(ref mtime) = current_mtime {
        let cache = s.cached_preview.lock().map_err(|e| e.to_string())?;
        if let Some(ref cached) = *cache {
            if cached.file_mtime == *mtime
                && cached.full_width == full_width
                && cached.template_id == tpl.cache_key()
                && cached.html_path.exists()
            {
                return Ok(cached.html_path.to_string_lossy().to_string());
            }
        }
    }

    // 缓存未命中，重新生成 HTML
    let md = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let is_skill = p
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n == "SKILL.md")
        .unwrap_or(false);
    let (metadata, body_md) = if is_skill {
        converter::parse_front_matter(&md)
    } else {
        (None, &md[..])
    };
    let hb =
        converter::convert_markdown_to_html_with_mermaid(body_md).map_err(|e| e.to_string())?;
    let hb = converter::resolve_image_srcs(&hb, &p);
    let t = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled");
    let fh = template::render_html_template_with_metadata(&hb, t, metadata.as_ref(), full_width, theme);

    let d = std::env::temp_dir().join("rust-mpe-browser");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let hp = d.join(format!("{}.html", file_stem));
    std::fs::write(&hp, &fh).map_err(|e| e.to_string())?;

    // 更新缓存中的 HTML 路径（不覆盖 PDF 缓存）
    if let Some(mtime) = current_mtime {
        let mut cache = s.cached_preview.lock().map_err(|e| e.to_string())?;
        match cache.as_mut() {
            Some(existing) => {
                existing.html_path = hp.clone();
                existing.pdf_path = PathBuf::new(); // 清除 PDF 缓存，强制重新生成
                existing.file_mtime = mtime;
                existing.full_width = full_width;
                existing.template_id = tpl.cache_key();
            }
            None => {
                *cache = Some(CachedPreview {
                    file_mtime: mtime,
                    pdf_path: PathBuf::new(), // 尚无 PDF
                    html_path: hp.clone(),
                    full_width,
                    template_id: tpl.cache_key(),
                });
            }
        }
    }

    Ok(hp.to_string_lossy().to_string())
}

/// 生成 PDF（通过 Chrome headless）并返回 base64 + 临时路径
/// 如果缓存命中且文件未变更，直接返回缓存的 PDF（不重新生成）
#[tauri::command(rename_all = "camelCase")]
fn preview_pdf(
    full_width: Option<bool>,
    theme: Option<String>,
    template_id: Option<String>,
    app: tauri::AppHandle,
    s: State<AppState>,
) -> Result<PreviewResult, String> {
    let full_width = full_width.unwrap_or(false);
    let theme = theme_arg::parse_opt(&theme);
    let tpl = load_selected_template(&app, &template_id);
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = cur.as_ref().ok_or_else(|| "No file".to_string())?.clone();
    let file_stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();
    drop(cur);

    // 获取当前文件 mtime
    let current_mtime = std::fs::metadata(&p)
        .ok()
        .and_then(|m| m.modified().ok())
        .ok_or_else(|| "无法获取文件修改时间".to_string())?;

    // 检查缓存：mtime 一致且 PDF 文件存在
    {
        let cache = s.cached_preview.lock().map_err(|e| e.to_string())?;
        if let Some(ref cached) = *cache {
            if cached.file_mtime == current_mtime
                && cached.full_width == full_width
                && cached.template_id == tpl.cache_key()
                && cached.pdf_path.exists()
            {
                let pd =
                    std::fs::read(&cached.pdf_path).map_err(|e| format!("读 PDF 失败: {}", e))?;
                return Ok(PreviewResult {
                    base64: b64(&pd),
                    temp_path: cached.pdf_path.to_string_lossy().to_string(),
                    file_name: format!("{}.pdf", file_stem),
                });
            }
        }
    }

    // 缓存未命中，重新生成 HTML + PDF
    let md = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let is_skill = p
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n == "SKILL.md")
        .unwrap_or(false);
    let (metadata, body_md) = if is_skill {
        converter::parse_front_matter(&md)
    } else {
        (None, &md[..])
    };
    let hb =
        converter::convert_markdown_to_html_with_mermaid(body_md).map_err(|e| e.to_string())?;
    let hb = converter::resolve_image_srcs(&hb, &p);
    let t = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled");
    let fh = template::render_html_with(&tpl, &hb, t, metadata.as_ref(), full_width, theme);

    let d = std::env::temp_dir().join("rust-mpe-browser");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let hp = d.join(format!("{}.html", file_stem));
    let pp = d.join(format!("{}.pdf", file_stem));

    // 写入 HTML
    std::fs::write(&hp, &fh).map_err(|e| e.to_string())?;

    // 生成 PDF（保留 HTML 文件供后续缓存复用）
    chrome::generate_pdf(&hp.to_string_lossy(), &pp.to_string_lossy())
        .map_err(|e| e.to_string())?;

    // 更新缓存
    {
        let mut cache = s.cached_preview.lock().map_err(|e| e.to_string())?;
        *cache = Some(CachedPreview {
            file_mtime: current_mtime,
            pdf_path: pp.clone(),
            html_path: hp,
            full_width,
            template_id: tpl.cache_key(),
        });
    }

    let pd = std::fs::read(&pp).map_err(|e| format!("读 PDF 失败: {}", e))?;
    Ok(PreviewResult {
        base64: b64(&pd),
        temp_path: pp.to_string_lossy().to_string(),
        file_name: format!("{}.pdf", file_stem),
    })
}

/// 保存 PDF 到用户指定位置
#[tauri::command]
fn save_pdf_as(src: String, dst: String) -> Result<(), String> {
    std::fs::copy(&src, &dst).map_err(|e| format!("保存失败: {}", e))?;
    Ok(())
}

/// 渲染完整 HTML（含模板、图片内嵌、SKILL 元数据），供导出命令复用。
fn render_full_html(
    p: &Path,
    full_width: bool,
    theme: template::Theme,
    tpl: &template::HtmlTemplate,
) -> Result<String, String> {
    let md = std::fs::read_to_string(p).map_err(|e| format!("读取失败: {e}"))?;
    let is_skill = p
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n == "SKILL.md")
        .unwrap_or(false);
    let (metadata, body_md) = if is_skill {
        converter::parse_front_matter(&md)
    } else {
        (None, &md[..])
    };
    let hb =
        converter::convert_markdown_to_html_with_mermaid(body_md).map_err(|e| e.to_string())?;
    let hb = converter::resolve_image_srcs(&hb, p);
    let t = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled");
    Ok(template::render_html_with(tpl, &hb, t, metadata.as_ref(), full_width, theme))
}

/// 列出可用模版：内置一项 + 模版目录自动扫描结果。
///
/// 扫描同时识别目录式模版包（`<id>/template.html` + `template.json` + 缩略图）
/// 与旧式单文件模版（`<id>.html`），缩略图在此阶段编码为 data URL，
/// 前端拿到即可直接渲染，无需额外的文件读取权限。
#[tauri::command]
fn list_templates(app: tauri::AppHandle) -> Result<Vec<TemplateInfo>, String> {
    let mut out = vec![TemplateInfo {
        id: template::BUILTIN_TEMPLATE_ID.to_string(),
        // 展示名/描述由前端按 id 走 i18n，这里是后端兜底文案
        name: "默认模板".to_string(),
        description: "内置模版：侧栏目录、明暗双主题、图表渲染".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        author: "md2x".to_string(),
        builtin: true,
        packaged: true,
        preview: String::new(),
        path: String::new(),
        vars: Vec::new(),
    }];
    let dir = ensure_templates_dir(&app)?;
    let store = read_var_store(&app);
    for e in template::scan_templates(&dir) {
        let values = store.get(&e.id);
        out.push(TemplateInfo {
            preview: e
                .preview
                .as_deref()
                .and_then(template::preview_data_url)
                .unwrap_or_default(),
            path: e.html_path.to_string_lossy().to_string(),
            id: e.id.clone(),
            name: e.name,
            description: e.description,
            version: e.version,
            author: e.author,
            builtin: false,
            packaged: e.packaged,
            vars: var_infos(&e.vars, values),
        });
    }
    Ok(out)
}

/// 把模版声明的变量与已保存取值合成为前端条目。
///
/// 存档里可能残留模版已删掉的 key（作者改了清单），这里以清单为准逐个取值，
/// 残留项自然被忽略。
fn var_infos(
    vars: &[template::TemplateVar],
    values: Option<&HashMap<String, String>>,
) -> Vec<TemplateVarInfo> {
    vars.iter()
        .map(|v| {
            let raw = values.and_then(|m| m.get(&v.key)).cloned().unwrap_or_default();
            let saved = template::sanitize_var_value(&raw);
            let value = if saved.is_empty() {
                v.default.clone()
            } else {
                saved
            };
            TemplateVarInfo {
                key: v.key.clone(),
                label: v.label.clone(),
                kind: v.kind.as_str().to_string(),
                default: v.default.clone(),
                value,
                hint: v.hint.clone(),
            }
        })
        .collect()
}

/// 保存某个模版的变量取值，返回保存后的生效值。
///
/// 前端传整份映射（缺省即回默认），后端按清单声明的变量逐项过滤：
/// 未声明的 key、与默认值相同的值都不落盘——这样存档里只剩「真正改过的项」，
/// 模版作者日后调整默认值时，用户没动过的变量能跟着更新。
#[tauri::command(rename_all = "camelCase")]
fn set_template_vars(
    app: tauri::AppHandle,
    template_id: String,
    vars: HashMap<String, String>,
) -> Result<Vec<TemplateVarInfo>, String> {
    let dir = ensure_templates_dir(&app)?;
    if !is_safe_template_id(&template_id) {
        return Err(format!("非法的模版 id: {template_id}"));
    }
    let tpl = template::load_template(&template_id, &dir);
    // 复用渲染侧同一套过滤逻辑，避免「存进去的值」与「渲染用的值」两套规则
    let tpl = tpl.with_var_overrides(&vars);
    let kept = tpl.var_overrides().clone();

    let mut store = read_var_store(&app);
    if kept.is_empty() {
        store.remove(&template_id);
    } else {
        store.insert(template_id, kept.clone());
    }
    write_var_store(&app, &store)?;
    Ok(var_infos(tpl.vars(), Some(&kept)))
}

/// 打开外置模板目录（不存在则创建，并写入契约说明 README）
#[tauri::command]
fn open_templates_dir(app: tauri::AppHandle) -> Result<(), String> {
    let dir = ensure_templates_dir(&app)?;
    #[cfg(target_os = "macos")]
    let ok = std::process::Command::new("open").arg(&dir).status();
    #[cfg(target_os = "windows")]
    let ok = std::process::Command::new("explorer").arg(&dir).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let ok = std::process::Command::new("xdg-open").arg(&dir).status();
    ok.map_err(|e| format!("打开目录失败: {e}"))?;
    Ok(())
}

/// 模版 id 是否可安全地当作**单层目录名**使用。
///
/// id 由前端回传（来源是历史选择记录或扫描结果），必须挡住 `..`、路径分隔符
/// 与隐藏目录，否则复制操作可能落到模版目录之外。
fn is_safe_template_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id.starts_with('.')
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains('\0')
}

/// 递归复制目录；符号链接一律跳过。
///
/// 跳过链接是安全考虑：模版目录里的链接可能指向任意位置，
/// 跟随复制会把无关文件卷进用户模版。
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            continue;
        }
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// 为副本挑一个不冲突的目录名：`<id>-copy`，已存在则 `<id>-copy-2`、`-3`……
fn unique_copy_id(dir: &Path, id: &str) -> String {
    let base = format!("{id}-copy");
    if !dir.join(&base).exists() {
        return base;
    }
    for n in 2..100 {
        let candidate = format!("{base}-{n}");
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", std::process::id())
}

/// 给副本的清单加上「副本」后缀。
///
/// 两张卡片同名时用户分不清哪张是自己的，直接在元数据里改掉最省事；
/// 清单缺失或损坏时按 id 生成一份最简清单，副本也能正常出现在列表里。
fn name_copy_package(dir: &Path, source_id: &str) {
    let path = dir.join(template::TEMPLATE_MANIFEST);
    let mut v: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let base = v
        .get("name")
        .and_then(|n| n.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(source_id)
        .to_string();
    v["name"] = serde_json::Value::String(format!("{base} 副本"));
    if let Ok(s) = serde_json::to_string_pretty(&v) {
        let _ = std::fs::write(&path, format!("{s}\n"));
    }
}

/// 复制模版包为副本，返回新模版 id。
///
/// 「复制自己改」比直接编辑自带模版更稳：原模版保持原样便于对照，
/// 改坏了删掉副本即可，也不会在后续版本更新时被误覆盖。
#[tauri::command]
fn duplicate_template(app: tauri::AppHandle, id: String) -> Result<String, String> {
    let dir = ensure_templates_dir(&app)?;
    if !is_safe_template_id(&id) {
        return Err(format!("非法的模版 id: {id}"));
    }
    let src = dir.join(&id);
    // 只支持模版包：旧式单文件模版没有目录，改造成模版包才有意义
    if !src.join(template::TEMPLATE_HTML).is_file() {
        return Err("该模版不是模版包（缺少 template.html），无法复制".to_string());
    }
    let dst_id = unique_copy_id(&dir, &id);
    let dst = dir.join(&dst_id);
    copy_dir(&src, &dst).map_err(|e| format!("复制模版失败: {e}"))?;
    name_copy_package(&dst, &id);
    // 自定义变量一并带过去：复制通常是为了在已调好的配色上继续改版式，
    // 副本却回落默认配色会让人以为「复制没生效」
    let mut store = read_var_store(&app);
    if let Some(vals) = store.get(&id).cloned() {
        store.insert(dst_id.clone(), vals);
        let _ = write_var_store(&app, &store);
    }
    Ok(dst_id)
}

/// 检测外部图表 CLI（d2 / dot）安装状态，供设置面板展示「装了即用」的功能可用性。
#[tauri::command]
fn check_external_tools() -> Vec<ext_diagram::ToolStatus> {
    ext_diagram::EXT_TOOLS
        .iter()
        .map(ext_diagram::check_tool)
        .collect()
}

/// 导出 HTML 到用户指定位置
#[tauri::command(rename_all = "camelCase")]
fn export_html(
    dst: String,
    full_width: Option<bool>,
    theme: Option<String>,
    template_id: Option<String>,
    app: tauri::AppHandle,
    s: State<AppState>,
) -> Result<(), String> {
    let full_width = full_width.unwrap_or(false);
    let theme = theme_arg::parse_opt(&theme);
    let tpl = load_selected_template(&app, &template_id);
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = cur
        .as_ref()
        .ok_or_else(|| "没有打开文件".to_string())?
        .clone();
    drop(cur);
    let html = render_full_html(&p, full_width, theme, &tpl)?;
    std::fs::write(&dst, html).map_err(|e| format!("导出失败: {e}"))?;
    Ok(())
}

/// 导出整个文件夹为单个文件：把目录下所有 Markdown 聚合，可选 HTML / PDF / DOCX
///
/// PDF 走 CDP 生成，带标准书签树（阅读器侧边栏可点击跳转）并在首页附目录页。
#[tauri::command(rename_all = "camelCase")]
fn export_folder_html(
    src: String,
    dst: String,
    full_width: Option<bool>,
    theme: Option<String>,
    template_id: Option<String>,
    app: tauri::AppHandle,
    format: Option<String>,
) -> Result<usize, String> {
    let full_width = full_width.unwrap_or(false);
    let theme = theme_arg::parse_opt(&theme);
    let tpl = load_selected_template(&app, &template_id);
    let root = PathBuf::from(&src);
    if !root.is_dir() {
        return Err(format!("不是有效的文件夹: {}", root.display()));
    }
    let result = md2x_core::aggregate::aggregate_folder_to_html(&root, full_width, theme, Some(&tpl))
        .map_err(|e| format!("导出失败: {e}"))?;

    let fmt = format.unwrap_or_else(|| "html".to_string());
    match fmt.as_str() {
        // PDF：先写临时 HTML 再经Chrome 打印，最后清理
        "pdf" => {
            let tmp = std::env::temp_dir().join(format!("md2x-agg-{}.html", std::process::id()));
            std::fs::write(&tmp, &result.html).map_err(|e| format!("导出失败: {e}"))?;
            let pdf_path = dst.trim_end_matches(".html").to_string();
            let pdf_res = md2x_core::chrome::generate_pdf(
                &tmp.to_string_lossy(),
                &pdf_path,
            );
            let _ = std::fs::remove_file(&tmp);
            pdf_res.map_err(|e| format!("导出失败: {e}"))?;
        }
        "docx" => {
            let md = aggregate::concat_docs(&root).map_err(|e| format!("导出失败: {e}"))?;
            // 转DOCX 需要一个真实文件路径以解析相对图片，故先落临时文件
            let tmp = std::env::temp_dir().join(format!("md2x-agg-{}.md", std::process::id()));
            std::fs::write(&tmp, &md).map_err(|e| format!("导出失败: {e}"))?;
            let docx_path = dst.trim_end_matches(".html").to_string();
            let res = md2x_core::docx::convert_markdown_to_docx(&md, &tmp, Path::new(&docx_path));
            let _ = std::fs::remove_file(&tmp);
            res.map_err(|e| format!("导出失败: {e}"))?;
        }
        _ => {
            std::fs::write(&dst, &result.html).map_err(|e| format!("导出失败: {e}"))?;
        }
    }
    Ok(result.doc_count)
}

/// 导出 PDF 到用户指定位置
#[tauri::command(rename_all = "camelCase")]
fn export_pdf(
    dst: String,
    full_width: Option<bool>,
    theme: Option<String>,
    template_id: Option<String>,
    app: tauri::AppHandle,
    s: State<AppState>,
) -> Result<(), String> {
    let full_width = full_width.unwrap_or(false);
    let theme = theme_arg::parse_opt(&theme);
    let tpl = load_selected_template(&app, &template_id);
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = cur
        .as_ref()
        .ok_or_else(|| "没有打开文件".to_string())?
        .clone();
    drop(cur);
    let html = render_full_html(&p, full_width, theme, &tpl)?;
    let d = std::env::temp_dir().join("rust-mpe-browser");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let hp = d.join("export-tmp.html");
    std::fs::write(&hp, html).map_err(|e| e.to_string())?;
    chrome::generate_pdf(&hp.to_string_lossy(), &dst).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&hp);
    Ok(())
}

/// 导出 DOCX 到用户指定位置（纯 Rust 生成，不依赖 Office）
#[tauri::command]
fn export_docx(dst: String, s: State<AppState>) -> Result<(), String> {
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = cur
        .as_ref()
        .ok_or_else(|| "没有打开文件".to_string())?
        .clone();
    drop(cur);
    let md = std::fs::read_to_string(&p).map_err(|e| format!("读取失败: {e}"))?;
    md2x_core::docx::convert_markdown_to_docx(&md, &p, Path::new(&dst))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 获取当前文件名
#[tauri::command]
fn get_file_name(s: State<AppState>) -> Result<Option<String>, String> {
    Ok(s.current_file
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .and_then(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        }))
}

/// 获取当前打开文件的完整路径（前端用来解析相对链接的基准目录）
#[tauri::command]
fn get_file_path(s: State<AppState>) -> Option<String> {
    // 统一 / 分隔：与 scan_tree 返回的节点 path 保持同一格式，
    // 否则 Windows 下前端用 startsWith / split 比较会全部失配
    s.current_file
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|p| normalize_path(p)))
}

/// 清空当前打开的文件（切换文件夹卸载旧预览时用），回到无文件初始状态
#[tauri::command]
fn clear_file(s: State<AppState>, window: tauri::WebviewWindow) -> Result<(), String> {
    *s.current_file.lock().map_err(|e| e.to_string())? = None;
    *s.last_known_mtime.lock().map_err(|e| e.to_string())? = None;
    *s.cached_preview.lock().map_err(|e| e.to_string())? = None;
    // 只关文件不清文件夹时，标题会回落到文件夹名
    apply_window_title(&window, &s);
    Ok(())
}

// ==================== 文件夹 / 文件树命令 ====================

/// 打开文件夹：记录根目录并递归扫描生成 .md 文件树（剪除不含 Markdown 的目录）
#[tauri::command]
fn open_folder(
    path: String,
    s: State<AppState>,
    window: tauri::WebviewWindow,
) -> Result<Vec<TreeNode>, String> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err(format!("不是有效的文件夹: {}", p.display()));
    }
    *s.folder_root.lock().map_err(|e| e.to_string())? = Some(p.clone());
    apply_window_title(&window, &s);
    scan_tree(&p)
}

/// 关闭文件夹：回到单文件视图
#[tauri::command]
fn close_folder(s: State<AppState>, window: tauri::WebviewWindow) -> Result<(), String> {
    *s.folder_root.lock().map_err(|e| e.to_string())? = None;
    apply_window_title(&window, &s);
    Ok(())
}

/// 获取当前打开的文件夹根目录（用于启动时自动展开文件树）
#[tauri::command]
fn get_folder_path(s: State<AppState>) -> Option<String> {
    // 同 get_file_path：统一 / 分隔，前端按 '/' split 取文件夹名才成立
    s.folder_root
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|p| normalize_path(p)))
}

/// 判断路径类型："dir" / "file" / "none"（拖拽、启动参数判断用）
#[tauri::command]
fn path_kind(path: String) -> String {
    let p = Path::new(&path);
    if p.is_dir() {
        "dir".to_string()
    } else if p.is_file() {
        "file".to_string()
    } else {
        "none".to_string()
    }
}

/// 递归扫描目录内 Markdown 文件树
fn scan_tree(dir: &Path) -> Result<Vec<TreeNode>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("读取文件夹失败: {e}"))?;
    let mut dirs: Vec<TreeNode> = Vec::new();
    let mut files: Vec<TreeNode> = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_dir() {
            // 递归子目录，剪除不含任何 .md 的目录
            if let Ok(children) = scan_tree(&path) {
                if !children.is_empty() {
                    dirs.push(TreeNode {
                        name,
                        path: normalize_path(&path),
                        is_dir: true,
                        children,
                    });
                }
            }
        } else if ft.is_file() && is_markdown(&name) {
            files.push(TreeNode {
                name,
                path: normalize_path(&path),
                is_dir: false,
                children: Vec::new(),
            });
        }
        // 符号链接不追踪，避免循环；指向 .md 的链接按非目录跳过
    }
    // 目录优先，各自按名称排序
    dirs.sort_by_key(|a| a.name.to_lowercase());
    files.sort_by_key(|a| a.name.to_lowercase());
    dirs.extend(files);
    Ok(dirs)
}

/// 是否 Markdown 文件（.md 后缀）
fn is_markdown(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".md")
}

/// 路径统一为 / 分隔（跨平台比较、拼接用）
fn normalize_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

// ==================== 窗口标题 ====================

/// 应用名，标题栏在无打开内容时的兜底文本
const APP_NAME: &str = "md2x";

/// 路径在标题里的显示名：取文件名；根目录（`/`、`C:\`）没有 file_name，
/// 退回整串路径，避免标题变成空串。
fn title_label(p: &Path) -> String {
    match p.file_name().and_then(|n| n.to_str()) {
        Some(name) => name.to_string(),
        None => normalize_path(p),
    }
}

/// 组装标题的纯函数：`文件名 — md2x`，无文件时用文件夹名，都没有就是 `md2x`。
///
/// 后端是「当前打开了什么」的唯一来源，标题也在这里推导，
/// 前端不需要额外调一次 setTitle——多一条链路就多一处可能不同步。
fn title_from(current: Option<&Path>, folder: Option<&Path>) -> String {
    match current.or(folder) {
        Some(p) => format!("{} — {APP_NAME}", title_label(p)),
        None => APP_NAME.to_string(),
    }
}

/// 从状态里取当前标题（读锁失败按「什么都没打开」处理，标题不该因锁中毒而报错）
fn window_title(s: &AppState) -> String {
    let current = s.current_file.lock().ok().and_then(|g| g.clone());
    let folder = s.folder_root.lock().ok().and_then(|g| g.clone());
    title_from(current.as_deref(), folder.as_deref())
}

/// 把标题写回窗口；标题未变化时跳过，避免无谓的跨线程 UI 调用。
fn apply_window_title(window: &tauri::WebviewWindow, s: &AppState) {
    let title = window_title(s);
    if window.title().ok().as_deref() == Some(title.as_str()) {
        return;
    }
    let _ = window.set_title(&title);
}

/// 百分号解码：`%E4%B8%AD` → `中`，非法的 `%` 序列按字面量保留。
///
/// 只用于 file URL 还原：URL 中的空格、中文都是编码形态，不解码就找不到文件。
/// 注意 `+` 在路径语义里是字面量，不按表单规则转空格。
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| -> Option<u8> {
                match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    b'A'..=b'F' => Some(b - b'A' + 10),
                    _ => None,
                }
            };
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// file URL 去掉 scheme 后的部分 → 路径字符串。`windows` 显式传入以便跨平台单测。
///
/// - Unix：`/a/b` → `/a/b`
/// - Windows 盘符：`/C:/a/b` → `C:\a\b`
/// - Windows UNC：`server/share/a`（`file://server/...`）与 `//server/share/a`
///   （`file:////server/...`）都还原为 `\\server\share\a`
fn file_url_rest_to_path_str(rest: &str, windows: bool) -> String {
    let decoded = percent_decode(rest);
    if !windows {
        return decoded;
    }
    let (unc, body) = if let Some(b) = decoded.strip_prefix("//") {
        (true, b)
    } else if let Some(b) = decoded.strip_prefix('/') {
        // 形如 "/C:/a"：盘符前的分隔符要去掉，否则得到 "\C:\a"
        (false, b)
    } else {
        // 不带前导斜杠 ⇒ file://host/share 形式，按 UNC 处理
        (true, decoded.as_str())
    };
    let mut s = if unc { String::from(r"\\") } else { String::new() };
    s.push_str(&body.replace('/', "\\"));
    s
}

/// file URL → 本地路径；非 file URL 返回 None。
fn file_url_to_path(raw: &str) -> Option<PathBuf> {
    let rest = raw.strip_prefix("file://")?;
    // file URL 可能带查询串或锚点（如 file:///a.md?v=1）
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let p = file_url_rest_to_path_str(rest, cfg!(windows));
    if p.is_empty() {
        None
    } else {
        Some(PathBuf::from(p))
    }
}

// ==================== Git 仓库相对路径 ====================

/// 向上逐级查找包含 .git 的祖先目录，返回（仓库根目录, 仓库目录名）
/// 兼容 .git 为目录（普通仓库）或文件（worktree / submodule）两种形态
fn find_git_root(p: &Path) -> Option<(PathBuf, String)> {
    let mut cur = Some(p);
    while let Some(dir) = cur {
        if dir.join(".git").exists() {
            let name = dir.file_name()?.to_string_lossy().into_owned();
            return Some((dir.to_path_buf(), name));
        }
        cur = dir.parent();
    }
    None
}

/// 计算路径相对其所在 git 仓库根的路径，含仓库名前缀：`repo/docs/a.md`
/// 不在任何仓库内时返回 None
fn repo_relative_label(path: &Path) -> Option<String> {
    let (root, name) = find_git_root(path)?;
    let rel = path.strip_prefix(&root).ok()?;
    let mut parts: Vec<String> = vec![name];
    for c in rel.components() {
        parts.push(c.as_os_str().to_string_lossy().into_owned());
    }
    Some(parts.join("/"))
}

/// 查询文件是否位于 git 仓库内，是则返回含仓库名的相对路径（前端右键菜单判定用）
#[tauri::command]
fn repo_relative_path(path: String) -> Option<String> {
    repo_relative_label(Path::new(&path))
}

/// 将文本写入系统剪贴板（macOS: pbcopy / Windows: Set-Clipboard / Linux: xclip 或 wl-copy）
#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    platform_copy_clipboard(&text)
}

#[cfg(target_os = "macos")]
fn platform_copy_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 pbcopy 失败: {e}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "无法写入 pbcopy".to_string())?
        .write_all(text.as_bytes())
        .map_err(|e| format!("写入剪贴板失败: {e}"))?;
    let st = child.wait().map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err("pbcopy 执行失败".to_string())
    }
}

#[cfg(target_os = "windows")]
fn platform_copy_clipboard(text: &str) -> Result<(), String> {
    // 通过带 BOM 的 UTF-8 临时文件中转，规避 clip.exe / PowerShell 管道的代码页乱码
    let tmp = std::env::temp_dir().join("md2x-clip.txt");
    std::fs::write(&tmp, format!("\u{feff}{text}")).map_err(|e| format!("写临时文件失败: {e}"))?;
    let script = format!(
        "Set-Clipboard -Value (Get-Content -Raw -Encoding UTF8 \"{}\")",
        tmp.to_string_lossy().replace('"', "`\"")
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|e| format!("启动 PowerShell 失败: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "写入剪贴板失败: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(target_os = "linux")]
fn platform_copy_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    // X11 优先 xclip，Wayland 用 wl-copy。
    // 走 resolve_binary 而非裸名 spawn：桌面环境下从图标启动的进程
    // 未必继承完整 PATH（/usr/local/bin 等常缺席），裸名会 ENOENT。
    let cmds: [(&str, Vec<&str>); 2] = [
        ("xclip", vec!["-selection", "clipboard"]),
        ("wl-copy", vec![]),
    ];
    for (bin, args) in cmds {
        let Some(bin_path) = md2x_core::ext_diagram::resolve_binary(bin) else {
            continue; // 未安装，尝试下一个
        };
        let child = Command::new(&bin_path)
            .args(args)
            .stdin(Stdio::piped())
            .spawn();
        match child {
            Ok(mut c) => {
                let ok = c
                    .stdin
                    .take()
                    .ok_or_else(|| "无法写入子进程".to_string())?
                    .write_all(text.as_bytes())
                    .is_ok()
                    && c.wait().map(|s| s.success()).unwrap_or(false);
                if ok {
                    return Ok(());
                }
            }
            Err(_) => continue, // 启动失败，尝试下一个
        }
    }
    Err("未找到 xclip 或 wl-copy，无法写入剪贴板".to_string())
}

// ==================== 默认打开程序 ====================

/// 将本应用注册为 .md 文件的默认打开程序（当前用户级）
/// 成功返回 Ok(true)；失败返回 Err(具体原因)
#[tauri::command]
fn register_default_md_handler() -> Result<bool, String> {
    platform_register_default()
}

#[cfg(target_os = "windows")]
fn platform_register_default() -> Result<bool, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy().replace('/', "\\");
    let command = format!("\"{exe_str}\" \"%1\"");
    let icon = format!("\"{exe_str}\",0");

    let steps: [&[&str]; 6] = [
        // .md 默认关联到自定义 ProgID
        &[
            "add",
            r"HKCU\Software\Classes\.md",
            "/ve",
            "/d",
            "md2x.markdown",
            "/f",
        ],
        &[
            "add",
            r"HKCU\Software\Classes\.md\OpenWithProgids",
            "/v",
            "md2x.markdown",
            "/f",
        ],
        &[
            "add",
            r"HKCU\Software\Classes\md2x.markdown",
            "/ve",
            "/d",
            "Markdown Document (md2x)",
            "/f",
        ],
        &[
            "add",
            r"HKCU\Software\Classes\md2x.markdown\DefaultIcon",
            "/ve",
            "/d",
            &icon,
            "/f",
        ],
        &[
            "add",
            r"HKCU\Software\Classes\md2x.markdown\shell\open\command",
            "/ve",
            "/d",
            &command,
            "/f",
        ],
        // 让 md2x 出现在「打开方式」列表中
        &[
            "add",
            r"HKCU\Software\Classes\Applications\md2x.exe\shell\open\command",
            "/ve",
            "/d",
            &command,
            "/f",
        ],
    ];
    for args in &steps {
        let out = std::process::Command::new("reg")
            .args(*args)
            .output()
            .map_err(|e| format!("reg 命令执行失败: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(format!("注册表写入失败: {}", err.trim()));
        }
    }
    Ok(true)
}

#[cfg(target_os = "macos")]
fn platform_register_default() -> Result<bool, String> {
    use std::process::Command;
    // 通过 LaunchServices 用户级偏好写入 .md 的默认打开程序（与 duti 同机制）
    let domain = "com.apple.LaunchServices/com.apple.launchservices.secure";
    // 兼容 .md / .markdown 两种常见 UTI 声明
    for uti in ["net.daringfireball.markdown", "public.markdown-file"] {
        let entry =
            format!("{{LSHandlerContentType = \"{uti}\"; LSHandlerRoleAll = \"com.mpe.md2x\";}}");
        let out = Command::new("/usr/bin/defaults")
            .args(["write", domain, "LSHandlers", "-array-add", &entry])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(format!("LaunchServices 写入失败: {}", err.trim()));
        }
    }
    // 重建 LaunchServices 数据库使修改生效
    let lsregister =
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    let _ = Command::new(lsregister)
        .args(["-kill", "-r", "-domain", "user"])
        .output();
    Ok(true)
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn platform_register_default() -> Result<bool, String> {
    use std::process::Command;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // 生成用户级 .desktop 文件，供 xdg-mime 关联
    let data_home = std::env::var("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".local/share"))
                .map_err(|e| e.to_string())
        })
        .map_err(|e| e.to_string())?;
    let apps_dir = data_home.join("applications");
    std::fs::create_dir_all(&apps_dir).map_err(|e| e.to_string())?;
    let desktop_path = apps_dir.join("md2x.desktop");
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=md2x\nComment=Markdown preview & export\nExec=\"{}\" %f\nTerminal=false\nCategories=Office;Viewer;Utility;\nMimeType=text/markdown;text/x-markdown;\n",
        exe.to_string_lossy()
    );
    std::fs::write(&desktop_path, content).map_err(|e| e.to_string())?;

    let out = Command::new("xdg-mime")
        .args([
            "default",
            "md2x.desktop",
            "text/markdown",
            "text/x-markdown",
        ])
        .output()
        .map_err(|e| format!("xdg-mime 不可用: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("xdg-mime 执行失败: {}", err.trim()));
    }
    // 可选刷新桌面数据库，失败不影响结果
    let _ = Command::new("update-desktop-database")
        .arg(&apps_dir)
        .output();
    Ok(true)
}

// ==================== 工具函数 ====================

fn b64(data: &[u8]) -> String {
    const C: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut r = String::new();
    for chunk in data.chunks(3) {
        let t = (chunk[0] as u32) << 16
            | (chunk.get(1).copied().unwrap_or(0) as u32) << 8
            | (chunk.get(2).copied().unwrap_or(0) as u32);
        r.push(C[((t >> 18) & 0x3F) as usize] as char);
        r.push(C[((t >> 12) & 0x3F) as usize] as char);
        r.push(if chunk.len() > 1 {
            C[((t >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        r.push(if chunk.len() > 2 {
            C[(t & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 导出菜单 id 必须一一映射到前端认识的后缀，未知 id 返回 None（静默忽略）
    #[test]
    fn export_menu_ids_map_to_expected_formats() {
        assert_eq!(export_format_of("export_folder"), Some("folder"));
        assert_eq!(export_format_of("export_html"), Some("html"));
        assert_eq!(export_format_of("export_pdf"), Some("pdf"));
        assert_eq!(export_format_of("export_docx"), Some("docx"));
        // 其他菜单项（打开、关闭、设置…）不能被误判成导出请求
        for id in ["open_file", "close_file", "settings", "toggle_tree", ""] {
            assert_eq!(export_format_of(id), None, "{id} 不该被当成导出格式");
        }
    }

    #[test]
    fn preset_menu_ids_route_to_their_own_request() {
        // 预设项必须原样带 id 透出来，前端才能查表拿到格式/模版/主题
        assert_eq!(
            menu_export_request("preset:p_weekly"),
            Some("preset:p_weekly".to_string())
        );
        // 固定格式项仍走原路
        assert_eq!(menu_export_request("export_pdf"), Some("pdf".to_string()));
        // 只有前缀没有 id 的残缺项、未知项一律丢弃，避免前端拿着空 id 查不到预设
        assert_eq!(menu_export_request("preset:"), None);
        assert_eq!(menu_export_request("preset"), None);
        assert_eq!(menu_export_request("about"), None);
    }

    /// 仅收集 .md（忽略大小写），剪除不含 Markdown 的目录，目录优先排序
    #[test]
    fn scan_tree_collects_only_md_and_prunes_empty_dirs() {
        let d = std::env::temp_dir().join(format!("md2x-scan-test-{}", std::process::id()));
        let sub = d.join("sub");
        std::fs::create_dir_all(sub.join("empty")).unwrap();
        std::fs::write(sub.join("a.md"), "# a").unwrap();
        std::fs::write(sub.join("b.MD"), "# b").unwrap();
        std::fs::write(sub.join("skip.txt"), "x").unwrap();
        std::fs::write(d.join("top.md"), "# top").unwrap();

        let tree = scan_tree(&d).unwrap();
        // 目录在前，文件在后；skip.txt 与空目录 empty 均被剪除
        assert_eq!(tree.len(), 2);
        assert!(tree[0].is_dir);
        assert_eq!(tree[0].name, "sub");
        assert_eq!(tree[0].children.len(), 2);
        assert_eq!(tree[0].children[0].name, "a.md");
        assert_eq!(tree[0].children[1].name, "b.MD");
        assert!(!tree[1].is_dir);
        assert_eq!(tree[1].name, "top.md");
        // 路径统一使用 / 分隔符
        assert!(!tree[0].path.contains('\\'));

        let _ = std::fs::remove_dir_all(&d);
    }

    /// 向上查找 .git 根：相对路径含仓库名前缀；仓库外返回 None
    #[test]
    fn repo_relative_label_walks_up_to_git_root() {
        let base = std::env::temp_dir().join(format!("md2x-repo-test-{}", std::process::id()));
        let repo = base.join("my-repo");
        let sub = repo.join("docs/guide");
        std::fs::create_dir_all(&sub).unwrap();
        // 仅创建 .git 目录即可判定为仓库根（无需真实 git 元数据）
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(sub.join("a.md"), "# a").unwrap();

        let f = sub.join("a.md");
        assert_eq!(
            repo_relative_label(&f).as_deref(),
            Some("my-repo/docs/guide/a.md")
        );
        // .git 为文件（worktree 形态）同样判定
        let wt = base.join("wt-repo/nested");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(base.join("wt-repo/.git"), "gitdir: /somewhere").unwrap();
        std::fs::write(wt.join("b.md"), "# b").unwrap();
        assert_eq!(
            repo_relative_label(&wt.join("b.md")).as_deref(),
            Some("wt-repo/nested/b.md")
        );
        // 仓库外的文件返回 None
        assert_eq!(repo_relative_label(&base.join("outside.md")), None);

        let _ = std::fs::remove_dir_all(&base);
    }

    /// 标题优先级：当前文件 > 文件夹根 > 应用名。
    /// 三点都要断言——只测有文件的情况会漏掉「关掉文件后标题没回落」这类缺陷。
    #[test]
    fn 窗口标题按文件优先于文件夹() {
        let file = PathBuf::from("/repo/docs/guide/intro.md");
        let dir = PathBuf::from("/repo/docs");
        assert_eq!(title_from(Some(&file), None), "intro.md — md2x");
        // 同时打开文件夹与文件时，标题显示文件名而不是文件夹名
        assert_eq!(title_from(Some(&file), Some(&dir)), "intro.md — md2x");
        // 只开文件夹（尚未点开任何文件）
        assert_eq!(title_from(None, Some(&dir)), "docs — md2x");
        // 都没打开
        assert_eq!(title_from(None, None), "md2x");
    }

    /// 根目录没有 file_name，标题要退回整串路径而不是留空
    #[test]
    fn 窗口标题在根目录下退回路径() {
        let root = PathBuf::from("/");
        assert_eq!(title_from(None, Some(&root)), "/ — md2x");
    }

    #[test]
    fn 百分号解码还原空格与中文() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(
            percent_decode("%E6%9E%B6%E6%9E%84.html"),
            "架构.html"
        );
        // 非法序列按字面量保留，不能吞字符
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        // '+' 在路径里是字面量，不转空格
        assert_eq!(percent_decode("a+b"), "a+b");
    }

    /// Windows 的 file URL 是 `file:///C:/a/b`，去掉 scheme 后是 `/C:/a/b`，
    /// 直接当路径用会得到 `\C:\a\b` 而判定文件不存在。
    #[test]
    fn file_url_还原windows盘符路径() {
        assert_eq!(file_url_rest_to_path_str("/C:/a/b.md", true), r"C:\a\b.md");
        assert_eq!(
            file_url_rest_to_path_str("/C:/Users/a%20b.md", true),
            r"C:\Users\a b.md"
        );
    }

    #[test]
    fn file_url_还原windows_unc路径() {
        // file://server/share → 去掉 scheme 后无前导斜杠
        assert_eq!(
            file_url_rest_to_path_str("server/share/a.md", true),
            r"\\server\share\a.md"
        );
        // file:////server/share（RFC 形式）→ 去掉 scheme 后以 // 开头
        assert_eq!(
            file_url_rest_to_path_str("//server/share/a.md", true),
            r"\\server\share\a.md"
        );
    }

    #[test]
    fn file_url_还原unix路径() {
        assert_eq!(file_url_rest_to_path_str("/tmp/a.md", false), "/tmp/a.md");
        assert_eq!(
            file_url_rest_to_path_str("/tmp/a%20b.md", false),
            "/tmp/a b.md"
        );
    }

    #[test]
    fn 非file协议不当作路径() {
        assert!(file_url_to_path("https://example.com/a.md").is_none());
        assert!(file_url_to_path("/plain/path.md").is_none());
    }

    // ===== 模版目录播种 =====

    fn fresh_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("md2x-gui-tpl-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn 首次启动铺齐全部自带模版() {
        let dir = fresh_dir("seed-empty");
        seed_bundled_templates(&dir);
        let found = template::scan_templates(&dir);
        let ids: Vec<&str> = found.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["docs", "paper", "sample"], "三套自带模版都应有展示名并进入列表");
        for e in &found {
            assert!(!e.name.trim().is_empty(), "{} 应带可读展示名", e.id);
            assert!(e.preview.is_some(), "{} 应带缩略图", e.id);
            assert!(
                template::preview_data_url(e.preview.as_ref().unwrap()).is_some(),
                "{} 的缩略图应能编码为 data URL",
                e.id
            );
        }
        // 再跑一次不应产生任何变化：播种是幂等的
        seed_bundled_templates(&dir);
        assert_eq!(template::scan_templates(&dir).len(), 3);
    }

    #[test]
    fn 已存在的模版目录不被覆盖() {
        // 用户改过 docs 之后，播种不能把他的改动冲掉
        let dir = fresh_dir("seed-existing");
        let docs = dir.join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("template.html"), "<html><body>我的改动{{BODY}}</body></html>")
            .unwrap();
        seed_bundled_templates(&dir);
        let html = std::fs::read_to_string(docs.join("template.html")).unwrap();
        assert!(html.contains("我的改动"), "已存在目录必须原样保留");
        assert!(dir.join("paper/template.html").is_file(), "缺失的自带模版应补齐");
        // 老版本只种过 sample、没有播种记录：这类目录也要补记，
        // 否则用户删掉后下次启动又会被写回来
        assert!(
            read_seed_state(&dir.join(SEED_STATE_FILE)).iter().any(|s| s == "docs"),
            "已存在的自带模版也应进入播种记录"
        );
    }

    #[test]
    fn 删掉的自带模版不再复活() {
        // 用户明确删掉的自带模版，下次启动不该又冒出来
        let dir = fresh_dir("seed-deleted");
        seed_bundled_templates(&dir);
        let docs = dir.join("docs");
        std::fs::remove_dir_all(&docs).unwrap();
        seed_bundled_templates(&dir);
        assert!(!docs.exists(), "删掉后不应被重新写入");
        assert!(dir.join(SEED_STATE_FILE).is_file(), "播种记录应落盘");
        let ids: Vec<String> = template::scan_templates(&dir)
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, vec!["paper", "sample"]);
    }

    #[test]
    fn 复制模版包生成带副本后缀的副本() {
        let dir = fresh_dir("duplicate");
        let src = dir.join("mine");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("template.html"), "<p>{{BODY}}</p>").unwrap();
        std::fs::write(src.join("template.json"), r#"{"name":"我的模版"}"#).unwrap();

        let id = unique_copy_id(&dir, "mine");
        assert_eq!(id, "mine-copy");
        copy_dir(&src, &dir.join(&id)).unwrap();
        name_copy_package(&dir.join(&id), "mine");

        let found = template::scan_templates(&dir);
        let copy = found
            .iter()
            .find(|e| e.id == "mine-copy")
            .expect("副本应被扫描到");
        assert_eq!(copy.name, "我的模版 副本", "副本应改名，避免与源模版同名");
        assert_eq!(
            unique_copy_id(&dir, "mine"),
            "mine-copy-2",
            "再次复制不能覆盖已有副本"
        );
    }

    #[test]
    fn 模版id拒绝路径穿越() {
        assert!(is_safe_template_id("docs"));
        for bad in ["", ".", "..", "../etc", "a/b", "a\\b", ".hidden"] {
            assert!(!is_safe_template_id(bad), "{bad} 应被拒绝");
        }
    }

    #[test]
    fn 变量存档可往返且容忍损坏() {
        let dir = fresh_dir("var-store");
        let p = dir.join("template-vars.json");

        // 缺失文件 → 空表（首次运行）
        assert!(read_var_store_file(&p).is_empty());

        let mut store: HashMap<String, HashMap<String, String>> = HashMap::new();
        store.insert(
            "docs".to_string(),
            HashMap::from([("accent".to_string(), "#e11d48".to_string())]),
        );
        write_var_store_file(&p, &store).unwrap();
        let back = read_var_store_file(&p);
        assert_eq!(back["docs"]["accent"], "#e11d48", "写入后应能原样读回");

        // 存档损坏 → 当作没有自定义，不能让预览/导出失败
        std::fs::write(&p, "{ not json").unwrap();
        assert!(read_var_store_file(&p).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 变量条目以清单为准忽略过期存档() {
        let vars = vec![
            template::TemplateVar {
                key: "accent".to_string(),
                label: "主色".to_string(),
                default: "#2f6feb".to_string(),
                kind: template::TemplateVarKind::Color,
                hint: String::new(),
            },
            template::TemplateVar {
                key: "doc_font_size".to_string(),
                label: "正文字号".to_string(),
                default: "15.5px".to_string(),
                kind: template::TemplateVarKind::Text,
                hint: "带单位".to_string(),
            },
        ];
        let saved = HashMap::from([
            ("accent".to_string(), "#e11d48".to_string()),
            // 作者已从清单里删掉的 key：存档里还留着，必须忽略而不是显示出来
            ("removed".to_string(), "x".to_string()),
            // 空值等于「没改」，应回落到默认值
            ("doc_font_size".to_string(), "  ".to_string()),
        ]);
        let infos = var_infos(&vars, Some(&saved));
        assert_eq!(infos.len(), 2, "只输出清单声明的变量");
        assert_eq!(infos[0].key, "accent");
        assert_eq!(infos[0].value, "#e11d48");
        assert_eq!(infos[0].kind, "color");
        assert_eq!(infos[1].value, "15.5px", "空存档应回落到默认值");
        assert_eq!(infos[1].default, "15.5px");

        // 完全没有存档时全部取默认
        let plain = var_infos(&vars, None);
        assert_eq!(plain[0].value, "#2f6feb");
    }
}
