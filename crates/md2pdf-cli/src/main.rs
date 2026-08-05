use clap::Parser;
use md2pdf_core::chrome;
use md2pdf_core::converter;
use md2pdf_core::error;
use md2pdf_core::template;
use std::path::Path;
use std::process;

#[derive(Parser)]
#[command(name = "md2pdf", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// Path to the markdown file
    file: String,

    /// Open the generated PDF with the default application
    #[arg(long)]
    preview: bool,
}

fn run() -> Result<(), error::MpeError> {
    let cli = Cli::parse();
    let path = Path::new(&cli.file);

    if !path.exists() {
        return Err(error::MpeError::FileNotFound(cli.file));
    }

    // 读取 Markdown
    let markdown = std::fs::read_to_string(path).map_err(error::MpeError::IoError)?;

    // 检测是否为 SKILL.md
    let is_skill = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n == "SKILL.md")
        .unwrap_or(false);
    let (metadata, body_md) = if is_skill {
        converter::parse_front_matter(&markdown)
    } else {
        (None, &markdown[..])
    };

    // 转换为 HTML
    let html_body = converter::convert_markdown_to_html(body_md)?;
    let html_body = converter::resolve_image_srcs(&html_body, path);

    // 生成标题
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");

    // 渲染完整 HTML
    let full_html = template::render_html_template_with_metadata(&html_body, title, metadata.as_ref());

    // 临时 HTML 文件（用于生成 PDF）
    let temp_dir = std::env::temp_dir().join("rust-mpe-browser");
    std::fs::create_dir_all(&temp_dir).map_err(error::MpeError::IoError)?;
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let html_path = temp_dir.join(format!("{}.html", file_stem));

    // 写出临时 HTML
    std::fs::write(&html_path, full_html).map_err(error::MpeError::IoError)?;

    // 生成 PDF
    let pdf_path = path.with_extension("pdf");
    let html_str = html_path.to_string_lossy();
    let pdf_str = pdf_path.to_string_lossy();
    chrome::generate_pdf(&html_str, &pdf_str)?;

    // 清理临时 HTML
    let _ = std::fs::remove_file(&html_path);

    // 用默认应用程序打开 PDF
    if cli.preview {
        chrome::open_pdf(&pdf_str)?;
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        process::exit(1);
    }
}
