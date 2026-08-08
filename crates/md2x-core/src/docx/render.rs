//! comrak AST → OOXML 片段渲染。

use crate::error::MpeError;
use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use std::path::{Path, PathBuf};

use super::{highlight, image, package::rfonts_xml};

/// 渲染上下文：统一分配 document.xml.rels 的 rId（rId1=styles、rId2=numbering，之后按文档顺序）。
#[derive(Default)]
pub struct RenderCtx {
    pub next_rid: usize,
    pub next_drawing_id: usize,
    /// 当前 Markdown 文件路径（图片相对路径基准）
    pub md_file: PathBuf,
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
    quote: bool,
    /// 字号（半磅）。None 表示正文 22（11pt）。
    size: Option<u32>,
}

/// 将 Markdown 渲染为 document.xml 的 body 内容，同时收集媒体与链接。
pub fn render_body(
    md: &str,
    md_file: &Path,
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
        next_drawing_id: 1,
        md_file: md_file.to_path_buf(),
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
            let size = heading_size(lvl);
            let (before, after) = heading_spacing(lvl);
            let mut content = String::new();
            for child in node.children() {
                content.push_str(&render_inline(
                    child,
                    InlineStyle {
                        size: Some(size),
                        ..InlineStyle::default()
                    },
                    ctx,
                ));
            }
            out.push_str(&format!(
                "<w:p><w:pPr><w:pStyle w:val=\"Heading{lvl}\"/>\
                 <w:spacing w:before=\"{before}\" w:after=\"{after}\" w:line=\"250\" \
                 w:lineRule=\"auto\"/></w:pPr>{content}</w:p>"
            ));
        }
        NodeValue::Paragraph => {
            render_para_children(
                node,
                InlineStyle::default(),
                "<w:spacing w:line=\"350\" w:lineRule=\"auto\" w:after=\"240\"/>",
                out,
                ctx,
            );
        }
        NodeValue::List(_) => {
            render_list(node, 0, false, out, ctx);
        }
        NodeValue::CodeBlock(cb) => {
            let lang = if cb.info.trim().is_empty() {
                None
            } else {
                Some(cb.info.trim())
            };
            render_code_block(lang, &cb.literal, out);
        }
        NodeValue::BlockQuote => {
            for child in node.children() {
                render_quote_block(child, out, ctx);
            }
        }
        NodeValue::ThematicBreak => {
            out.push_str(
                "<w:p><w:pPr><w:pBdr><w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"1\" \
                 w:color=\"eaecef\"/></w:pBdr>\
                 <w:spacing w:before=\"360\" w:after=\"360\"/></w:pPr></w:p>",
            );
        }
        NodeValue::Table(table) => {
            render_table(table.alignments.clone(), node, out, ctx);
        }
        _ => {}
    }
}

/// 渲染段落子节点：普通行内内容合成一个段落；图片独立成居中段落（防非法嵌套）。
fn render_para_children<'a>(
    node: &'a AstNode<'a>,
    style: InlineStyle,
    ppr: &str,
    out: &mut String,
    ctx: &mut RenderCtx,
) {
    let mut content = String::new();
    for child in node.children() {
        if matches!(&child.data.borrow().value, NodeValue::Image(_)) {
            if !content.is_empty() {
                out.push_str(&format!(
                    "<w:p><w:pPr>{ppr}</w:pPr>{content}</w:p>"
                ));
                content.clear();
            }
            out.push_str(&render_image(child, ctx));
        } else {
            content.push_str(&render_inline(child, style, ctx));
        }
    }
    if content.is_empty() {
        return;
    }
    out.push_str(&format!("<w:p><w:pPr>{ppr}</w:pPr>{content}</w:p>"));
}

