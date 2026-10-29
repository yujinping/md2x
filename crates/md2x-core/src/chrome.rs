use crate::error::MpeError;
use std::process::Command;

// ── Chrome / Chromium / Edge 候选路径 ──────────────────

#[cfg(target_os = "macos")]
const CHROME_CANDIDATES: &[&str] = &[
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
];

#[cfg(target_os = "windows")]
const CHROME_CANDIDATES: &[&str] = &[
    "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
    "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
    "C:\\Program Files\\Chromium\\Application\\chrome.exe",
    "C:\\Program Files (x86)\\Chromium\\Application\\chrome.exe",
    "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
    "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
];

#[cfg(target_os = "linux")]
const CHROME_CANDIDATES: &[&str] = &[
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/usr/bin/microsoft-edge",
    "/usr/bin/microsoft-edge-stable",
    "/snap/bin/chromium",
];

/// PATH 检索用的裸名（用户自行安装 / 包管理器安装的情况）。
///
/// 走的仍是 `ext_diagram::resolve_binary`：macOS 从 Finder 启动的 GUI
/// 进程继承 launchd 最小 PATH，只靠裸名 spawn 一律找不到。
#[cfg(target_os = "macos")]
const CHROME_BIN_NAMES: &[&str] = &["google-chrome", "chromium", "chrome", "msedge"];

#[cfg(target_os = "windows")]
const CHROME_BIN_NAMES: &[&str] = &["chrome", "msedge"];

#[cfg(target_os = "linux")]
const CHROME_BIN_NAMES: &[&str] = &[
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "microsoft-edge",
];

/// 用户级安装位置（安装器默认写当前用户目录，不在 Program Files / /Applications）。
///
/// - macOS：`~/Applications/...app/Contents/MacOS/...`
/// - Windows：`%LOCALAPPDATA%\...\chrome.exe`（Chrome 默认「仅为我安装」走这里）
fn user_level_candidates() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();

    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        let apps = std::path::PathBuf::from(home).join("Applications");
        for rel in [
            "Google Chrome.app/Contents/MacOS/Google Chrome",
            "Chromium.app/Contents/MacOS/Chromium",
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ] {
            out.push(apps.join(rel));
        }
    }

    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let base = std::path::PathBuf::from(local);
        out.push(base.join("Google").join("Chrome").join("Application").join("chrome.exe"));
        out.push(base.join("Chromium").join("Application").join("chrome.exe"));
        out.push(base.join("Microsoft").join("Edge").join("Application").join("msedge.exe"));
    }

    out
}

pub(crate) fn find_chrome() -> Result<String, MpeError> {
    // 1) 固定安装位置（系统级 + 用户级）
    for candidate in CHROME_CANDIDATES
        .iter()
        .map(std::path::PathBuf::from)
        .chain(user_level_candidates())
    {
        if candidate.is_file() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    // 2) 裸名解析（含 PATH 与平台回退目录），返回绝对路径
    for name in CHROME_BIN_NAMES {
        if let Some(p) = crate::ext_diagram::resolve_binary(name) {
            return Ok(p.to_string_lossy().to_string());
        }
    }
    Err(MpeError::ChromeNotFound)
}

/// 生成 PDF（无头 Chrome / Edge）
///
/// 优先走 CDP：`Page.printToPDF` 传 `generateDocumentOutline: true` 会生成
/// **标准 PDF 书签树（/Outlines）**，即阅读器侧边栏里可点击跳转的目录。
/// 命令行的 `--print-to-pdf` 不具备该能力，只能拿到「无书签」的 PDF。
///
/// 因此这里以 CDP 为主路径；CDP 不可用时（端口占用、Chrome 版本过旧等）
/// 降级到命令行方式，保证「至少能导出」，只是没有书签。
pub fn generate_pdf(html_path: &str, pdf_path: &str) -> Result<(), MpeError> {
    match generate_pdf_via_cdp(html_path, pdf_path, true) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("[CDP-FAIL] {}", e);
            // 书签拿不到不算失败，降级生成无书签 PDF
            generate_pdf_via_cli(html_path, pdf_path)
        }
    }
}

