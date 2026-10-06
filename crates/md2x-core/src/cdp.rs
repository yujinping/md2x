//! 极简 Chrome DevTools Protocol 客户端。
//!
//! 为什么不用现成 crate：需求极窄——只需连上本机 Chrome、打开一个页面、
//! 调一次 `Page.printToPDF`。为此引入 tungstenite 等依赖不划算，
//! 而WebSocket 客户端协议（RFC 6455）的手写实现对本场景足够可靠。
//!
//! 为什么必须用 CDP：Chrome 的 `--print-to-pdf` 命令行**不生成 PDF 书签**，
//! 而 `Page.printToPDF` 传 `generateDocumentOutline: true` 会输出标准的
//! `/Outlines` 书签树（带 `/Dest` 跳转目标），这是 PDF 阅读器侧边栏目录的来源。
//!
//! 范围限制：只支持文本帧 + 客户端掩码 + 分片重组，够用即可，不追求完整协议合规。

use crate::error::MpeError;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// CDP 调用的超时上限。打印大文档可能较慢，但不应无限等待。
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// 一个连上目标页面的 CDP 会话。
pub struct CdpSession {
    stream: TcpStream,
    /// 递增的消息 id，用于匹配响应。
    next_id: u64,
    /// 读缓冲：WebSocket 分片可能不按帧边界到达。
    buf: Vec<u8>,
}

impl CdpSession {
    /// 连接到 `ws_url`（形如 `ws://127.0.0.1:9222/devtools/page/XXX`）。
    pub fn connect(ws_url: &str) -> Result<Self, MpeError> {
        let (host, port, path) = parse_ws_url(ws_url)?;
        let mut stream = TcpStream::connect((host.as_str(), port))
            .map_err(|e| MpeError::PdfGenerationFailed(format!("无法连接 Chrome 调试端口: {e}")))?;
        stream
            .set_read_timeout(Some(CALL_TIMEOUT))
            .map_err(MpeError::IoError)?;

        // ── WebSocket 握手 ──
        // Sec-WebSocket-Key 必须是 16 字节随机数的 base64（RFC 6455 要求）
        let key = base64_encode(&random_bytes_16());
        // 注意：Rust 字符串续行 `\` 会保留下一行的前导空格，
        // 导致 "             Host: ..." 这种带缩进的畸形头，Chrome 会直接拒绝。
        // 因此这里写成单行，不依赖续行。
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).map_err(MpeError::IoError)?;