/// 渲染图片为居中段落 + w:drawing。
fn render_image<'a>(node: &'a AstNode<'a>, ctx: &mut RenderCtx) -> String {
    let value = &node.data.borrow().value;
    let url = match value {
        NodeValue::Image(l) => l.url.clone(),
        _ => return String::new(),
    };
    let alt = collect_text(node);

    let Some((bytes, mime)) = image::resolve_image_bytes(&url, &ctx.md_file) else {
        // 图片不可用：输出 alt 文本兜底
        let mut alt_run = String::new();
        if !alt.is_empty() {
            alt_run = format!(
                "<w:r><w:rPr><w:color w:val=\"8b949e\"/></w:rPr>\
                 <w:t xml:space=\"preserve\">[{}]</w:t></w:r>",
                escape_xml(&alt)
            );
        }
        return format!(
            "<w:p><w:pPr><w:spacing w:line=\"350\" w:lineRule=\"auto\" w:after=\"240\"/>\
             </w:pPr>{alt_run}</w:p>"
        );
    };

    // 尺寸解析失败时兜底为 480x360，保证图片不丢失
    let (w, h) = image::image_dimensions(&bytes, &mime).unwrap_or((480, 360));
    // 像素 → EMU（96dpi：1px = 9525 EMU），超宽等比缩放
    let max_cx = 5_760_720u64;
    let mut cx = w as u64 * 9525;
    let mut cy = h as u64 * 9525;
    if cx > max_cx {
        cy = cy * max_cx / cx;
        cx = max_cx;
    }

    let rid = ctx.next_rid;
    ctx.next_rid += 1;
    let did = ctx.next_drawing_id;
    ctx.next_drawing_id += 1;
    let name = format!("image{rid}.{}", image::ext_from_mime(&mime));
    let ct = if mime.contains("svg") {
        "image/svg+xml"
    } else if mime.contains("jpeg") {
        "image/jpeg"
    } else if mime.contains("gif") {
        "image/gif"
    } else if mime.contains("bmp") {
        "image/bmp"
    } else if mime.contains("webp") {
        "image/webp"
    } else {
        "image/png"
    };
    ctx.media.push((name.clone(), bytes, ct.to_string(), rid));

    format!(
        "<w:p><w:pPr><w:jc w:val=\"center\"/>\
         <w:spacing w:line=\"350\" w:lineRule=\"auto\" w:after=\"240\"/></w:pPr>\
         <w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
         <wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
         <wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>\
         <wp:docPr id=\"{did}\" name=\"{name}\"/>\
         <wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>\
         <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:pic><pic:nvPicPr><pic:cNvPr id=\"{did}\" name=\"{name}\"/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"rId{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>\
         </pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
    )
}

/// 表格：全宽、单线边框、表头底纹、按列对齐。
fn render_table<'a>(
    alignments: Vec<TableAlignment>,
    node: &'a AstNode<'a>,
    out: &mut String,
    ctx: &mut RenderCtx,
) {
    let ncols = alignments.len().max(1);
    let col_w = 9638 / ncols as u32;
    let mut grid = String::new();
    for _ in 0..ncols {
        grid.push_str(&format!("<w:gridCol w:w=\"{col_w}\"/>"));
    }

    let mut rows = String::new();
    for row in node.children() {
        let is_header = matches!(&row.data.borrow().value, NodeValue::TableRow(true));
        let mut cells = String::new();
        for (i, cell) in row.children().enumerate() {
            let align = alignments.get(i).copied().unwrap_or(TableAlignment::None);
            let jc = match align {
                TableAlignment::Center => Some("center"),
                TableAlignment::Right => Some("right"),
                _ => None,
            };
            let mut content = String::new();
            for gc in cell.children() {
                content.push_str(&render_inline(
                    gc,
                    InlineStyle {
                        bold: is_header,
                        ..InlineStyle::default()
                    },
                    ctx,
                ));
            }
            let shd = if is_header {
                "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"f6f8fa\"/>"
            } else {
                ""
            };
            let jc_xml = jc
                .map(|j| format!("<w:jc w:val=\"{j}\"/>"))
                .unwrap_or_default();
            cells.push_str(&format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"{col_w}\" w:type=\"dxa\"/>{shd}\
                 <w:vAlign w:val=\"top\"/></w:tcPr>\
                 <w:p><w:pPr>{jc_xml}<w:spacing w:line=\"300\" w:lineRule=\"auto\" w:after=\"0\"/>\
                 </w:pPr>{content}</w:p></w:tc>"
            ));
        }
        rows.push_str(&format!("<w:tr>{cells}</w:tr>"));
    }

    out.push_str(&format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"9638\" w:type=\"dxa\"/>\
         <w:tblBorders>\
         <w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"dfe2e5\"/>\
         <w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"dfe2e5\"/>\
         <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"dfe2e5\"/>\
         <w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"dfe2e5\"/>\
         <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"dfe2e5\"/>\
         <w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"dfe2e5\"/>\
         </w:tblBorders>\
         <w:tblCellMar>\
         <w:top w:w=\"120\" w:type=\"dxa\"/><w:left w:w=\"180\" w:type=\"dxa\"/>\
         <w:bottom w:w=\"120\" w:type=\"dxa\"/><w:right w:w=\"180\" w:type=\"dxa\"/>\
         </w:tblCellMar>\
         <w:tblLayout w:type=\"autofit\"/></w:tblPr>\
         <w:tblGrid>{grid}</w:tblGrid>{rows}</w:tbl>"
    ));
}

