//! 基于 syntect 的代码高亮（One Dark 配色，与 HTML 版 atom-one-dark 一致）。

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

const ONE_DARK_THEME: &str = include_str!("../../assets/atom-one-dark.tmTheme");

fn syntax_set() -> &'static SyntaxSet {
    use std::sync::OnceLock;
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    use std::sync::OnceLock;
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let mut reader = std::io::Cursor::new(ONE_DARK_THEME);
        ThemeSet::load_from_reader(&mut reader)
            .expect("内嵌 atom-one-dark.tmTheme 解析失败")
    })
}

/// 高亮代码，返回按 token 切分的 (文本, 颜色 hex 大写)。
/// 语言未知或解析失败时回退为纯文本。
pub fn highlight_code(lang: Option<&str>, code: &str) -> Vec<(String, String)> {
    let syntax = match lang {
        Some(l) if !l.is_empty() => syntax_set()
            .find_syntax_by_token(l)
            .or_else(|| syntax_set().find_syntax_by_extension(l)),
        _ => None,
    }
    .unwrap_or_else(|| syntax_set().find_syntax_plain_text());

    let mut out: Vec<(String, String)> = Vec::new();
    let mut highlighter = HighlightLines::new(syntax, theme());
    for line in LinesWithEndings::from(code) {
        let regions = match highlighter.highlight_line(line, syntax_set()) {
            Ok(r) => r,
            Err(_) => return vec![(code.to_string(), "abb2bf".to_string())],
        };
        for (style, text) in regions {
            if text.is_empty() {
                continue;
            }
            let color = style.foreground;
            out.push((
                text.to_string(),
                format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b),
            ));
        }
    }
    out
}
