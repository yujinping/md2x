//! 把 Markdown 中的 mermaid / flowchart 等图表代码块一次性渲染成 SVG，
//! 直接嵌入 HTML / DOCX（SVG 矢量图），PDF / PNG 亦走同一 HTML 路径因此内含静态 SVG。
//! 采用「转换期烘焙」策略：借助项目已有的 headless Chrome 加载内置的 `mermaid.min.js`
//! 渲染，再用 `--dump-dom` 取出结果，打开即显示，无需运行时再次解析。
//!
//! 降级策略：若找不到 Chrome 或渲染失败，保留原始代码块，绝不中断整个转换。

use crate::chrome;
use serde::Deserialize;
use std::process::Command;
use std::sync::OnceLock;

/// 别名围栏语言（即 ```mermaid / ```flowchart / ```sequenceDiagram ...）。
const MERMAID_LANGS: &[&str] = &[
    "mermaid", "flowchart", "flow", "sequence", "sequencediagram", "sequenceDiagram",
    "classdiagram", "classDiagram", "statediagram", "stateDiagram", "erdiagram", "erDiagram",
    "gantt", "pie", "journey", "gitgraph", "mindmap", "timeline", "quadrantchart",
    "quadrantChart", "requirementdiagram", "requirementDiagram", "block-beta", "kanban",
    "architecture", "c4context", "c4container", "c4component", "info",
];

/// 无语言围栏时，凭首行关键字嗅探是否为 mermaid 源码（完整识别的兜底）。
const MERMAID_KEYWORDS: &[&str] = &[
    "flowchart", "graph", "sequenceDiagram", "classDiagram", "stateDiagram", "state",
    "erDiagram", "gantt", "pie", "journey", "gitgraph", "mindmap", "timeline",
    "quadrantChart", "requirementDiagram", "block-beta", "kanban", "architecture",
    "c4context", "c4container", "c4component", "info", "requirement",
];

/// 前缀匹配放行（避免 graph/flowchart 等被更严格的关键字集合漏掉）。
const MERMAID_PREFIX: &[&str] = &[
    "flowchart", "graph", "statediagram", "classdiagram", "erdiagram", "sequencediagram", "c4",
];

/// 内置的 mermaid UMD 构建（离线可用，编译进二进制）。
const MERMAID_JS: &str = include_str!("../../../templates/assets/mermaid.min.js");

/// Chrome dump-dom 一次渲染的虚拟时间预算（毫秒）。
const VIRTUAL_TIME_BUDGET: u64 = 20000;

#[derive(Deserialize)]
struct DiagramResult {
    i: usize,
    svg: Option<String>,
}

/// 从 HTML 中识别出的 mermaid 代码块（保留原始区间以便原位替换）。
struct Block {
    start: usize,
    end: usize,
    source: String,
}

/// 判断某代码块是否为 mermaid 图表。
///
/// - 有明确语言且属于别名集合 → 是；
/// - 无语言时，按首行关键字嗅探（graph / flowchart / sequenceDiagram ...）。
pub fn is_mermaid_source(lang: &str, source: &str) -> bool {
    let l = lang.trim().to_lowercase();
    if !l.is_empty() {
        return MERMAID_LANGS.contains(&l.as_str());
    }
    // 嗅探：取第一个非空行的首个 token
    for line in source.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let first = t.split_whitespace().next().unwrap_or("");
        let exact = MERMAID_KEYWORDS.iter().any(|k| first.eq_ignore_ascii_case(k));
        let prefix = MERMAID_PREFIX
            .iter()
            .any(|p| first.len() >= p.len() && first[..p.len()].eq_ignore_ascii_case(p));
        return exact || prefix;
    }
    false
}

/// HTML 转义还原（comrak 会把代码内容转义，mermaid 需要原始文本）。
fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// 扫描 HTML，找出所有 mermaid 代码块（按出现顺序）。
fn extract_mermaid_blocks(html: &str) -> Vec<Block> {
    let re = mermaid_block_regex();
    let mut blocks = Vec::new();
    for cap in re.captures_iter(html) {
        let whole = cap.get(0).unwrap();
        let lang = cap.name("lang").map(|m| m.as_str()).unwrap_or("");
        let raw = cap.name("src").map(|m| m.as_str()).unwrap_or("");
        let body = unescape_html(raw);
        if is_mermaid_source(lang, &body) {
            // comrak 会把 ```` ```sequenceDiagram ```` 的 `sequenceDiagram` 当作
            // 「语言」放进 class，导致代码体本身丢失首行关键字。这里把指令补回去。
            let source = reconstruct_source(lang, &body);
            blocks.push(Block {
                start: whole.start(),
                end: whole.end(),
                source,
            });
        }
    }
    blocks
}

