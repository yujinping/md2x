//! 文件夹聚合导出：把一个目录树内的多篇 Markdown 合并为单个自包含 HTML。
//!
//! 与单文件导出的差异在于三处：
//! 1. 标题锚点需加文档前缀，否则跨文档同名标题会产生重复 id（见 [`template::inject_heading_ids`]）；
//! 2. 文内本地链接需从「指向 .md 文件」改写为「页内锚点跳转」（见 [`rewrite`]）；
//! 3. 侧栏除目录外还需一棵文档树，让用户知道自己正在读哪一篇。

pub mod collector;
pub mod rewrite;

use crate::converter;
use crate::error::MpeError;
use crate::template::{self, HeadingItem};
use collector::{DocEntry, Collected};
use rewrite::RewriteCtx;
use std::collections::HashMap;
use std::path::Path;

/// 单篇文档的渲染结果
struct RenderedDoc {
    /// 注入锚点后的 HTML 正文
    html: String,
    /// 标题清单（id 已带文档前缀）
    headings: Vec<HeadingItem>,
}

/// 把目录下所有 Markdown 按顺序拼成单个 Markdown（供 DOCX 导出用）。
///
/// 各篇之间以水平线分隔，标题层级原样保留。front matter 会被剥离，
/// 避免第二篇之后出现重复的元数据块。
pub fn concat_docs(root: &Path) -> Result<String, MpeError> {
    let collected = collector::collect(root);
    let mut parts: Vec<String> = Vec::new();
    for doc in &collected.docs {
        let md = std::fs::read_to_string(&doc.path)?;
        let (_fm, body) = converter::parse_front_matter(&md);
        parts.push(body.trim().to_string());
    }
    if parts.is_empty() {
        return Err(MpeError::FileNotFound(format!(
            "{} 内没有 Markdown 文件",
            root.display()
        )));
    }
    Ok(parts.join("\n\n---\n\n"))
}

/// 聚合导出结果
pub struct AggregateResult {
    /// 完整的单文件 HTML
    pub html: String,
    /// 实际聚合的文档数
    pub doc_count: usize,
}

/// 把 `root` 文件夹内的所有 Markdown 聚合为一个自包含 HTML 文件。
///
/// 图片在渲染阶段即转为 base64、mermaid 烘焙为内嵌 SVG、CSS/JS 内联，
/// 因此产物可脱离文件系统独立打开。
pub fn aggregate_folder_to_html(
    root: &Path,
    full_width: bool,
    theme: crate::template::Theme,
) -> Result<AggregateResult, MpeError> {
    if !root.is_dir() {
        return Err(MpeError::FileNotFound(root.to_string_lossy().to_string()));
    }

    let collected = collector::collect(root);
    if collected.docs.is_empty() {
        return Err(MpeError::FileNotFound(format!(
            "{} 内没有 Markdown 文件",
            root.display()
        )));
    }

    // 第一遍：渲染各篇正文，拿到全部标题才能建立跨文档链接映射
    let mut rendered: Vec<RenderedDoc> = Vec::with_capacity(collected.docs.len());
    for doc in &collected.docs {
        rendered.push(render_doc(doc)?);
    }

    // 建立 section id → (规范化标题 → 真实锚点 id) 索引
    let heading_index = build_heading_index(&collected, &rendered);

    // 第二遍：改写本地链接并拼接成完整页面
    let mut body = String::new();
    for (doc, r) in collected.docs.iter().zip(rendered.iter()) {
        let self_dir = dir_of(&doc.rel_path);
        let ctx = RewriteCtx {
            self_prefix: &doc.anchor_prefix,
            self_dir: &self_dir,
            by_rel: &collected.by_rel,
            headings: &heading_index,
        };
        let fixed = rewrite::rewrite_links(&r.html, &ctx);
        body.push_str(&section_html(doc, &fixed));
    }

    let doc_tree = render_doc_tree(&collected, &heading_index);
    let title = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("文档合集")
        .to_string();

    let html = template::render_aggregate_html(&body, &title, &doc_tree, full_width, theme);

    Ok(AggregateResult {
        html,
        doc_count: collected.docs.len(),
    })
}

