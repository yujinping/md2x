//! DOCX 导出：Markdown → OOXML 文档。
//!
//! 直接生成 OOXML（zip + XML），不依赖本机 Office/WPS。

pub mod package;
pub mod render;
pub mod highlight;
pub mod image;

use crate::error::MpeError;
use crate::converter;
use std::path::Path;

/// 生成的 docx 内容：document.xml 正文 + 内嵌媒体（文件名、字节、content type）。
pub struct DocxContent {
    pub document_xml: String,
    /// (文件名, 字节, content type, rId)
    pub media: Vec<(String, Vec<u8>, String, usize)>,
    /// (rId, 目标 URL)
    pub links: Vec<(usize, String)>,
}

/// 将 Markdown 转换为 docx 内容（不落盘）。
pub fn markdown_to_docx_content(md: &str, md_file: &Path) -> Result<DocxContent, MpeError> {
    let (_metadata, body_md) = converter::parse_front_matter(md);
    let (body, ctx) = render::render_body(body_md, md_file)?;
    Ok(DocxContent {
        document_xml: package::document_wrapper(&body),
        media: ctx.media,
        links: ctx.links,
    })
}

/// 将内容写入 .docx 文件。
pub fn write_docx(content: &DocxContent, dst: &Path) -> Result<(), MpeError> {
    package::write_package(content, dst)
}

