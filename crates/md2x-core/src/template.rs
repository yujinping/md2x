use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::MpeError;

/// 内置默认模板的 id。设置面板、CLI 均用该 id 指代编译进二进制的 mpe.html。
pub const BUILTIN_TEMPLATE_ID: &str = "default";

/// 模版包目录内固定的正文文件名。
///
/// 目录名即模版 id，目录内用固定文件名而非「任意 *.html」，是为了让扫描器
/// 无需猜测哪个文件才是入口——与 AI Skill 用 `SKILL.md` 作固定入口同理。
pub const TEMPLATE_HTML: &str = "template.html";

/// 模版包元数据文件名（JSON），与 `template.html` 同级。
pub const TEMPLATE_MANIFEST: &str = "template.json";

/// 约定的缩略图文件名，按数组顺序探测，命中第一个即用。
///
/// 清单里的 `preview` 字段可覆盖本约定，用于缩略图不叫这些名字的场景。
const PREVIEW_NAMES: [&str; 5] = [
    "preview.png",
    "preview.jpg",
    "preview.jpeg",
    "preview.webp",
    "preview.svg",
];

/// 缩略图内联上限：缩略图只用于列表展示，超过这个尺寸说明放错了文件
const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;

/// 模版包清单（`template.json`）。
///
/// 全部字段可缺省：模版作者可以只放 `template.html` 就让模版可用，
/// 此时名称取目录名、缩略图按约定名探测。未知字段直接忽略，
/// 便于未来给清单加字段而不破坏旧模版。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TemplateManifest {
    /// 展示名（缺省取目录名）
    pub name: Option<String>,
    /// 一句话描述，展示在模版卡片上
    pub description: Option<String>,
    /// 版本号，纯展示用途
    pub version: Option<String>,
    /// 作者
    pub author: Option<String>,
    /// 缩略图文件名（相对模版目录）；缺省按 [`PREVIEW_NAMES`] 约定探测
    pub preview: Option<String>,
    /// 可调变量声明：key → 取值声明。
    ///
    /// 用 [`BTreeMap`] 而非 `HashMap` 是为了顺序稳定：设置面板按 key 排序渲染控件，
    /// 每次打开顺序一致，用户不会觉得控件在乱跳。
    pub vars: BTreeMap<String, TemplateVarDef>,
}

/// 变量声明的两种写法（均合法，短写只为省事）。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TemplateVarDef {
    /// 简写：`"accent": "#2f6feb"` 只给默认值，标签取 key、控件为文本框
    Plain(String),
    /// 完整写法：`{"label": "主色", "default": "#2f6feb", "type": "color", "hint": "…"}`
    Full(TemplateVarFull),
}

/// 变量声明的完整写法；所有字段可缺省。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TemplateVarFull {
    /// 控件标签，缺省取 key
    pub label: Option<String>,
    /// 默认值
    #[serde(rename = "default")]
    pub default_value: Option<String>,
    /// 控件形态：`color` / `text`，缺省 `text`（JSON 里写作 `type`）
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// 一行说明，展示在控件下方
    pub hint: Option<String>,
}

/// 变量控件形态。
///
/// 只保留两种：取色器与文本框。字号、字体栈、间距都能用文本框表达，
/// 再多形态就得引入校验与单位换算，收益远不及复杂度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemplateVarKind {
    /// 普通文本（字号、字体栈、宽度等自由取值）
    #[default]
    Text,
    /// 颜色：前端给取色器，渲染前校验是否为合法 CSS 颜色
    Color,
}

impl TemplateVarKind {
    /// 传给前端的形态名
    pub fn as_str(&self) -> &'static str {
        match self {
            TemplateVarKind::Color => "color",
            TemplateVarKind::Text => "text",
        }
    }
}

/// 模版声明的单个可调变量。
#[derive(Debug, Clone)]
pub struct TemplateVar {
    /// 变量 key（小写，仅 `[a-z0-9_]`）：同时决定占位符与 CSS 变量名
    pub key: String,
    /// 控件标签
    pub label: String,
    /// 默认值（已被 [`sanitize_var_value`] 清洗）
    pub default: String,
    /// 控件形态
    pub kind: TemplateVarKind,
    /// 一行说明
    pub hint: String,
}

impl TemplateVar {
    /// 直接替换用的占位符：`{{VAR_ACCENT}}`
    pub fn placeholder(&self) -> String {
        format!("{{{{VAR_{}}}}}", self.key.to_ascii_uppercase())
    }

    /// 自动注入时使用的 CSS 自定义属性名：`font_size` → `--font-size`
    pub fn css_name(&self) -> String {
        format!("--{}", self.key.replace('_', "-"))
    }
}

/// 一份模版最多声明多少个变量。
///
/// 上限只为挡住「清单写错把几百个键灌进设置面板」这类事故，
/// 正常模版远远用不到。
const MAX_TEMPLATE_VARS: usize = 24;

/// 变量取值长度上限：够放字体栈与 `rgb()`，又不至于把 CSS 撑爆
const MAX_VAR_VALUE_CHARS: usize = 120;

/// 变量 key 是否合法：必须能同时安全地生成 `{{VAR_X}}` 与 `--x`。
fn is_valid_var_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 32
        && key
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 清洗变量取值。
///
/// 取值最终会落进 CSS 声明（或模版里的任意文本位置），因此只挡能打破
/// 声明/标签/引号的字符；颜色、字号、字体栈都用不到它们，误伤面为零。
pub fn sanitize_var_value(raw: &str) -> String {
    let s: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .filter(|c| !matches!(c, '<' | '>' | '{' | '}' | ';' | '"' | '\'' | '\\' | '`'))
        .take(MAX_VAR_VALUE_CHARS)
        .collect();
    s.trim().to_string()
}

/// 是否为可接受的 CSS 颜色字面量。
///
/// 只做形状判断，不校验色值语义：目的是挡住把 `banana;` 之类
/// 塞进取色器（前端被绕过时）导致整段样式失效。
pub fn is_css_color(v: &str) -> bool {
    let s = v.trim();
    if s.is_empty() {
        return false;
    }
    if let Some(hex) = s.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    let lower = s.to_ascii_lowercase();
    for f in ["rgb(", "rgba(", "hsl(", "hsla("] {
        if let Some(rest) = lower.strip_prefix(f) {
            return rest.ends_with(')')
                && rest
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '%' | ' ' | ')'));
        }
    }
    // 具名颜色（red / transparent / currentcolor …）
    s.chars().all(|c| c.is_ascii_alphabetic())
}

/// 把清单里的变量声明归一化为渲染用的列表。
///
/// 非法 key（含 `-`、空格、非 ASCII 等）直接丢弃：它们无法安全地生成
/// CSS 变量名与占位符，留着只会得到「设置了却没生效」的困惑。
fn normalize_vars(raw: BTreeMap<String, TemplateVarDef>) -> Vec<TemplateVar> {
    let mut out: Vec<TemplateVar> = Vec::new();
    for (key, def) in raw {
        if out.len() >= MAX_TEMPLATE_VARS {
            break;
        }
        let key = key.trim().to_ascii_lowercase();
        if !is_valid_var_key(&key) || out.iter().any(|v| v.key == key) {
            continue;
        }
        let (label, default, kind, hint) = match def {
            TemplateVarDef::Plain(v) => (None, v, None, None),
            TemplateVarDef::Full(f) => (
                f.label,
                f.default_value.unwrap_or_default(),
                f.kind,
                f.hint,
            ),
        };
        let kind = match kind.as_deref().map(str::trim) {
            Some(k) if k.eq_ignore_ascii_case("color") => TemplateVarKind::Color,
            _ => TemplateVarKind::Text,
        };
        let default = sanitize_var_value(&default);
        out.push(TemplateVar {
            label: label
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| key.clone()),
            default,
            kind,
            hint: hint.unwrap_or_default().trim().to_string(),
            key,
        });
    }
    out
}