/// 渲染单篇文档：读取 → 转 HTML → mermaid 烘焙 → 图片内联 → 注入带前缀的锚点
fn render_doc(doc: &DocEntry) -> Result<RenderedDoc, MpeError> {
    let md = std::fs::read_to_string(&doc.path)?;
    let (_, body_md) = converter::parse_front_matter(&md);
    let html = converter::convert_markdown_to_html_with_mermaid(body_md)?;
    let html = converter::resolve_image_srcs(&html, &doc.path);
    let (headings, html) = template::inject_heading_ids(&html, &doc.anchor_prefix);
    Ok(RenderedDoc { html, headings })
}

/// 建立 section id → (规范化标题文本 → 真实锚点 id) 的映射
fn build_heading_index(
    collected: &Collected,
    rendered: &[RenderedDoc],
) -> HashMap<String, HashMap<String, String>> {
    let mut index: HashMap<String, HashMap<String, String>> = HashMap::new();
    for (doc, r) in collected.docs.iter().zip(rendered.iter()) {
        let mut map = HashMap::with_capacity(r.headings.len());
        for h in &r.headings {
            // 键用原始标题文本经 make_id 归一，与链接锚点归一后的形态对齐
            let key = template::normalize_anchor(&h.text);
            // 同名标题以首个为准，与浏览器「取第一个 id」的解析行为一致
            map.entry(key).or_insert_with(|| h.id.clone());
        }
        index.insert(doc.section_id.clone(), map);
    }
    index
}

/// 取相对路径所在目录（顶层文件为空串）
fn dir_of(rel_path: &str) -> String {
    match rel_path.rfind('/') {
        Some(i) => rel_path[..i].to_string(),
        None => String::new(),
    }
}

/// 把单篇正文包进带锚点的 section，供侧栏目录树跳转
fn section_html(doc: &DocEntry, html: &str) -> String {
    format!(
        r#"<section class="doc-section" id="{}">
<div class="doc-banner"><h1 class="doc-title">{}</h1><span class="doc-path">{}</span></div>
<div class="doc-body">{}</div>
</section>
"#,
        doc.section_id,
        escape_html(&doc.title),
        escape_html(&doc.rel_path),
        html
    )
}