/// 引用块内的块级元素（继承引用样式：缩进、左边框、浅色背景、灰字）。
fn render_quote_block<'a>(node: &'a AstNode<'a>, out: &mut String, ctx: &mut RenderCtx) {
    const QUOTE_PPR: &str = "<w:pBdr><w:left w:val=\"single\" w:sz=\"24\" w:space=\"8\" \
         w:color=\"dfe2e5\"/></w:pBdr><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"fafbfc\"/>\
         <w:spacing w:line=\"350\" w:lineRule=\"auto\" w:after=\"240\"/><w:ind w:left=\"240\"/>";

    match &node.data.borrow().value {
        NodeValue::Paragraph => {
            render_para_children(
                node,
                InlineStyle {
                    quote: true,
                    ..InlineStyle::default()
                },
                QUOTE_PPR,
                out,
                ctx,
            );
        }
        NodeValue::List(_) => render_list(node, 0, true, out, ctx),
        NodeValue::CodeBlock(cb) => {
            let lang = if cb.info.trim().is_empty() {
                None
            } else {
                Some(cb.info.trim())
            };
            render_code_block(lang, &cb.literal, out);
        }
        NodeValue::BlockQuote => {
            for child in node.children() {
                render_quote_block(child, out, ctx);
            }
        }
        NodeValue::Heading(h) => {
            let lvl = h.level.clamp(1, 6);
            let size = heading_size(lvl);
            let mut content = String::new();
            for gc in node.children() {
                content.push_str(&render_inline(
                    gc,
                    InlineStyle {
                        quote: true,
                        size: Some(size),
                        ..InlineStyle::default()
                    },
                    ctx,
                ));
            }
            out.push_str(&format!(
                "<w:p><w:pPr><w:pStyle w:val=\"Heading{lvl}\"/>\
                 <w:spacing w:line=\"250\" w:lineRule=\"auto\"/>{QUOTE_PPR}</w:pPr>{content}</w:p>"
            ));
        }
        _ => {}
    }
}

/// 代码块：短块用圆角文本框（圆角 + 内边距，观感对齐 HTML）；长块回退为
/// 段落底纹（避免文本框跨页截断内容）。
fn render_code_block(lang: Option<&str>, code: &str, out: &mut String) {
    let tokens = highlight::highlight_code(lang, code);
    // 将 token 按换行拆分为行
    let mut lines: Vec<Vec<(String, String)>> = vec![Vec::new()];
    for (text, color) in tokens {
        let parts: Vec<&str> = text.split('\n').collect();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                lines.push(Vec::new());
            }
            if !part.is_empty() {
                if let Some(last) = lines.last_mut() {
                    last.push((part.to_string(), color.clone()));
                }
            }
        }
    }

    if lines.len() <= 40 {
        render_rounded_code_block(&lines, out);
    } else {
        render_shaded_code_block(&lines, out);
    }
}

/// 圆角文本框代码块：VML roundrect + textbox，深色填充 + 内边距。
fn render_rounded_code_block(lines: &[Vec<(String, String)>], out: &mut String) {
    let mut content = String::new();
    for line in lines {
        let mut runs = String::new();
        for (text, color) in line {
            runs.push_str(&format!(
                "<w:r><w:rPr>{}\
                 <w:color w:val=\"{color}\"/><w:sz w:val=\"20\"/><w:szCs w:val=\"20\"/>\
                 </w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
                rfonts_xml(true),
                escape_xml(text)
            ));
        }
        content.push_str(&format!(
            "<w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"320\" w:lineRule=\"auto\"/>\
             </w:pPr>{runs}</w:p>"
        ));
    }
    out.push_str(&format!(
        "<w:p><w:pPr><w:spacing w:before=\"240\" w:after=\"240\"/></w:pPr>\
         <w:r><w:pict>\
         <v:roundrect style=\"width:481.9pt;mso-fit-shape-to-text:true\" \
         fillcolor=\"#282C34\" strokecolor=\"#282C34\" arcsize=\"0.05\">\
         <v:textbox inset=\"15pt,12pt,15pt,12pt\">\
         <w:txbxContent>{content}</w:txbxContent>\
         </v:textbox></v:roundrect></w:pict></w:r></w:p>"
    ));
}

/// 段落底纹代码块（长代码块兜底，保证跨页内容完整）。
fn render_shaded_code_block(lines: &[Vec<(String, String)>], out: &mut String) {
    let n = lines.len();
    for (i, line) in lines.iter().enumerate() {
        let (before, after) = if n == 1 {
            ("240", "240")
        } else if i == 0 {
            ("240", "0")
        } else if i == n - 1 {
            ("0", "240")
        } else {
            ("0", "0")
        };
        let mut runs = String::new();
        for (text, color) in line {
            runs.push_str(&format!(
                "<w:r><w:rPr>\
                 {}\
                 <w:color w:val=\"{color}\"/><w:sz w:val=\"20\"/><w:szCs w:val=\"20\"/>\
                 </w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
                rfonts_xml(true),
                escape_xml(text)
            ));
        }
        out.push_str(&format!(
            "<w:p><w:pPr>\
             <w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"282C34\"/>\
             <w:ind w:left=\"240\" w:right=\"240\"/>\
             <w:spacing w:before=\"{before}\" w:after=\"{after}\" w:line=\"320\" w:lineRule=\"auto\"/>\
             </w:pPr>{runs}</w:p>"
        ));
    }
}

