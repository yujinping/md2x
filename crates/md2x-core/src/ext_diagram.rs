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

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

/// 平台固定回退目录（按检索优先级排列）。
///
/// GUI 应用不继承 shell 的 PATH：macOS 从 Finder / Dock 启动的进程拿到的是
/// launchd 的最小 PATH（`/usr/bin:/bin:/usr/sbin:/sbin`），Homebrew、MacPorts、
/// 官方安装脚本写进 `~/.local/bin` 的可执行文件都不在其中。若只按裸名 spawn，
/// 就会出现「终端里 `d2 --version` 正常、应用内检测不到」的割裂。
/// 这里补上常见安装位置作为兜底，PATH 命中时优先，行为不变。
#[cfg(target_os = "macos")]
const FALLBACK_SYS_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/opt/local/bin",
    "/usr/bin",
    "/bin",
];

/// Linux 常见安装位置。
#[cfg(all(unix, not(target_os = "macos")))]
const FALLBACK_SYS_DIRS: &[&str] = &["/usr/local/bin", "/usr/bin", "/bin", "/snap/bin"];

/// Windows 无固定回退目录，改由环境变量推导（见 `windows_extra_dirs`）。
#[cfg(windows)]
const FALLBACK_SYS_DIRS: &[&str] = &[];

/// 用户目录下的相对安装位置（官方安装脚本、pipx / cargo 风格布局）。
#[cfg(not(windows))]
const FALLBACK_HOME_DIRS: &[&str] = &[".local/bin", "bin"];
/// Windows 不扫用户目录。
#[cfg(windows)]
const FALLBACK_HOME_DIRS: &[&str] = &[];

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
    /// 实际解析到的可执行文件绝对路径；未安装时为空。
    /// 设置面板用它说明「到底是哪个 d2 生效」，避免多版本共存时无从排查。
    pub path: String,
}

/// 判断是否为可执行文件（Unix 额外校验执行位，Windows 仅看存在性）。
fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(p) {
            Ok(m) => m.is_file() && m.permissions().mode() & 0o111 != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// 候选文件名：Windows 除裸名外再补 `.exe`（PATH 检索不经过 PATHEXT 展开）。
fn candidate_names(binary: &str) -> Vec<OsString> {
    #[cfg(windows)]
    {
        vec![
            OsString::from(binary),
            OsString::from(format!("{binary}.exe")),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![OsString::from(binary)]
    }
}

/// Windows 下由环境变量推导的额外目录（Graphviz / d2 安装器的默认落点）。
#[cfg(windows)]
fn windows_extra_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for key in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(key) {
            let base = PathBuf::from(base);
            out.push(base.join("Graphviz").join("bin"));
            out.push(base.join("d2").join("bin"));
            out.push(base.join("Programs").join("d2").join("bin"));
        }
    }
    out
}

#[cfg(not(windows))]
fn windows_extra_dirs() -> Vec<PathBuf> {
    Vec::new()
}

/// 组装候选目录：PATH 原序优先 → 用户目录回退 → 平台固定目录 → Windows 推导目录。
///
/// 拆成纯函数以便测试：入参为注入的 PATH / HOME，不触碰进程环境。
/// 重复目录按首次出现去重，保持检索顺序稳定。
fn candidate_dirs_from(path_var: Option<&OsStr>, home: Option<&OsStr>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !p.as_os_str().is_empty() && !out.contains(&p) {
            out.push(p);
        }
    };

    if let Some(paths) = path_var {
        for seg in std::env::split_paths(paths) {
            push(seg);
        }
    }
    if let Some(home) = home {
        let home = PathBuf::from(home);
        for rel in FALLBACK_HOME_DIRS {
            push(home.join(rel));
        }
    }
    for dir in FALLBACK_SYS_DIRS {
        push(PathBuf::from(dir));
    }
    for dir in windows_extra_dirs() {
        push(dir);
    }
    out
}

/// 用当前进程环境组装候选目录。
fn candidate_dirs() -> Vec<PathBuf> {
    let path_var = std::env::var_os("PATH");
    // Windows 无 HOME，退回 USERPROFILE
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    candidate_dirs_from(path_var.as_deref(), home.as_deref())
}

