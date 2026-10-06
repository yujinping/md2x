use std::collections::HashMap;

/// 重写链接所需的上下文
pub struct RewriteCtx<'a> {
    /// 当前文档 section 的锚点前缀，形如 `doc-0-`
    pub self_prefix: &'a str,
    /// 当前文档相对根目录的所在目录，形如 `sub/deep`（顶层为空串）
    pub self_dir: &'a str,
    /// 相对路径（`/` 分隔）→ section id
    pub by_rel: &'a HashMap<String, String>,
    /// section id → 该文档的「规范化标题 → 真实锚点 id」表
    pub headings: &'a HashMap<String, HashMap<String, String>>,
}

impl RewriteCtx<'_> {
    fn is_external(href: &str) -> bool {
        let lower = href.trim().to_ascii_lowercase();
        lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:")
            || lower.starts_with("tel:")
            || lower.starts_with("data:")
            || lower.starts_with("file://")
            || lower.starts_with("javascript:")
            // Hugo 风格绝对路径指向站点根，非仓库内相对路径
            || href.starts_with('/')
    }
}

/// 重写 HTML 中的本地文档链接，使其在合并后的单页内跳转。
///
/// 处理规则：
/// - 外部链接（http/https/mailto/data/file/绝对路径）原样保留；
/// - 纯锚点 `#x` → 加上当前文档前缀，指向本文档内标题；
/// - 本地 md 链接（可带 `./`、`../` 与 `#锚点`）→ 解析为文档间跳转；
/// - 目标不存在或不是 md（图片、pdf 等）→ 原样保留，避免产出死链。
pub fn rewrite_links(html: &str, ctx: &RewriteCtx) -> String {
    let mut out = String::with_capacity(html.len());
    let mut pos = 0usize;

    while pos < html.len() {
        // 只在 <a 标签内处理 href
        let Some(tag_start) = find_tag(html, pos, "<a") else {
            out.push_str(&html[pos..]);
            break;
        };
        out.push_str(&html[pos..tag_start]);

        let Some(tag_end) = html[tag_start..].find('>').map(|i| tag_start + i + 1) else {
            out.push_str(&html[tag_start..]);
            break;
        };

        let tag = &html[tag_start..tag_end];
        out.push_str(&rewrite_tag(tag, ctx));
        pos = tag_end;
    }
    out
}

/// 从 `from` 起查找下一个 `<a` 标签的 `<` 位置，跳过 `<abbr` 之类前缀相同的标签
fn find_tag(html: &str, from: usize, open: &str) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut i = from;
    while let Some(rel) = html[i..].find(open) {
        let abs = i + rel;
        // 标签名后必须是空白或 `>`，避免把 <abbr>误判为 <a>
        let next = bytes.get(abs + open.len()).copied();
        match next {
            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'>') | Some(b'\r') => {
                return Some(abs)
            }
            _ => i = abs + open.len(),
        }
    }
    None
}

fn rewrite_tag(tag: &str, ctx: &RewriteCtx) -> String {
    let Some((attr_start, attr_end, href)) = find_href_attr(tag) else {
        return tag.to_string();
    };
    let Some(new_href) = map_href(href, ctx) else {
        return tag.to_string();
    };
    if new_href == href {
        return tag.to_string();
    }
    let mut out = String::with_capacity(tag.len() + new_href.len());
    out.push_str(&tag[..attr_start]);
    out.push_str(&new_href);
    out.push_str(&tag[attr_end..]);
    out
}

/// 定位 `href="..."` 的值区间（不含引号），返回 (值起点, 值终点, 值)
///
/// 用逐字符的属性扫描而非字符串查找：`title="see href= here"` 这类属性值内部
/// 同样会出现 `href=` 字样，只有真正在标签结构里遇到属性名才认定为链接目标。
fn find_href_attr(tag: &str) -> Option<(usize, usize, &str)> {
    let bytes = tag.as_bytes();
    let mut i = 0usize;

    // 跳过标签名（如 `<a`）
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
        i += 1;
    }

    while i < bytes.len() {
        // 跳过属性间空白
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'>' || bytes[i] == b'/' {
            return None;
        }

        // 读属性名
        let name_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let name = &tag[name_start..i];

        // 跳过 = 与其周围空白
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            // 无值的属性（如 `disabled`），继续扫下一个
            continue;
        }
        i += 1; // 消费 =
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        // 读属性值
        if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
            let q = bytes[i];
            let vstart = i + 1;
            let off = tag[vstart..].find(q as char)?;
            let vend = vstart + off;
            if name == "href" {
                return Some((vstart, vend, &tag[vstart..vend]));
            }
            // 跳过整个带引号的值，天然排除值内部的 href= 字样
            i = vend + 1;
        } else {
            // 无引号的裸值
            let vstart = i;
            let vend = tag[vstart..]
                .find(|c: char| c.is_whitespace() || c == '>')
                .map(|e| vstart + e)
                .unwrap_or(tag.len());
            if name == "href" {
                return Some((vstart, vend, &tag[vstart..vend]));
            }
            i = vend;
        }
    }
    None
}