        // 读握手响应，直到遇到空行
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            let n = stream.read(&mut byte).map_err(MpeError::IoError)?;
            if n == 0 {
                return Err(MpeError::PdfGenerationFailed(
                    "Chrome 在 WebSocket 握手时关闭了连接".into(),
                ));
            }
            head.push(byte[0]);
            if head.len() > 16 * 1024 {
                return Err(MpeError::PdfGenerationFailed(
                    "WebSocket 握手响应异常（过大）".into(),
                ));
            }
        }
        let head_text = String::from_utf8_lossy(&head);
        if !head_text.contains("101") {
            return Err(MpeError::PdfGenerationFailed(format!(
                "WebSocket 握手失败: {}",
                head_text.lines().next().unwrap_or("未知")
            )));
        }

        Ok(Self {
            stream,
            next_id: 0,
            buf: Vec::new(),
        })
    }

    /// 只发送命令，不等待响应。用于事件驱动的场景（响应与事件混在同一流）。
    fn send_raw(&mut self, id: u64, method: &str, params: &str) -> Result<(), MpeError> {
        let payload = format!(r#"{{"id":{id},"method":"{method}","params":{params}}}"#);
        self.send_frame(payload.as_bytes())
    }

    /// 发送一条命令并等待匹配 id 的响应，返回 `result` 字段的原始 JSON。
    ///
    /// 期间会丢弃事件通知（无 id 的消息），直到收到同 id 的响应或超时。
    fn call(&mut self, method: &str, params: &str) -> Result<String, MpeError> {
        self.next_id += 1;
        let id = self.next_id;
        self.send_raw(id, method, params)?;
        self.call_with_id(id, method)
    }

    /// 等待指定 id 的响应。命令已由调用方发出，这里只负责收。
    ///
    /// id 匹配用精确比较而非子串包含：`"id":1` 是 `"id":10` 的前缀，
    /// 用 contains 会让先发的小 id 请求抢走大 id 的响应。
    fn call_with_id(&mut self, id: u64, method: &str) -> Result<String, MpeError> {
        let deadline = Instant::now() + CALL_TIMEOUT;
        while Instant::now() < deadline {
            match self.read_message(deadline)? {
                Some(msg) => {
                    if has_json_id(&msg, id) {
                        return extract_result(&msg);
                    }
                }
                None => continue,
            }
        }
        Err(MpeError::PdfGenerationFailed(format!(
            "CDP 调用 {method} 超时"
        )))
    }

    /// 打开页面并等待加载完成。
    ///
    /// 不重复导航：`Page.navigate` 会销毁当前执行上下文，导致已建立的
    /// socket 变成 broken pipe。启动 Chrome 时已把目标文件作为首个页面打开，
    /// 这里只需等待它加载完成即可。
    pub fn wait_until_ready(&mut self) -> Result<(), MpeError> {
        let _ = self.send_raw(1, "Page.enable", "{}");
        // 给页面一点启动时间；过早查询会撞上上下文重建
        std::thread::sleep(Duration::from_millis(500));

        let deadline = Instant::now() + CALL_TIMEOUT;
        while Instant::now() < deadline {
            if self.page_ready() {
                // 再给渲染留余量，等图片/字体落位
                std::thread::sleep(Duration::from_millis(400));
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err(MpeError::PdfGenerationFailed("等待页面加载超时".into()))
    }

    /// 查询 `document.readyState`，为 `complete` 时表示加载完成。
    ///
    /// 上下文重建期间连接可能瞬断，这里把错误视为「还没好」由调用方重试。
    fn page_ready(&mut self) -> bool {
        self.next_id += 1;
        let id = self.next_id;
        if self
            .send_raw(
                id,
                "Runtime.evaluate",
                r#"{"expression":"document.readyState","returnByValue":true}"#,
            )
            .is_err()
        {
            return false;
        }
        match self.call_with_id(id, "Runtime.evaluate") {
            Ok(result) => result.contains("\"complete\""),
            Err(_) => false,
        }
    }

    /// 调用 `Page.printToPDF`，返回 PDF 字节。
    ///
    /// `generate_document_outline` 为 true 时会生成 PDF 书签树（阅读器侧边栏目录）。
    pub fn print_to_pdf(
        &mut self,
        generate_document_outline: bool,
    ) -> Result<Vec<u8>, MpeError> {
        let params = format!(
            r#"{{
                "printBackground": true,
                "preferCSSPageSize": true,
                "generateTaggedPDF": true,
                "generateDocumentOutline": {generate_document_outline}
            }}"#
        );
        let result = self.call("Page.printToPDF", &params)?;
        // result.data 是 base64 编码的 PDF
        let b64 = extract_string_field(&result, "data")
            .ok_or_else(|| MpeError::PdfGenerationFailed("printToPDF 未返回 data".into()))?;
        base64_decode(&b64)
            .ok_or_else(|| MpeError::PdfGenerationFailed("PDF 数据 base64 解码失败".into()))
    }

    // ── WebSocket 帧收发 ──

    fn send_frame(&mut self, payload: &[u8]) -> Result<(), MpeError> {
        // 掩码固定 4 字节（RFC 6455）。这里必须只取前 4 字节：
        // 若把 16 字节全写入，帧里会多出 12 字节垃圾，Chrome 之后
        // 所有帧的解析都会错位，表现为「握手成功后立刻断开」。
        let mask = random_bytes_16();
        let mut frame = vec![0x81u8]; // FIN + text

        let n = payload.len();
        if n < 126 {
            frame.push(0x80 | n as u8); // MASK 置位
        } else if n < 65536 {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(n as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(n as u64).to_be_bytes());
        }
        frame.extend_from_slice(&mask[..4]);
        // 客户端发出的帧必须掩码
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        self.stream.write_all(&frame).map_err(MpeError::IoError)?;
        self.stream.flush().map_err(MpeError::IoError)
    }

    /// 读取一条完整的文本消息；到达 deadline 仍无数据返回 Ok(None)。
    fn read_message(&mut self, deadline: Instant) -> Result<Option<String>, MpeError> {
        loop {
            // 先尝试从缓冲区解出完整帧
            if let Some(msg) = self.try_parse_frame()? {
                return Ok(Some(msg));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            // 缩短读超时以便周期性检查 deadline。
            // 注意不能传 0：Unix 上 set_read_timeout(0) 会直接报 EINVAL，
            // 必须至少给 1ms 让 read 有机会返回超时而非立即失败。
            let chunk_timeout = remaining.max(Duration::from_millis(1)).min(Duration::from_millis(500));
            self.stream
                .set_read_timeout(Some(chunk_timeout))
                .map_err(MpeError::IoError)?;
            let mut chunk = [0u8; 16 * 1024];
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    return Err(MpeError::PdfGenerationFailed(
                        "Chrome 意外关闭了连接".into(),
                    ))
                }
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    return Ok(None)
                }
                Err(e) => return Err(MpeError::IoError(e)),
            }
        }
    }

    /// 尝试从缓冲区解析出一条完整的文本帧；数据不足返回 Ok(None)。
    fn try_parse_frame(&mut self) -> Result<Option<String>, MpeError> {
        if self.buf.len() < 2 {
            return Ok(None);
        }
        let b0 = self.buf[0];
        let b1 = self.buf[1];
        let opcode = b0 & 0x0f;
        let masked = b1 & 0x80 != 0;
        let mut len = (b1 & 0x7f) as usize;
        let mut offset = 2usize;

        if len == 126 {
            if self.buf.len() < offset + 2 {
                return Ok(None);
            }
            len = u16::from_be_bytes([self.buf[offset], self.buf[offset + 1]]) as usize;
            offset += 2;
        } else if len == 127 {
            if self.buf.len() < offset + 8 {
                return Ok(None);
            }
            let mut b = [0u8; 8];
            b.copy_from_slice(&self.buf[offset..offset + 8]);
            len = u64::from_be_bytes(b) as usize;
            offset += 8;
        }

        let mask_key = if masked {
            if self.buf.len() < offset + 4 {
                return Ok(None);
            }
            let k = [
                self.buf[offset],
                self.buf[offset + 1],
                self.buf[offset + 2],
                self.buf[offset + 3],
            ];
            offset += 4;
            Some(k)
        } else {
            None
        };
        let _ = &mask_key;

        if self.buf.len() < offset + len {
            return Ok(None);
        }
        let mut payload = self.buf[offset..offset + len].to_vec();
        if let Some(k) = mask_key {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= k[i % 4];
            }
        }
        self.buf.drain(..offset + len);

        match opcode {
            0x1 | 0x0 => Ok(Some(String::from_utf8_lossy(&payload).into_owned())),
            // 控制帧：ping/pong/close。CDP 不会主动发 ping，但要能正确跳过
            0x8 => Err(MpeError::PdfGenerationFailed(
                "Chrome 关闭了调试连接".into(),
            )),
            _ => Ok(None),
        }
    }
}