/// 通过命令行生成 PDF（无书签）。CDP 失败时的降级路径。
fn generate_pdf_via_cli(html_path: &str, pdf_path: &str) -> Result<(), MpeError> {
    let chrome = find_chrome()?;
    let abs_html = std::fs::canonicalize(html_path).map_err(MpeError::IoError)?;
    let file_url = to_file_url(&abs_html);

    let output = Command::new(&chrome)
        .args([
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-dev-shm-usage",
            "--no-pdf-header-footer",
            "--print-to-pdf-background",
            &format!("--print-to-pdf={}", pdf_path),
            &file_url,
        ])
        .output()
        .map_err(MpeError::IoError)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MpeError::PdfGenerationFailed(stderr.to_string()));
    }
    Ok(())
}

/// 通过 CDP 生成 PDF，可选生成书签树。
fn generate_pdf_via_cdp(
    html_path: &str,
    pdf_path: &str,
    outline: bool,
) -> Result<(), MpeError> {
    use crate::cdp;
    use std::process::{Child, Stdio};

    let chrome = find_chrome()?;
    let abs_html = std::fs::canonicalize(html_path).map_err(MpeError::IoError)?;
    // URL 必须做百分号编码：路径含中文/空格时，裸UTF-8 会被 Chrome 判为非法
    // URL 导致新建标签页直接失败（表现为连接被立刻关闭）。
    let file_url = to_file_url(&abs_html);
    let port = cdp::pick_free_port(9300, 60)
        .ok_or_else(|| MpeError::PdfGenerationFailed("没有可用的调试端口".into()))?;
    let user_data_dir = std::env::temp_dir().join(format!("md2x-cdp-{port}"));
    let _ = std::fs::remove_dir_all(&user_data_dir);

    // 启动时直接打开目标文件，随后从目标列表取页连接。
    // 不走 /json/new：新建的标签页可能在握手完成前被 Chrome 回收。
    let mut child: Child = Command::new(&chrome)
        .args([
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-dev-shm-usage",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            // 关掉这些可避免加载无关资源拖慢打印
            "--disable-background-networking",
            "--disable-sync",
            // 固定视口宽度，避免打印布局随窗口尺寸漂移
            "--window-size=1280,1600",
            &format!("--remote-debugging-port={port}"),
            &format!("--user-data-dir={}", user_data_dir.display()),
            &file_url,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(MpeError::IoError)?;

    // 无论成功失败都要清理：kill 子进程 + 删临时 profile
    // file_url 已作为 Chrome 启动参数传入，此处无需再用
    let _ = &file_url;
    let result = run_cdp_print(&mut child, port, pdf_path, outline);
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&user_data_dir);
    result
}

/// 把本地路径编码为 `file://` URL。
///
/// 只对路径部分做百分号编码，保留分隔符；中文与空格必须转义，
/// 否则 Chrome 会拒绝该 URL（新建标签页会立刻关闭）。
/// 把本地路径转换为 file URL（跨平台正确性由此保证）。
///
/// 三个平台的关键差异：
/// - **分隔符**：file URL 只认 `/`，Windows 的 `\` 必须转换（否则被百分号编码成 `%5C`）；
/// - **盘符**：`C:\a\b.png` 需要三个斜杠 `file:///C:/a/b.png`，
///   直接拼 `file://C:...` 会把 `C:` 当成主机名；
/// - **UNC 共享**：`\\server\share` 应得到 `file://server/share`（两个斜杠）。
///
/// 非 ASCII（中文路径、空格）一律按 UTF-8 字节做百分号编码——
/// 裸 UTF-8 会被 Chrome 判为非法 URL，表现为新建标签页立刻断开。
pub(crate) fn to_file_url(path: &std::path::Path) -> String {
    path_to_file_url(&path.to_string_lossy(), cfg!(windows))
}

/// 去掉 Windows `canonicalize` 产生的 verbatim 前缀。
///
/// `std::fs::canonicalize` 在 Windows 上返回 `\\?\C:\a\b` 形式；这个前缀
/// 会让 file URL 变成 `file://?/C:/a/b`（把 `?` 当主机名），部分 Shell API
/// 也不接受。UNC 形式 `\\?\UNC\server\share` 需还原为 `\\server\share`。
///
/// 不做 cfg 限定：以便在任意平台单测真实字符串形态。
fn strip_verbatim_prefix(raw: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        Cow::Owned(format!(r"\\{rest}"))
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        Cow::Borrowed(rest)
    } else {
        Cow::Borrowed(raw)
    }
}

