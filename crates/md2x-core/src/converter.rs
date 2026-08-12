use crate::error::MpeError;
use comrak::ComrakOptions;
use regex::Regex;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub fn convert_markdown_to_html(markdown: &str) -> Result<String, MpeError> {
    let mut options = ComrakOptions::default();
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.footnotes = true;
    options.parse.smart = true;
    options.render.hardbreaks = true;

    let body = strip_front_matter(markdown);
    // 修复含空格的链接目标：CommonMark 规定括号内裸链接目标不能含空格，
    // 否则 comrak 不会将其解析为链接。此处自动用 <> 包裹含空格且未包裹的目标。
    let body = fix_spaced_link_destinations(body);
    let html = comrak::markdown_to_html(&body, &options);
    let stripped = strip_p_in_task_lists(&html);
    Ok(stripped)
}

/// 修复含空格的链接/图片目标，使其能被 CommonMark 正确解析为链接。
///
/// CommonMark 中，形如 `[text](url)` 的裸链接目标不能包含空格，否则整段会被当作
/// 普通文本。合法写法是用尖括号包裹：`[text](<url with space>)`。本函数在目标
/// 含空格且尚未用 `<>` 包裹时自动加上 `<>`，从而让 comrak 渲染出真正的链接。
///
/// 处理规则：
/// - 跳过围栏代码块（```），避免改动代码中的字面文本；
/// - 跳过行内代码（`...`），避免误改字面文本；
/// - 已用 `<>` 包裹的目标保持不变；
/// - 含空格的目标自动包裹为 `<...>`；
/// - 支持可选的链接标题（"title" / 'title' / (title)）。
pub(crate) fn fix_spaced_link_destinations(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut in_fence = false;
    // split_inclusive 保留每行的换行符，从而精确保留原始换行结构
    for chunk in markdown.split_inclusive('\n') {
        let (line, nl) = match chunk.strip_suffix('\n') {
            Some(l) => (l, "\n"),
            None => (chunk, ""),
        };
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            out.push_str(line);
            out.push_str(nl);
            continue;
        }
        if in_fence {
            out.push_str(line);
            out.push_str(nl);
            continue;
        }
        out.push_str(&fix_links_in_line(line));
        out.push_str(nl);
    }
    out
}

/// 对单行应用链接目标修复，跳过行内代码段。
fn fix_links_in_line(line: &str) -> String {
    let link_re = link_dest_regex();
    let code_re = inline_code_regex();
    let mut out = String::with_capacity(line.len());
    let mut last = 0;
    for m in code_re.find_iter(line) {
        // 先在代码段之前的非代码文本上应用修复
        let before = &line[last..m.start()];
        out.push_str(&link_re.replace_all(before, replace_link));
        // 行内代码原样保留
        out.push_str(m.as_str());
        last = m.end();
    }
    let after = &line[last..];
    out.push_str(&link_re.replace_all(after, replace_link));
    out
}

/// 匹配内联链接/图片的整体结构，捕获 label、dest（目标）与可选 title（标题）。
fn link_dest_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            "(?P<label>!?\\[[^\\]]*\\])\\((?P<dest>[^)]*?)(?P<title>(?:\\s+\"[^\"]*\")?(?:\\s+'[^']*')?(?:\\s+\\([^)]*\\))?)\\)",
        )
        .unwrap()
    })
}

/// 匹配行内代码段 `code`（单反引号）。
fn inline_code_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("`[^`\n]*`").unwrap())
}

/// 将单个匹配重写成合法链接：必要时用 <> 包裹含空格的目标。
fn replace_link(caps: &regex::Captures) -> String {
    let label = &caps["label"];
    let dest = caps.name("dest").map(|m| m.as_str()).unwrap_or("");
    let title = caps.name("title").map(|m| m.as_str()).unwrap_or("");
    if dest.starts_with('<') {
        // 已用 <> 包裹，保持原样
        format!("{}({}{})", label, dest, title)
    } else if dest.chars().any(|c| c.is_whitespace()) {
        // 含空格，自动包裹
        format!("{}(<{}>{})", label, dest, title)
    } else {
        format!("{}({}{})", label, dest, title)
    }
}