/// 计算新href；返回 None 表示无需改动
fn map_href(href: &str, ctx: &RewriteCtx) -> Option<String> {
    let raw = href.trim();
    if raw.is_empty() || RewriteCtx::is_external(raw) {
        return None;
    }

    // comrak 会对 href 做 percent-encoding，必须先解码才能匹配真实文件名
    let decoded = percent_decode(raw);

    let (path_part, frag) = match decoded.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (decoded.as_str(), None),
    };

    // 纯锚点：本文档内跳转
    if path_part.is_empty() {
        let f = frag?;
        if f.is_empty() {
            return None;
        }
        let own = ctx
            .headings
            .get(&own_section_id(ctx))
            .and_then(|m| m.get(&crate::template::normalize_anchor(f)));
        return Some(match own {
            Some(id) => format!("#{}", id),
            // 未匹配到标题时退回「前缀 + 规范化锚点」，至少不会跳出页面
            None => format!("#{}{}", ctx.self_prefix, crate::template::normalize_anchor(f)),
        });
    }

    // 本地文档链接
    if !is_markdown_path(path_part) {
        return None;
    }
    let rel = super::collector::normalize_rel(ctx.self_dir, path_part);
    let Some(section_id) = ctx.by_rel.get(&rel) else {
        // 未被聚合的文档（不存在 / 非 md）保留原样，避免死链
        return None;
    };

    match frag {
        Some(f) if !f.is_empty() => {
            let key = crate::template::normalize_anchor(f);
            match ctx.headings.get(section_id).and_then(|m| m.get(&key)) {
                Some(id) => Some(format!("#{}", id)),
                // 目标文档存在但锚点没匹配上：退化为跳到文档开头
                None => Some(format!("#{}", section_id)),
            }
        }
        _ => Some(format!("#{}", section_id)),
    }
}

/// 由锚点前缀反推所属 section id（`doc-0-` → `doc-0`）
fn own_section_id(ctx: &RewriteCtx) -> String {
    ctx.self_prefix.trim_end_matches('-').to_string()
}

