use std::collections::HashMap;

#[allow(dead_code)]
pub fn render_html_template(html_body: &str, title: &str) -> String {
    render_html_template_with_metadata(html_body, title, None, false)
}

pub fn render_html_template_with_metadata(
    html_body: &str,
    title: &str,
    metadata: Option<&HashMap<String, String>>,
    full_width: bool,
) -> String {
    let template = include_str!("../../../templates/mpe.html");
    let github_css = include_str!("../../../templates/assets/github-markdown.min.css");
    let atom_css = include_str!("../../../templates/assets/atom-one-dark.min.css");
    let highlight_js = include_str!("../../../templates/assets/highlight.min.js");

    let (toc_html, body_fixed) = generate_toc(html_body);
    let metadata_html = metadata
        .map(render_skill_metadata_html)
        .unwrap_or_default();

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

    template
        .replace("{{TITLE}}", title)
        .replace("{{GITHUB_MD_CSS}}", github_css)
        .replace("{{ATOM_ONE_DARK_CSS}}", atom_css)
        .replace("{{HIGHLIGHT_JS}}", highlight_js)
        .replace("{{TOC}}", &toc_html)
        .replace("{{SKILL_METADATA}}", &metadata_html)
        .replace("{{WIDTH_VARS}}", width_vars)
        .replace("{{BODY}}", &body_fixed)
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

fn generate_toc(body: &str) -> (String, String) {
    let mut items: Vec<(String, String, String)> = Vec::new(); // (level, id, text)
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
            let id = make_id(text, counter);

            // 注入 id 属性到 heading 标签
            let before_gt = &result[tag_start..content_start - 1]; // <hN ...attrs...
            let has_id = before_gt.contains(" id=");
            if !has_id {
                let ins = format!(" id=\"{}\"", id);
                result.insert_str(content_start - 1, &ins);
                // 调整位置偏移
                let shift = ins.len();
                items.push((format!("h{}", level), id, text.to_string()));
                pos = content_end + end_tag.len() + shift;
            } else {
                items.push((format!("h{}", level), id, text.to_string()));
                pos = content_end + end_tag.len();
            }
        } else {
            pos = content_end + end_tag.len();
        }
    }

    // 构建 TOC HTML：按标题层级重建为可折叠的嵌套树
    let toc = if items.is_empty() {
        String::new()
    } else {
        render_toc_tree(&items)
    };

    (toc, result)
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
fn build_toc_forest(items: &[(String, String, String)]) -> (Vec<TocNode>, Vec<usize>) {
    let mut arena: Vec<TocNode> = Vec::with_capacity(items.len());
    let mut roots: Vec<usize> = Vec::new();
    // 祖先链栈，栈顶即当前标题的父节点
    let mut stack: Vec<usize> = Vec::new();

    for (level, id, text) in items {
        let lvl: u32 = level[1..].parse().unwrap_or(1);
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
            id: id.clone(),
            text: text.clone(),
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
fn render_toc_tree(items: &[(String, String, String)]) -> String {
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

    fn items(levels: &[(&str, u32)]) -> Vec<(String, String, String)> {
        levels
            .iter()
            .enumerate()
            .map(|(i, (t, l))| {
                (format!("h{}", l), format!("id-{}", i), t.to_string())
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
        let list = vec![("h2".to_string(), "x".to_string(), "<b>&\"".to_string())];
        let html = render_toc_tree(&list);
        assert!(html.contains("&lt;b&gt;&amp;&quot;"));
        assert!(!html.contains("<b>"));
    }

    #[test]
    fn 正文标题被注入锚点id() {
        let body = "<h1>标题一</h1><p>正文</p><h2>子节</h2>";
        let (toc, fixed) = generate_toc(body);
        assert!(fixed.contains("id=\"标题一\""));
        assert!(fixed.contains("id=\"子节\""));
        assert!(toc.contains("href=\"#标题一\""));
        assert!(toc.contains("toc-children"), "h2 应嵌套在 h1 下");
    }

    #[test]
    fn 无标题时不输出目录容器() {
        let (toc, _) = generate_toc("<p>只有正文</p>");
        assert!(toc.is_empty());
    }

    #[test]
    fn 已带id的标题不重复注入() {
        let body = "<h2 id=\"custom\">已有 id</h2>";
        let (_, fixed) = generate_toc(body);
        assert_eq!(fixed.matches("id=\"custom\"").count(), 1);
    }
}