/// 解析 HTML 中的图片路径，转为 base64 data URI。
/// - 相对路径 → 读取本地文件转为 data URI
/// - / 开头的路径 → 按 Hugo 项目结构查找（遍历父目录找 static/ 目录）
/// - http/https 远程图片 → 下载并转为 data URI
/// - 无法读取/下载的图片跳过（保留原 src），不中断转换
pub fn resolve_image_srcs(html: &str, md_file: &Path) -> String {
    let base = md_file.parent().unwrap_or(Path::new("."));
    let hugo_root = find_hugo_root(md_file);
    let mut result = String::with_capacity(html.len() + 4096);
    let mut pos = 0;
    let bytes = html.as_bytes();

    while pos < bytes.len() {
        // 查找 <img
        if let Some(start) = bytes[pos..].windows(4).position(|w| w == b"<img") {
            let tag_start = pos + start;
            result.push_str(&html[pos..tag_start]);

            // 找到 > 结束
            let tag_end = match bytes[tag_start..].iter().position(|&b| b == b'>') {
                Some(e) => tag_start + e + 1,
                None => {
                    result.push_str(&html[tag_start..]);
                    break;
                }
            };

            let tag = &html[tag_start..tag_end];
            // 尝试提取 src="..." 或 src='...'
            let resolved = try_resolve_src(tag, base, hugo_root.as_deref());
            result.push_str(&resolved);
            pos = tag_end;
        } else {
            result.push_str(&html[pos..]);
            break;
        }
    }
    result
}

/// 尝试解析单个 <img 标签中的 src 属性
fn try_resolve_src(tag: &str, base: &Path, hugo_root: Option<&Path>) -> String {
    // 找 src="..." 或 src='...'
    let src = tag
        .find("src=\"")
        .and_then(|i| {
            let val_start = i + 5;
            let val_end = tag[val_start..].find('"')?;
            Some(&tag[val_start..val_start + val_end])
        })
        .or_else(|| {
            tag.find("src='").and_then(|i| {
                let val_start = i + 5;
                let val_end = tag[val_start..].find('\'')?;
                Some(&tag[val_start..val_start + val_end])
            })
        });

    match src {
        Some(s) if s.starts_with("http://") || s.starts_with("https://") => {
            // 远程图片：下载并转为 data URI 内嵌到 HTML 中
            match download_image_as_data_uri(s) {
                Some(data_uri) => {
                    let quote = if tag.contains("src=\"") { '"' } else { '\'' };
                    let src_attr = format!("src={0}{1}{0}", quote, s);
                    tag.replace(&src_attr, &format!("src=\"{}\"", data_uri))
                }
                None => tag.to_string(), // 下载失败，保留原始 src
            }
        }
        Some(s) if is_resolvable_path(s) => {
            let img_path = if s.starts_with('/') {
                // Hugo 风格：/images/foo.png → <hugo_root>/static/images/foo.png
                match hugo_root {
                    Some(root) => root.join("static").join(s.strip_prefix('/').unwrap_or(s)),
                    None => return tag.to_string(), // 未找到 Hugo 根目录，跳过
                }
            } else {
                // 普通相对路径
                base.join(s)
            };
            match std::fs::read(&img_path) {
                Ok(data) => {
                    let mime = mime_from_ext(&img_path);
                    let b64 = base64_encode(&data);
                    let replacement = format!("src=\"data:{};base64,{}\"", mime, b64);
                    let quote = if tag.contains("src=\"") { '"' } else { '\'' };
                    let src_attr = format!("src={0}{1}{0}", quote, s);
                    tag.replace(&src_attr, &replacement)
                }
                Err(_) => tag.to_string(), // 保留原值
            }
        }
        _ => tag.to_string(),
    }
}

/// 下载远程图片并转为 data URI 字符串
fn download_image_as_data_uri(url: &str) -> Option<String> {
    let (body, mime) = download_image_bytes(url)?;
    let b64 = base64_encode(&body);
    Some(format!("data:{};base64,{}", mime, b64))
}