/// 还原 mermaid 源码首行：当 fence 语言本身是 mermaid 图表关键字
/// （如 sequenceDiagram / classDiagram / flowchart / graph ...）且代码体首行
/// 还不是关键字时，把该语言补为第一行指令。
fn reconstruct_source(lang: &str, body: &str) -> String {
    let l = lang.trim();
    if l.is_empty() || l.eq_ignore_ascii_case("mermaid") {
        return body.to_string();
    }
    if source_starts_with_keyword(body) {
        return body.to_string();
    }
    format!("{}\n{}", l, body)
}

/// 代码体首行是否已是 mermaid 图表关键字。
fn source_starts_with_keyword(body: &str) -> bool {
    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let first = t.split_whitespace().next().unwrap_or("");
        let exact = MERMAID_KEYWORDS.iter().any(|k| first.eq_ignore_ascii_case(k));
        let prefix = MERMAID_PREFIX
            .iter()
            .any(|p| first.len() >= p.len() && first[..p.len()].eq_ignore_ascii_case(p));
        return exact || prefix;
    }
    false
}

fn mermaid_block_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let langs = MERMAID_LANGS.join("|");
        // 匹配 <pre><code [class="language-ALIAS"]?>...</code></pre>
        // 有 class 时必须是别名之一；无 class 时进入嗅探分支。
        let pattern = format!(
            r#"(?s)<pre><code(?:\s+class="language-(?P<lang>{})")?\s*>(?P<src>.*?)</code></pre>"#,
            langs
        );
        regex::Regex::new(&pattern).unwrap()
    })
}