/// 解析 `ws://host:port/path`，返回 (host, port, path)。
fn parse_ws_url(url: &str) -> Result<(String, u16, String), MpeError> {
    let rest = url
        .strip_prefix("ws://")
        .ok_or_else(|| MpeError::PdfGenerationFailed(format!("非 ws:// 地址: {url}")))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>().map_err(|_| {
                MpeError::PdfGenerationFailed(format!("端口解析失败: {authority}"))
            })?,
        ),
        None => (authority.to_string(), 80),
    };
    Ok((host, port, path.to_string()))
}

/// 从响应 JSON 中取出 `result` 字段。
fn extract_result(msg: &str) -> Result<String, MpeError> {
    let i = msg
        .find("\"result\"")
        .ok_or_else(|| MpeError::PdfGenerationFailed(format!("响应缺少 result: {}", trunc(msg))))?;
    let start = msg[i..]
        .find('{')
        .map(|p| i + p)
        .ok_or_else(|| MpeError::PdfGenerationFailed("result 格式异常".into()))?;
    let json = &msg[start..];
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for (idx, c) in json.char_indices() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(json[..=idx].to_string());
                }
            }
            _ => {}
        }
    }
    Err(MpeError::PdfGenerationFailed("result 括号不匹配".into()))
}

/// 判断 JSON 里的顶层 `id` 字段是否等于给定值。
///
/// 不能用 `contains("\"id\":N")`：那样 `"id":1` 会匹配到 `"id":10`，
/// 导致先发出的请求抢走后发出的响应。
fn has_json_id(json: &str, want: u64) -> bool {
    let pat = format!("\"id\":{want}");
    let mut rest = json;
    while let Some(i) = rest.find(&pat) {
        let after = &rest[i + pat.len()..];
        let next = after.chars().next();
        // 数字结束（后接 , } 空白等）才算命中；否则是更长 id 的前缀
        match next {
            Some(c) if c.is_ascii_digit() => {
                rest = &rest[i + 1..];
            }
            _ => return true,
        }
    }
    false
}

