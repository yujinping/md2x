//! PDF 打印专用目录页。
//!
//! 为什么需要：CDP 生成的 PDF 书签（`/Outlines`）是「侧边栏目录」，
//! 需读者主动展开才能看到。对长文档来说，**首页一张可见的目录**
//! 更符合纸质阅读习惯，也让打印出的 PDF 本身具备导航性。
//!
//! 关键设计：该页只在 `@media print` 下渲染与显示，
//! 屏幕浏览（导出 HTML / GUI 预览）完全不受影响，侧栏目录仍是唯一入口。

use crate::template::HeadingItem;

/// 生成打印目录页的 HTML 片段（空目录返回空串）。
///
/// 标题层级会被压缩到最多 3 级：PDF 目录页通常只列到章节级，
/// 过深的层级会让页面被小字标题塞满，反而失去导航价值。
pub fn render_print_toc(headings: &[HeadingItem]) -> String {
    // 只保留 1-3 级标题。
    // 聚合导出时每篇文档都有一个 banner 标题（.doc-title h1），
    // 它与该篇正文首个 h1 文本相同，会让目录出现「知识库索引 ×2」。
    // 打印目录只关心正文小节，故按 class 排除 banner 标题。
    let entries: Vec<&HeadingItem> = headings
        .iter()
        .filter(|h| (1..=3).contains(&level_num(&h.level)))
        .filter(|h| !h.in_banner)
        .collect();

    // 条目太少时目录页反而累赘，不生成
    if entries.len() < 2 {
        return String::new();
    }

    let mut items = String::new();
    for h in &entries {
        let lvl = level_num(&h.level);
        let text = escape_html(&h.text);
        //缩进靠内联 margin实现，避免引入嵌套 <ul>（打印样式里嵌套列表容易错位）
        let indent = (lvl - 1) * 16;
        items.push_str(&format!(
            r##"<li class="print-toc-item" style="margin-left:{indent}px"><a href="#{}">{}</a></li>"##,
            escape_attr(&h.id),
            text
        ));
    }

    format!(
        // 目录页自身不用 <h1>-<h6>：CDP 生成 PDF 大纲是按标题标签层级收录的，
        // 用 <h2>「目录」会被当成一个章节塞进书签树。用 <div> 视觉等价但不会入选。
        r#"<nav class="print-toc" aria-label="目录">
<div class="print-toc-title">目录</div>
<ul>{items}</ul>
</nav>"#
    )
}

/// 标题层级字符串（如 `"h2"`）转数字（2）。无法解析时返回 1。
fn level_num(level: &str) -> usize {
    level
        .trim_start_matches(['h', 'H'])
        .parse::<usize>()
        .unwrap_or(1)
        .clamp(1, 6)
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

    fn h(level: &str, id: &str, text: &str) -> HeadingItem {
        HeadingItem {
            level: level.to_string(),
            id: id.to_string(),
            text: text.to_string(),
            in_banner: false,
        }
    }

    #[test]
    fn 生成目录页含可点击链接() {
        let html = render_print_toc(
            &[
                h("h1", "a", "第一章"),
                h("h2", "a-1", "第一节"),
                h("h2", "b", "第二章"),
            ],
        );
        assert!(html.contains("print-toc"));
        assert!(html.contains(r##"<a href="#a">第一章</a>"##));
        assert!(html.contains(r##"<a href="#b">第二章</a>"##));
    }

    #[test]
    fn 层级压缩到三级() {
        // h4 及更深不进入打印目录，否则页面会被小标题塞满
        let html = render_print_toc(
            &[
                h("h1", "a", "第一章"),
                h("h2", "b", "第二节"),
                h("h4", "d", "深处"),
                h("h2", "c", "第三节"),
            ],
        );
        assert!(!html.contains("深处"), "h4 不应出现在打印目录");
    }

    #[test]
    fn 条目过少时不生成目录页() {
        // 只有 1 个标题时目录页没有导航价值
        let html = render_print_toc(&[h("h1", "a", "唯一标题")]);
        assert_eq!(html, "");
    }

    #[test]
    fn 层级缩进递增() {
        let html = render_print_toc(
            &[h("h1", "a", "一"), h("h2", "b", "一之一"), h("h3", "c", "再深")],
        );
        assert!(html.contains("margin-left:0px"));
        assert!(html.contains("margin-left:16px"));
        assert!(html.contains("margin-left:32px"));
    }

    #[test]
    fn 标题中的尖括号被转义() {
        let html = render_print_toc(
            &[h("h1", "a", "<b>&x</b>"), h("h2", "b", "正常")],
        );
        assert!(!html.contains("<b>&x</b>"), "标题必须转义防破坏结构");
        assert!(html.contains("&lt;b&gt;&amp;x&lt;/b&gt;"));
    }

    #[test]
    fn 层级字符串解析健壮() {
        assert_eq!(level_num("h1"), 1);
        assert_eq!(level_num("H3"), 3);
        assert_eq!(level_num("h6"), 6);
        assert_eq!(level_num("garbage"), 1);
        assert_eq!(level_num("h99"), 6, "超范围应被夹紧");
    }
}