/// 渲染侧栏文档树。
///
/// 刻意复用模板既有的 `.toc-node` / `.toc-row` / `.toc-toggle` / `.toc-children`
/// class，使 `mpe.html` 里那套折叠、状态持久化、点击展开祖先的 JS 零改动生效。
/// `data-key` 加 `doc:` 前缀，避免与标题锚点折叠状态串味。
fn render_doc_tree(
    collected: &Collected,
    heading_index: &HashMap<String, HashMap<String, String>>,
) -> String {
    let mut out = String::new();
    out.push_str(r#"<ul class="toc doc-tree">"#);
    for doc in &collected.docs {
        let mut children = String::new();
        if let Some(map) = heading_index.get(&doc.section_id) {
            let mut items: Vec<(String, String)> = map
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            // 标题按 id 排序近似正文顺序：前缀相同则后缀字典序，
            // 保证同一文档内目录层级稳定（渲染顺序已由 inject_heading_ids 决定，
            // 这里仅在收集侧无法还原顺序时退化为字典序）
            items.sort_by(|a, b| a.1.cmp(&b.1));
            if !items.is_empty() {
                children.push_str(r#"<ul class="toc-children">"#);
                for (_, id) in items {
                    // key 用锚点 id 本身：与父文档节点区分开，避免折叠状态串味
                    // 展示文本剥掉文档前缀（strip_prefix 精确匹配，不用 rsplit）
                    let label = id.strip_prefix(&doc.anchor_prefix).unwrap_or(&id);
                    children.push_str(&format!(
                        r##"<li class="toc-node" data-key="doc:{}" data-leaf="1"><div class="toc-row">{}<a href="#{}">{}</a></div></li>"##,
                        escape_attr(&id),
                        leaf_toggle(),
                        escape_attr(&id),
                        escape_html(label)
                    ));
                }
                children.push_str("</ul>");
            }
        }

        if children.is_empty() {
            out.push_str(&format!(
                r##"<li class="toc-node doc-node" data-key="doc:{}" data-leaf="1"><div class="toc-row">{}<a class="doc-link" href="#{}">{}</a></div></li>"##,
                escape_attr(&doc.section_id),
                leaf_toggle(),
                escape_attr(&doc.section_id),
                escape_html(&doc.title)
            ));
        } else {
            out.push_str(&format!(
                r##"<li class="toc-node doc-node expanded" data-key="doc:{}"><div class="toc-row">{}<a class="doc-link" href="#{}">{}</a></div>{}</li>"##,
                escape_attr(&doc.section_id),
                toggle_btn(),
                escape_attr(&doc.section_id),
                escape_html(&doc.title),
                children
            ));
        }
    }
    out.push_str("</ul>");
    out
}

fn toggle_btn() -> &'static str {
    r#"<button type="button" class="toc-toggle" aria-label="toggle" aria-expanded="true"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4l4 4-4 4"/></svg></button>"#
}

fn leaf_toggle() -> &'static str {
    r#"<button type="button" class="toc-toggle toc-toggle-leaf" aria-label="toggle" aria-expanded="true"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4l4 4-4 4"/></svg></button>"#
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(s: &str) -> String {
    escape_html(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("md2x-aggout-{}", tag));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    #[test]
    fn 聚合出单个html并含全部文档() {
        let d = tmpdir("basic");
        write(d.as_path(), "README.md", "# 索引\n\n见 [A](a.md) 与 [B](sub/b.md)");
        write(d.as_path(), "a.md", "# A\n\n## 概述\n\n正文");
        write(d.as_path(), "sub/b.md", "# B\n\n## 概述\n\n正文");

        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        assert_eq!(r.doc_count, 3);
        assert!(r.html.contains("id=\"doc-0\""));
        assert!(r.html.contains("id=\"doc-1\""));
        assert!(r.html.contains("id=\"doc-2\""));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 同名标题锚点不冲突() {
        // 这是合并导出最核心的正确性保障
        let d = tmpdir("dup");
        write(d.as_path(), "README.md", "# 索引");
        write(d.as_path(), "a.md", "# A\n\n## 概述");
        write(d.as_path(), "b.md", "# B\n\n## 概述");

        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        assert!(r.html.contains("doc-1-概述"));
        assert!(r.html.contains("doc-2-概述"));
        // 页面上不应同时存在两个裸的 id="概述"
        assert!(!r.html.contains("id=\"概述\""));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 跨文档链接被改写为页内跳转() {
        let d = tmpdir("links");
        write(d.as_path(), "README.md", "# 索引\n\n[A](a.md) [B](sub/b.md)");
        write(d.as_path(), "a.md", "# A");
        write(d.as_path(), "sub/b.md", "# B");

        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        // 原 .md 链接应已被替换为锚点
        assert!(!r.html.contains("href=\"a.md\""));
        assert!(r.html.contains("href=\"#doc-"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 同文档锚点链接加前缀() {
        let d = tmpdir("anchor");
        write(d.as_path(), "README.md", "# 索引\n\n[跳自己](#标题)");

        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        assert!(r.html.contains("href=\"#doc-0-标题\""));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 侧栏文档树列出每篇文档() {
        let d = tmpdir("tree");
        write(d.as_path(), "README.md", "# 索引");
        write(d.as_path(), "a.md", "# A");

        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        assert!(r.html.contains("doc-tree"));
        assert!(r.html.contains("href=\"#doc-0\""));
        assert!(r.html.contains("href=\"#doc-1\""));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 外部链接在聚合后保持可用() {
        let d = tmpdir("ext");
        write(d.as_path(), "README.md", "# 索引\n\n[站外](https://example.com)");

        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        assert!(r.html.contains("https://example.com"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 空目录返回错误() {
        let d = tmpdir("empty");
        let e = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto);
        assert!(e.is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 非目录返回错误() {
        let d = tmpdir("notdir");
        write(d.as_path(), "a.md", "# A");
        let e = aggregate_folder_to_html(&d.join("a.md"), false, crate::template::Theme::Auto);
        assert!(e.is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn front_matter被正确剥离() {
        let d = tmpdir("fm");
        write(
            d.as_path(),
            "a.md",
            "---\ntitle: T\n---\n\n# 正文标题",
        );
        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        assert!(!r.html.contains("title: T"), "front matter 不应出现在正文");
        assert!(r.html.contains("正文标题"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 转义标题中的尖括号防破坏结构() {
        let d = tmpdir("esc");
        write(d.as_path(), "a.md", "---\ntitle: \"<b>&x</b>\"\n---\n\n# A");
        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        assert!(!r.html.contains("<b>&x</b>"), "标题应被转义");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 主题被固化进导出的html() {
        // 核心保障：接收方系统深浅不同时，打开仍应看到导出时的样子
        let d = tmpdir("theme");
        write(d.as_path(), "README.md", "# 索引");

        let dark = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Dark)
            .unwrap();
        assert!(
            dark.html.contains("var injected = 'dark'"),
            "暗色档位应写入注入值"
        );
        assert!(
            !dark.html.contains("{{THEME_LOCK}}"),
            "占位符必须被替换干净"
        );

        let light = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Light)
            .unwrap();
        assert!(light.html.contains("var injected = 'light'"));

        let auto = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto)
            .unwrap();
        assert!(auto.html.contains("var injected = 'auto'"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 两套主题色板都存在于产物中() {
        // 无论注入哪档，CSS 里都要同时备好浅色与暗色变量，
        // 否则接收方点主题切换按钮会失效
        let d = tmpdir("palette");
        write(d.as_path(), "README.md", "# 索引");
        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Dark).unwrap();
        assert!(r.html.contains("html[data-theme=\"dark\"]"));
        assert!(r.html.contains("--bg: #f5f5f5"), "浅色变量应保留");
        assert!(r.html.contains("--bg: #0d1117"), "暗色变量应保留");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 移动端遮罩元素存在() {
        // 抽屉式侧栏依赖遮罩点击关闭，缺失会导致手机上关不掉目录
        let d = tmpdir("scrim");
        write(d.as_path(), "README.md", "# 索引");
        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        assert!(r.html.contains("id=\"sidebarScrim\""));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 无js降级标记与提示条就位() {
        // 微信/钉钉内置浏览器屏蔽 JS 时，页面须仍可读：
        // 1) <html class="no-js"> 供 CSS 兜底（JS 正常时会被移除）
        // 2) <noscript> 提示条引导用户改用系统浏览器
        let d = tmpdir("nojs");
        write(d.as_path(), "README.md", "# 索引");
        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        assert!(r.html.contains(r#"<html lang="zh-CN" class="no-js">"#));
        assert!(r.html.contains("<noscript>"));
        assert!(r.html.contains("nojs-notice"), "提示条容器应存在");
        assert!(
            r.html.contains("在系统浏览器中打开"),
            "提示语应引导用户换浏览器"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn js探测脚本位于最前面() {
        // no-js 的移除必须发生在 DOM 渲染前，否则会看到降级样式一闪
        let d = tmpdir("detect");
        write(d.as_path(), "README.md", "# 索引");
        let r = aggregate_folder_to_html(d.as_path(), false, crate::template::Theme::Auto).unwrap();
        let head = r.html.find("<head>").expect("应有 head");
        let detect = r
            .html
            .find("classList.remove('no-js')")
            .expect("应含JS 探测脚本");
        assert!(detect > head, "探测脚本应在 head 内");
        // 探测脚本应早于主题脚本（首个 <script> 即为它）
        let first_script = r.html.find("<script>").expect("应有脚本");
        assert!(
            detect < first_script + 400,
            "探测脚本应紧随 <head>，避免样式闪烁"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
