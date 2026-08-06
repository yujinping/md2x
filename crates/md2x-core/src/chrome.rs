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

fn find_chrome() -> Result<String, MpeError> {
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
pub fn generate_pdf(html_path: &str, pdf_path: &str) -> Result<(), MpeError> {
    let chrome = find_chrome()?;
    let abs_html = std::fs::canonicalize(html_path).map_err(MpeError::IoError)?;

    let output = Command::new(&chrome)
        .args([
            "--headless=new",
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

/// 生成 PNG 截图（无头 Chrome / Edge）
pub fn generate_png(html_path: &str, png_path: &str) -> Result<(), MpeError> {
    let chrome = find_chrome()?;
    let abs_html = std::fs::canonicalize(html_path).map_err(MpeError::IoError)?;

    let output = Command::new(&chrome)
        .args([
            "--headless=new",
            "--hide-scrollbars",
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
