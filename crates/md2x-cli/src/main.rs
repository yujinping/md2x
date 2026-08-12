use clap::{Parser, ValueEnum};
use md2x_core::chrome;
use md2x_core::converter;
use md2x_core::error;
use md2x_core::template;
use std::path::Path;
use std::process;

#[derive(Parser)]
#[command(name = "md2x", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// Path to the markdown file
    file: String,

    /// Output format: pdf, html, png or docx
    #[arg(long, value_enum, default_value_t = OutputFormat::Pdf)]
    format: OutputFormat,

    /// Open the generated PDF with the default application (only for pdf)
    #[arg(long)]
    preview: bool,

    /// Render at full width: content spans ~98% of the screen instead of a fixed 860px column
    #[arg(long)]
    full_width: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Pdf,
    Html,
    Png,
    Docx,
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

    // 转换为 HTML（含 mermaid 图表一次性烘焙为内嵌 SVG）
    let html_body = converter::convert_markdown_to_html_with_mermaid(body_md)?;
    let html_body = converter::resolve_image_srcs(&html_body, path);

    // 生成标题
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");

    // 渲染完整 HTML
    let full_html = template::render_html_template_with_metadata(&html_body, title, metadata.as_ref(), cli.full_width);

    // 按输出格式分发：HTML 直接写出，PDF / PNG 先渲染到临时 HTML 再交给 Chrome
    match cli.format {
        OutputFormat::Html => {
            let html_path = path.with_extension("html");
            std::fs::write(&html_path, full_html).map_err(error::MpeError::IoError)?;
        }
        OutputFormat::Docx => {
            let docx_path = path.with_extension("docx");
            md2x_core::docx::convert_markdown_to_docx(body_md, path, &docx_path)?;
        }
        OutputFormat::Pdf | OutputFormat::Png => {
            let temp_dir = std::env::temp_dir().join("rust-mpe-browser");
            std::fs::create_dir_all(&temp_dir).map_err(error::MpeError::IoError)?;
            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            let html_path = temp_dir.join(format!("{}.html", file_stem));

            // 写出临时 HTML
            std::fs::write(&html_path, full_html).map_err(error::MpeError::IoError)?;

            if let OutputFormat::Pdf = cli.format {
                // 生成 PDF
                let pdf_path = path.with_extension("pdf");
                let html_str = html_path.to_string_lossy();
                let pdf_str = pdf_path.to_string_lossy();
                chrome::generate_pdf(&html_str, &pdf_str)?;

                // 用默认应用程序打开 PDF
                if cli.preview {
                    chrome::open_pdf(&pdf_str)?;
                }
            } else {
                // 生成 PNG 截图
                let png_path = path.with_extension("png");
                let html_str = html_path.to_string_lossy();
                let png_str = png_path.to_string_lossy();
                chrome::generate_png(&html_str, &png_str)?;
            }

            // 清理临时 HTML
            let _ = std::fs::remove_file(&html_path);
        }
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        process::exit(1);
    }
}