fn is_markdown_path(p: &str) -> bool {
    let lower = p.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

/// percent-decode：把 `%E4%B8%AD` 还原为 UTF-8 字节再转字符串。
///
/// 非法转义（如 `%zz` 或残缺 `%E4`）原样保留，不让单个坏链接中断整篇转换。
fn percent_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_fixture<'a>(
        by_rel: &'a HashMap<String, String>,
        headings: &'a HashMap<String, HashMap<String, String>>,
    ) -> RewriteCtx<'a> {
        RewriteCtx {
            self_prefix: "doc-0-",
            self_dir: "",
            by_rel,
            headings,
        }
    }

    /// 构造与真实链路一致的索引：键为 `normalize_anchor(标题文本)`，值为带前缀的真实 id
    fn maps() -> (HashMap<String, String>, HashMap<String, HashMap<String, String>>) {
        let mut by_rel = HashMap::new();
        by_rel.insert("a.md".to_string(), "doc-0".to_string());
        by_rel.insert("sub/b.md".to_string(), "doc-1".to_string());
        let mut headings = HashMap::new();
        let mut h0 = HashMap::new();
        h0.insert(
            crate::template::normalize_anchor("概述"),
            "doc-0-概述".to_string(),
        );
        headings.insert("doc-0".to_string(), h0);
        let mut h1 = HashMap::new();
        h1.insert(
            crate::template::normalize_anchor("概述"),
            "doc-1-概述".to_string(),
        );
        headings.insert("doc-1".to_string(), h1);
        (by_rel, headings)
    }

    #[test]
    fn 外部链接原样保留() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        for href in [
            "https://example.com",
            "http://a.b/c.md",
            "mailto:x@y.com",
            "data:image/png;base64,AAA",
            "/images/a.png",
        ] {
            let html = format!("<a href=\"{}\">x</a>", href);
            assert_eq!(rewrite_links(&html, &c), html, "{} 不应被改写", href);
        }
    }

    #[test]
    fn 跨文档链接改写为锚点跳转() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        let out = rewrite_links("<a href=\"sub/b.md\">B</a>", &c);
        assert_eq!(out, "<a href=\"#doc-1\">B</a>");
    }

    #[test]
    fn 同文档锚点加前缀() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        // 「概述」是 doc-0 的真实标题，应映射到带前缀的锚点
        let out = rewrite_links("<a href=\"#%E6%A6%82%E8%BF%B0\">概述</a>", &c);
        assert_eq!(out, "<a href=\"#doc-0-概述\">概述</a>");
    }

    #[test]
    fn 同文档锚点未匹配时按前缀兜底() {
        // 锚点指向本页不存在的标题：仍应留在本页内，而不是变成死链
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        let out = rewrite_links("<a href=\"#不存在的节\">X</a>", &c);
        assert_eq!(out, "<a href=\"#doc-0-不存在的节\">X</a>");
    }

    #[test]
    fn 跨文档带锚点时两级映射() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        // sub/b.md 的锚点「概述」→ doc-1 文档内的真实锚点 doc-1-概述
        let out = rewrite_links("<a href=\"sub/b.md#%E6%A6%82%E8%BF%B0\">B</a>", &c);
        assert_eq!(out, "<a href=\"#doc-1-概述\">B</a>");
    }

    #[test]
    fn 相对上级路径被正确消解() {
        let (b, h) = maps();
        let mut c = ctx_fixture(&b, &h);
        c.self_dir = "sub/deep";
        // sub/deep/../b.md → sub/b.md → doc-1
        let out = rewrite_links("<a href=\"../b.md\">B</a>", &c);
        assert_eq!(out, "<a href=\"#doc-1\">B</a>");
    }

    #[test]
    fn 当前目录前缀被正确消解() {
        let (b, h) = maps();
        let mut c = ctx_fixture(&b, &h);
        c.self_dir = "sub";
        let out = rewrite_links("<a href=\"./b.md\">B</a>", &c);
        assert_eq!(out, "<a href=\"#doc-1\">B</a>");
    }

    #[test]
    fn 未聚合的文档保留原链接() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        let html = "<a href=\"missing.md\">M</a>";
        assert_eq!(rewrite_links(html, &c), html);
    }

    #[test]
    fn 非md链接如图片保持原样() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        let html = "<a href=\"pic.png\">P</a>";
        assert_eq!(rewrite_links(html, &c), html);
    }

    #[test]
    fn 多个链接一次性正确改写() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        let html = "<p><a href=\"a.md\">A</a> 和 <a href=\"sub/b.md\">B</a> 与 <a href=\"https://x.com\">X</a></p>";
        let out = rewrite_links(html, &c);
        assert!(out.contains("href=\"#doc-0\""));
        assert!(out.contains("href=\"#doc-1\""));
        assert!(out.contains("href=\"https://x.com\""));
    }

    #[test]
    fn 不误伤abbr标签() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        // <abbr title="x"> 内不含 href，不应被当成 <a> 处理导致截断
        let html = "<p><abbr title=\"HTML\">H</abbr> <a href=\"a.md\">A</a></p>";
        let out = rewrite_links(html, &c);
        assert!(out.contains("<abbr title=\"HTML\">"));
        assert!(out.contains("href=\"#doc-0\""));
    }

    #[test]
    fn 单引号href也能改写() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        let out = rewrite_links("<a href='sub/b.md'>B</a>", &c);
        assert_eq!(out, "<a href='#doc-1'>B</a>");
    }

    #[test]
    fn 属性值内的href字样不被误判() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        let html = "<a title=\"see href= here\" href=\"a.md\">A</a>";
        let out = rewrite_links(html, &c);
        assert!(out.contains("title=\"see href= here\""));
        assert!(out.contains("href=\"#doc-0\""));
    }

    #[test]
    fn percent解码还原中文与空格() {
        assert_eq!(percent_decode("b%20c.md"), "b c.md");
        assert_eq!(percent_decode("%E4%B8%AD%E6%96%87.md"), "中文.md");
    }

    #[test]
    fn 非法转义原样保留不中断() {
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("a%"), "a%");
    }

    #[test]
    fn 目标文档存在但锚点未匹配时退化为文档开头() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        let out = rewrite_links("<a href=\"sub/b.md#不存在的节\">B</a>", &c);
        assert_eq!(out, "<a href=\"#doc-1\">B</a>");
    }

    #[test]
    fn 空href不处理() {
        let (b, h) = maps();
        let c = ctx_fixture(&b, &h);
        let html = "<a href=\"\">x</a>";
        assert_eq!(rewrite_links(html, &c), html);
    }
}
