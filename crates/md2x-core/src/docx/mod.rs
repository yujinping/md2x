//! DOCX 导出：Markdown → OOXML 文档。
//!
//! 直接生成 OOXML（zip + XML），不依赖本机 Office/WPS。

pub mod package;
pub mod render;

use crate::error::MpeError;
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
    let (body, ctx) = render::render_body(md, md_file)?;
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
}
