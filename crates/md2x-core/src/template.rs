use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::MpeError;

/// 内置默认模板的 id。设置面板、CLI 均用该 id 指代编译进二进制的 mpe.html。
pub const BUILTIN_TEMPLATE_ID: &str = "default";

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
}

impl HtmlTemplate {
    /// 编译期打入的默认模板
    pub fn builtin() -> Self {
        HtmlTemplate {
            id: BUILTIN_TEMPLATE_ID.to_string(),
            source: "builtin".to_string(),
            html: include_str!("../../../templates/mpe.html").to_string(),
        }
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
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("external")
            .to_string();
        Ok(HtmlTemplate {
            id,
            source: path.to_string_lossy().to_string(),
            html,
        })
    }

    /// 模板 HTML 全文
    pub fn as_str(&self) -> &str {
        &self.html
    }
}

/// 模板有效性：必须含 `{{BODY}}` 占位符，否则正文无处安放
pub fn is_valid_template(html: &str) -> bool {
    html.contains("{{BODY}}")
}

/// 外部模板扫描结果：id（文件名去扩展名）与文件路径
#[derive(Debug, Clone)]
pub struct ExternalTemplateInfo {
    pub id: String,
    pub path: PathBuf,
}

/// 扫描外部模板目录下可用的模板（*.html 且含 {{BODY}}），按文件名排序。
/// 目录不存在时返回空列表，不报错——外置模板是可选能力。
pub fn scan_external_templates(dir: &Path) -> Vec<ExternalTemplateInfo> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("html")) != Some(true)
        {
            continue;
        }
        let ok = std::fs::read_to_string(&p)
            .map(|s| is_valid_template(&s))
            .unwrap_or(false);
        if !ok {
            continue;
        }
        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
            out.push(ExternalTemplateInfo {
                id: stem.to_string(),
                path: p,
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// 按 id 加载模板，永不失败：
/// - "default" / 空 → 内置模板
/// - 其他 id → 外部目录下 `<id>.html`；文件缺失或校验失败时回退内置
///
/// 刻意宽松：设置里残留的失效 id 不应导致预览/导出失败。
pub fn load_template(id: &str, external_dir: &Path) -> HtmlTemplate {
    if id.is_empty() || id == BUILTIN_TEMPLATE_ID {
        return HtmlTemplate::builtin();
    }
    let path = external_dir.join(format!("{}.html", id));
    HtmlTemplate::from_file(&path).unwrap_or_else(|_| HtmlTemplate::builtin())
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
        tpl.as_str(),
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
        tpl.as_str(),
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
    template: &str,
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
    template
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
        .replace("{{BODY}}", &body_fixed)
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
        let found = scan_external_templates(&dir);
        let ids: Vec<&str> = found.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"], "无效文件与非 html 应被排除，且按名称排序");
    }

    #[test]
    fn 扫描不存在的目录返回空() {
        let dir = std::env::temp_dir().join("md2x-tpl-test-no-such-dir-xyz");
        assert!(scan_external_templates(&dir).is_empty());
    }

    #[test]
    fn 自定义模板能完成渲染填充() {
        // 端到端：外部模板 + 正文 → 完整页面，正文落位于 {{BODY}} 处
        let tpl = HtmlTemplate {
            id: "mini".to_string(),
            source: "test".to_string(),
            html: "<html><head><title>{{TITLE}}</title></head><body>{{BODY}}</body></html>"
                .to_string(),
        };
        let out = render_html_with(&tpl, "<h2>标题</h2><p>正文</p>", "文档", None, false, Theme::Auto);
        assert!(out.contains("<title>文档</title>"));
        assert!(out.contains("正文"));
        assert!(!out.contains("{{BODY}}"), "渲染后不应残留 BODY 占位符");
        assert!(out.contains("id=\"标题\""), "TOC 锚点注入与模板无关，应照常生效");
    }
}
