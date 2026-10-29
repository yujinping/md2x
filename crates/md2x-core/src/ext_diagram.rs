//! 外部 CLI 图表引擎：D2 与 Graphviz（dot）。
//!
//! 与 mermaid（内置 JS bundle + headless Chrome）不同，这两者的渲染器
//! 生态没有可内嵌的构建产物（D2 是 Go CLI、Graphviz 是 C 库），因此
//! 走外部命令行方案：检测 PATH 中的可执行文件，转换期一次性烘焙为
//! 内嵌 SVG。
//!
//! 降级链与 mermaid 一致：CLI 不存在或渲染失败 → 保留原始代码块，
//! 不中断导出；装上依赖后重新导出即可生效。
//!
//! @author yujinping

use std::process::Command;

/// 支持的外部图表工具定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtTool {
    /// 工具 id（GUI 设置面板、状态查询用）
    pub id: &'static str,
    /// PATH 中检索的可执行文件名
    pub binary: &'static str,
    /// 触发渲染的 fence 语言（小写）
    pub fence_langs: &'static [&'static str],
}

/// 全部支持的外部图表工具（顺序即设置面板展示顺序）。
pub const EXT_TOOLS: &[ExtTool] = &[
    ExtTool {
        id: "d2",
        binary: "d2",
        fence_langs: &["d2"],
    },
    ExtTool {
        id: "dot",
        binary: "dot",
        fence_langs: &["dot", "graphviz"],
    },
];

/// 按 id 查工具定义。
pub fn tool_by_id(id: &str) -> Option<&'static ExtTool> {
    EXT_TOOLS.iter().find(|t| t.id == id)
}

/// fence 语言对应的工具；非外部图表语言返回 None。
pub fn tool_for_lang(lang: &str) -> Option<&'static ExtTool> {
    let l = lang.trim().to_lowercase();
    EXT_TOOLS
        .iter()
        .find(|t| t.fence_langs.contains(&l.as_str()))
}

/// 工具安装状态（GUI 设置面板展示用）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    /// 工具 id（d2 / dot）
    pub id: String,
    /// 可执行文件名
    pub binary: String,
    /// 是否在 PATH 中检出
    pub installed: bool,
    /// 版本号首行；未安装或无法解析时为空
    pub version: String,
}

/// 检测工具安装状态：能否在 PATH 中找到并成功执行版本查询。
pub fn check_tool(tool: &ExtTool) -> ToolStatus {
    let (ok, version) = match run_version(tool.binary) {
        Some(v) => (true, v),
        None => (false, String::new()),
    };
    ToolStatus {
        id: tool.id.to_string(),
        binary: tool.binary.to_string(),
        installed: ok,
        version,
    }
}

/// 查询版本号：`d2 --version` 输出到 stdout，`dot -V` 输出到 stderr，
/// 统一取两者中首个非空行的首段。
fn run_version(binary: &str) -> Option<String> {
    let out = Command::new(binary)
        .arg(if binary == "dot" { "-V" } else { "--version" })
        .output()
        .ok()?;
    if !out.status.success() && out.stdout.is_empty() && out.stderr.is_empty() {
        return None;
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let line = text.lines().find(|l| !l.trim().is_empty())?;
    Some(line.trim().to_string())
}

/// HTML 转义还原（comrak 会转义代码内容，CLI 需要原始文本）。
fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// 从 HTML 中识别出的外部图表代码块（保留原始区间以便原位替换）。
struct ExtBlock {
    start: usize,
    end: usize,
    tool: &'static ExtTool,
    source: String,
}

/// 围栏代码块正则：`<pre><code class="language-xxx">...</code></pre>`，
/// class 必须是外部工具的语言别名（不做无语言嗅探——dot 语法太宽，
/// 误判风险高，显式写语言才触发）。
fn ext_block_regex() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        let mut langs = Vec::new();
        for t in EXT_TOOLS {
            langs.extend(t.fence_langs.iter().copied());
        }
        let pattern = format!(
            r#"(?s)<pre><code\s+class="language-({})"\s*>(?P<src>.*?)</code></pre>"#,
            langs.join("|")
        );
        regex::Regex::new(&pattern).unwrap()
    })
}

/// 扫描 HTML，找出所有外部图表代码块（按出现顺序）。
fn extract_ext_blocks(html: &str) -> Vec<ExtBlock> {
    let re = ext_block_regex();
    let mut blocks = Vec::new();
    for cap in re.captures_iter(html) {
        let whole = cap.get(0).unwrap();
        let lang = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let raw = cap.name("src").map(|m| m.as_str()).unwrap_or("");
        let Some(tool) = tool_for_lang(lang) else {
            continue;
        };
        blocks.push(ExtBlock {
            start: whole.start(),
            end: whole.end(),
            tool,
            source: unescape_html(raw),
        });
    }
    blocks
}