/// 判断路径是否需要本地解析（排除 data/file 协议和锚点）
pub(crate) fn is_resolvable_path(src: &str) -> bool {
    !(src.starts_with("http://")
        || src.starts_with("https://")
        || src.starts_with("data:")
        || src.starts_with("file://")
        || src.starts_with('#'))
}

/// 从 markdown 文件向上遍历，查找 Hugo 项目根目录（含 static/ 子目录的祖先目录）
pub(crate) fn find_hugo_root(md_file: &Path) -> Option<PathBuf> {
    let mut dir = md_file.parent()?;
    loop {
        if dir.join("static").is_dir() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

pub(crate) fn mime_from_ext(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// 下载远程图片，返回 (字节, content type)。
pub(crate) fn download_image_bytes(url: &str) -> Option<(Vec<u8>, String)> {
    let resp = ureq::get(url).set("User-Agent", "md2x/0.1").call().ok()?;
    let mime = resp
        .header("content-type")
        .unwrap_or("application/octet-stream")
        .to_string();
    let mut body: Vec<u8> = Vec::new();
    resp.into_reader().read_to_end(&mut body).ok()?;
    Some((body, mime))
}

/// 极简 base64 编码（无依赖）
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// 解析 YAML frontmatter，返回 (metadata_map, 去掉 frontmatter 后的正文)
/// 若不存在 frontmatter，返回 (None, 原始文本)
pub fn parse_front_matter(md: &str) -> (Option<HashMap<String, String>>, &str) {
    let s = md.trim_start();
    if s.starts_with("---") {
        if let Some(end) = s[3..].find("\n---") {
            let front = &s[3..3 + end];
            let body = s[3 + end + 4..].trim_start();

            let mut map = HashMap::new();
            for line in front.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    let k = key.trim().to_string();
                    let v = value.trim().to_string();
                    if !k.is_empty() {
                        map.insert(k, v);
                    }
                }
            }

            return (if map.is_empty() { None } else { Some(map) }, body);
        }
    }
    (None, md)
}

/// 去掉 YAML 元数据（`---\n...\n---` 开头的内容）
fn strip_front_matter(md: &str) -> &str {
    let s = md.trim_start();
    if s.starts_with("---") {
        if let Some(end) = s[3..].find("\n---") {
            // end 是 \n--- 中 \n 的位置，\n--- 共 4 字符
            return s[3 + end + 4..].trim_start();
        }
    }
    md
}

/// comrak 在 task list 内有空行时会包一层 <p>，导致 checkbox 和文字分两行。
/// 去掉 task-list-item 内部的 <p> 和 </p>（comrak 可能不关闭 </li>，用下一个 <li 做边界）。
fn strip_p_in_task_lists(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut remain = html;

    while let Some(cb) = remain.find("type=\"checkbox\"") {
        // 找到包含这个 checkbox 的 <li
        let li_start = remain[..cb].rfind("<li").unwrap_or(0);
        out.push_str(&remain[..li_start]);

        let part = &remain[li_start..];
        // 跳过当前 <li...>
        let open_end = part.find('>').unwrap_or(0) + 1;
        // 找此 <li> 的结尾（下一个 <li 或 </li>）
        let after_open = &part[open_end..];
        let next = after_open.find("<li").or_else(|| after_open.find("</li>"));
        let boundary = next.map(|p| open_end + p).unwrap_or(part.len());

        let item = &part[..boundary];
        let cleaned = item.replace("<p>", "").replace("</p>", "");
        out.push_str(&cleaned);
        remain = &remain[li_start + boundary..];
    }
    out.push_str(remain);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_p_in_task_list() {
        let input =
            "<li><input type=\"checkbox\" disabled=\"\" />\n<p><strong>步骤 1</strong></p>\n</li>";
        let result = strip_p_in_task_lists(input);
        assert!(!result.contains("<p>"));
        assert!(result.contains("<strong>步骤 1</strong>"));
    }

    #[test]
    fn test_strip_front_matter() {
        let md = "---\ntitle: test\n---\n\n# Hello";
        assert_eq!(strip_front_matter(md), "# Hello");
    }

    #[test]
    fn test_no_front_matter() {
        assert_eq!(strip_front_matter("# Hello"), "# Hello");
    }

    #[test]
    fn test_parse_front_matter_returns_metadata() {
        let md = "---\nname: my-skill\ndescription: A test skill\n---\n\n# Hello";
        let (meta, body) = parse_front_matter(md);
        assert_eq!(body, "# Hello");
        assert!(meta.is_some());
        let m = meta.unwrap();
        assert_eq!(m.get("name").unwrap(), "my-skill");
        assert_eq!(m.get("description").unwrap(), "A test skill");
    }

    #[test]
    fn test_parse_front_matter_no_metadata() {
        let (meta, body) = parse_front_matter("# Just content");
        assert!(meta.is_none());
        assert_eq!(body, "# Just content");
    }

    #[test]
    fn test_parse_front_matter_empty_returns_none() {
        let md = "---\n---\n\n# Content";
        let (meta, body) = parse_front_matter(md);
        assert!(meta.is_none());
        assert_eq!(body, "# Content");
    }

    #[test]
    fn test_parse_front_matter_extra_fields() {
        let md = "---\nname: my-skill\ndescription: Test\nrunAs: subagent\nscope: global\nmodel: claude\n---\n\nBody";
        let (meta, body) = parse_front_matter(md);
        assert_eq!(body, "Body");
        let m = meta.unwrap();
        assert_eq!(m.get("runAs").unwrap(), "subagent");
        assert_eq!(m.get("scope").unwrap(), "global");
        assert_eq!(m.get("model").unwrap(), "claude");
    }

    #[test]
    fn test_regular_p_untouched() {
        let input = "<p>普通段落</p>";
        let result = strip_p_in_task_lists(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_is_resolvable_path() {
        assert!(!is_resolvable_path("https://example.com/img.png"));
        assert!(!is_resolvable_path("http://example.com/img.png"));
        assert!(!is_resolvable_path("data:image/png;base64,abc"));
        assert!(!is_resolvable_path("file:///tmp/img.png"));
        assert!(is_resolvable_path("/absolute/path.png")); // Hugo 风格，应该解析
        assert!(is_resolvable_path("images/foo.png"));
        assert!(is_resolvable_path("./img.png"));
        assert!(is_resolvable_path("../img.png"));
        assert!(is_resolvable_path("sub/nested/img.jpg"));
    }

    #[test]
    fn test_mime_from_ext() {
        assert_eq!(mime_from_ext(Path::new("img.png")), "image/png");
        assert_eq!(mime_from_ext(Path::new("img.jpg")), "image/jpeg");
        assert_eq!(mime_from_ext(Path::new("img.jpeg")), "image/jpeg");
        assert_eq!(mime_from_ext(Path::new("img.gif")), "image/gif");
        assert_eq!(mime_from_ext(Path::new("img.svg")), "image/svg+xml");
        assert_eq!(mime_from_ext(Path::new("img.webp")), "image/webp");
        assert_eq!(
            mime_from_ext(Path::new("img.unknown")),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
    }

    #[test]
    fn test_remote_image_download_fails_keeps_original() {
        // 远程图片下载失败时，保留原始 src
        let html = r#"<img src="https://example.com/img.png">"#;
        let result = resolve_image_srcs(html, Path::new("/tmp/test.md"));
        assert_eq!(result, html);
    }

    #[test]
    fn test_resolve_image_hugo_path_when_no_root_keeps_original() {
        // / 开头的路径但没找到 Hugo 根目录，保留原 src
        let html = r#"<img src="/images/hero.png">"#;
        let result = resolve_image_srcs(html, Path::new("/tmp/test.md"));
        assert_eq!(result, html);
    }

    #[test]
    fn test_resolve_image_no_img_tag() {
        let html = "<p>hello</p>";
        let result = resolve_image_srcs(html, Path::new("/tmp/test.md"));
        assert_eq!(result, html);
    }

    #[test]
    fn test_resolve_image_file_not_found_keeps_original() {
        // 文件不存在时保留原 src
        let html = r#"<img src="nonexistent.png">"#;
        let result = resolve_image_srcs(html, Path::new("/tmp/test.md"));
        assert_eq!(result, html);
    }

    #[test]
    fn test_hugo_image_path_resolved_correctly() {
        // 模拟 Hugo 项目结构：在临时目录中创建 static/images/ 和图片
        let dir = std::env::temp_dir().join("hugo-test-img");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("static/images")).unwrap();
        std::fs::create_dir_all(dir.join("content/posts")).unwrap();

        // 创建一张 1x1 红色 PNG（最小合法 PNG）
        let red_png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
            0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, // IDAT chunk
            0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x36,
            0x28, 0x19, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND chunk
            0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(dir.join("static/images/hero.png"), red_png).unwrap();

        // 模拟 Hugo 风格的 md 文件路径
        let md_path = dir.join("content/posts/my-post.md");
        let html = r#"<img src="/images/hero.png">"#;

        let result = resolve_image_srcs(html, &md_path);

        // 验证图片被解析为 data URI（不再是 /images/hero.png）
        assert!(
            result.starts_with(r#"<img src="data:image/png;base64,"#),
            "Hugo 图片应被解析为 base64 data URI"
        );
        assert!(!result.contains("/images/hero.png"), "原始路径应被替换");

        // 清理
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fix_spaced_link_destinations() {
        // 含空格的链接目标应被 <> 包裹
        assert_eq!(
            fix_spaced_link_destinations("[a](file with space.md)"),
            "[a](<file with space.md>)"
        );
        // 不含空格的链接不改动
        let plain = "[b](normal.md)";
        assert_eq!(fix_spaced_link_destinations(plain), plain);
        // 已用 <> 包裹的不重复包裹
        let wrapped = "[c](<already wrapped.md>)";
        assert_eq!(fix_spaced_link_destinations(wrapped), wrapped);
        // 图片链接同样处理
        assert_eq!(
            fix_spaced_link_destinations("![img](path with space.png)"),
            "![img](<path with space.png>)"
        );
        // 含空格且带标题的链接，仅包裹目标
        assert_eq!(
            fix_spaced_link_destinations("[d](url with space \"标题\")"),
            "[d](<url with space> \"标题\")"
        );
        // 表格中的链接
        let table = "| [08-x.md](08-x.md) | [09-y z.md](09-y z.md) |";
        let out = fix_spaced_link_destinations(table);
        assert!(out.contains("[08-x.md](08-x.md)"), "无空格链接不变");
        assert!(
            out.contains("[09-y z.md](<09-y z.md>)"),
            "表格中含空格链接应被包裹"
        );
    }

    #[test]
    fn test_fix_spaced_link_skips_code() {
        // 围栏代码块中的字面文本不应被改动
        let md = "```\n[code](a b.md)\n```\n[real](a b.md)";
        let out = fix_spaced_link_destinations(md);
        assert!(out.contains("```\n[code](a b.md)\n```"), "代码块内容不变");
        assert!(out.contains("[real](<a b.md>)"), "代码块外链接应修复");

        // 行内代码中的字面文本不应被改动
        let inline = "见 `[x](a b)` 与 [y](a b.md)";
        let out = fix_spaced_link_destinations(inline);
        assert!(out.contains("`[x](a b)`"), "行内代码不变");
        assert!(out.contains("[y](<a b.md>)"), "行内代码外链接应修复");
    }

    #[test]
    fn test_spaced_link_renders_as_anchor() {
        // 复现用户报告的场景：文件名含空格的链接应渲染为真正的 <a>
        let md = "| [08-新仓库重建与追溯规范.md](08-新仓库重建与追溯规范.md) | \
                  [09-Kotlin 重构决策与风险.md](09-Kotlin 重构决策与风险.md) |";
        let html = convert_markdown_to_html(md).unwrap();
        assert!(html.contains("<a "), "含空格链接应渲染为 <a> 标签");
        assert!(
            html.contains("09-Kotlin"),
            "链接文本/目标应保留 Kotlin 文件名"
        );
        // 两个链接都应存在
        let anchor_count = html.matches("<a ").count();
        assert!(anchor_count >= 2, "应至少渲染出两个链接，实际：{anchor_count}");
    }
}