/// 路径字符串 → file URL。`windows` 显式传入以便在任意平台单测 Windows 分支。
fn path_to_file_url(raw: &str, windows: bool) -> String {
    let normalized = if windows {
        strip_verbatim_prefix(raw).replace('\\', "/")
    } else {
        raw.to_string()
    };

    let mut out = String::from("file:");
    if normalized.starts_with("//") {
        // UNC：\\server\share\x → file://server/share/x（不要再补斜杠）
        out.push_str(&encode_url_path(&normalized));
    } else if normalized.starts_with('/') {
        // Unix 绝对路径：/a/b → file:///a/b（"file:" + "//" + "/a/b"）
        out.push_str("//");
        out.push_str(&encode_url_path(&normalized));
    } else {
        // Windows 盘符路径：C:/a/b → file:///C:/a/b（需补第三个斜杠）
        out.push_str("///");
        out.push_str(&encode_url_path(&normalized));
    }
    out
}

/// 保留 URL 路径中的安全字符，其余按字节做百分号编码。
/// `:` 保留——盘符与 URI 端口都依赖它。
fn encode_url_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn run_cdp_print(
    child: &mut std::process::Child,
    port: u16,
    pdf_path: &str,
    outline: bool,
) -> Result<(), MpeError> {
    use crate::cdp;
    use std::time::{Duration, Instant};

    // 等待调试端口就绪，同时确认 Chrome 没有提前退出
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if cdp::wait_for_devtools(port, Duration::from_millis(200)) {
            break;
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(MpeError::PdfGenerationFailed(format!(
                "Chrome 启动后立即退出: {status}"
            )));
        }
        if Instant::now() > deadline {
            return Err(MpeError::PdfGenerationFailed(
                "等待 Chrome 调试端口超时".into(),
            ));
        }
    }

    // 端口就绪 ≠ 页面就绪：过早连接会拿到仍在初始化的目标，
    // 表现为握手成功后立刻收到 FIN。留出时间让渲染进程就位。
    std::thread::sleep(Duration::from_millis(1500));
    let ws_url = cdp::find_page_ws_url(port, Duration::from_secs(8))
        .ok_or_else(|| MpeError::PdfGenerationFailed("未找到可用的调试目标".into()))?;
    let mut session = cdp::CdpSession::connect(&ws_url)?;

    // 启动参数已把目标文件作为首个页面打开，不再重复导航：
    // Page.navigate 会重建执行上下文，把已建好的 socket 变成 broken pipe。
    session.wait_until_ready()?;
    let pdf = session.print_to_pdf(outline)?;
    std::fs::write(pdf_path, pdf).map_err(MpeError::IoError)?;
    Ok(())
}

/// 生成 PNG 截图（无头 Chrome / Edge）
pub fn generate_png(html_path: &str, png_path: &str) -> Result<(), MpeError> {
    let chrome = find_chrome()?;
    let abs_html = std::fs::canonicalize(html_path).map_err(MpeError::IoError)?;
    let file_url = to_file_url(&abs_html);

    let output = Command::new(&chrome)
        .args([
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-dev-shm-usage",
            "--hide-scrollbars",
            "--force-device-scale-factor=2",
            "--window-size=1920,1080",
            &format!("--screenshot={}", png_path),
            &file_url,
        ])
        .output()
        .map_err(MpeError::IoError)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MpeError::ScreenshotGenerationFailed(stderr.to_string()));
    }
    Ok(())
}

