use md2x_core::chrome;
use md2x_core::converter;
use md2x_core::error::MpeError;
use md2x_core::template;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;
use tauri::{Manager, State};
use tauri::menu::MenuItem;
use tokio::sync::oneshot;

struct CachedPreview {
    /// 生成时的文件 mtime
    file_mtime: SystemTime,
    /// 缓存的 PDF 文件路径
    pdf_path: PathBuf,
    /// 缓存的 HTML 文件路径
    html_path: PathBuf,
}

struct AppState {
    current_file: Mutex<Option<PathBuf>>,
    /// 文件上次刷新时的 mtime，用于前端轮询检测变更
    last_known_mtime: Mutex<Option<SystemTime>>,
    /// 菜单「关于」点击标记，前端轮询消费
    show_about_flag: Mutex<bool>,
    /// 菜单「设置」点击标记，前端轮询消费
    settings_flag: Mutex<bool>,
    about_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    settings_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    quit_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    /// 缓存的预览结果（PDF + HTML），文件没变时复用
    cached_preview: Mutex<Option<CachedPreview>>,
}

/// 拖拽事件通道：前端 async wait_for_drop 通过 oneshot 接收文件路径
struct DropChannel {
    tx: Mutex<Option<oneshot::Sender<String>>>,
}

#[derive(Serialize)]
struct PreviewResult {
    base64: String,
    temp_path: String,
    file_name: String,
}