/// 一个可渲染的 HTML 模板：完整 HTML 文档，内含占位符（{{BODY}}、{{TITLE}} 等）。
///
/// 契约：必须包含 `{{BODY}}`；其余占位符可选，缺失时不填充（见 `fill_template`）。
/// 外部模板作者需自行提供样式与交互脚本；内置资源占位符
/// （{{GITHUB_MD_CSS}} 等）仍会注入对应内容，想用直接放占位符即可。
#[derive(Debug, Clone)]
pub struct HtmlTemplate {
    /// 模板 id：内置为 "default"，外部为文件名去扩展名
    pub id: String,
    /// 来源描述（"builtin" 或文件路径），用于错误信息定位
    pub source: String,
    /// 模板 HTML 全文
    html: String,
    /// 模版声明的可调变量（内置模版为空）
    vars: Vec<TemplateVar>,
    /// 用户覆盖值：只保存「确实偏离默认值」的项，故空表 = 全部走默认
    overrides: HashMap<String, String>,
}

impl HtmlTemplate {
    /// 编译期打入的默认模板
    pub fn builtin() -> Self {
        HtmlTemplate {
            id: BUILTIN_TEMPLATE_ID.to_string(),
            source: "builtin".to_string(),
            html: include_str!("../../../templates/mpe.html").to_string(),
            vars: Vec::new(),
            overrides: HashMap::new(),
        }
    }

    /// 从外部路径加载模板：既接受模版包目录（内含 `template.html`），也接受单个 HTML 文件。
    ///
    /// 目录形态是当前约定，单文件形态是旧版遗留（设置里已存在的旧模版不能因此失效）。
    pub fn from_path(path: &Path) -> Result<Self, MpeError> {
        if path.is_dir() {
            return Self::from_file(&path.join(TEMPLATE_HTML));
        }
        Self::from_file(path)
    }

    /// 从外部文件加载模板：读文件并校验 `{{BODY}}` 占位符。
    /// 文件缺失或校验失败均报错，由调用方决定回退策略（GUI 回退内置，CLI 直接报错退出）。
    pub fn from_file(path: &Path) -> Result<Self, MpeError> {
        let html = std::fs::read_to_string(path).map_err(MpeError::IoError)?;
        if !is_valid_template(&html) {
            return Err(MpeError::TemplateError(format!(
                "模板缺少 {{{{BODY}}}} 占位符: {}",
                path.display()
            )));
        }
        // id 取「模版包目录名」或「文件名去扩展名」：入口文件叫 template.html 时
        // 文件名恒为 template，只能退到父目录名才能拿到有意义的 id
        let is_pkg_entry = path.file_name().and_then(|n| n.to_str()) == Some(TEMPLATE_HTML);
        let id = if is_pkg_entry {
            path.parent()
                .and_then(|d| d.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("external")
        } else {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("external")
        }
        .to_string();
        // 变量只在模版包形态下有意义（清单与入口同级）；旧式单文件模版没有清单位置
        let vars = if is_pkg_entry {
            path.parent()
                .and_then(read_manifest)
                .map(|m| normalize_vars(m.vars))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(HtmlTemplate {
            id,
            source: path.to_string_lossy().to_string(),
            html,
            vars,
            overrides: HashMap::new(),
        })
    }

    /// 模板 HTML 全文
    pub fn as_str(&self) -> &str {
        &self.html
    }

    /// 模版声明的可调变量（按 key 排序，顺序稳定）
    pub fn vars(&self) -> &[TemplateVar] {
        &self.vars
    }

    /// 用用户保存的取值覆盖默认值。
    ///
    /// 三道过滤，都是为了「宁可显示默认样子，也不要显示坏样子」：
    /// - 未声明的 key 忽略（清单改过、存档过期都会出现）
    /// - 空值 / 与默认值相同 → 视为没覆盖（删存档即回默认）
    /// - 颜色变量的取值必须像颜色，否则回退默认
    pub fn with_var_overrides(mut self, values: &HashMap<String, String>) -> Self {
        for v in &self.vars {
            let Some(raw) = values.get(&v.key) else {
                continue;
            };
            let val = sanitize_var_value(raw);
            if val.is_empty() || val == v.default {
                continue;
            }
            if v.kind == TemplateVarKind::Color && !is_css_color(&val) {
                continue;
            }
            self.overrides.insert(v.key.clone(), val);
        }
        self
    }

    /// 用户覆盖值（仅含「确实改动过」的项）；调用方用于回写存档。
    pub fn var_overrides(&self) -> &HashMap<String, String> {
        &self.overrides
    }

    /// 变量的实际生效值：覆盖值优先，否则默认值。
    pub fn var_value(&self, key: &str) -> &str {
        if let Some(v) = self.overrides.get(key) {
            return v;
        }
        self.vars
            .iter()
            .find(|v| v.key == key)
            .map(|v| v.default.as_str())
            .unwrap_or("")
    }

    /// 渲染缓存键：模版 id + 变量取值。
    ///
    /// 换模版要重渲染是显然的；同一模版改了配色同样必须失效，
    /// 否则预览会停在旧颜色上，用户以为设置没生效。
    pub fn cache_key(&self) -> String {
        if self.overrides.is_empty() {
            return self.id.clone();
        }
        let mut pairs: Vec<(&String, &String)> = self.overrides.iter().collect();
        pairs.sort();
        let mut s = self.id.clone();
        for (k, v) in pairs {
            s.push('|');
            s.push_str(k);
            s.push('=');
            s.push_str(v);
        }
        s
    }

    /// 自动注入的变量覆盖样式。
    ///
    /// 只输出「用户确实改过」的项，因此没改过的模版渲染结果与手写 HTML
    /// 完全一致：不会把深色档位调过的配色覆盖成浅色档位的值。
    /// 深色档位一并覆盖——用户显式指定了配色，就不该在深色下被模版自带值顶回去。
    fn vars_css(&self) -> String {
        let mut decl = String::new();
        for v in &self.vars {
            if let Some(val) = self.overrides.get(&v.key) {
                decl.push_str(&format!("\n  {}: {};", v.css_name(), val));
            }
        }
        if decl.is_empty() {
            return String::new();
        }
        format!(
            "\n/* 用户自定义变量：设置 → 导出 → 模版变量 */\n\
             :root {{{decl}\n}}\n\
             html[data-theme=\"dark\"] {{{decl}\n}}\n"
        )
    }
}

/// 模板有效性：必须含 `{{BODY}}` 占位符，否则正文无处安放
pub fn is_valid_template(html: &str) -> bool {
    html.contains("{{BODY}}")
}

/// 扫描到的一个模版包/模版文件。
///
/// 字段在扫描阶段就解析完毕，调用方（GUI 设置面板、CLI）不必再次读盘。
#[derive(Debug, Clone)]
pub struct TemplateEntry {
    /// 模版 id：模版包目录名，或旧式单文件模版的文件名去扩展名
    pub id: String,
    /// 展示名：清单 `name`，缺省为 id
    pub name: String,
    /// 一句话描述，缺省空串
    pub description: String,
    /// 版本号，缺省空串
    pub version: String,
    /// 作者，缺省空串
    pub author: String,
    /// 是否为目录式模版包（false 表示旧式单文件模版）
    pub packaged: bool,
    /// 模版 HTML 文件路径
    pub html_path: PathBuf,
    /// 缩略图路径；无缩略图时为 None，由前端渲染占位图
    pub preview: Option<PathBuf>,
    /// 模版声明的可调变量；空表示该模版没有可调项
    pub vars: Vec<TemplateVar>,
}

/// 读取模版包目录；`template.html` 缺失或不合格时返回 None（不进入列表）。
fn read_package(dir: &Path) -> Option<TemplateEntry> {
    let html_path = dir.join(TEMPLATE_HTML);
    let html = std::fs::read_to_string(&html_path).ok()?;
    if !is_valid_template(&html) {
        return None;
    }
    let id = dir.file_name()?.to_str()?.to_string();
    // 清单损坏不应让模版不可用：解析失败时按「无元数据」处理
    let manifest = read_manifest(dir).unwrap_or_default();
    let name = manifest
        .name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| id.clone());
    let preview = manifest
        .preview
        .and_then(|p| {
            let p = dir.join(p);
            p.is_file().then_some(p)
        })
        .or_else(|| {
            PREVIEW_NAMES
                .iter()
                .map(|n| dir.join(n))
                .find(|p| p.is_file())
        });
    Some(TemplateEntry {
        id,
        name,
        description: manifest.description.unwrap_or_default(),
        version: manifest.version.unwrap_or_default(),
        author: manifest.author.unwrap_or_default(),
        packaged: true,
        html_path,
        preview,
        vars: normalize_vars(manifest.vars),
    })
}

/// 读取并解析模版包清单，缺失或非法 JSON 均返回 None
fn read_manifest(dir: &Path) -> Option<TemplateManifest> {
    let raw = std::fs::read_to_string(dir.join(TEMPLATE_MANIFEST)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 读取旧式单文件模版（`<id>.html` 直接放在模版根目录）
fn read_flat(path: &Path) -> Option<TemplateEntry> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| !e.eq_ignore_ascii_case("html"))
        .unwrap_or(true)
    {
        return None;
    }
    let html = std::fs::read_to_string(path).ok()?;
    if !is_valid_template(&html) {
        return None;
    }
    let id = path.file_stem()?.to_str()?.to_string();
    Some(TemplateEntry {
        name: id.clone(),
        id,
        description: String::new(),
        version: String::new(),
        author: String::new(),
        packaged: false,
        html_path: path.to_path_buf(),
        preview: None,
        vars: Vec::new(),
    })
}

/// 扫描模版根目录，识别其中的模版包（子目录）与旧式单文件模版。
///
/// 目录不存在时返回空列表，不报错——外置模版是可选能力。
/// 结果按展示名排序（忽略大小写），名称相同时按 id 兜底，保证顺序稳定。
pub fn scan_templates(dir: &Path) -> Vec<TemplateEntry> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let found = if is_dir {
            read_package(&p)
        } else {
            read_flat(&p)
        };
        if let Some(e) = found {
            out.push(e);
        }
    }
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// 把缩略图内联为 data URL。
///
/// GUI 前端无法直接引用本地文件路径（asset 协议需要额外授权与 scope 配置），
/// 而缩略图只用于列表面板，直接编码进返回值最简单可靠。
/// 非图片扩展名或超过 [`MAX_PREVIEW_BYTES`] 时返回 None，由前端渲染占位图。
pub fn preview_data_url(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    if std::fs::metadata(path).ok()?.len() > MAX_PREVIEW_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    Some(format!(
        "data:{};base64,{}",
        mime,
        crate::converter::base64_encode(&bytes)
    ))
}