/// 把一组图表源码渲染成 SVG（及 PNG），返回与输入顺序一致的结果。
/// 任何失败都返回 None，调用方据此保留原始代码块。
fn render_via_chrome(chrome: &str, sources: &[String]) -> Option<Vec<DiagramResult>> {
    let tmp = std::env::temp_dir().join("rust-mpe-browser");
    std::fs::create_dir_all(&tmp).ok()?;

    // 写出内置 mermaid 库（仅首次）
    let mermaid_path = tmp.join("mermaid.min.js");
    if !mermaid_path.exists() {
        std::fs::write(&mermaid_path, MERMAID_JS).ok()?;
    }

    let diagrams_json = serde_json::to_string(sources).ok()?;
    let harness = build_harness(&diagrams_json);
    let harness_path = tmp.join("mermaid-harness.html");
    std::fs::write(&harness_path, harness).ok()?;
    let url = format!(
        "file://{}",
        std::fs::canonicalize(&harness_path).ok()?.display()
    );

    // 最多重试两次：虚拟时间预算内未渲染完（`data-status` 缺失）时再给更大预算。
    let mut budget = VIRTUAL_TIME_BUDGET;
    for _ in 0..2 {
        let output = Command::new(chrome)
            .args([
                "--headless=new",
                "--no-sandbox",
                "--disable-gpu",
                "--disable-dev-shm-usage",
                "--hide-scrollbars",
                &format!("--virtual-time-budget={}", budget),
                "--dump-dom",
                &url,
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let dom = String::from_utf8_lossy(&output.stdout);
        if let Some(mut results) = parse_mermaid_data(&dom) {
            // 按返回序号对齐，避免顺序错乱
            results.sort_by_key(|r| r.i);
            // 若渲染尚未完成（缺少 done 标记），重试一次；否则即便部分失败也接受。
            if dom.contains("data-status=\"done\"") || dom.contains("data-status='done'") {
                return Some(results);
            }
        }
        budget *= 2;
    }
    None
}

/// 解析 dump 出的 DOM 中 `__mermaid_data` 脚本里的 JSON（属性顺序无关）。
fn parse_mermaid_data(dom: &str) -> Option<Vec<DiagramResult>> {
    let marker = "id=\"__mermaid_data\"";
    let open = dom.find(marker)?;
    // 找到该 <script 标签的结束 `>`
    let after_tag = dom[open..].find('>')?;
    let start = open + after_tag + 1;
    let rest = &dom[start..];
    let end = rest.find("</script>")?;
    let json = &rest[..end];
    serde_json::from_str(json).ok()
}

/// 构造 harness 页面：加载内置 mermaid，逐个渲染并序列化结果到 JSON 脚本。
fn build_harness(diagrams_json: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<script src="mermaid.min.js"></script>
</head>
<body>
<script>
function reportError(msg, stack) {{
  try {{
    var e = document.createElement('script');
    e.id = '__mermaid_err';
    e.type = 'application/json';
    e.textContent = JSON.stringify({{ error: String(msg), stack: stack ? String(stack) : '' }});
    document.body.appendChild(e);
  }} catch (x) {{}}
  document.body.setAttribute('data-status', 'error');
}}
window.addEventListener('error', function(ev) {{
  reportError(ev.message, ev.error && ev.error.stack);
}});

const diagrams = {diagrams};
if (typeof mermaid === 'undefined') {{
  reportError('mermaid global is undefined', '');
}} else {{
  try {{
    mermaid.initialize({{ startOnLoad: false, securityLevel: 'loose', theme: 'default' }});
  }} catch (e) {{
    reportError('mermaid.initialize failed: ' + e.message, e.stack);
  }}
}}

(async () => {{
  const results = [];
  for (let i = 0; i < diagrams.length; i++) {{
    const code = diagrams[i];
    try {{
      const {{ svg }} = await mermaid.render('mmd-' + i, code);
      results.push({{ i: i, svg: svg }});
    }} catch (e) {{
      results.push({{ i: i, svg: null }});
    }}
  }}
  const s = document.createElement('script');
  s.id = '__mermaid_data';
  s.type = 'application/json';
  s.textContent = JSON.stringify(results);
  document.body.appendChild(s);
  document.body.setAttribute('data-status', 'done');
}})();
</script>
</body>
</html>"#,
        diagrams = diagrams_json
    )
}

/// 渲染整段 HTML 中的 mermaid 代码块为内嵌 SVG。
///
/// - 无 mermaid 块 → 原样返回；
/// - 找不到 Chrome / 渲染失败 → 原样返回（保留代码块）；
/// - 单个图渲染失败 → 该块保留代码块，其余正常替换。
pub fn render_mermaid_blocks(html: &str) -> String {
    let blocks = extract_mermaid_blocks(html);
    if blocks.is_empty() {
        return html.to_string();
    }
    let chrome = match chrome::find_chrome() {
        Ok(c) => c,
        Err(_) => return html.to_string(),
    };
    let sources: Vec<String> = blocks.iter().map(|b| b.source.clone()).collect();
    let results = match render_via_chrome(&chrome, &sources) {
        Some(r) => r,
        None => return html.to_string(),
    };

    let mut out = String::with_capacity(html.len());
    let mut pos = 0usize;
    for (k, blk) in blocks.iter().enumerate() {
        out.push_str(&html[pos..blk.start]);
        let replacement = match results.get(k) {
            Some(r) => match &r.svg {
                Some(svg) => format!("<div class=\"mermaid-diagram\">{}</div>", svg),
                None => html[blk.start..blk.end].to_string(),
            },
            None => html[blk.start..blk.end].to_string(),
        };
        out.push_str(&replacement);
        pos = blk.end;
    }
    out.push_str(&html[pos..]);
    out
}

/// 渲染单个 mermaid 源码为 SVG 字符串（供 DOCX 等直接内嵌矢量图使用）。
/// 失败返回 None。
pub fn render_mermaid_svg(source: &str) -> Option<String> {
    let chrome = chrome::find_chrome().ok()?;
    let sources = vec![source.to_string()];
    let results = render_via_chrome(&chrome, &sources)?;
    results.first()?.svg.clone()
}

/// 供 DOCX 等 AST 路径使用：给定 fence 语言与代码体，先还原缺失的首行关键字，
/// 再渲染为 SVG 字符串。失败返回 None。
pub fn render_mermaid_svg_from(lang: &str, literal: &str) -> Option<String> {
    let source = reconstruct_source(lang, literal);
    render_mermaid_svg(&source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_alias_languages() {
        assert!(is_mermaid_source("mermaid", "flowchart TD; A-->B"));
        assert!(is_mermaid_source("sequenceDiagram", "Alice->>Bob: hi"));
        assert!(is_mermaid_source("classDiagram", "A <|-- B"));
        assert!(is_mermaid_source("flowchart", "TD; A-->B"));
    }

    #[test]
    fn rejects_plain_code() {
        assert!(!is_mermaid_source("rust", "fn main() {}"));
        assert!(!is_mermaid_source("python", "print('hi')"));
    }

    #[test]
    fn sniffs_bare_fence_by_first_line() {
        assert!(is_mermaid_source("", "graph LR\n A --- B"));
        assert!(is_mermaid_source("", "sequenceDiagram\n A->>B: x"));
        assert!(!is_mermaid_source("", "just some text\n no keyword"));
    }

    #[test]
    fn reconstruct_prepends_missing_keyword() {
        // sequenceDiagram 被 comrak 当语言吃掉，代码体缺首行关键字
        let out = reconstruct_source("sequenceDiagram", "    Alice->>Bob: hi\n    Bob-->>Alice: ok");
        assert!(out.starts_with("sequenceDiagram\n"));
        assert!(out.contains("Alice->>Bob"));

        // flowchart 同理：若正文没带关键字则补回
        let out = reconstruct_source("flowchart", "TD\n A-->B");
        assert!(out.starts_with("flowchart\n"));
    }

    #[test]
    fn reconstruct_keeps_body_keyword() {
        // 正文已含关键字时不应重复前置
        let out = reconstruct_source("flowchart", "flowchart TD\n A-->B");
        assert_eq!(out, "flowchart TD\n A-->B");

        // mermaid 语言本身不需要前置
        let out = reconstruct_source("mermaid", "sequenceDiagram\n A->>B: x");
        assert_eq!(out, "sequenceDiagram\n A->>B: x");
    }

    #[test]
    fn unescape_html_reverts_comrak() {
        assert_eq!(unescape_html("&lt;tag&gt;"), "<tag>");
        assert_eq!(unescape_html("a &amp;&amp; b"), "a && b");
    }
}