/// 用系统默认应用程序打开 PDF
/// - macOS: `open`
/// - Windows: `cmd /c start`
/// - Linux: `xdg-open`
pub fn open_pdf(pdf_path: &str) -> Result<(), MpeError> {
    if std::env::var("MPE_NO_OPEN").is_ok() {
        return Ok(());
    }

    let abs_pdf = std::fs::canonicalize(pdf_path).map_err(MpeError::IoError)?;

    #[cfg(target_os = "macos")]
    let status = Command::new("open")
        .arg(abs_pdf.as_os_str())
        .output()
        .map_err(MpeError::IoError)?;

    #[cfg(target_os = "windows")]
    let status = {
        // canonicalize 的 \\?\ 前缀会让 start 解析失败；空串是 start 的标题占位，
        // 否则含空格的路径会被当成窗口标题。
        let target = strip_verbatim_prefix(&abs_pdf.to_string_lossy()).to_string();
        Command::new("cmd")
            .args(["/c", "start", ""])
            .arg(target)
            .output()
            .map_err(MpeError::IoError)?
    };

    #[cfg(target_os = "linux")]
    let status = Command::new("xdg-open")
        .arg(abs_pdf.as_os_str())
        .output()
        .map_err(MpeError::IoError)?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(MpeError::IoError(std::io::Error::other(format!(
            "Failed to open PDF: {stderr}"
        ))));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_unix绝对路径() {
        assert_eq!(
            path_to_file_url("/a/b-c_d.e~f/g.html", false),
            "file:///a/b-c_d.e~f/g.html"
        );
    }

    #[test]
    fn file_url_中文与空格按字节编码() {
        assert_eq!(
            path_to_file_url("/tmp/01-架构 图.html", false),
            "file:///tmp/01-%E6%9E%B6%E6%9E%84%20%E5%9B%BE.html"
        );
    }

    /// Windows 盘符路径：反斜杠必须转正斜杠，且需要第三个斜杠，
    /// 否则 `file://C:...` 会把 `C:` 解析成主机名，Chrome 直接拒绝加载。
    #[test]
    fn file_url_windows盘符路径() {
        assert_eq!(
            path_to_file_url(r"C:\Users\张三\文档\a b.png", true),
            "file:///C:/Users/%E5%BC%A0%E4%B8%89/%E6%96%87%E6%A1%A3/a%20b.png"
        );
    }

    #[test]
    fn file_url_windows正斜杠路径同样可接受() {
        assert_eq!(
            path_to_file_url("C:/tmp/a.html", true),
            "file:///C:/tmp/a.html"
        );
    }

    /// UNC 共享：两个斜杠 + 主机名，不能补成三个斜杠
    #[test]
    fn file_url_windows_unc() {
        assert_eq!(
            path_to_file_url(r"\\server\share\a.html", true),
            "file://server/share/a.html"
        );
    }

    /// canonicalize 的 verbatim 前缀必须剥掉，否则盘符路径会被当成主机名
    #[test]
    fn file_url_剥离verbatim前缀() {
        assert_eq!(
            path_to_file_url(r"\\?\C:\tmp\a.html", true),
            "file:///C:/tmp/a.html"
        );
        assert_eq!(
            path_to_file_url(r"\\?\UNC\server\share\a.html", true),
            "file://server/share/a.html"
        );
        // 非 verbatim 路径不受影响
        assert_eq!(
            path_to_file_url(r"C:\tmp\a.html", true),
            "file:///C:/tmp/a.html"
        );
    }

    #[test]
    fn 候选路径均不依赖裸名spawn() {
        // 回归：旧实现把 "chrome" / "msedge" 混在绝对路径候选里，
        // 靠 `where` 判定，非 Windows 平台直接失效。
        for c in CHROME_CANDIDATES {
            assert!(!c.is_empty());
        }
        for n in CHROME_BIN_NAMES {
            assert!(!n.contains('/'), "裸名列表不应含路径分隔符: {n}");
            assert!(!n.contains('\\'), "裸名列表不应含路径分隔符: {n}");
        }
    }
}