/// 按 id 加载模板，永不失败：
/// - "default" / 空 → 内置模板
/// - 其他 id → `<dir>/<id>/template.html`，其次旧式 `<dir>/<id>.html`；
///   均缺失或校验失败时回退内置
///
/// 刻意宽松：设置里残留的失效 id 不应导致预览/导出失败。
pub fn load_template(id: &str, external_dir: &Path) -> HtmlTemplate {
    if id.is_empty() || id == BUILTIN_TEMPLATE_ID {
        return HtmlTemplate::builtin();
    }
    let pkg = external_dir.join(id);
    if pkg.join(TEMPLATE_HTML).is_file() {
        if let Ok(t) = HtmlTemplate::from_file(&pkg.join(TEMPLATE_HTML)) {
            return t;
        }
    }
    HtmlTemplate::from_file(&external_dir.join(format!("{}.html", id)))
        .unwrap_or_else(|_| HtmlTemplate::builtin())
}

/// 主题档位。
///
/// - `Auto`：跟随系统 `prefers-color-scheme`
/// - `Light` / `Dark`：强制指定
///
/// 导出时把当前档位固化进 HTML，保证「导出时看到的样子」与
/// 「接收方打开时的样子」一致——否则对方系统深浅不同时会看到另一套配色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Auto,
    Light,
    Dark,
}

