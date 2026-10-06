use clap::{Parser, ValueEnum};
use md2x_core::chrome;
use md2x_core::converter;
use md2x_core::error;
use md2x_core::template;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Parser)]
#[command(name = "md2x", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// Path to the markdown file, or a folder to aggregate into a single HTML
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

    /// Output path for folder aggregation (defaults to <folder>/<folder-name>.html)
    #[arg(long)]
    output: Option<String>,

    /// Theme baked into the exported HTML: auto (follow system), light or dark.
    /// Baking it in keeps the file looking the same on any machine.
    #[arg(long, default_value = "auto")]
    theme: String,
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
    let theme = template::Theme::parse(&cli.theme);

    if !path.exists() {
        return Err(error::MpeError::FileNotFound(cli.file));
    }

    // 传入目录时走聚合导出：把整个目录树的 Markdown 合并为单个文件
    if path.is_dir() {
        // 目录名作为各格式默认文件名
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("document")
            .to_string();
        let ext = match cli.format {
            OutputFormat::Pdf => "pdf",
            OutputFormat::Docx => "docx",
            _ => "html",
        };
        let dst = match &cli.output {
            Some(o) => PathBuf::from(o),
            None => path.join(format!("{name}.{ext}")),
        };
        let result =
            md2x_core::aggregate::aggregate_folder_to_html(path, cli.full_width, theme)?;
        //按 --format 分发：PDF 走 CDP 以生成书签树，DOCX 先拼 Markdown 再转换
        match cli.format {
            OutputFormat::Pdf => {
                let tmp = std::env::temp_dir().join("md2x-cli-agg.html");
                std::fs::write(&tmp, &result.html).map_err(error::MpeError::IoError)?;
                let res = md2x_core::chrome::generate_pdf(
                    &tmp.to_string_lossy(),
                    &dst.to_string_lossy(),
                );
                let _ = std::fs::remove_file(&tmp);
                res?;
                eprintln!("已聚合 {} 篇文档 -> {}", result.doc_count, dst.display());
            }
            OutputFormat::Docx => {
                let md = md2x_core::aggregate::concat_docs(path)?;
                let tmp = std::env::temp_dir().join("md2x-cli-agg.md");
                std::fs::write(&tmp, &md).map_err(error::MpeError::IoError)?;
                let res = md2x_core::docx::convert_markdown_to_docx(&md, &tmp, &dst);
                let _ = std::fs::remove_file(&tmp);
                res?;
                eprintln!("已聚合 {} 篇文档 -> {}", result.doc_count, dst.display());
            }
            _ => {
                std::fs::write(&dst, &result.html).map_err(error::MpeError::IoError)?;
                eprintln!("已聚合 {} 篇文档 -> {}", result.doc_count, dst.display());
            }
        }
        return Ok(());
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
    let full_html = template::render_html_template_with_metadata(
        &html_body,
        title,
        metadata.as_ref(),
        cli.full_width,
        theme,
    );

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