/// 在给定目录列表中检索可执行文件。
fn resolve_in_dirs(binary: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in dirs {
        for name in candidate_names(binary) {
            let p = dir.join(name);
            if is_executable(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// 解析可执行文件的真实路径。
///
/// - 含路径分隔符 → 视为显式路径，仅校验存在性与执行位；
/// - 裸名 → 按 PATH → 平台回退目录 顺序检索。
///
/// **不做缓存**：设置面板的「刷新」按钮需要在用户装完依赖后立即生效，
/// 任何负缓存都会让刷新变成无效操作。每次检索只做几次 stat，
/// 相对 spawn 一个 CLI 进程的开销可忽略。
pub fn resolve_binary(binary: &str) -> Option<PathBuf> {
    if binary.contains('/') || binary.contains('\\') {
        let p = PathBuf::from(binary);
        return is_executable(&p).then_some(p);
    }
    resolve_in_dirs(binary, &candidate_dirs())
}

/// 版本查询参数：Graphviz 的 dot 用 `-V`，其余用 `--version`（按工具 id 判定，
/// 不按文件名判定——解析后的路径已不含裸名信息）。
fn version_flag(tool: &ExtTool) -> &'static str {
    if tool.id == "dot" {
        "-V"
    } else {
        "--version"
    }
}

/// 检测工具安装状态：能解析到可执行文件且版本查询成功才算已安装。
pub fn check_tool(tool: &ExtTool) -> ToolStatus {
    let resolved = resolve_binary(tool.binary);
    let version = resolved
        .as_deref()
        .and_then(|bin| run_version(bin, tool))
        .unwrap_or_default();
    ToolStatus {
        id: tool.id.to_string(),
        binary: tool.binary.to_string(),
        installed: !version.is_empty(),
        version,
        path: resolved
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    }
}

/// 查询版本号：`d2 --version` 输出到 stdout，`dot -V` 输出到 stderr，
/// 统一取两者中首个非空行的首段。
fn run_version(bin: &Path, tool: &ExtTool) -> Option<String> {
    let out = Command::new(bin).arg(version_flag(tool)).output().ok()?;
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
    // 与检测走同一套解析：GUI 进程的 PATH 里通常没有 d2 / dot，
    // 裸名 spawn 会直接 ENOENT，必须用解析出的绝对路径。
    let bin = resolve_binary(tool.binary)?;
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
        "d2" => Command::new(&bin).arg(&input).arg(&output).output(),
        _ => Command::new(&bin)
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
        // 测试环境不保证装了 d2/dot，仅验证不 panic、字段自洽
        for t in EXT_TOOLS {
            let s = check_tool(t);
            assert_eq!(s.id, t.id);
            assert_eq!(s.binary, t.binary);
            // installed 由版本查询成功与否决定
            assert_eq!(s.installed, !s.version.is_empty());
            // 解析不到路径 ⇒ 必然未安装（反向不成立：解析到但执行失败时
            // path 有值、installed 为 false，这正是可诊断的信息）
            if s.path.is_empty() {
                assert!(!s.installed);
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

    /// 造一个可执行假 CLI，返回其所在目录（每个测试用独立目录，避免并行互删）。
    #[cfg(unix)]
    fn make_fake_cli(dir_name: &str, file_name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(dir_name);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(file_name);
        std::fs::write(&p, "#!/bin/sh\nprintf 'fake-v1.0.0\\n'\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        dir
    }

    /// 在指定目录下造可执行假 CLI（用于需要嵌套层级、如 `home/.local/bin` 的场景）。
    #[cfg(unix)]
    fn make_exec_in(dir: &Path, file_name: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join(file_name);
        std::fs::write(&p, "#!/bin/sh\nprintf 'fake-v1.0.0\\n'\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn 候选目录_path在前回退在后且去重() {
        let dirs = candidate_dirs_from(
            Some(OsStr::new("/aaa/bin:/bbb/bin:/aaa/bin")),
            Some(OsStr::new("/home/u")),
        );
        assert_eq!(dirs[0], PathBuf::from("/aaa/bin"));
        assert_eq!(dirs[1], PathBuf::from("/bbb/bin"));
        // 重复项只保留首次出现
        assert_eq!(
            dirs.iter()
                .filter(|d| d.as_path() == Path::new("/aaa/bin"))
                .count(),
            1
        );
        // 用户目录回退目录必须存在（这是 GUI 场景能命中 ~/.local/bin 的前提）
        #[cfg(not(windows))]
        assert!(dirs.contains(&PathBuf::from("/home/u/.local/bin")));
        // PATH 项全部排在回退项之前
        let last_path_idx = dirs
            .iter()
            .position(|d| d.as_path() == Path::new("/bbb/bin"))
            .unwrap();
        #[cfg(not(windows))]
        {
            let home_idx = dirs
                .iter()
                .position(|d| d.as_path() == Path::new("/home/u/.local/bin"))
                .unwrap();
            assert!(last_path_idx < home_idx);
        }
    }

    /// 回归：GUI 应用（macOS 从 Finder 启动）拿到的 PATH 只有
    /// /usr/bin:/bin:/usr/sbin:/sbin，装在家目录 ~/.local/bin 的 CLI
    /// 必须仍能检出——否则设置面板永远显示「未安装」，刷新也无用。
    #[test]
    fn gui最小path下仍能回退命中用户目录() {
        if !cfg!(unix) {
            return;
        }
        // 模拟真实布局：<home>/.local/bin/<cli>
        let home = std::env::temp_dir().join("md2x-ext-test-home");
        let bin_dir = home.join(".local/bin");
        make_exec_in(&bin_dir, "fake-d2-in-home");

        let dirs = candidate_dirs_from(
            Some(OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin")),
            Some(home.as_os_str()),
        );
        let found = resolve_in_dirs("fake-d2-in-home", &dirs);
        assert_eq!(found, Some(bin_dir.join("fake-d2-in-home")));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn path命中优先于回退目录() {
        if !cfg!(unix) {
            return;
        }
        let first = make_fake_cli("md2x-ext-test-prio-a", "prio-cli");
        let second = make_fake_cli("md2x-ext-test-prio-b", "prio-cli");
        let path_var = std::env::join_paths([&first, &second]).unwrap();
        let dirs = candidate_dirs_from(Some(&path_var), Some(OsStr::new("/nonexistent")));
        assert_eq!(resolve_in_dirs("prio-cli", &dirs), Some(first.join("prio-cli")));
        let _ = std::fs::remove_dir_all(&first);
        let _ = std::fs::remove_dir_all(&second);
    }

    #[test]
    fn 非可执行文件不算命中() {
        if !cfg!(unix) {
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join("md2x-ext-test-nonexec");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("not-exec-cli");
        std::fs::write(&p, "data").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(resolve_in_dirs("not-exec-cli", std::slice::from_ref(&dir)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 显式路径按存在性校验() {
        if !cfg!(unix) {
            return;
        }
        let dir = make_fake_cli("md2x-ext-test-abspath", "abs-cli");
        let abs = dir.join("abs-cli");
        assert_eq!(resolve_binary(abs.to_str().unwrap()), Some(abs));
        assert!(resolve_binary("/no/such/dir/abs-cli").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 本机实测用（`cargo test -- --ignored`）：验证真实 GUI 环境能解析到已装的 d2。
    /// 默认忽略，避免 CI / 未装 d2 的机器因环境差异失败。
    #[test]
    #[ignore = "依赖本机真实安装的 d2"]
    fn 本机gui最小path可解析真实d2() {
        let dirs = candidate_dirs_from(
            Some(OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin")),
            std::env::var_os("HOME").as_deref(),
        );
        let found = resolve_in_dirs("d2", &dirs).expect("应能在 GUI 最小 PATH 下解析到 d2");
        assert!(found.is_absolute());
        let out = Command::new(&found).arg("--version").output().unwrap();
        assert!(out.status.success());
    }
}