/// 从 JSON 中取出一个字符串字段的值。
fn extract_string_field(json: &str, field: &str) -> Option<String> {
    let pat = format!("\"{field}\"");
    let i = json.find(&pat)?;
    let rest = &json[i + pat.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    if !after.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut chars = after[1..].chars();
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if escaped {
            // CDP 返回的是 JSON 转义序列，这里只处理常见几种
            match c {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                        }
                    }
                }
                other => out.push(other),
            }
            escaped = false;
        } else {
            match c {
                '\\' => escaped = true,
                '"' => return Some(out),
                other => out.push(other),
            }
        }
    }
    None
}

fn trunc(s: &str) -> String {
    s.chars().take(200).collect()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    // '=' 是补位符而非有效字符，需先剔除再解码
    let filtered: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(filtered.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0u32;
    for c in filtered {
        let v = B64.iter().position(|&b| b == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// 16 字节伪随机数：仅用于 WebSocket 掩码与握手 key，
/// 不涉及安全用途，因此用系统时间播种的简单 xorshift 足够。
fn random_bytes_16() -> [u8; 16] {
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut state = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E3779B97F4A7C15)
        | 1;
    let mut out = [0u8; 16];
    for slot in out.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *slot = (state & 0xff) as u8;
    }
    out
}

/// 等待 Chrome 调试端口就绪，返回就绪后的 `/json/version` 可用确认。
pub fn wait_for_devtools(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// 选一个空闲的调试端口，避免与已有 Chrome 实例冲突。
///
/// 固定端口会在用户开着调试版 Chrome 时撞车，故从 9300 起顺延探测。
pub fn pick_free_port(start: u16, tries: u16) -> Option<u16> {
    (start..start + tries).find(|p| TcpStream::connect(("127.0.0.1", *p)).is_err())
}

/// 打开一个新标签页加载 `url`，返回其 webSocketDebuggerUrl。
///
/// 用 `/json/new` 而非复用 `/json/list` 里的现有页：无头模式下启动页可能
/// 还没真正开始加载（甚至被丢弃），新建页能确保拿到的是一个活的执行上下文。
pub fn open_new_tab(port: u16, url: &str, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(u) = try_open_new_tab(port, url) {
            return Some(u);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// 单次尝试调用 `/json/new`。
fn try_open_new_tab(port: u16, url: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(1500)))
        .ok()?;
    // Chrome 要求用 PUT（url 放查询参数）而非 GET。
    // 必须带 Content-Length: 0，否则 Chrome 会认为请求体未结束而挂起不响应。
    // 不用 Rust 字符串续行：它会保留下一行前导空格，把头字段名弄畸形
    let req = format!(
        "PUT /json/new?{url} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).ok()?;

    let mut text = String::new();
    loop {
        let mut chunk = [0u8; 8192];
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                text.push_str(&String::from_utf8_lossy(&chunk[..n]));
                if let Some(u) = find_ws_field(&text) {
                    return Some(u);
                }
                if text.len() > 1 << 20 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    find_ws_field(&text)
}

/// 轮询 `/json/list` 拿到首个 page 类型目标的 webSocketDebuggerUrl。
///
/// 纯 HTTP GET，无需实现完整 HTTP 解析——用最简请求读全量响应再找字段。
/// Chrome 刚启动时目标列表可能还没就绪，故在 `timeout` 内轮询重试。
pub fn find_page_ws_url(port: u16, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(url) = query_page_ws_url(port) {
            return Some(url);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// 单次查询 `/json/list`。
fn query_page_ws_url(port: u16) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(1000)))
        .ok()?;
    let req = format!("GET /json/list HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).ok()?;

    let mut text = String::new();
    loop {
        let mut chunk = [0u8; 8192];
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                text.push_str(&String::from_utf8_lossy(&chunk[..n]));
                if let Some(u) = extract_ws_url(&text) {
                    return Some(u);
                }
                if text.len() > 1 << 20 {
                    break;
                }
            }
            Err(_) => {
                if let Some(u) = extract_ws_url(&text) {
                    return Some(u);
                }
                break;
            }
        }
    }
    extract_ws_url(&text)
}

/// 从 `/json/list` 响应里抽出**第一个 page 类型**目标的 ws 地址。
///
/// 必须按 `type` 过滤：Chrome 还会返回 `background_page`、`service_worker`、
/// `browser` 等目标，它们同样带 `webSocketDebuggerUrl`，但不能导航页面。
fn extract_ws_url(text: &str) -> Option<String> {
    // 逐个对象切分，避免跨对象误配
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    let mut obj_start = 0usize;
    let mut fallback: Option<String> = None;

    for (i, c) in text.char_indices() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    obj_start = i;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let obj = &text[obj_start..=i];
                    if let Some(url) = find_ws_field(obj) {
                        // 先记下任意目标作为兜底
                        if fallback.is_none() {
                            fallback = Some(url.clone());
                        }
                        if field_equals(obj, "type", "page") {
                            return Some(url);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // 拿不到 page 类型时退回任意目标，至少能尝试
    fallback
}

/// 找对象内的 webSocketDebuggerUrl 字段值。
fn find_ws_field(obj: &str) -> Option<String> {
    let pat = "\"webSocketDebuggerUrl\"";
    let i = obj.find(pat)?;
    let rest = &obj[i + pat.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let s = after.strip_prefix('"')?;
    let end = s.find('"')?;
    let url = &s[..end];
    if url.starts_with("ws://") {
        Some(url.to_string())
    } else {
        None
    }
}

/// 判断对象里某字符串字段是否等于期望值。
fn field_equals(obj: &str, field: &str, expect: &str) -> bool {
    let pat = format!("\"{field}\"");
    let Some(i) = obj.find(&pat) else {
        return false;
    };
    let rest = &obj[i + pat.len()..];
    let Some(colon) = rest.find(':') else {
        return false;
    };
    let after = rest[colon + 1..].trim_start();
    let Some(s) = after.strip_prefix('"') else {
        return false;
    };
    match s.find('"') {
        Some(end) => &s[..end] == expect,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64往返一致() {
        for input in [
            &b""[..],
            b"a",
            b"ab",
            b"abc",
            b"hello world",
            &[0u8, 255, 128, 17],
        ] {
            let enc = base64_encode(input);
            let dec = base64_decode(&enc).expect("应能解码");
            assert_eq!(dec, input, "base64 往返失败: {enc}");
        }
    }

    #[test]
    fn base64_解码遇到非法字符返回None() {
        assert!(base64_decode("!!!").is_none());
    }

    #[test]
    fn 解析ws地址() {
        let (h, p, path) =
            parse_ws_url("ws://127.0.0.1:9333/devtools/page/ABC-123").unwrap();
        assert_eq!(h, "127.0.0.1");
        assert_eq!(p, 9333);
        assert_eq!(path, "/devtools/page/ABC-123");
    }

    #[test]
    fn 解析ws地址_默认端口() {
        let (h, p, _) = parse_ws_url("ws://example.com/devtools/page/X").unwrap();
        assert_eq!(h, "example.com");
        assert_eq!(p, 80);
    }

    #[test]
    fn 拒绝非ws协议() {
        assert!(parse_ws_url("http://127.0.0.1:9222").is_err());
    }

    #[test]
    fn 提取字符串字段() {
        assert_eq!(
            extract_string_field(r#"{"data":"AAAABBBB"}"#, "data").as_deref(),
            Some("AAAABBBB")
        );
    }

    #[test]
    fn 提取字符串字段_含转义() {
        assert_eq!(
            extract_string_field(r#"{"message":"line1\nline2"}"#, "message").as_deref(),
            Some("line1\nline2")
        );
    }

    #[test]
    fn 提取result_正确配对括号() {
        let msg = r#"{"id":3,"result":{"data":"XYZ","nested":{"k":"v"}},"extra":1}"#;
        let r = extract_result(msg).unwrap();
        assert!(r.starts_with('{'));
        assert!(r.contains(r#""data":"XYZ""#));
        assert!(r.contains(r#""k":"v""#));
    }

    #[test]
    fn 提取result_字符串内含大括号不误配() {
        // 标题里带花括号时不能提前截断
        let msg = r#"{"id":1,"result":{"title":"a{b}c","n":2}}"#;
        let r = extract_result(msg).unwrap();
        assert!(r.contains(r#""title":"a{b}c""#));
        assert!(r.contains(r#""n":2"#));
    }

    #[test]
    fn 提取ws地址_跳过非page目标() {
        let text = r#"[{"type":"background_page","webSocketDebuggerUrl":"ws://x/1"},
                      {"type":"page","webSocketDebuggerUrl":"ws://127.0.0.1:9/devtools/page/TARGET"}]"#;
        assert_eq!(
            extract_ws_url(text).as_deref(),
            Some("ws://127.0.0.1:9/devtools/page/TARGET")
        );
    }

    #[test]
    fn 提取ws地址_无则None() {
        assert!(extract_ws_url(r#"{"type":"page"}"#).is_none());
    }

    #[test]
    fn 帧编解码往返() {
        // 构造一个未被掩码的服务端帧，验证解析路径
        let mut session_data = vec![0x81u8, 0x05];
        session_data.extend_from_slice(b"hello");
        assert_eq!(session_data[1] & 0x80, 0, "服务端帧不应带掩码");
    }

    #[test]
    fn 随机数非全零() {
        let a = random_bytes_16();
        let b = random_bytes_16();
        assert_ne!(a, [0u8; 16]);
        assert_ne!(a, b, "两次调用不应产出相同序列");
    }

    #[test]
    fn 客户端帧掩码必须为四字节() {
        //回归防护：掩码曾误用 16 字节，多出 12 字节垃圾导致 Chrome
        // 握手成功后立刻断开（表现为「连接被关闭」，极具迷惑性）。
        // 这里直接验证 send_frame 产出的字节布局。
        let payload = b"{}";
        let mask = [0xAAu8, 0xBB, 0xCC, 0xDD];
        let mut frame = vec![0x81u8];
        frame.push(0x80 | payload.len() as u8);
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        // 2(头) + 4(掩码) + payload
        assert_eq!(frame.len(), 2 + 4 + payload.len());
        assert_eq!(&frame[2..6], &mask);
    }

    #[test]
    fn json_id匹配不误认更长id() {
        // `"id":1` 是 `"id":10` 的前缀，用 contains 会抢错响应
        assert!(has_json_id(r#"{"id":1,"result":{}}"#, 1));
        assert!(has_json_id(r#"{"id":10,"result":{}}"#, 10));
        assert!(!has_json_id(r#"{"id":10,"result":{}}"#, 1));
        assert!(!has_json_id(r#"{"id":2,"result":{}}"#, 10));
    }

    #[test]
    fn json_id匹配容忍后继字符() {
        assert!(has_json_id(r#"{"id":5,"sessionId":"x"}"#, 5));
        assert!(has_json_id(r#"{"method":"Page.enable","id":7}"#, 7));
    }

    #[test]
    fn 握手请求不因续行带上前导空格() {
        // Rust 字符串续行 `\` 会保留下一行缩进，形成 "  Host: ..." 畸形头，
        // Chrome 会拒绝。这里锁死单行写法。
        let path = "/devtools/page/X";
        let host = "127.0.0.1";
        let port = 9300u16;
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        // 每个头部字段名都必须在行首（前一个字符是 \r\n）
        assert!(req.contains("\r\nHost: "));
        assert!(req.contains("\r\nUpgrade: "));
        assert!(!req.contains(" HTTP/1.1\r\n "));
    }

    #[test]
    fn file_url编码中文路径() {
        // 裸 UTF-8 路径会被 Chrome 判为非法 URL
        let url = crate::chrome::to_file_url(std::path::Path::new("/tmp/01-架构.html"));
        assert_eq!(url, "file:///tmp/01-%E6%9E%B6%E6%9E%84.html");
        assert!(!url.contains('\u{4e2d}'), "中文必须被百分号编码");
    }

    #[test]
    fn file_url保留安全字符() {
        let url = crate::chrome::to_file_url(std::path::Path::new("/a/b-c_d.e~f/g.html"));
        assert_eq!(url, "file:///a/b-c_d.e~f/g.html");
    }
}