/// 一步完成 Markdown → .docx 文件。
pub fn convert_markdown_to_docx(md: &str, md_file: &Path, dst: &Path) -> Result<(), MpeError> {
    let content = markdown_to_docx_content(md, md_file)?;
    write_docx(&content, dst)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn empty_docx_is_valid_zip_with_required_parts() {
        let dir = std::env::temp_dir().join(format!("md2x-docx-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dst = dir.join("empty.docx");
        let content = super::markdown_to_docx_content("# 标题", Path::new("test.md")).unwrap();
        super::write_docx(&content, &dst).unwrap();

        let file = std::fs::File::open(&dst).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = zip.file_names().map(|s| s.to_string()).collect();
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/document.xml",
            "word/styles.xml",
        ] {
            assert!(
                names.iter().any(|n| n == required),
                "缺少部件 {required}: {names:?}"
            );
        }
        let mut doc = zip.by_name("word/document.xml").unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut doc, &mut xml).unwrap();
        assert!(xml.contains("Heading1"), "标题应使用 Heading1 样式");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn paragraph_renders_runs_and_inline_styles() {
        let content = super::markdown_to_docx_content(
            "普通 **粗体** *斜体* ~~删除~~ `code` [链接](https://example.com)\n第二行",
            Path::new("test.md"),
        )
        .unwrap();
        assert!(content.document_xml.contains("<w:b/>"), "应有加粗");
        assert!(content.document_xml.contains("<w:i/>"), "应有斜体");
        assert!(content.document_xml.contains("<w:strike/>"), "应有删除线");
        assert!(content.document_xml.contains("d63384"), "行内代码颜色");
        assert!(content.document_xml.contains("w:hyperlink"), "应有超链接");
        assert!(content.document_xml.contains("0366d6"), "链接颜色");
        assert!(content.document_xml.contains("eastAsia"), "中文字体");
        assert!(
            content.document_xml.contains("w:line=\"350\""),
            "行距 1.75"
        );
        assert!(content.document_xml.contains("<w:br/>"), "换行");
    }

    #[test]
    fn headings_use_heading_styles_and_outline() {
        let content =
            super::markdown_to_docx_content("# H1\n\n## H2\n\n### H3", Path::new("t.md"))
                .unwrap();
        for lvl in ["Heading1", "Heading2", "Heading3"] {
            assert!(
                content
                    .document_xml
                    .contains(&format!("w:val=\"{lvl}\"")),
                "缺少 {lvl}"
            );
        }
        // 标题段距（与 HTML margin 对应）
        assert!(content.document_xml.contains("w:before=\"450\""));
        assert!(content.document_xml.contains("w:before=\"420\""));

        let dir = std::env::temp_dir().join("md2x-docx-styles");
        std::fs::create_dir_all(&dir).unwrap();
        let dst = dir.join("h.docx");
        super::write_docx(&content, &dst).unwrap();
        let file = std::fs::File::open(&dst).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut styles = String::new();
        std::io::Read::read_to_string(
            &mut zip.by_name("word/styles.xml").unwrap(),
            &mut styles,
        )
        .unwrap();
        assert!(styles.contains("Heading1"), "styles.xml 应含 Heading1");
        assert!(styles.contains("outlineLvl"), "styles.xml 应含 outlineLvl");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lists_render_bullets_numbers_and_tasks() {
        let content = super::markdown_to_docx_content(
            "- 甲\n- 乙\n\n1. 一\n2. 二\n\n- [x] 完成\n- [ ] 未完成",
            Path::new("t.md"),
        )
        .unwrap();
        assert!(content.document_xml.contains("w:numId"), "应有编号");
        assert!(content.document_xml.contains("w:numId w:val=\"1\""), "无序列表");
        assert!(content.document_xml.contains("w:numId w:val=\"2\""), "有序列表");
        assert!(content.document_xml.contains("☑"), "已完成任务");
        assert!(content.document_xml.contains("☐"), "未完成任务");
    }

    #[test]
    fn code_block_has_dark_shading_and_highlight_colors() {
        let content =
            super::markdown_to_docx_content("```rust\nfn main() {}\n```", Path::new("t.md"))
                .unwrap();
        assert!(content.document_xml.contains("282C34"), "代码块底纹");
        assert!(content.document_xml.contains("ABB2BF"), "代码前景");
        assert!(content.document_xml.contains("C678DD"), "关键字高亮色");
        assert!(content.document_xml.contains("Consolas"), "等宽字体");
    }

    #[test]
    fn blockquote_and_hr_render() {
        let content = super::markdown_to_docx_content("> 引用内容\n\n---", Path::new("t.md"))
            .unwrap();
        assert!(content.document_xml.contains("6a737d"), "引用灰字");
        assert!(content.document_xml.contains("w:left"), "左边框");
        assert!(content.document_xml.contains("fafbfc"), "引用背景");
        assert!(content.document_xml.contains("eaecef"), "分割线颜色");
    }

    #[test]
    fn table_renders_with_borders_and_header_shading() {
        let content = super::markdown_to_docx_content(
            "| 名称 | 数量 |\n| --- | ---: |\n| 苹果 | 3 |",
            Path::new("t.md"),
        )
        .unwrap();
        assert!(content.document_xml.contains("<w:tbl>"), "应有表格");
        assert!(content.document_xml.contains("dfe2e5"), "边框色");
        assert!(content.document_xml.contains("f6f8fa"), "表头底纹");
        assert!(
            content
                .document_xml
                .contains("w:jc w:val=\"right\""),
            "右对齐列"
        );
        assert!(content.document_xml.contains("<w:tblGrid>"), "表格网格");
    }

    #[test]
    fn image_embeds_into_media_and_drawing() {
        let png_bytes: &[u8] = include_bytes!("../../../../crates/md2x-gui/icons/32x32.png");
        let dir = std::env::temp_dir().join("md2x-docx-img");
        std::fs::create_dir_all(&dir).unwrap();
        let png_path = dir.join("pixel.png");
        std::fs::write(&png_path, png_bytes).unwrap();

        let md = format!("![像素]({})", png_path.display());
        let content = super::markdown_to_docx_content(&md, &png_path).unwrap();
        assert_eq!(content.media.len(), 1, "应嵌入 1 张图片");
        assert!(content.document_xml.contains("<w:drawing>"), "应有 drawing");
        assert!(content.document_xml.contains("rId3"), "图片关系");

        let out_dir = std::env::temp_dir().join("md2x-docx-img-out");
        std::fs::create_dir_all(&out_dir).unwrap();
        let dst = out_dir.join("img.docx");
        super::write_docx(&content, &dst).unwrap();
        let file = std::fs::File::open(&dst).unwrap();
        let zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = zip.file_names().map(|s| s.to_string()).collect();
        assert!(
            names.iter().any(|n| n.starts_with("word/media/")),
            "缺少 media: {names:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&out_dir).ok();
    }

    #[test]
    fn frontmatter_stripped_and_mixed_doc_renders() {
        let content = super::markdown_to_docx_content(
            "---\ntitle: 演示\n---\n\n# 标题\n\n- 项一\n- 项二\n\n```rust\nlet x = 1;\n```",
            Path::new("t.md"),
        )
        .unwrap();
        assert!(
            !content.document_xml.contains("title: 演示"),
            "frontmatter 不应进入正文"
        );
        assert!(content.document_xml.contains("项一"), "列表内容");
    }

    #[test]
    fn heading_run_size_matches_html() {
        let content =
            super::markdown_to_docx_content("# H1\n\n## H2\n\n### H3", Path::new("t.md"))
                .unwrap();
        // 标题 run 应使用与 HTML 一致的字号（h1=45、h2=36、h3=30 半磅），而非正文 22
        assert!(
            content.document_xml.contains("<w:sz w:val=\"45\"/>"),
            "H1 应 45 半磅"
        );
        assert!(
            content.document_xml.contains("<w:sz w:val=\"36\"/>"),
            "H2 应 36 半磅"
        );
        assert!(
            content.document_xml.contains("<w:sz w:val=\"30\"/>"),
            "H3 应 30 半磅"
        );
    }

    #[test]
    fn svg_image_embeds_with_dimensions() {
        let dir = std::env::temp_dir().join("md2x-docx-svg");
        std::fs::create_dir_all(&dir).unwrap();
        let svg_path = dir.join("icon.svg");
        std::fs::write(
            &svg_path,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="200" height="100" fill="#0969da"/></svg>"##,
        )
        .unwrap();
        let md = format!("![SVG]({})", svg_path.display());
        let content = super::markdown_to_docx_content(&md, &svg_path).unwrap();
        assert_eq!(content.media.len(), 1, "SVG 应被嵌入");
        assert!(
            content.media[0].2.contains("svg"),
            "SVG content type: {}",
            content.media[0].2
        );
        assert!(content.document_xml.contains("<w:drawing>"), "应有 drawing");
        assert!(
            content.document_xml.contains("cx=\"1905000\""),
            "SVG 宽度 200px → 1905000 EMU"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hugo_static_image_path_resolves() {
        // 构造 Hugo 结构：root/static/images/logo.png + root/content/post/test.md
        let root = std::env::temp_dir().join("md2x-docx-hugo");
        let static_img = root.join("static/images/logo.png");
        std::fs::create_dir_all(static_img.parent().unwrap()).unwrap();
        let png: &[u8] = include_bytes!("../../../../crates/md2x-gui/icons/32x32.png");
        std::fs::write(&static_img, png).unwrap();
        let md_path = root.join("content/post/test.md");
        std::fs::create_dir_all(md_path.parent().unwrap()).unwrap();

        let content =
            super::markdown_to_docx_content("![Hugo](/images/logo.png)", &md_path).unwrap();
        assert_eq!(content.media.len(), 1, "Hugo 静态路径应解析为 static 目录");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn image_inside_list_item_embeds() {
        let dir = std::env::temp_dir().join("md2x-docx-li-img");
        std::fs::create_dir_all(&dir).unwrap();
        let png_path = dir.join("p.png");
        let png: &[u8] = include_bytes!("../../../../crates/md2x-gui/icons/32x32.png");
        std::fs::write(&png_path, png).unwrap();
        let md = format!("- 列表项\n  ![图]({})", png_path.display());
        let content = super::markdown_to_docx_content(&md, &png_path).unwrap();
        assert_eq!(content.media.len(), 1, "列表内图片应嵌入");
        assert!(
            content.document_xml.contains("<w:drawing>"),
            "列表内图片应有 drawing"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
