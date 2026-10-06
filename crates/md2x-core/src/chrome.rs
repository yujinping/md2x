use crate::error::MpeError;
use std::path::Path;
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
    // 也查 PATH（用户自行安装的情况）
    "chrome",
    "msedge",
];

#[cfg(target_os = "linux")]
const CHROME_CANDIDATES: &[&str] = &[
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/usr/bin/microsoft-edge",
    "/usr/bin/microsoft-edge-stable",
];

pub(crate) fn find_chrome() -> Result<String, MpeError> {
    for candidate in CHROME_CANDIDATES {
        if Path::new(candidate).exists() {
            return Ok(candidate.to_string());
        }
        // Windows 上也尝试直接当命令执行（如果候选不含路径分隔符）
        if cfg!(target_os = "windows") && !candidate.contains('\\') {
            if Command::new("where")
                .arg(candidate)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return Ok(candidate.to_string());
            }
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

    let output = Command::new(&chrome)
        .args([
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-dev-shm-usage",
            "--no-pdf-header-footer",
            "--print-to-pdf-background",
            &format!("--print-to-pdf={}", pdf_path),
            &format!("file://{}", abs_html.display()),
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
pub(crate) fn to_file_url(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    let mut out = String::from("file://");
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
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
            &format!("file://{}", abs_html.display()),
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
    let status = Command::new("cmd")
        .args(["/c", "start", "", abs_pdf.to_str().unwrap_or("")])
        .output()
        .map_err(MpeError::IoError)?;

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