impl Theme {
    /// 解析外部传入的档位字符串；无法识别时回退到 `Auto`。
    ///
    /// 刻意宽松：前端配置、CLI 参数都走这里，脏数据不应导致渲染失败。
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => Theme::Auto,
        }
    }

    /// 写入模板的字符串形式。
    fn as_str(self) -> &'static str {
        match self {
            Theme::Auto => "auto",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

#[allow(dead_code)]
pub fn render_html_template(html_body: &str, title: &str) -> String {
    render_html_template_with_metadata(html_body, title, None, false, Theme::Auto)
}

/// 渲染多文档聚合的完整 HTML 页面。
///
/// 与单文件的 [`render_html_template_with_metadata`] 共用同一套模板与内联资源，
/// 差别在于：正文已是多段拼接好的 section，且侧栏额外注入一棵文档树
/// （`{{DOC_TREE}}`）。文档树为空时该区块由 CSS 隐藏，单文档场景无副作用。
pub fn render_aggregate_html(
    body: &str,
    title: &str,
    doc_tree: &str,
    full_width: bool,
    theme: Theme,
) -> String {
    render_aggregate_html_with(&HtmlTemplate::builtin(), body, title, doc_tree, full_width, theme)
}

/// 渲染多文档聚合页面（指定模板版本）。
///
/// 外部模板若缺少 `{{DOC_TREE}}` 占位符，文档树不会出现（其余内容不受影响）。
pub fn render_aggregate_html_with(
    tpl: &HtmlTemplate,
    body: &str,
    title: &str,
    doc_tree: &str,
    full_width: bool,
    theme: Theme,
) -> String {
    // 聚合页的正文已在合并阶段注入过带前缀的锚点，此处只取正文不再改写 id
    let (items, body_fixed) = inject_heading_ids(body, "");
    let print_toc = crate::print_toc::render_print_toc(&items);
    fill_template(
        tpl,
        body_fixed,
        title,
        "",
        "",
        doc_tree,
        full_width,
        theme,
        &print_toc,
    )
}

/// 渲染单文档的完整 HTML 页面。
pub fn render_html_template_with_metadata(
    html_body: &str,
    title: &str,
    metadata: Option<&HashMap<String, String>>,
    full_width: bool,
    theme: Theme,
) -> String {
    render_html_with(
        &HtmlTemplate::builtin(),
        html_body,
        title,
        metadata,
        full_width,
        theme,
    )
}

/// 渲染单文档的完整 HTML 页面（指定模板版本）。
pub fn render_html_with(
    tpl: &HtmlTemplate,
    html_body: &str,
    title: &str,
    metadata: Option<&HashMap<String, String>>,
    full_width: bool,
    theme: Theme,
) -> String {
    let (items, body_fixed) = inject_heading_ids(html_body, "");
    let toc_html = build_toc_html(&items);
    let metadata_html = metadata
        .map(render_skill_metadata_html)
        .unwrap_or_default();
    let print_toc = crate::print_toc::render_print_toc(&items);
    fill_template(
        tpl,
        body_fixed,
        title,
        &toc_html,
        &metadata_html,
        "",
        full_width,
        theme,
        &print_toc,
    )
}

/// 填充模板占位符：单文档与聚合页面共用的收尾步骤
// 参数对应模板的各占位符，均为独立语义，强行打包成结构体反而降低可读性
#[allow(clippy::too_many_arguments)]
fn fill_template(
    tpl: &HtmlTemplate,
    body_fixed: String,
    title: &str,
    toc_html: &str,
    metadata_html: &str,
    doc_tree: &str,
    full_width: bool,
    theme: Theme,
    print_toc: &str,
) -> String {
    let github_css = include_str!("../../../templates/assets/github-markdown.min.css");
    let atom_css = include_str!("../../../templates/assets/atom-one-dark.min.css");
    let highlight_js = include_str!("../../../templates/assets/highlight.min.js");

    // 全宽模式：放开布局与正文的最大宽度，并隐藏侧栏、归零其占用的空间，
    // 让正文真正占满整屏（100% 撑满 + 小内边距，绝不用 vw 以免 iframe 视口差异）。
    // 仅全宽时注入；{{WIDTH_VARS}} 平时为空。
    let width_vars = if full_width {
        ":root {\
\n  --layout-max-width: none !important;\
\n  --content-max-width: 100% !important;\
\n}\
\n/* 全宽：隐藏侧栏并让正文占满整屏宽度 */\
\n.sidebar,\
\n.sidebar-resizer,\
\n.sidebar-toggle { display: none !important; }\
\n.layout {\
\n  max-width: none !important;\
\n}\
\n.main-content {\
\n  margin-left: 0 !important;\
\n  padding-left: 24px !important;\
\n  padding-right: 24px !important;\
\n}\
\n.markdown-body {\
\n  max-width: 100% !important;\
\n}"
    } else {
        ""
    };

    // {{DOC_TREE}} 必须先于 {{BODY}} 替换：正文里若出现同名占位符字面量，
    // 先替换文档树可避免它被后续步骤当作待填占位符处理。
    let mut out = tpl
        .as_str()
        .replace("{{TITLE}}", title)
        .replace("{{GITHUB_MD_CSS}}", github_css)
        .replace("{{ATOM_ONE_DARK_CSS}}", atom_css)
        .replace("{{HIGHLIGHT_JS}}", highlight_js)
        .replace("{{TOC}}", toc_html)
        .replace("{{SKILL_METADATA}}", metadata_html)
        .replace("{{WIDTH_VARS}}", width_vars)
        // 主题档位固化进 HTML：导出的文件不依赖接收方系统深浅设定
        .replace("{{THEME_LOCK}}", theme.as_str())
        // 文档树：聚合页传入真实树，单文档传空串（外层 CSS 会隐藏该区块）
        .replace("{{DOC_TREE}}", doc_tree)
        // 打印目录页：仅 @media print 下可见，屏幕浏览时 display:none
        .replace("{{PRINT_TOC}}", print_toc)
        // 用户自定义变量：未改过任何变量时为空串，渲染结果与原始模版一致
        .replace("{{VARS_CSS}}", &tpl.vars_css());

    // {{VAR_XXX}} 直接替换也放在正文注入之前：正文里出现同名文本来自用户 Markdown，
    // 那是要原样展示的内容，不该被当成占位符吃掉。
    for v in tpl.vars() {
        let ph = v.placeholder();
        if out.contains(&ph) {
            out = out.replace(&ph, tpl.var_value(&v.key));
        }
    }

    out.replace("{{BODY}}", &body_fixed)
        // 聚合页给 body 加标记，供 CSS 决定是否显示文档树区块
        .replace(
            "<body>",
            if doc_tree.is_empty() {
                "<body>"
            } else {
                "<body class=\"is-aggregate\">"
            },
        )
}

/// 将 SKILL.md 的元数据渲染为 HTML 卡片
fn render_skill_metadata_html(meta: &HashMap<String, String>) -> String {
    let name = meta.get("name").map(|s| s.as_str()).unwrap_or("");
    let description = meta.get("description").map(|s| s.as_str()).unwrap_or("");

    let mut extra = String::new();
    // 渲染 name/description 之外的可选字段
    for key in ["runAs", "scope", "model", "effort", "allowedTools"] {
        if let Some(val) = meta.get(key) {
            extra.push_str(&format!(
                r#"<span class="skill-meta-item"><span class="skill-meta-label">{}</span><span class="skill-meta-value">{}</span></span>"#,
                key,
                val.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
            ));
        }
    }

    format!(
        r#"<div class="skill-metadata">
  <div class="skill-metadata-header">
    <h1>{name}</h1>
    <span class="skill-badge">Skill</span>
  </div>
  <p class="skill-description">{description}</p>
  <div class="skill-meta-grid">{extra}</div>
</div>"#,
        name = name.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"),
        description = description.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"),
        extra = extra,
    )
}

/// 由标题清单生成侧栏目录 HTML。
fn build_toc_html(items: &[HeadingItem]) -> String {
    if items.is_empty() {
        String::new()
    } else {
        render_toc_tree(items)
    }
}

/// 一个标题的元信息：层级、锚点 id、纯文本。
pub struct HeadingItem {
    /// 标题层级 1-6，形如 `"h2"`
    pub level: String,
    /// 锚点 id
    pub id: String,
    /// 标题纯文本
    pub text: String,
    /// 是否位于聚合页的文档 banner 内（`.doc-title`）。
    ///
    /// banner 标题与该篇正文首个一级标题文本相同，若一并计入打印目录，
    /// 会出现同一标题连续两条。打印目录据此排除 banner。
    pub in_banner: bool,
}

/// 判断标题标签是否属于聚合页的文档 banner。
///
/// banner 形如 `<h1 class="doc-title">`，是聚合导出时为每篇文档生成的定位标题，
/// 与该篇正文首个一级标题内容重复。打印目录需据此排除，避免同一条目出现两次。
fn is_banner_heading(attrs: &str) -> bool {
    attrs.contains("doc-title")
}

/// 扫描 body 中的 h1-h6，为缺少 id 的标题注入锚点，返回标题清单与注入后的正文。
///
/// `id_prefix` 用于多文档合并场景：`make_id` 仅由标题文本推导，两篇文档出现
/// 同名标题（如各自的「概述」）时锚点必然重复，导致目录跳转失效。合并时为每篇
/// 文档传入不同前缀（形如 `doc-3-`）即可保证全页面锚点唯一；单文档场景传空串，
/// 行为与原先完全一致。
pub fn inject_heading_ids(body: &str, id_prefix: &str) -> (Vec<HeadingItem>, String) {
    let mut items: Vec<HeadingItem> = Vec::new();
    let mut result = body.to_string();
    let mut counter = 0u64;
    let mut pos = 0;

    // 逐个查找 h1-h6 标签
    while pos < result.len() {
        let remaining = &result[pos..];
        let mut tag_match = None;

        for level in 1..=6 {
            let open = format!("<h{}", level);
            if let Some(start) = remaining.find(&open) {
                match tag_match {
                    None => tag_match = Some((level, start)),
                    Some((_, existing_start)) if start < existing_start => tag_match = Some((level, start)),
                    _ => {}
                }
            }
        }

        let (level, start) = match tag_match {
            Some(v) => v,
            None => break,
        };

        let abs_start = pos + start;
        let tag_start = abs_start;

        // 找到 closing >
        let after_tag = &result[tag_start..];
        let gt = after_tag.find('>').unwrap();
        let content_start = tag_start + gt + 1;

        // 找到 </hN>
        let end_tag = format!("</h{}>", level);
        let after_content = &result[content_start..];
        let et = after_content.find(&end_tag);
        let content_end = match et {
            Some(e) => content_start + e,
            None => { pos = content_start; continue; }
        };

        let raw_content = &result[content_start..content_end];
        let clean = strip_tags(raw_content);
        let text = clean.trim();
        if !text.is_empty() {
            counter += 1;
            let id = prefixed_id(id_prefix, text, counter);

            // 注入 id 属性到 heading 标签
            let before_gt = &result[tag_start..content_start - 1]; // <hN ...attrs...
            let has_id = before_gt.contains(" id=");
            if !has_id {
                let ins = format!(" id=\"{}\"", id);
                result.insert_str(content_start - 1, &ins);
                // 调整位置偏移
                let shift = ins.len();
                items.push(HeadingItem {
                    level: format!("h{}", level),
                    id,
                    text: text.to_string(),
                    in_banner: is_banner_heading(&result[tag_start..content_start]),
                });
                pos = content_end + end_tag.len() + shift;
            } else {
                // 已有 id：沿用原值，不加前缀（尊重作者显式指定）
                let existing = extract_attr(&result[tag_start..content_start], "id");
                items.push(HeadingItem {
                    level: format!("h{}", level),
                    id: existing.unwrap_or_else(|| prefixed_id(id_prefix, text, counter)),
                    text: text.to_string(),
                    in_banner: is_banner_heading(&result[tag_start..content_start]),
                });
                pos = content_end + end_tag.len();
            }
        } else {
            pos = content_end + end_tag.len();
        }
    }

    (items, result)
}

/// 读取形如 `id="xxx"` 的属性值
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let pat = format!("{}=\"", attr);
    let i = tag.find(&pat)?;
    let start = i + pat.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}

