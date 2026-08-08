//! comrak AST → OOXML 片段渲染。

use crate::error::MpeError;
use comrak::nodes::{AstNode, ListType, NodeValue};
use std::path::Path;

/// 渲染上下文：统一分配 document.xml.rels 的 rId（rId1=styles、rId2=numbering，之后按文档顺序）。
#[derive(Default)]
pub struct RenderCtx {
    pub next_rid: usize,
    /// (文件名, 字节, content type, rId)
    pub media: Vec<(String, Vec<u8>, String, usize)>,
    /// (rId, 目标 URL)
    pub links: Vec<(usize, String)>,
}

/// 行内样式（随递归叠加）。
#[derive(Clone, Copy, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: bool,
}

/// 将 Markdown 渲染为 document.xml 的 body 内容，同时收集媒体与链接。
pub fn render_body(
    md: &str,
    _md_file: &Path,
) -> Result<(String, RenderCtx), MpeError> {
    let mut options = comrak::ComrakOptions::default();
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.footnotes = true;
    options.parse.smart = true;
    options.render.hardbreaks = true;

    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, md, &options);
    let mut ctx = RenderCtx {
        next_rid: 3,
        ..Default::default()
    };
    let mut out = String::new();
    for node in root.children() {
        render_block(node, &mut out, &mut ctx);
    }
    Ok((out, ctx))
}

fn render_block<'a>(node: &'a AstNode<'a>, out: &mut String, ctx: &mut RenderCtx) {
    let value = &node.data.borrow().value;
    match value {
        NodeValue::Heading(h) => {
            let lvl = h.level.clamp(1, 6);
            let mut content = String::new();
            for child in node.children() {
                content.push_str(&render_inline(child, InlineStyle::default(), ctx));
            }
            let (before, after) = heading_spacing(lvl);
            out.push_str(&format!(
                "<w:p><w:pPr><w:pStyle w:val=\"Heading{lvl}\"/>\
                 <w:spacing w:before=\"{before}\" w:after=\"{after}\"/></w:pPr>{content}</w:p>"
            ));
        }
        NodeValue::Paragraph => {
            let mut content = String::new();
            for child in node.children() {
                content.push_str(&render_inline(child, InlineStyle::default(), ctx));
            }
            if !content.is_empty() {
                out.push_str(&format!(
                    "<w:p><w:pPr><w:spacing w:line=\"350\" w:lineRule=\"auto\" w:after=\"240\"/>\
                     </w:pPr>{content}</w:p>"
                ));
            }
        }
        NodeValue::List(_) => {
            render_list(node, 0, out, ctx);
        }
        _ => {}
    }
}

/// 渲染一个列表（含其下所有条目与嵌套列表）。
fn render_list<'a>(node: &'a AstNode<'a>, depth: u32, out: &mut String, ctx: &mut RenderCtx) {
    let num_id = match &node.data.borrow().value {
        NodeValue::List(l) if l.list_type == ListType::Ordered => 2,
        _ => 1,
    };
    for item in node.children() {
        render_list_item(item, num_id, depth, out, ctx);
    }
}

fn render_list_item<'a>(
    node: &'a AstNode<'a>,
    num_id: u32,
    depth: u32,
    out: &mut String,
    ctx: &mut RenderCtx,
) {
    let value = &node.data.borrow().value;
    let (checked, is_task) = match value {
        NodeValue::TaskItem(checked) => (*checked, true),
        _ => (None, false),
    };

    let mut content = String::new();
    let mut nested = String::new();
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Paragraph => {
                for gc in child.children() {
                    content.push_str(&render_inline(gc, InlineStyle::default(), ctx));
                }
            }
            NodeValue::List(_) => {
                render_list(child, depth + 1, &mut nested, ctx);
            }
            _ => {
                content.push_str(&render_inline(child, InlineStyle::default(), ctx));
            }
        }
    }

    if is_task {
        let mark = if checked.is_some() { "☑ " } else { "☐ " };
        content.insert_str(0, &run_xml(mark, &InlineStyle::default()));
        out.push_str(&format!(
            "<w:p><w:pPr><w:ind w:left=\"480\"/>\
             <w:spacing w:line=\"350\" w:lineRule=\"auto\" w:after=\"60\"/></w:pPr>{content}</w:p>"
        ));
    } else {
        out.push_str(&format!(
            "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{depth}\"/><w:numId w:val=\"{num_id}\"/>\
             </w:numPr><w:spacing w:line=\"350\" w:lineRule=\"auto\" w:after=\"60\"/></w:pPr>\
             {content}</w:p>"
        ));
    }
    out.push_str(&nested);
}

