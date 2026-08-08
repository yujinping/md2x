//! comrak AST → OOXML 片段渲染。

use crate::error::MpeError;
use comrak::nodes::{AstNode, NodeValue};
use std::path::Path;

/// 将 Markdown 渲染为 document.xml 的 body 内容。
pub fn render_body(md: &str, _md_file: &Path) -> Result<String, MpeError> {
    let mut options = comrak::ComrakOptions::default();
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.footnotes = true;
    options.parse.smart = true;
    options.render.hardbreaks = true;

    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, md, &options);
    let mut out = String::new();
    for node in root.children() {
        render_block(node, &mut out);
    }
    Ok(out)
}

fn render_block<'a>(node: &'a AstNode<'a>, out: &mut String) {
    let value = &node.data.borrow().value;
    match value {
        NodeValue::Heading(h) => {
            let lvl = h.level.clamp(1, 6);
            let text = inline_text(node);
            out.push_str(&format!(
                "<w:p><w:pPr><w:pStyle w:val=\"Heading{lvl}\"/></w:pPr>\
                 <w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                escape_xml(&text)
            ));
        }
        NodeValue::Paragraph => {
            let text = inline_text(node);
            if !text.is_empty() {
                out.push_str(&format!(
                    "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                    escape_xml(&text)
                ));
            }
        }
        _ => {}
    }
}

/// 提取节点下所有内联文本（按顺序拼接），用于尚未细分的任务兜底。
fn inline_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut s = String::new();
    collect_text(node, &mut s);
    s
}

fn collect_text<'a>(node: &'a AstNode<'a>, out: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Text(t) => out.push_str(t),
        NodeValue::Code(c) => out.push_str(&c.literal),
        NodeValue::SoftBreak | NodeValue::LineBreak => out.push('\n'),
        _ => {
            for child in node.children() {
                collect_text(child, out);
            }
        }
    }
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
