//! 图片解析：定位（本地/Hugo/远程）、读取、尺寸。

use crate::converter;
use std::path::Path;

/// 解析图片来源，返回 (字节, content type)。失败返回 None（调用方跳过）。
pub fn resolve_image_bytes(src: &str, md_file: &Path) -> Option<(Vec<u8>, String)> {
    if !converter::is_resolvable_path(src) {
        return None;
    }
    if src.starts_with("http://") || src.starts_with("https://") {
        return converter::download_image_bytes(src);
    }

    let base = md_file.parent().unwrap_or(Path::new("."));
    let hugo_root = converter::find_hugo_root(md_file);
    let img_path = if Path::new(src).is_absolute() {
        std::path::PathBuf::from(src)
    } else if src.starts_with('/') {
        match hugo_root {
            Some(root) => root
                .join("static")
                .join(src.strip_prefix('/').unwrap_or(src)),
            None => return None,
        }
    } else {
        base.join(src)
    };
    let data = std::fs::read(&img_path).ok()?;
    let mime = converter::mime_from_ext(&img_path).to_string();
    Some((data, mime))
}

/// 读取图片像素尺寸 (宽, 高)。支持 PNG/JPEG/GIF/BMP/WebP。
pub fn image_dimensions(bytes: &[u8], mime: &str) -> Option<(u32, u32)> {
    match mime {
        m if m.contains("png") => png_dimensions(bytes),
        m if m.contains("jpeg") => jpeg_dimensions(bytes),
        m if m.contains("gif") => gif_dimensions(bytes),
        m if m.contains("bmp") => bmp_dimensions(bytes),
        m if m.contains("webp") => webp_dimensions(bytes),
        _ => None,
    }
}

fn png_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || b[0..8] != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    Some((u32_be(&b[16..20])?, u32_be(&b[20..24])?))
}

fn jpeg_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 4 || b[0..2] != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        // SOF0-SOF15（跳过 C4/C8/CC）
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = u16_be(&b[i + 5..i + 7])?;
            let w = u16_be(&b[i + 7..i + 9])?;
            return Some((w as u32, h as u32));
        }
        let len = u16_be(&b[i + 2..i + 4])? as usize;
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

fn gif_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 10 || (&b[0..3] != b"GIF" && &b[0..3] != b"GIF") {
        return None;
    }
    Some((u16_le(&b[6..8])? as u32, u16_le(&b[8..10])? as u32))
}

fn bmp_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 26 || &b[0..2] != b"BM" {
        return None;
    }
    let w = i32_le(&b[18..22])?;
    let h = i32_le(&b[22..26])?;
    if w <= 0 || h == 0 {
        return None;
    }
    Some((w as u32, h.unsigned_abs()))
}

fn webp_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 30 || &b[0..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return None;
    }
    match &b[12..16] {
        b"VP8 " => {
            // VP8 lossy: width/height 为 14 位小端
            let w = u16_le(&b[26..28])? & 0x3FFF;
            let h = u16_le(&b[28..30])? & 0x3FFF;
            Some((w as u32, h as u32))
        }
        b"VP8L" => {
            // VP8L lossless: 4 字节位域
            if b.len() < 25 {
                return None;
            }
            let bits = u32_le(&b[21..25])?;
            let w = (bits & 0x3FFF) + 1;
            let h = ((bits >> 14) & 0x3FFF) + 1;
            Some((w, h))
        }
        _ => None,
    }
}

fn u16_be(b: &[u8]) -> Option<u16> {
    Some(u16::from_be_bytes([b[0], b[1]]))
}

fn u32_be(b: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16_le(b: &[u8]) -> Option<u16> {
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn u32_le(b: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn i32_le(b: &[u8]) -> Option<i32> {
    Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// 由 content type 推断文件扩展名。
pub fn ext_from_mime(mime: &str) -> &'static str {
    if mime.contains("jpeg") {
        "jpeg"
    } else if mime.contains("gif") {
        "gif"
    } else if mime.contains("bmp") {
        "bmp"
    } else if mime.contains("webp") {
        "webp"
    } else {
        "png"
    }
}