/// 标题段距（对应 HTML 模板 h1-h6 的 margin）。
fn heading_spacing(level: u8) -> (u32, u32) {
    match level {
        1 | 2 => (450, 150),
        3 => (420, 120),
        4 => (390, 90),
        _ => (360, 90),
    }
}

/// 渲染一个内联节点，返回 run（或 hyperlink）XML。
fn render_inline<'a>(
    node: &'a AstNode<'a>,
    style: InlineStyle,
    ctx: &mut RenderCtx,
) -> String {
    let value = &node.data.borrow().value;
    match value {
        NodeValue::Text(t) => run_xml(t, &style),
        NodeValue::Code(c) => {
            let s = InlineStyle {
                code: true,
                ..style
            };
            run_xml(&c.literal, &s)
        }
        NodeValue::Emph => render_children(node, InlineStyle { italic: true, ..style }, ctx),
        NodeValue::Strong => render_children(node, InlineStyle { bold: true, ..style }, ctx),
        NodeValue::Strikethrough => {
            render_children(node, InlineStyle { strike: true, ..style }, ctx)
        }
        NodeValue::Link(link) => {
            let rid = ctx.next_rid;
            ctx.next_rid += 1;
            ctx.links.push((rid, link.url.clone()));
            let inner = render_children(
                node,
                InlineStyle {
                    link: true,
                    ..style
                },
                ctx,
            );
            format!("<w:hyperlink r:id=\"rId{rid}\">{inner}</w:hyperlink>")
        }
        NodeValue::SoftBreak | NodeValue::LineBreak => {
            format!(
                "<w:r><w:rPr>{}</w:rPr><w:br/></w:r>",
                run_properties(&style)
            )
        }
        _ => {
            // 未知内联节点：递归取子节点文本兜底
            render_children(node, style, ctx)
        }
    }
}

fn render_children<'a>(
    node: &'a AstNode<'a>,
    style: InlineStyle,
    ctx: &mut RenderCtx,
) -> String {
    let mut out = String::new();
    for child in node.children() {
        out.push_str(&render_inline(child, style, ctx));
    }
    out
}

fn run_xml(text: &str, style: &InlineStyle) -> String {
    format!(
        "<w:r><w:rPr>{}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
        run_properties(style),
        escape_xml(text)
    )
}

fn run_properties(style: &InlineStyle) -> String {
    let mut rpr = String::new();
    if style.code {
        rpr.push_str(
            "<w:rFonts w:ascii=\"Consolas\" w:eastAsia=\"Microsoft YaHei\" \
             w:hAnsi=\"Consolas\" w:cs=\"Consolas\"/>",
        );
        rpr.push_str("<w:color w:val=\"d63384\"/><w:sz w:val=\"20\"/><w:szCs w:val=\"20\"/>");
        rpr.push_str("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"f0f0f0\"/>");
    } else {
        rpr.push_str(
            "<w:rFonts w:ascii=\"Segoe UI\" w:eastAsia=\"Microsoft YaHei\" \
             w:hAnsi=\"Segoe UI\" w:cs=\"Segoe UI\"/>",
        );
        rpr.push_str("<w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/>");
        if style.link {
            rpr.push_str("<w:color w:val=\"0366d6\"/><w:u w:val=\"single\"/>");
        }
    }
    if style.bold {
        rpr.push_str("<w:b/>");
    }
    if style.italic {
        rpr.push_str("<w:i/>");
    }
    if style.strike {
        rpr.push_str("<w:strike/>");
    }
    rpr
}

/// XML 文本转义。
pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}