/// 生成带前缀的锚点 id；前缀为空时退化为原有行为。
fn prefixed_id(prefix: &str, text: &str, counter: u64) -> String {
    if prefix.is_empty() {
        return make_id(text, counter);
    }
    format!("{}{}", prefix, make_id(text, counter))
}

/// 把链接里的锚点文本规范化为可与标题 id 比对的形式。
///
/// 合并多文档时，链接锚点来自作者手写（可能是 `%E6%A0%87%E9%A2%98`、含空格、
/// 大小写混杂），而标题 id 由 `make_id` 生成。两侧都用本函数归一后才能匹配。
pub fn normalize_anchor(text: &str) -> String {
    make_id(text, 0)
}

/// TOC 树节点：仅保存渲染所需字段，`children` 存子节点在 arena 中的下标
struct TocNode {
    /// 标题层级 1-6
    level: u32,
    /// 锚点 id（同时作为折叠状态的持久化 key）
    id: String,
    /// 标题纯文本
    text: String,
    /// 子节点下标
    children: Vec<usize>,
}

/// 将扁平的标题序列按层级还原为树，再渲染成嵌套 `ul/li`。
///
/// 用 arena（Vec + 下标）而非嵌套结构体，避免在遍历过程中同时持有父节点
/// 可变借用与新节点自身。
fn build_toc_forest(items: &[HeadingItem]) -> (Vec<TocNode>, Vec<usize>) {
    let mut arena: Vec<TocNode> = Vec::with_capacity(items.len());
    let mut roots: Vec<usize> = Vec::new();
    // 祖先链栈，栈顶即当前标题的父节点
    let mut stack: Vec<usize> = Vec::new();

    for item in items {
        let lvl: u32 = item.level[1..].parse().unwrap_or(1);
        // 弹出层级 >= 当前标题的节点，它们不再是祖先（兼容 h1 后直接出现 h3 的跳级写法）
        while let Some(&top) = stack.last() {
            if arena[top].level >= lvl {
                stack.pop();
            } else {
                break;
            }
        }

        let idx = arena.len();
        arena.push(TocNode {
            level: lvl,
            id: item.id.clone(),
            text: item.text.clone(),
            children: Vec::new(),
        });
        match stack.last() {
            Some(&parent) => arena[parent].children.push(idx),
            None => roots.push(idx),
        }
        stack.push(idx);
    }

    (arena, roots)
}

/// 渲染嵌套 TOC 的节点列表（不含外层 `ul`，外层由模板提供）。
///
/// 有子节点的 `li` 额外带折叠箭头（`.toc-toggle`），叶子节点的箭头占位隐藏以保持对齐。
fn render_toc_tree(items: &[HeadingItem]) -> String {
    let (arena, roots) = build_toc_forest(items);
    let mut out = String::new();
    for &root in &roots {
        render_toc_node(&arena, root, &mut out);
    }
    out
}

fn render_toc_node(arena: &[TocNode], idx: usize, out: &mut String) {
    let node = &arena[idx];
    let has_children = !node.children.is_empty();
    let cls = format!("toc-h{}", node.level.min(6));
    let safe = escape_html(&node.text);
    // data-key 供前端按锚点 id 记忆每个节点的折叠状态
    out.push_str(&format!(
        "<li class=\"toc-node\" data-key=\"{}\"><div class=\"toc-row\">",
        escape_html(&node.id)
    ));
    out.push_str(&format!(
        "<button type=\"button\" class=\"toc-toggle{}\" aria-label=\"toggle\" aria-expanded=\"true\"><svg viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M6 4l4 4-4 4\"/></svg></button>",
        if has_children { "" } else { " toc-toggle-leaf" }
    ));
    out.push_str(&format!(
        "<a href=\"#{}\" class=\"{}\">{}</a></div>",
        node.id, cls, safe
    ));

    if has_children {
        out.push_str("<ul class=\"toc-children\">");
        for &child in &node.children {
            render_toc_node(arena, child, out);
        }
        out.push_str("</ul>");
    }
    out.push_str("</li>");
}