/// 渲染整段 HTML 中的 d2 / dot 代码块为内嵌 SVG。
///
/// - 无外部图表块 → 原样返回；
/// - CLI 未安装 / 渲染失败 → 原样返回（保留代码块）；
/// - 单个块失败只影响该块，其余正常替换。
pub fn render_ext_blocks(html: &str) -> String {
    let blocks = extract_ext_blocks(html);
    if blocks.is_empty() {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    let mut pos = 0usize;
    for blk in &blocks {
        out.push_str(&html[pos..blk.start]);
        let replacement = match render_svg(blk.tool, &blk.source) {
            // 与 mermaid 图一致的包裹结构，样式随 .mermaid-diagram 协同
            Some(svg) => format!("<div class=\"mermaid-diagram\">{}</div>", svg),
            None => html[blk.start..blk.end].to_string(),
        };
        out.push_str(&replacement);
        pos = blk.end;
    }
    out.push_str(&html[pos..]);
    out
}

/// 调用外部 CLI 把源码渲染为 SVG；任何一步失败返回 None。
///
/// - d2：`d2 input.d2 output.svg`
/// - dot：`dot -Tsvg input.dot -o output.svg`
pub fn render_svg(tool: &ExtTool, source: &str) -> Option<String> {
    let tmp = std::env::temp_dir().join("rust-mpe-ext-diagram");
    std::fs::create_dir_all(&tmp).ok()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let input = tmp.join(format!("{}-{}.in", tool.id, stamp));
    let output = tmp.join(format!("{}-{}.svg", tool.id, stamp));
    std::fs::write(&input, source).ok()?;

    let result = match tool.id {
        "d2" => Command::new(tool.binary)
            .arg(&input)
            .arg(&output)
            .output(),
        _ => Command::new(tool.binary)
            .args(["-Tsvg", &input.to_string_lossy(), "-o", &output.to_string_lossy()])
            .output(),
    };
    // 无论成败都清理输入；输出文件在读取后清理
    let _ = std::fs::remove_file(&input);
    let out = result.ok()?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&output);
        return None;
    }
    let svg = std::fs::read_to_string(&output).ok()?;
    let _ = std::fs::remove_file(&output);
    // 防御：CLI 成功退出也可能产出空文件或非 SVG 内容
    if !svg.contains("<svg") {
        return None;
    }
    Some(svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence语言映射到正确工具() {
        assert_eq!(tool_for_lang("d2").unwrap().id, "d2");
        assert_eq!(tool_for_lang("D2").unwrap().id, "d2");
        assert_eq!(tool_for_lang("dot").unwrap().id, "dot");
        assert_eq!(tool_for_lang("graphviz").unwrap().id, "dot");
        assert!(tool_for_lang("python").is_none());
        assert!(tool_for_lang("").is_none());
    }

    #[test]
    fn 工具id查询() {
        assert!(tool_by_id("d2").is_some());
        assert!(tool_by_id("dot").is_some());
        assert!(tool_by_id("mermaid").is_none());
    }

    #[test]
    fn 未安装的cli返回安装状态false() {
        // 测试环境不保证装了 d2/dot，仅验证不 panic、字段一致
        for t in EXT_TOOLS {
            let s = check_tool(t);
            assert_eq!(s.id, t.id);
            assert_eq!(s.binary, t.binary);
            if !s.installed {
                assert!(s.version.is_empty());
            }
        }
    }

    #[test]
    fn 无外部图表块时原样返回() {
        let html = "<pre><code class=\"language-rust\">fn main() {}</code></pre>";
        assert_eq!(render_ext_blocks(html), html);
    }

    #[test]
    fn cli缺失时保留代码块() {
        // 用不存在的二进制构造：直接测 render_svg 的失败路径
        let fake = ExtTool {
            id: "no-such-tool",
            binary: "no-such-binary-md2x-test",
            fence_langs: &["x"],
        };
        assert!(render_svg(&fake, "a -> b").is_none());
    }

    #[test]
    fn html扫描识别与替换降级() {
        // d2 块：本机大概率没装 d2 → 降级保留；但提取逻辑必须命中
        let html = "<p>前</p><pre><code class=\"language-d2\">x -&gt; y</code></pre><p>后</p>";
        let blocks = extract_ext_blocks(html);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].source, "x -> y"); // 已还原 HTML 转义
        assert_eq!(blocks[0].tool.id, "d2");
        let out = render_ext_blocks(html);
        assert!(out.contains("前"));
        // 命中的块要么被替换为 SVG，要么原样保留（降级），不能凭空消失
        assert!(out.contains("<code") || out.contains("mermaid-diagram"));
    }

    #[test]
    fn graphviz别名也被识别() {
        let html = "<pre><code class=\"language-graphviz\">digraph{a-&gt;b}</code></pre>";
        let blocks = extract_ext_blocks(html);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].tool.id, "dot");
    }

    #[test]
    fn 假cli脚本走通成功路径() {
        // 用 sh 假装 dot：读入参数里的输入文件，写出 -o 参数指向的 SVG。
        // 验证「CLI 成功 → 读 SVG → 返回」链路，不依赖真实 Graphviz。
        if !cfg!(unix) {
            return;
        }
        // 每个测试独立子目录：并行测试会共享 temp_dir，共用目录会被
        // 另一个测试的 remove_dir_all 删掉假脚本，造成竞态
        let dir = std::env::temp_dir().join("md2x-ext-test-bin-ok");
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake-dot-md2x");
        std::fs::write(
            &script,
            "#!/bin/sh\n# $1=-Tsvg $2=input $3=-o $4=output\nprintf '<svg xmlns=\"http://www.w3.org/2000/svg\"><text>ok</text></svg>' > \"$4\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        // binary 字段要求 'static，用 Box::leak（测试进程一次性，可接受）
        let bin: &'static str =
            Box::leak(script.to_string_lossy().to_string().into_boxed_str());
        let fake = ExtTool {
            id: "dot",
            binary: bin,
            fence_langs: &["dot"],
        };
        let svg = render_svg(&fake, "digraph { a -> b }").unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("ok"));

        let _ = std::fs::remove_file(&script);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cli失败退出时不产出svg() {
        if !cfg!(unix) {
            return;
        }
        let dir = std::env::temp_dir().join("md2x-ext-test-bin-fail");
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake-fail-md2x");
        std::fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let bin: &'static str =
            Box::leak(script.to_string_lossy().to_string().into_boxed_str());
        let fake = ExtTool {
            id: "d2",
            binary: bin,
            fence_langs: &["d2"],
        };
        assert!(render_svg(&fake, "bad").is_none());
        let _ = std::fs::remove_file(&script);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
