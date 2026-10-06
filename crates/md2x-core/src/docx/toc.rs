//! DOCX 目录（TOC）域代码生成。
//!
//! 为什么用域代码而不是写死目录文字：.docx 是 zip 包，正文是流式 XML，
//! **生成端无法知道最终分页结果**。写死页码必然不准。
//! 标准做法是插入一个 TOC 域，并在 settings.xml 里置 `updateFields`，
//! 让 Word / WPS 打开文档时弹出「此文档包含可更新的域，是否更新？」，
//! 用户点一下即由阅读器按真实分页重建目录——页码总是准确的。
//!
//! 正文标题已使用标准 `Heading1`~`Heading6` 样式（见 package.rs），
//! 这正是 TOC 域 `\\o "1-3"` 的取值依据，因此无需额外登记。

/// 生成 TOC 域段落。
///
/// 结构是 Word 域的三段式：`begin` → `instrText` → `separate` → 缓存结果 → `end`。
/// 缓存结果里放一行提示文字，这样即使阅读器不更新域（部分纯文本预览工具），
/// 也不会显示空白。
pub fn render_toc_field(levels: &str) -> String {
    // \o "1-3"：收录 1-3 级标题；\h：条目超链接；\z：Web 视图时隐藏页码
    let instruction = format!(" TOC \\o \"{levels}\" \\h \\z \\u ");
    let escaped_instruction = escape_xml_text(&instruction);
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="TOCHeading"/></w:pPr><w:r><w:t>目录</w:t></w:r></w:p>
<w:p><w:pPr><w:tabs><w:tab w:val="right" w:leader="dot" w:pos="8306"/></w:tabs></w:pPr>
<w:r><w:fldChar w:fldCharType="begin" w:dirty="true"/></w:r>
<w:r><w:instrText xml:space="preserve">{escaped_instruction}</w:instrText></w:r>
<w:r><w:fldChar w:fldCharType="separate"/></w:r>
<w:r><w:rPr><w:i/><w:color w:val="6e6e73"/></w:rPr><w:t xml:space="preserve">在 Word / WPS 中右键此处选择「更新域」以生成目录</w:t></w:r>
<w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#
    )
}

/// 在 settings.xml 中开启「打开时更新域」。
///
/// 没有这一句，Word 不会主动提示更新，目录会一直停留在占位文字。
pub fn update_fields_setting() -> &'static str {
    "<w:updateFields w:val=\"true\"/>"
}

/// XML 文本转义（TOC 指令里含双引号，必须处理）。
fn escape_xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 生成域代码含三段式结构() {
        let xml = render_toc_field("1-3");
        assert!(xml.contains(r#"w:fldCharType="begin""#));
        assert!(xml.contains(r#"w:fldCharType="separate""#));
        assert!(xml.contains(r#"w:fldCharType="end""#));
    }

    #[test]
    fn 域指令含级别与超链接开关() {
        let xml = render_toc_field("1-3");
        assert!(xml.contains("TOC"));
        assert!(xml.contains(r"\o &quot;1-3&quot;"), "级别参数应被转义");
        assert!(xml.contains(r"\h"), "应包含超链接开关");
    }

    #[test]
    fn 标记域为脏以促使更新() {
        // w:dirty="true" 让阅读器知道该域需要重算
        let xml = render_toc_field("1-3");
        assert!(xml.contains(r#"w:dirty="true""#));
    }

    #[test]
    fn 占位文字可读而非空白() {
        let xml = render_toc_field("1-3");
        assert!(xml.contains("更新域"), "不更新域时也应给出提示");
    }

    #[test]
    fn 目录标题使用独立样式() {
        let xml = render_toc_field("1-3");
        assert!(xml.contains(r#"w:pStyle w:val="TOCHeading""#));
    }

    #[test]
    fn 更新域设置存在() {
        assert_eq!(update_fields_setting(), r#"<w:updateFields w:val="true"/>"#);
    }

    #[test]
    fn xml转义处理引号() {
        assert_eq!(escape_xml_text(r#"a"b"#), "a&quot;b");
        assert_eq!(escape_xml_text("a&b"), "a&amp;b");
        assert_eq!(escape_xml_text("<x>"), "&lt;x&gt;");
    }

    #[test]
    fn 级别参数可定制() {
        let xml = render_toc_field("1-2");
        assert!(xml.contains(r"\o &quot;1-2&quot;"));
    }
}
