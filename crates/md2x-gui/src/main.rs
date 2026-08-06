// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // 检查是否有 --pdf 标志（CLI 模式）
    if let Some(pos) = args.iter().position(|a| a == "--pdf") {
        // 提取文件路径（--pdf 后的下一个参数，或最后一个参数）
        let file = if pos + 1 < args.len() {
            &args[pos + 1]
        } else {
            eprintln!("Usage: md2x --pdf <file.md>");
            std::process::exit(1);
        };
        cli_run(file);
    } else if args.len() > 1 {
        // 有文件参数但无 --pdf → GUI 模式，通过环境变量传递路径
        let file = &args[1];
        std::env::set_var("MD2X_GUI_FILE", file);
        gui_run();
    } else {
        // 无参数 → 启动 GUI
        gui_run();
    }
}

fn cli_run(file: &str) {
    let path = std::path::Path::new(file);
    if !path.exists() {
        eprintln!("File not found: {file}");
        std::process::exit(1);
    }

    match md2x_gui_lib::generate_pdf_from_file(path) {
        Ok(pdf_path) => {
            println!("PDF generated: {}", pdf_path.display());
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
}

fn gui_run() {
    md2x_gui_lib::run()
}