/// 渲染一个列表（含其下所有条目与嵌套列表）。
fn render_list<'a>(
    node: &'a AstNode<'a>,
    depth: u32,
    quote: bool,
    out: &mut String,
    ctx: &mut RenderCtx,
) {
    let num_id = match &node.data.borrow().value {
        NodeValue::List(l) if l.list_type == ListType::Ordered => 2,
        _ => 1,
    };
    for item in node.children() {
        render_list_item(item, num_id, depth, quote, out, ctx);
    }
}

fn render_list_item<'a>(
    node: &'a AstNode<'a>,
    num_id: u32,
    depth: u32,
    quote: bool,
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
                    if matches!(&gc.data.borrow().value, NodeValue::Image(_)) {
                        // 列表项内的图片独立成居中段落（带 numPr 会破坏布局）
                        nested.push_str(&render_image(gc, ctx));
                    } else {
                        content.push_str(&render_inline(gc, InlineStyle::default(), ctx));
                    }
                }
            }
            NodeValue::List(_) => {
                render_list(child, depth + 1, quote, &mut nested, ctx);
            }
            _ => content.push_str(&render_inline(child, InlineStyle::default(), ctx)),
        }
    }

    if is_task {
        let mark = if checked.is_some() { "☑ " } else { "☐ " };
        content.insert_str(0, &run_xml(mark, &InlineStyle::default()));
        let quote_ppr = if quote { QUOTE_LIST_PPR } else { "" };
        if !content.is_empty() {
            out.push_str(&format!(
                "<w:p><w:pPr>{quote_ppr}<w:ind w:left=\"480\"/>\
                 <w:spacing w:line=\"350\" w:lineRule=\"auto\" w:after=\"60\"/></w:pPr>{content}</w:p>"
            ));
        }
    } else if !content.is_empty() {
        let quote_ppr = if quote { QUOTE_LIST_PPR } else { "" };
        out.push_str(&format!(
            "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{depth}\"/><w:numId w:val=\"{num_id}\"/>\
             </w:numPr>{quote_ppr}\
             <w:spacing w:line=\"350\" w:lineRule=\"auto\" w:after=\"60\"/></w:pPr>\
             {content}</w:p>"
        ));
    }
    out.push_str(&nested);
}

const QUOTE_LIST_PPR: &str =
    "<w:pBdr><w:left w:val=\"single\" w:sz=\"24\" w:space=\"8\" w:color=\"dfe2e5\"/></w:pBdr>\
     <w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"fafbfc\"/>";

/// 标题段距（对应 HTML 模板 h1-h6 的 margin）。
fn heading_spacing(level: u8) -> (u32, u32) {
    match level {
        1 | 2 => (450, 150),
        3 => (420, 120),
        4 => (390, 90),
        _ => (360, 90),
    }
}

/// 标题字号（半磅），对应 HTML 模板 h1-h6 的 font-size。
fn heading_size(level: u8) -> u32 {
    match level {
        1 => 45,
        2 => 36,
        3 => 30,
        4 => 26,
        _ => 22,
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
        // 图片由块级段落渲染处理（独立居中段落），此处兜底防嵌套非法 XML。
        NodeValue::Image(_) => String::new(),
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

/// 提取节点下所有内联文本（按顺序拼接）。
fn collect_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut s = String::new();
    collect_text_into(node, &mut s);
    s
}

fn collect_text_into<'a>(node: &'a AstNode<'a>, out: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Text(t) => out.push_str(t),
        NodeValue::Code(c) => out.push_str(&c.literal),
        NodeValue::SoftBreak | NodeValue::LineBreak => out.push('\n'),
        _ => {
            for child in node.children() {
                collect_text_into(child, out);
            }
        }
    }
}

fn run_properties(style: &InlineStyle) -> String {
    let mut rpr = String::new();
    if style.code {
        rpr.push_str(&rfonts_xml(true));
        let sz = style.size.unwrap_or(20);
        rpr.push_str(&format!(
            "<w:color w:val=\"d63384\"/><w:sz w:val=\"{sz}\"/><w:szCs w:val=\"{sz}\"/>"
        ));
        rpr.push_str("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"f0f0f0\"/>");
    } else {
        rpr.push_str(&rfonts_xml(false));
        let sz = style.size.unwrap_or(22);
        rpr.push_str(&format!("<w:sz w:val=\"{sz}\"/><w:szCs w:val=\"{sz}\"/>"));
        if style.link {
            rpr.push_str("<w:color w:val=\"0366d6\"/><w:u w:val=\"single\"/>");
        } else if style.quote {
            rpr.push_str("<w:color w:val=\"6a737d\"/>");
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