pub fn run() {
    let f = std::env::var("MD2X_GUI_FILE").ok();
    let s = AppState {
        current_file: Mutex::new(f.as_ref().map(|s| PathBuf::from(s))),
        last_known_mtime: Mutex::new(None),
        show_about_flag: Mutex::new(false),
        settings_flag: Mutex::new(false),
        about_item: Mutex::new(None),
        settings_item: Mutex::new(None),
        quit_item: Mutex::new(None),
        cached_preview: Mutex::new(None),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(s)
        .manage(DropChannel {
            tx: Mutex::new(None),
        })
        .on_window_event(|w, e| {
            if let tauri::WindowEvent::DragDrop(evt) = e {
                if let tauri::DragDropEvent::Drop { paths, .. } = evt {
                    if let Some(p) = paths.first() {
                        let path = p.to_string_lossy().to_string();
                        // 通知前端等待的 wait_for_drop
                        if let Some(state) = w.try_state::<DropChannel>() {
                            if let Ok(mut guard) = state.tx.lock() {
                                if let Some(tx) = guard.take() {
                                    let _ = tx.send(path);
                                }
                            }
                        }
                    }
                }
            }
        })
        .setup(|app| {
            let about_item: MenuItem<tauri::Wry> = tauri::menu::MenuItemBuilder::with_id("about", "关于")
                .build(app)?;
            let settings_item: MenuItem<tauri::Wry> = tauri::menu::MenuItemBuilder::with_id("settings", "设置…")
                .build(app)?;
            let quit_item: MenuItem<tauri::Wry> = tauri::menu::MenuItemBuilder::with_id("quit", "退出")
                .accelerator("CmdOrCtrl+Q")
                .build(app)?;
            let menu = tauri::menu::MenuBuilder::new(app)
                .item(
                    &tauri::menu::SubmenuBuilder::new(app, "md2x")
                        .item(&about_item)
                        .separator()
                        .item(&settings_item)
                        .separator()
                        .item(&quit_item)
                        .build()?,
                )
                .build()?;
            app.set_menu(menu)?;
            // 保存菜单项引用供语言切换
            if let Some(state) = app.try_state::<AppState>() {
                *state.about_item.lock().unwrap() = Some(about_item);
                *state.settings_item.lock().unwrap() = Some(settings_item);
                *state.quit_item.lock().unwrap() = Some(quit_item);
            }
            Ok(())
        })
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "about" => {
                    if let Some(state) = app.try_state::<AppState>() {
                        if let Ok(mut flag) = state.show_about_flag.lock() {
                            *flag = true;
                        }
                    }
                }
                "settings" => {
                    if let Some(state) = app.try_state::<AppState>() {
                        if let Ok(mut flag) = state.settings_flag.lock() {
                            *flag = true;
                        }
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            set_file,
            get_html,
            read_file_chunk,
            preview_pdf,
            save_pdf_as,
            export_html,
            export_pdf,
            export_docx,
            get_file_name,
            wait_for_drop,
            check_file_changed,
            get_platform,
            get_app_info,
            check_show_about,
            check_show_settings,
            set_menu_language,
        ])
        .build(tauri::generate_context!())
        .expect("error")
        .run(|handle, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = event {
                for url in urls {
                    if url.scheme() == "file" {
                        if let Ok(path) = url.to_file_path() {
                            let path_str = path.to_string_lossy().to_string();
                            if let Some(state) = handle.try_state::<AppState>() {
                                if let Ok(mut cur) = state.current_file.lock() {
                                    *cur = Some(path.clone());
                                }
                            }
                            if let Some(state) = handle.try_state::<DropChannel>() {
                                if let Ok(mut guard) = state.tx.lock() {
                                    if let Some(tx) = guard.take() {
                                        let _ = tx.send(path_str);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (handle, event);
        });
}

/// CLI 模式：静默生成 PDF
pub fn generate_pdf_from_file(path: &Path) -> Result<PathBuf, MpeError> {
    let md = std::fs::read_to_string(path)?;
    let is_skill = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n == "SKILL.md")
        .unwrap_or(false);
    let (metadata, body_md) = if is_skill {
        converter::parse_front_matter(&md)
    } else {
        (None, &md[..])
    };
    let h = converter::convert_markdown_to_html(body_md)?;
    let h = converter::resolve_image_srcs(&h, path);
    let t = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");
    let f = template::render_html_template_with_metadata(&h, t, metadata.as_ref());

    let d = std::env::temp_dir().join("rust-mpe-browser");
    std::fs::create_dir_all(&d)?;
    let n = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let hp = d.join(format!("{}.html", n));
    std::fs::write(&hp, &f)?;

    let pp = path.with_extension("pdf");
    chrome::generate_pdf(&hp.to_string_lossy(), &pp.to_string_lossy())?;
    let _ = std::fs::remove_file(&hp);
    Ok(pp)
}

// ==================== IPC 命令 ====================

/// 等待用户拖拽文件（异步，不轮询）
#[tauri::command]
async fn wait_for_drop(state: State<'_, DropChannel>) -> Result<String, String> {
    let (tx, rx) = oneshot::channel();
    {
        let mut guard = state.tx.lock().map_err(|e| e.to_string())?;
        *guard = Some(tx);
    }
    rx.await.map_err(|_| "操作已取消".to_string())
}

/// 读取文件分块（用于大文件流式加载）
#[tauri::command]
fn read_file_chunk(path: String, offset: usize) -> Result<(String, bool), String> {
    let data = std::fs::read(&path).map_err(|e| e.to_string())?;
    let end = std::cmp::min(offset + 524288, data.len());
    Ok((
        String::from_utf8_lossy(&data[offset..end]).to_string(),
        end >= data.len(),
    ))
}

/// 设置当前文件，记录初始 mtime 供前端轮询检测变更
#[tauri::command]
fn set_file(path: String, s: State<AppState>) -> Result<(), String> {
    let p = if path.starts_with("file://") {
        PathBuf::from(path.strip_prefix("file://").unwrap_or(&path))
    } else {
        PathBuf::from(&path)
    };
    if !p.exists() {
        return Err(format!("文件不存在: {}", p.display()));
    }
    s.current_file
        .lock()
        .map_err(|e| e.to_string())?
        .replace(p.clone());

    // 记录当前 mtime
    let mtime = std::fs::metadata(&p).ok().and_then(|m| m.modified().ok());
    *s.last_known_mtime.lock().map_err(|e| e.to_string())? = mtime;

    // 清空预览缓存（文件已切换）
    *s.cached_preview.lock().map_err(|e| e.to_string())? = None;

    Ok(())
}

/// 获取当前运行平台（macos / windows / linux）
#[tauri::command]
fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

/// 获取应用信息（版本号、版权）
#[derive(Serialize)]
struct AppInfo {
    version: String,
    copyright: String,
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        copyright: "yujinping8620@qq.com".to_string(),
    }
}

/// 前端轮询：检查菜单「关于」是否被点击
#[tauri::command]
fn check_show_about(s: State<AppState>) -> bool {
    let mut guard = s.show_about_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 前端轮询：检查菜单「设置」是否被点击
#[tauri::command]
fn check_show_settings(s: State<AppState>) -> bool {
    let mut guard = s.settings_flag.lock().unwrap();
    let val = *guard;
    *guard = false;
    val
}

/// 切换菜单文本语言
#[tauri::command]
fn set_menu_language(lang: String, s: State<AppState>) -> Result<(), String> {
    let about = s.about_item.lock().map_err(|e| e.to_string())?;
    let settings = s.settings_item.lock().map_err(|e| e.to_string())?;
    let quit = s.quit_item.lock().map_err(|e| e.to_string())?;
    match lang.as_str() {
        "en" => {
            if let Some(ref item) = *about { item.set_text("About").ok(); }
            if let Some(ref item) = *settings { item.set_text("Settings…").ok(); }
            if let Some(ref item) = *quit { item.set_text("Quit").ok(); }
        }
        _ => {
            if let Some(ref item) = *about { item.set_text("关于").ok(); }
            if let Some(ref item) = *settings { item.set_text("设置…").ok(); }
            if let Some(ref item) = *quit { item.set_text("退出").ok(); }
        }
    }
    Ok(())
}

/// 前端轮询调用：检查当前文件 mtime 是否有变化
#[tauri::command]
fn check_file_changed(s: State<AppState>) -> Result<bool, String> {
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = match cur.as_ref() {
        Some(p) => p.clone(),
        None => return Ok(false),
    };
    drop(cur);

    let current_mtime = std::fs::metadata(&p)
        .ok()
        .and_then(|m| m.modified().ok());

    let mut stored = s.last_known_mtime.lock().map_err(|e| e.to_string())?;
    match (stored.as_ref(), current_mtime.as_ref()) {
        (Some(last), Some(cur)) if cur > last => {
            // 更新记录
            *stored = current_mtime;
            Ok(true)
        }
        (None, Some(_)) => {
            // 首次获取到 mtime
            *stored = current_mtime;
            Ok(false)
        }
        _ => Ok(false),
    }
}

/// 生成 HTML 预览（写入临时文件，返回路径）
/// 如果缓存命中且文件未变更，直接返回缓存的 HTML 路径
#[tauri::command]
fn get_html(s: State<AppState>) -> Result<String, String> {
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = cur.as_ref().ok_or_else(|| "No file".to_string())?.clone();
    let file_stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("preview")
        .to_string();
    drop(cur);

    // 获取当前文件 mtime
    let current_mtime = std::fs::metadata(&p)
        .ok()
        .and_then(|m| m.modified().ok());

    // 检查缓存：mtime 一致且 HTML 文件存在
    if let Some(ref mtime) = current_mtime {
        let cache = s.cached_preview.lock().map_err(|e| e.to_string())?;
        if let Some(ref cached) = *cache {
            if cached.file_mtime == *mtime && cached.html_path.exists() {
                return Ok(cached.html_path.to_string_lossy().to_string());
            }
        }
    }

    // 缓存未命中，重新生成 HTML
    let md = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let is_skill = p
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n == "SKILL.md")
        .unwrap_or(false);
    let (metadata, body_md) = if is_skill {
        converter::parse_front_matter(&md)
    } else {
        (None, &md[..])
    };
    let hb = converter::convert_markdown_to_html(body_md).map_err(|e| e.to_string())?;
    let hb = converter::resolve_image_srcs(&hb, &p);
    let t = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");
    let fh = template::render_html_template_with_metadata(&hb, t, metadata.as_ref());

    let d = std::env::temp_dir().join("rust-mpe-browser");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let hp = d.join(format!("{}.html", file_stem));
    std::fs::write(&hp, &fh).map_err(|e| e.to_string())?;

    // 更新缓存中的 HTML 路径（不覆盖 PDF 缓存）
    if let Some(mtime) = current_mtime {
        let mut cache = s.cached_preview.lock().map_err(|e| e.to_string())?;
        match cache.as_mut() {
            Some(existing) => {
                existing.html_path = hp.clone();
                existing.pdf_path = PathBuf::new(); // 清除 PDF 缓存，强制重新生成
                existing.file_mtime = mtime;
            }
            None => {
                *cache = Some(CachedPreview {
                    file_mtime: mtime,
                    pdf_path: PathBuf::new(), // 尚无 PDF
                    html_path: hp.clone(),
                });
            }
        }
    }

    Ok(hp.to_string_lossy().to_string())
}

/// 生成 PDF（通过 Chrome headless）并返回 base64 + 临时路径
/// 如果缓存命中且文件未变更，直接返回缓存的 PDF（不重新生成）
#[tauri::command]
fn preview_pdf(s: State<AppState>) -> Result<PreviewResult, String> {
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = cur.as_ref().ok_or_else(|| "No file".to_string())?.clone();
    let file_stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();
    drop(cur);

    // 获取当前文件 mtime
    let current_mtime = std::fs::metadata(&p)
        .ok()
        .and_then(|m| m.modified().ok())
        .ok_or_else(|| "无法获取文件修改时间".to_string())?;

    // 检查缓存：mtime 一致且 PDF 文件存在
    {
        let cache = s.cached_preview.lock().map_err(|e| e.to_string())?;
        if let Some(ref cached) = *cache {
            if cached.file_mtime == current_mtime && cached.pdf_path.exists() {
                let pd = std::fs::read(&cached.pdf_path)
                    .map_err(|e| format!("读 PDF 失败: {}", e))?;
                return Ok(PreviewResult {
                    base64: b64(&pd),
                    temp_path: cached.pdf_path.to_string_lossy().to_string(),
                    file_name: format!("{}.pdf", file_stem),
                });
            }
        }
    }

    // 缓存未命中，重新生成 HTML + PDF
    let md = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let is_skill = p
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n == "SKILL.md")
        .unwrap_or(false);
    let (metadata, body_md) = if is_skill {
        converter::parse_front_matter(&md)
    } else {
        (None, &md[..])
    };
    let hb = converter::convert_markdown_to_html(body_md).map_err(|e| e.to_string())?;
    let hb = converter::resolve_image_srcs(&hb, &p);
    let t = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");
    let fh = template::render_html_template_with_metadata(&hb, t, metadata.as_ref());

    let d = std::env::temp_dir().join("rust-mpe-browser");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let hp = d.join(format!("{}.html", file_stem));
    let pp = d.join(format!("{}.pdf", file_stem));

    // 写入 HTML
    std::fs::write(&hp, &fh).map_err(|e| e.to_string())?;

    // 生成 PDF（保留 HTML 文件供后续缓存复用）
    chrome::generate_pdf(&hp.to_string_lossy(), &pp.to_string_lossy())
        .map_err(|e| e.to_string())?;

    // 更新缓存
    {
        let mut cache = s.cached_preview.lock().map_err(|e| e.to_string())?;
        *cache = Some(CachedPreview {
            file_mtime: current_mtime,
            pdf_path: pp.clone(),
            html_path: hp,
        });
    }

    let pd = std::fs::read(&pp).map_err(|e| format!("读 PDF 失败: {}", e))?;
    Ok(PreviewResult {
        base64: b64(&pd),
        temp_path: pp.to_string_lossy().to_string(),
        file_name: format!("{}.pdf", file_stem),
    })
}

/// 保存 PDF 到用户指定位置
#[tauri::command]
fn save_pdf_as(src: String, dst: String) -> Result<(), String> {
    std::fs::copy(&src, &dst).map_err(|e| format!("保存失败: {}", e))?;
    Ok(())
}

/// 渲染完整 HTML（含模板、图片内嵌、SKILL 元数据），供导出命令复用。
fn render_full_html(p: &Path) -> Result<String, String> {
    let md = std::fs::read_to_string(p).map_err(|e| format!("读取失败: {e}"))?;
    let is_skill = p
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n == "SKILL.md")
        .unwrap_or(false);
    let (metadata, body_md) = if is_skill {
        converter::parse_front_matter(&md)
    } else {
        (None, &md[..])
    };
    let hb = converter::convert_markdown_to_html(body_md).map_err(|e| e.to_string())?;
    let hb = converter::resolve_image_srcs(&hb, p);
    let t = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");
    Ok(template::render_html_template_with_metadata(&hb, t, metadata.as_ref()))
}

/// 导出 HTML 到用户指定位置
#[tauri::command]
fn export_html(dst: String, s: State<AppState>) -> Result<(), String> {
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = cur
        .as_ref()
        .ok_or_else(|| "没有打开文件".to_string())?
        .clone();
    drop(cur);
    let html = render_full_html(&p)?;
    std::fs::write(&dst, html).map_err(|e| format!("导出失败: {e}"))?;
    Ok(())
}

/// 导出 PDF 到用户指定位置
#[tauri::command]
fn export_pdf(dst: String, s: State<AppState>) -> Result<(), String> {
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = cur
        .as_ref()
        .ok_or_else(|| "没有打开文件".to_string())?
        .clone();
    drop(cur);
    let html = render_full_html(&p)?;
    let d = std::env::temp_dir().join("rust-mpe-browser");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let hp = d.join("export-tmp.html");
    std::fs::write(&hp, html).map_err(|e| e.to_string())?;
    chrome::generate_pdf(&hp.to_string_lossy(), &dst).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&hp);
    Ok(())
}

/// 导出 DOCX 到用户指定位置（纯 Rust 生成，不依赖 Office）
#[tauri::command]
fn export_docx(dst: String, s: State<AppState>) -> Result<(), String> {
    let cur = s.current_file.lock().map_err(|e| e.to_string())?;
    let p = cur
        .as_ref()
        .ok_or_else(|| "没有打开文件".to_string())?
        .clone();
    drop(cur);
    let md = std::fs::read_to_string(&p).map_err(|e| format!("读取失败: {e}"))?;
    md2x_core::docx::convert_markdown_to_docx(&md, &p, Path::new(&dst))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 获取当前文件名
#[tauri::command]
fn get_file_name(s: State<AppState>) -> Result<Option<String>, String> {
    Ok(s.current_file
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()).map(|s| s.to_string())))
}

// ==================== 工具函数 ====================

fn b64(data: &[u8]) -> String {
    const C: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut r = String::new();
    for chunk in data.chunks(3) {
        let t = (chunk[0] as u32) << 16
            | (chunk.get(1).copied().unwrap_or(0) as u32) << 8
            | (chunk.get(2).copied().unwrap_or(0) as u32);
        r.push(C[((t >> 18) & 0x3F) as usize] as char);
        r.push(C[((t >> 12) & 0x3F) as usize] as char);
        r.push(if chunk.len() > 1 {
            C[((t >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        r.push(if chunk.len() > 2 {
            C[(t & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    r
}