/// TOC 文本来自剥离标签后的标题内容，仍需转义以防`<` 之类字符破坏结构
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn make_id(text: &str, counter: u64) -> String {
    let id: String = text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '_')
        .collect();
    let id = id.trim().to_lowercase();
    let id: String = id.split_whitespace().collect::<Vec<_>>().join("-");
    if id.is_empty() { format!("heading-{}", counter) } else { id }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(levels: &[(&str, u32)]) -> Vec<HeadingItem> {
        levels
            .iter()
            .enumerate()
            .map(|(i, (t, l))| HeadingItem {
                level: format!("h{}", l),
                id: format!("id-{}", i),
                text: t.to_string(),
                in_banner: false,
            })
            .collect()
    }

    #[test]
    fn 层级被还原为父子结构() {
        let list = items(&[("A", 1), ("B", 2), ("C", 3), ("D", 2)]);
        let (arena, roots) = build_toc_forest(&list);
        assert_eq!(roots.len(), 1, "h1 应为唯一根节点");
        let a = &arena[roots[0]];
        assert_eq!(a.text, "A");
        assert_eq!(a.children.len(), 2, "A 下应挂 B 与 D 两个子节点");
        assert_eq!(arena[a.children[0]].children.len(), 1, "B 下应挂 C");
        let c = a.children[0];
        assert_eq!(arena[arena[c].children[0]].text, "C");
        assert!(arena[a.children[1]].children.is_empty(), "D 为叶子");
    }

    #[test]
    fn 跳级标题挂到最近的较浅祖先() {
        // h1 后直接出现 h3：h3 应作为 h1 的子节点，而不是新建根
        let list = items(&[("A", 1), ("C", 3)]);
        let (arena, roots) = build_toc_forest(&list);
        assert_eq!(roots.len(), 1);
        assert_eq!(arena[roots[0]].children.len(), 1);
    }

    #[test]
    fn 多个根标题保持平级() {
        let list = items(&[("A", 1), ("B", 1), ("C", 2)]);
        let (arena, roots) = build_toc_forest(&list);
        assert_eq!(roots.len(), 2);
        assert_eq!(arena[roots[1]].children.len(), 1, "C 挂在 B 下");
    }

    #[test]
    fn 渲染结果含折叠按钮且叶子占位对齐() {
        // A 下挂 B，A 有子节点应给可折叠按钮；B 为叶子应输出占位按钮以保持缩进对齐
        let list = items(&[("A", 1), ("B", 2)]);
        let html = render_toc_tree(&list);
        assert!(html.starts_with("<li class=\"toc-node\""), "只输出节点，外层 ul 由模板提供");
        assert!(html.contains("class=\"toc-toggle\""), "有子节点应带可折叠按钮");
        assert!(html.contains("class=\"toc-toggle toc-toggle-leaf\""), "叶子应输出占位按钮");
        assert!(html.contains("data-key=\"id-0\""));
        assert!(html.contains("class=\"toc-h2\""), "层级类名用于字号缩进");
        assert_eq!(html.matches("toc-toggle").count(), 3, "两个按钮各含一次类名，子 ul 一次");
    }

    #[test]
    fn 标题文本被转义防止破坏结构() {
        let list = vec![HeadingItem {
            level: "h2".to_string(),
            id: "x".to_string(),
            in_banner: false,
            text: "<b>&\"".to_string(),
        }];
        let html = render_toc_tree(&list);
        assert!(html.contains("&lt;b&gt;&amp;&quot;"));
        assert!(!html.contains("<b>"));
    }

    #[test]
    fn 正文标题被注入锚点id() {
        let body = "<h1>标题一</h1><p>正文</p><h2>子节</h2>";
        let (items, fixed) = inject_heading_ids(body, "");
        let toc = build_toc_html(&items);
        assert!(fixed.contains("id=\"标题一\""));
        assert!(fixed.contains("id=\"子节\""));
        assert!(toc.contains("href=\"#标题一\""));
        assert!(toc.contains("toc-children"), "h2 应嵌套在 h1 下");
    }

    #[test]
    fn 无标题时不输出目录容器() {
        let (items, _) = inject_heading_ids("<p>只有正文</p>", "");
        assert!(build_toc_html(&items).is_empty());
    }

    #[test]
    fn 已带id的标题不重复注入() {
        let body = "<h2 id=\"custom\">已有 id</h2>";
        let (items, fixed) = inject_heading_ids(body, "");
        let _ = build_toc_html(&items);
        assert_eq!(fixed.matches("id=\"custom\"").count(), 1);
    }

    #[test]
    fn 前缀使同名标题锚点互不冲突() {
        // 合并场景核心保障：两篇文档都有「概述」时，锚点必须区分开
        let body = "<h2>概述</h2>";
        let (_, a) = inject_heading_ids(body, "doc-a-");
        let (_, b) = inject_heading_ids(body, "doc-b-");
        assert!(a.contains("id=\"doc-a-概述\""));
        assert!(b.contains("id=\"doc-b-概述\""));
        assert_ne!(
            a.matches("id=\"").count(),
            0,
            "带前缀时仍应注入 id"
        );
    }

    #[test]
    fn 空前缀时行为与原先一致() {
        // 单文档导出走空前缀，回归保护：不能因为重构而改变原有锚点
        let (_, body) = inject_heading_ids("<h2>概述</h2>", "");
        assert!(body.contains("id=\"概述\""));
    }

    #[test]
    fn 前缀模式下返回的标题项带前缀id() {
        let body = "<h1>A</h1><h2>B</h2>";
        let (items, body) = inject_heading_ids(body, "d1-");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "d1-a");
        assert_eq!(items[1].id, "d1-b");
        assert!(body.contains("id=\"d1-a\""));
    }

    #[test]
    fn 作者显式指定的id不被前缀覆盖() {
        // 尊重作者手写的锚点，否则文内自引链接会失效
        let body = "<h2 id=\"custom\">标题</h2>";
        let (items, body) = inject_heading_ids(body, "d1-");
        assert!(body.contains("id=\"custom\""));
        assert_eq!(items[0].id, "custom");
    }

    #[test]
    fn 主题档位解析() {
        assert_eq!(Theme::parse("dark"), Theme::Dark);
        assert_eq!(Theme::parse("LIGHT"), Theme::Light, "应大小写不敏感");
        assert_eq!(Theme::parse(" light "), Theme::Light, "应容忍首尾空白");
        assert_eq!(Theme::parse("auto"), Theme::Auto);
    }

    #[test]
    fn 非法主题回退到跟随系统() {
        // 脏数据不应导致渲染失败
        assert_eq!(Theme::parse(""), Theme::Auto);
        assert_eq!(Theme::parse("blue"), Theme::Auto);
        assert_eq!(Theme::parse("dark-mode"), Theme::Auto);
    }

    // ===== 模板加载与校验 =====

    #[test]
    fn 内置模板有效且id为default() {
        let t = HtmlTemplate::builtin();
        assert_eq!(t.id, BUILTIN_TEMPLATE_ID);
        assert!(is_valid_template(t.as_str()));
        assert!(t.as_str().contains("{{BODY}}"));
    }

    #[test]
    fn 缺body占位符的模板无效() {
        assert!(!is_valid_template("<html><body>正文</body></html>"));
        assert!(is_valid_template("<html><body>{{BODY}}</body></html>"));
    }

    #[test]
    fn 从文件加载模板并取文件名为id() {
        let dir = std::env::temp_dir().join("md2x-tpl-test-load");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("paper.html");
        std::fs::write(&p, "<html><body>{{BODY}}</body></html>").unwrap();
        let t = HtmlTemplate::from_file(&p).unwrap();
        assert_eq!(t.id, "paper");
        assert!(t.as_str().contains("{{BODY}}"));
    }

    #[test]
    fn 加载缺占位符模板报错() {
        let dir = std::env::temp_dir().join("md2x-tpl-test-invalid");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("broken.html");
        std::fs::write(&p, "<html><body>no placeholder</body></html>").unwrap();
        assert!(HtmlTemplate::from_file(&p).is_err());
    }

    #[test]
    fn 加载不存在的模板文件报错() {
        let p = std::env::temp_dir().join("md2x-tpl-not-exist-xyz.html");
        assert!(HtmlTemplate::from_file(&p).is_err());
    }

    #[test]
    fn load_template内置id与空串返回内置() {
        let dir = std::env::temp_dir().join("md2x-tpl-test-dir");
        let t1 = load_template(BUILTIN_TEMPLATE_ID, &dir);
        let t2 = load_template("", &dir);
        assert_eq!(t1.id, BUILTIN_TEMPLATE_ID);
        assert_eq!(t2.id, BUILTIN_TEMPLATE_ID);
    }

    #[test]
    fn load_template外部id命中文件() {
        let dir = std::env::temp_dir().join("md2x-tpl-test-hit");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("custom.html"), "<html><body>{{BODY}}</body></html>").unwrap();
        let t = load_template("custom", &dir);
        assert_eq!(t.id, "custom");
    }

    #[test]
    fn load_template失效id回退内置() {
        // 设置里残留的失效 id 不应导致渲染失败
        let dir = std::env::temp_dir().join("md2x-tpl-test-fallback");
        std::fs::create_dir_all(&dir).unwrap();
        let t = load_template("missing-template", &dir);
        assert_eq!(t.id, BUILTIN_TEMPLATE_ID);
    }

    #[test]
    fn 扫描外部模板只收含占位符的html() {
        let dir = std::env::temp_dir().join("md2x-tpl-test-scan");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b.html"), "<html><body>{{BODY}}</body></html>").unwrap();
        std::fs::write(dir.join("a.html"), "<html><body>{{BODY}}</body></html>").unwrap();
        std::fs::write(dir.join("bad.html"), "<html><body>x</body></html>").unwrap();
        std::fs::write(dir.join("note.txt"), "{{BODY}}").unwrap();
        let found = scan_templates(&dir);
        let ids: Vec<&str> = found.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["a", "b"],
            "无效文件与非 html 应被排除，且按名称排序"
        );
    }

    #[test]
    fn 扫描不存在的目录返回空() {
        let dir = std::env::temp_dir().join("md2x-tpl-test-no-such-dir-xyz");
        assert!(scan_templates(&dir).is_empty());
    }

    // ===== 模版包（目录 + template.html + template.json + 缩略图） =====

    /// 构造一个模版包目录，返回其根目录（调用方再往里塞文件）
    fn pkg_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("md2x-tpl-pkg-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_pkg(root: &Path, id: &str, manifest: Option<&str>, preview: Option<&str>) {
        let d = root.join(id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(TEMPLATE_HTML), "<html><body>{{BODY}}</body></html>").unwrap();
        if let Some(m) = manifest {
            std::fs::write(d.join(TEMPLATE_MANIFEST), m).unwrap();
        }
        if let Some(p) = preview {
            std::fs::write(d.join(p), b"\x89PNG\r\n\x1a\n").unwrap();
        }
    }

    #[test]
    fn 模版包按清单解析元数据与约定缩略图() {
        let root = pkg_root("meta");
        write_pkg(
            &root,
            "paper",
            Some(
                r#"{"name":"论文体","description":"学术排版","version":"1.2.0","author":"yujinping"}"#,
            ),
            Some("preview.png"),
        );
        let found = scan_templates(&root);
        assert_eq!(found.len(), 1);
        let e = &found[0];
        assert_eq!(e.id, "paper", "id 取目录名");
        assert_eq!(e.name, "论文体", "展示名优先取清单");
        assert_eq!(e.description, "学术排版");
        assert_eq!(e.version, "1.2.0");
        assert_eq!(e.author, "yujinping");
        assert!(e.packaged);
        assert_eq!(
            e.preview.as_deref(),
            Some(root.join("paper/preview.png").as_path())
        );
    }

    #[test]
    fn 缺清单时以目录名为名且仍可用() {
        // 作者可以只放 template.html：能列出、能选中，只是没有描述与缩略图
        let root = pkg_root("no-manifest");
        write_pkg(&root, "plain", None, None);
        let found = scan_templates(&root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "plain");
        assert!(found[0].description.is_empty());
        assert!(found[0].preview.is_none());
    }

    #[test]
    fn 清单非法json时退化为无元数据() {
        let root = pkg_root("bad-json");
        write_pkg(&root, "broken", Some("{ not json"), None);
        let found = scan_templates(&root);
        assert_eq!(found.len(), 1, "清单损坏不应让模版从列表消失");
        assert_eq!(found[0].name, "broken");
    }

    #[test]
    fn 清单preview字段可指定非约定缩略图名() {
        let root = pkg_root("custom-preview");
        write_pkg(
            &root,
            "custom",
            Some(r#"{"preview":"shot.webp"}"#),
            Some("shot.webp"),
        );
        let found = scan_templates(&root);
        assert_eq!(
            found[0].preview.as_deref(),
            Some(root.join("custom/shot.webp").as_path())
        );
    }

    #[test]
    fn 缺template_html的目录不进列表() {
        let root = pkg_root("missing-html");
        let d = root.join("empty");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(TEMPLATE_MANIFEST), r#"{"name":"x"}"#).unwrap();
        // 入口文件名写错（index.html）同样不算模版包：约定即固定名
        let d2 = root.join("wrong-entry");
        std::fs::create_dir_all(&d2).unwrap();
        std::fs::write(d2.join("index.html"), "<html><body>{{BODY}}</body></html>").unwrap();
        assert!(scan_templates(&root).is_empty());
    }

    #[test]
    fn 模版包与旧式单文件模版可共存并按名排序() {
        let root = pkg_root("mixed");
        write_pkg(&root, "zeta", Some(r#"{"name":"A 模版"}"#), None);
        std::fs::write(
            root.join("legacy.html"),
            "<html><body>{{BODY}}</body></html>",
        )
        .unwrap();
        let found = scan_templates(&root);
        let ids: Vec<&str> = found.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["zeta", "legacy"],
            "按展示名排序，A 模版 在 legacy 之前"
        );
        assert!(!found[1].packaged, "旧式单文件应标记为非模版包");
    }

    #[test]
    fn load_template命中模版包入口() {
        let root = pkg_root("load-pkg");
        write_pkg(&root, "paper", None, None);
        let t = load_template("paper", &root);
        assert_eq!(t.id, "paper", "id 应为目录名而非 template");
    }

    #[test]
    fn from_path接受模版包目录与单文件() {
        let root = pkg_root("from-path");
        write_pkg(&root, "paper", None, None);
        assert_eq!(
            HtmlTemplate::from_path(&root.join("paper")).unwrap().id,
            "paper"
        );
        assert_eq!(
            HtmlTemplate::from_path(&root.join("paper/template.html"))
                .unwrap()
                .id,
            "paper"
        );
        std::fs::write(root.join("flat.html"), "<html><body>{{BODY}}</body></html>").unwrap();
        assert_eq!(
            HtmlTemplate::from_path(&root.join("flat.html")).unwrap().id,
            "flat"
        );
    }

    #[test]
    fn 缩略图编码为data_url并按扩展名判mime() {
        let root = pkg_root("preview-url");
        let p = root.join("preview.png");
        std::fs::write(&p, b"hi").unwrap();
        let url = preview_data_url(&p).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(url.ends_with("aGk="), "内容应为 hi 的 base64");
        // 非图片扩展名不编码，避免把任意文件塞进 img src
        let txt = root.join("note.txt");
        std::fs::write(&txt, b"x").unwrap();
        assert!(preview_data_url(&txt).is_none());
    }

    #[test]
    fn 自定义模板能完成渲染填充() {
        // 端到端：外部模板 + 正文 → 完整页面，正文落位于 {{BODY}} 处
        let tpl = HtmlTemplate {
            id: "mini".to_string(),
            source: "test".to_string(),
            html: "<html><head><title>{{TITLE}}</title></head><body>{{BODY}}</body></html>"
                .to_string(),
            vars: Vec::new(),
            overrides: HashMap::new(),
        };
        let out = render_html_with(&tpl, "<h2>标题</h2><p>正文</p>", "文档", None, false, Theme::Auto);
        assert!(out.contains("<title>文档</title>"));
        assert!(out.contains("正文"));
        assert!(!out.contains("{{BODY}}"), "渲染后不应残留 BODY 占位符");
        assert!(out.contains("id=\"标题\""), "TOC 锚点注入与模板无关，应照常生效");
    }

    // ===== 模版变量 =====

    /// 造一个带变量声明的模版包，返回目录
    fn var_pkg(name: &str, manifest: &str, html: &str) -> PathBuf {
        let dir = pkg_root(name).join("theme");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(TEMPLATE_MANIFEST), manifest).unwrap();
        std::fs::write(dir.join(TEMPLATE_HTML), html).unwrap();
        dir
    }

    #[test]
    fn 清单变量支持简写与完整写法() {
        let dir = var_pkg(
            "var-parse",
            r##"{"vars":{
                "accent":{"label":"主色","default":"#2f6feb","type":"color","hint":"整站主色"},
                "font_size":"17px",
                "Bad-Key":"x"
            }}"##,
            "<html><body>{{BODY}}</body></html>",
        );
        let tpl = HtmlTemplate::from_path(&dir).unwrap();
        let vars = tpl.vars();
        assert_eq!(vars.len(), 2, "非法 key（含短横线）应被丢弃");
        assert_eq!(vars[0].key, "accent");
        assert_eq!(vars[0].kind, TemplateVarKind::Color);
        assert_eq!(vars[0].label, "主色");
        assert_eq!(vars[0].hint, "整站主色");
        // 简写只给默认值，标签退化为 key，形态为文本
        assert_eq!(vars[1].key, "font_size");
        assert_eq!(vars[1].default, "17px");
        assert_eq!(vars[1].label, "font_size");
        assert_eq!(vars[1].kind, TemplateVarKind::Text);
    }

    #[test]
    fn 变量覆盖生成注入样式并遵守默认值() {
        let dir = var_pkg(
            "var-css",
            r##"{"vars":{"accent":{"default":"#2f6feb","type":"color"},"font_size":"16px"}}"##,
            "<html><head><style>:root{--accent:#2f6feb}</style><style>{{VARS_CSS}}</style></head>\
             <body>{{BODY}}</body></html>",
        );
        let tpl = HtmlTemplate::from_path(&dir).unwrap();

        // 没有任何覆盖：{{VARS_CSS}} 空，渲染结果与手写 HTML 一致
        let plain = render_html_with(&tpl, "<p>x</p>", "t", None, false, Theme::Auto);
        assert!(plain.contains("<style></style>"), "未覆盖时不应注入任何样式");

        let mut ov = HashMap::new();
        ov.insert("accent".to_string(), "#e11d48".to_string());
        ov.insert("font_size".to_string(), "18px".to_string());
        let tpl = tpl.with_var_overrides(&ov);
        let out = render_html_with(&tpl, "<p>x</p>", "t", None, false, Theme::Auto);
        assert!(out.contains("--accent: #e11d48;"), "自定义主色应注入");
        assert!(out.contains("--font-size: 18px;"), "下划线应转成 CSS 连字符");
        assert!(
            out.contains("html[data-theme=\"dark\"]"),
            "深色档位同样要覆盖，否则深色下会被模版自带值顶回去"
        );
    }

    #[test]
    fn 变量直接占位符按key大写替换() {
        let dir = var_pkg(
            "var-direct",
            r##"{"vars":{"brand":{"label":"品牌名","default":"md2x"}}}"##,
            "<html><body><span>{{VAR_BRAND}}</span>{{BODY}}</body></html>",
        );
        let mut ov = HashMap::new();
        ov.insert("brand".to_string(), "内部文档".to_string());
        let tpl = HtmlTemplate::from_path(&dir).unwrap().with_var_overrides(&ov);
        let out = render_html_with(&tpl, "<p>x</p>", "t", None, false, Theme::Auto);
        assert!(out.contains("<span>内部文档</span>"));
        assert!(!out.contains("{{VAR_BRAND}}"));
    }

    #[test]
    fn 变量取值挡注入且非法颜色回退默认() {
        assert_eq!(sanitize_var_value("  16px  "), "16px");
        // 能打破声明/标签的字符一律剔除，正常的字体栈与 rgb() 放行
        assert_eq!(sanitize_var_value("a;b{}<c>\"d\""), "abcd");
        assert_eq!(sanitize_var_value("Georgia, serif"), "Georgia, serif");
        assert_eq!(sanitize_var_value("rgb(1, 2, 3)"), "rgb(1, 2, 3)");

        let dir = var_pkg(
            "var-inject",
            r##"{"vars":{"accent":{"default":"#2f6feb","type":"color"}}}"##,
            "<html><head><style>{{VARS_CSS}}</style></head><body>{{BODY}}</body></html>",
        );
        let mut ov = HashMap::new();
        ov.insert("accent".to_string(), "12px".to_string());
        let tpl = HtmlTemplate::from_path(&dir).unwrap().with_var_overrides(&ov);
        assert!(
            tpl.cache_key() == "theme",
            "非法颜色视为未覆盖，缓存键应仍是纯 id"
        );
    }

    #[test]
    fn 缓存键区分变量取值() {
        let dir = var_pkg(
            "var-cache",
            r##"{"vars":{"accent":{"default":"#2f6feb","type":"color"}}}"##,
            "<html><head><style>{{VARS_CSS}}</style></head><body>{{BODY}}</body></html>",
        );
        let base = HtmlTemplate::from_path(&dir).unwrap();
        let mut a = HashMap::new();
        a.insert("accent".to_string(), "#e11d48".to_string());
        let mut b = HashMap::new();
        b.insert("accent".to_string(), "#16a34a".to_string());
        let ka = base.clone().with_var_overrides(&a).cache_key();
        let kb = base.clone().with_var_overrides(&b).cache_key();
        assert_ne!(ka, kb, "改了配色必须换缓存键，否则预览停在旧颜色");
        assert_eq!(ka, base.clone().with_var_overrides(&a).cache_key(), "同值应同键");
    }

    #[test]
    fn 正文里的变量占位符不被替换() {
        // 用户 Markdown 里写的 {{VAR_X}} 是要展示的文本，不能被当成占位符吃掉
        let dir = var_pkg(
            "var-body",
            r##"{"vars":{"accent":{"default":"#2f6feb"}}}"##,
            "<html><body>{{BODY}}</body></html>",
        );
        let tpl = HtmlTemplate::from_path(&dir).unwrap();
        let out = render_html_with(&tpl, "<p>{{VAR_ACCENT}}</p>", "t", None, false, Theme::Auto);
        assert!(out.contains("<p>{{VAR_ACCENT}}</p>"));
    }
}
