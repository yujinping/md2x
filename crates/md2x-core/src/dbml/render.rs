//! DBML 渲染：数据字典表格 + 可缩放画布，输出**HTML body 片段**。
//!
//! 与早期独立版的关键差别：
//! - 不再自带 `<html>/<head>` 和配色定义，而是输出一段可塞进任意模板
//!   `{{BODY}}` 的片段，配色一律走模板已有的 CSS 变量
//!   （`--accent` / `--bg-panel` / `--border` / `--fg` …）。
//!   这样「md 与 dbml 用同一个模板、明暗主题一致」是天然成立的，
//!   而不是两套配色各自维护、迟早漂移。
//! - 所有类名加 `dx-` 前缀。片段会落进 `.markdown-body` 里，裸的 `.card` /
//!   `.tab` 会和 markdown 正文的样式互相污染。
//! - 变量都有兜底值 `var(--accent, #0969da)`：万一某套模版没定义全套变量，
//!   也不至于渲染成黑白。
//!
//! 产物零外部请求：SVG/CSS/JS 全部内联，不依赖任何 CDN。
//!
//! @author yujinping

use super::model::{Schema, Table};

/// 转义 HTML 特殊字符。
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

/// 生成可嵌入模板 `{{BODY}}` 的 HTML 片段。
///
/// 不再输出 `<h1>`：标题由模板的 `{{TITLE}}` 负责（各模板的标题样式不同），
/// 这里再塞一个 h1 会出现「标题显示两遍」。
pub fn render_body(sc: &Schema) -> String {
    let (tbl_n, col_n, fk_n) = sc.stats();
    let head = summary(sc, tbl_n, col_n, fk_n);

    format!(
        r##"{head}
<div class="dx-tabs" role="tablist">
  <button class="dx-tab dx-on" data-dx-panel="dict" role="tab">数据字典</button>
  <button class="dx-tab" data-dx-panel="canvas" role="tab">Schema 画布</button>
  <button class="dx-tab" data-dx-panel="refs" role="tab">引用关系 ({fk_n})</button>
</div>
<section id="dx-panel-dict" class="dx-panel dx-on">{dict}{enums}</section>
<section id="dx-panel-canvas" class="dx-panel">{canvas}</section>
<section id="dx-panel-refs" class="dx-panel">{refs}</section>
<style>{css}</style>
<script>{js}</script>"##,
        head = head,
        dict = render_dictionary(sc),
        enums = render_enums(sc),
        canvas = render_canvas(sc),
        refs = render_refs(sc),
        fk_n = fk_n,
        css = CSS,
        js = JS
    )
}

/// 极简 Markdown 渲染：只处理 DBML note 里实际会出现的写法。
///
/// 不做完整 CommonMark——note 里几乎只有标题、粗体、行内代码、换行。
/// 引入完整解析器对「注释」这个用途是过度的，且会在片段里再套一层
/// 段落/列表容器，与 markdown-body 的既有样式打架。
///
/// 标题单独处理成一个 `<div class="dx-nh">` 而不是 `<h1>`：note 整体落在
/// `.dx-pnote` / `.dx-tnote` 容器里，真插一个 `<h1>` 会参与文档大纲，
/// 也会被模板的标题样式（字号、上下留白、锚点）接管，撑破卡片头部。
fn md_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 32);
    for line in s.lines() {
        let t = line.trim_end();
        if t.trim().is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("<br>");
        }
        // `#` 开头的行按标题处理：note 首行常写成 `# 标题`，
        // 原样输出会在正文里露出一个赤裸的 `#`（用户会以为渲染坏了）。
        if let Some(rest) = t.trim_start().strip_prefix('#') {
            let title = rest.trim_start_matches('#').trim();
            if !title.is_empty() {
                out.push_str(&format!(
                    "<span class=\"dx-nh\">{}</span>",
                    inline_code(&bold(title))
                ));
                continue;
            }
        }
        out.push_str(&inline_code(&bold(t)));
    }
    out
}

/// 处理 `**粗体**`。
fn bold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let mut open = false;
    while let Some(i) = rest.find("**") {
        out.push_str(&esc(&rest[..i]));
        rest = &rest[i + 2..];
        open = !open;
        out.push_str(if open { "<strong>" } else { "</strong>" });
    }
    out.push_str(&esc(rest));
    if open {
        // 未闭合：补上收尾，避免整段被浏览器当作未闭合标签吞掉
        out.push_str("</strong>");
    }
    out
}

/// 处理 `` `代码` ``。
fn inline_code(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('`') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('`') {
            Some(j) => {
                out.push_str(&format!("<code>{}</code>", &after[..j]));
                rest = &after[j + 1..];
            }
            None => {
                // 单个反引号：原样保留
                out.push('`');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 顶部元信息摘要块。
fn summary(sc: &Schema, tbl: usize, col: usize, fk: usize) -> String {
    let mut parts = Vec::new();
    if let Some(p) = &sc.project {
        parts.push(format!("<b>项目</b> {}", esc(p)));
    }
    if let Some(d) = &sc.db_type {
        parts.push(format!("<b>数据库</b> {}", esc(d)));
    }
    parts.push(format!("<b>表</b> {tbl}"));
    parts.push(format!("<b>字段</b> {col}"));
    parts.push(format!("<b>外键</b> {fk}"));
    let meta = format!(
        "<div class=\"dx-meta\">{}</div>",
        parts.join(" <span class=\"dx-dot\">·</span> ")
    );
    let note = if sc.project_note.trim().is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"dx-pnote\">{}</div>",
            md_inline(&sc.project_note)
        )
    };
    meta + &note
}

/// 数据字典：按 TableGroup 归类，每张表一张卡片 + 字段表格。
fn render_dictionary(sc: &Schema) -> String {
    let mut out = String::new();
    let mut order: Vec<(String, Vec<&Table>)> = Vec::new();
    for g in &sc.groups {
        let ts: Vec<&Table> = sc
            .tables
            .iter()
            .filter(|t| t.group.as_deref() == Some(&g.name))
            .collect();
        if !ts.is_empty() {
            order.push((g.name.clone(), ts));
        }
    }
    let loose: Vec<&Table> = sc.tables.iter().filter(|t| t.group.is_none()).collect();
    if !loose.is_empty() {
        order.push(("未分组".to_string(), loose));
    }

    // 只有一组且组名为「未分组」时不显示分组标题：整篇只有一个标题会显得多余
    let show_group_title = !(order.len() == 1 && order[0].0 == "未分组");
    for (gname, tables) in order {
        if show_group_title {
            out.push_str(&format!("<h2 class=\"dx-gsec\">{}</h2>", esc(&gname)));
        }
        for t in tables {
            out.push_str(&render_table_card(sc, t));
        }
    }
    out
}

/// 单张表的卡片。
fn render_table_card(sc: &Schema, t: &Table) -> String {
    let short = t.name.rsplit('.').next().unwrap_or(&t.name);
    let alias = t
        .alias
        .as_ref()
        .map(|a| format!("<span class=\"dx-alias\">as {}</span>", esc(a)))
        .unwrap_or_default();

    let mut rows = String::new();
    for c in &t.cols {
        let badge = |txt: &str, cls: &str| format!("<span class=\"dx-bdg {cls}\">{txt}</span>");
        let mut flags = String::new();
        if c.pk {
            flags.push_str(&badge("PK", "dx-b-pk"));
        }
        if c.unique {
            flags.push_str(&badge("UQ", "dx-b-uq"));
        }
        if c.increment {
            flags.push_str(&badge("INC", "dx-b-inc"));
        }
        if c.not_null {
            flags.push_str(&badge("NN", "dx-b-nn"));
        }

        // 外键：箭头指向被引用的父表，基数用 title 提示，
        // 避免 "tenants.id >" 这种看起来像子表的倒序歧义
        let fk = match sc
            .fk_of(&t.name, &c.name)
            .or_else(|| sc.fk_of(short, &c.name))
        {
            Some((tt, tc, rel)) => format!(
                "<a class=\"dx-fk\" href=\"#{anc}\" title=\"{} {}\">→ {}.{}</a>",
                esc(rel.label()),
                esc(rel.symbol()),
                esc(tt),
                esc(tc),
                anc = anchor(tt)
            ),
            None => String::new(),
        };

        rows.push_str(&format!(
            "<tr><td class=\"dx-c-name\">{name}</td><td class=\"dx-c-type\"><code>{ty}</code></td>\
<td class=\"dx-c-flag\">{flags}</td><td class=\"dx-c-null\">{null}</td><td class=\"dx-c-def\">{dft}</td>\
<td class=\"dx-c-fk\">{fk}</td><td class=\"dx-c-note\">{note}</td></tr>",
            name = esc(&c.name),
            ty = esc(&c.ty),
            flags = flags,
            null = if c.not_null { "NOT NULL" } else { "NULL" },
            dft = c
                .default
                .as_ref()
                .map(|d| format!("<code>{}</code>", esc(d)))
                .unwrap_or_else(|| "<span class=\"dx-nil\">—</span>".into()),
            fk = if fk.is_empty() {
                "<span class=\"dx-nil\">—</span>".to_string()
            } else {
                fk
            },
            note = if c.note.is_empty() {
                "<span class=\"dx-nil\">—</span>".to_string()
            } else {
                esc(&c.note)
            }
        ));
    }

    let idx_html = if t.indexes.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = t
            .indexes
            .iter()
            .map(|i| {
                let kind = if i.pk {
                    "PK"
                } else if i.unique {
                    "UNIQUE"
                } else {
                    "INDEX"
                };
                let name = i
                    .name
                    .as_ref()
                    .map(|n| format!(" <code>{}</code>", esc(n)))
                    .unwrap_or_default();
                let ty = i
                    .ty
                    .as_ref()
                    .map(|x| format!(" <i>{}</i>", esc(x)))
                    .unwrap_or_default();
                format!(
                    "<li><span class=\"dx-bdg dx-b-idx\">{}</span> ({}){}{}</li>",
                    kind,
                    esc(&i.cols.join(", ")),
                    name,
                    ty
                )
            })
            .collect();
        format!(
            "<div class=\"dx-idx\"><h4>索引</h4><ul>{}</ul></div>",
            items.join("")
        )
    };

    let note_html = if t.note.is_empty() {
        String::new()
    } else {
        format!("<div class=\"dx-tnote\">{}</div>", md_inline(&t.note))
    };

    format!(
        r#"<div class="dx-card" id="{anc}">
<div class="dx-card-hd"><h3>{name}</h3>{alias}<span class="dx-cnum">{n} 字段</span></div>
{note}
<div class="dx-tw"><table class="dx-cols"><thead><tr><th>字段</th><th>类型</th><th>约束</th><th>可空</th><th>默认值</th><th>外键</th><th>说明</th></tr></thead>
<tbody>{rows}</tbody></table></div>
{idx}
</div>"#,
        anc = anchor(&t.name),
        name = esc(&t.name),
        alias = alias,
        n = t.cols.len(),
        note = note_html,
        rows = rows,
        idx = idx_html
    )
}

/// 锚点 id：表名里的 `.` 换成 `--`。
fn anchor(name: &str) -> String {
    format!("dx-t-{}", name.replace('.', "--").replace(' ', "_"))
}

/// 枚举定义。
fn render_enums(sc: &Schema) -> String {
    if sc.enums.is_empty() {
        return String::new();
    }
    let mut out = String::from("<h2 class=\"dx-gsec\">枚举类型</h2>");
    for e in &sc.enums {
        let rows: String = e
            .values
            .iter()
            .map(|(v, n)| {
                format!(
                    "<tr><td class=\"dx-c-name\"><code>{}</code></td><td>{}</td></tr>",
                    esc(v),
                    if n.is_empty() {
                        "<span class=\"dx-nil\">—</span>".into()
                    } else {
                        esc(n)
                    }
                )
            })
            .collect();
        out.push_str(&format!(
            "<div class=\"dx-card\" id=\"{}\"><div class=\"dx-card-hd\"><h3>{}</h3><span class=\"dx-cnum\">{} 个值</span></div>\
<table class=\"dx-cols\"><thead><tr><th>值</th><th>说明</th></tr></thead><tbody>{}</tbody></table></div>",
            anchor(&e.name),
            esc(&e.name),
            e.values.len(),
            rows
        ));
    }
    out
}

/// 引用关系清单。
fn render_refs(sc: &Schema) -> String {
    if sc.refs.is_empty() {
        return "<p class=\"dx-nil\">无外键定义</p>".to_string();
    }
    let rows: String = sc
        .refs
        .iter()
        .map(|r| {
            let del = r.on_delete.clone().unwrap_or_else(|| "—".into());
            let upd = r.on_update.clone().unwrap_or_else(|| "—".into());
            format!(
                "<tr><td><a href=\"#{fa}\">{ft}.{fc}</a></td><td class=\"dx-op\">{op} {lbl}</td>\
<td><a href=\"#{ta}\">{tt}.{tc}</a></td><td><code>{del}</code></td><td><code>{upd}</code></td></tr>",
                fa = anchor(&r.from_table),
                ta = anchor(&r.to_table),
                ft = esc(&r.from_table),
                fc = esc(&r.from_col),
                tt = esc(&r.to_table),
                tc = esc(&r.to_col),
                op = esc(r.rel.symbol()),
                lbl = r.rel.label(),
                del = esc(&del),
                upd = esc(&upd)
            )
        })
        .collect();
    format!(
        "<table class=\"dx-cols dx-refs\"><thead><tr><th>子表.字段</th><th>关系</th><th>父表.字段</th><th>ON DELETE</th><th>ON UPDATE</th></tr></thead><tbody>{}</tbody></table>",
        rows
    )
}

/// 可缩放画布：内联 SVG + JS 拖动缩放。
fn render_canvas(sc: &Schema) -> String {
    let (svg, w, h) = super::layout::build_svg(sc);
    format!(
        r#"<div class="dx-cv-bar">
  <button class="dx-btn" id="dx-fit">适应窗口</button>
  <button class="dx-btn" id="dx-zin">放大 +</button>
  <button class="dx-btn" id="dx-zout">缩小 −</button>
  <input id="dx-q" class="dx-q" placeholder="搜索表名或字段…" autocomplete="off">
  <span id="dx-zoom" class="dx-zoom">100%</span>
  <span class="dx-hint">滚轮缩放 · 拖拽平移 · 点击表定位</span>
</div>
<div class="dx-cv-wrap" id="dx-cv-wrap">
  <div class="dx-cv-stage" id="dx-cv-stage" data-w="{w}" data-h="{h}">{svg}</div>
</div>"#,
        svg = svg,
        w = w,
        h = h
    )
}

/// 内联样式。
///
/// 全部走模板变量并带兜底值：模板切换配色时这里自动跟随，
/// 换一套自带模版不需要改这个文件一个字。
const CSS: &str = r#"
.dx-meta{color:var(--fg-muted,#57606a);font-size:13px;margin:0 0 6px}
.dx-meta b{font-weight:500;color:var(--fg,#333)}
.dx-dot{color:var(--border-strong,#d0d7de);margin:0 4px}
.dx-pnote{margin:10px 0 0;padding:12px 14px;background:var(--bg-subtle,#f6f8fa);border-left:3px solid var(--accent,#0366d6);border-radius:0 6px 6px 0;color:var(--fg-muted,#57606a);font-size:13px}
.dx-nh{display:block;font-weight:600;font-size:14px;color:var(--fg-strong,#1f2328);margin:0 0 2px}
/* note 里的行内代码：默认会被模板的 .markdown-body code 接管，
   而它用的是 GitHub 的 --color-neutral-muted（一个固定的浅灰），
   深色主题下就是一坨发亮的灰块贴在深色卡片上。这里按模板变量重画，
   并提高特异性压过模板规则。 */
.dx-pnote code,.dx-tnote code{font-family:var(--mono,ui-monospace,monospace);font-size:12px;background:var(--code-inline-bg,rgba(27,31,35,.08));color:var(--fg,#333);padding:1px 5px;border-radius:4px;border:0;white-space:nowrap}
.dx-tabs{display:flex;gap:2px;margin:22px 0 0;border-bottom:1px solid var(--border,#e8e8e8)}
.dx-tab{appearance:none;border:0;background:none;padding:9px 16px;font:inherit;font-size:13px;color:var(--fg-muted,#57606a);cursor:pointer;border-bottom:2px solid transparent;margin-bottom:-1px}
.dx-tab:hover{color:var(--fg,#333)}
.dx-tab.dx-on{color:var(--accent,#0366d6);border-bottom-color:var(--accent,#0366d6);font-weight:500}
.dx-panel{display:none;padding-top:20px}
.dx-panel.dx-on{display:block}
.dx-card{border:1px solid var(--border,#e8e8e8);border-radius:10px;margin:0 0 18px;overflow:hidden;background:var(--bg-panel,#fff)}
.dx-card:target{border-color:var(--accent,#0366d6);box-shadow:0 0 0 3px rgba(3,102,214,.12)}
.dx-card-hd{display:flex;align-items:baseline;gap:10px;padding:12px 16px;background:var(--bg-subtle,#f6f8fa);border-bottom:1px solid var(--border,#e8e8e8)}
.dx-card-hd h3{margin:0;font-size:15px;font-weight:600;font-family:var(--mono,ui-monospace,SFMono-Regular,Menlo,monospace);color:var(--fg-strong,#1f2328)}
.dx-alias{font-size:12px;color:var(--fg-muted,#57606a);font-family:var(--mono,ui-monospace,monospace)}
.dx-cnum{margin-left:auto;font-size:12px;color:var(--fg-muted,#57606a)}
.dx-gsec{font-size:16px;font-weight:600;margin:36px 0 14px;padding-bottom:8px;border-bottom:1px solid var(--border,#e8e8e8);color:var(--fg-strong,#1f2328)}
.dx-tnote{padding:10px 16px;color:var(--fg-muted,#57606a);font-size:13px;border-bottom:1px solid var(--border,#e8e8e8);background:var(--bg-panel,#fff)}
table.dx-cols{width:100%;border-collapse:collapse;font-size:13px;margin:0;display:table;table-layout:auto}
/* 数据字典表格是「列对齐」的产物，不是正文表格：模板对 .markdown-body table
   有 display:block!important 与 code{white-space:normal!important} 两条通用规则
   （前者为了横向滚动，后者为了长代码换行）。落到 7 列的数据字典上就是灾难：
   table 不再是表格、列宽不再按内容分配；类型 `varchar(255)` 会在括号处断行、
   `timestamptz` 被切成两截、字段名 `created_at` 断成 `created_a|t`。
   这里用更高特异性 + !important 把三条都按回去——字典表格宁可宽一点也不折，
   折了就没法按行扫读了。
   注意 `max-width` 必须留 `none`、`width` 用 `max-content`：表格一旦被限制在
   容器宽度内，浏览器为塞下 7 列会把列压扁，`nowrap` 也救不回来（单词只能
   在中间断开）。`max-content + min-width:100%` 的含义是「内容需要多宽就多宽，
   但不比容器窄」，超出的部分交给外层 `.dx-tw` 横向滚动。 */
.dx-tw{width:100%;overflow-x:auto;-webkit-overflow-scrolling:touch}
.markdown-body table.dx-cols{display:table!important;width:max-content!important;min-width:100%!important;max-width:none!important;overflow:visible!important;table-layout:auto}
.markdown-body table.dx-cols code{white-space:nowrap!important;word-break:normal!important;overflow-wrap:normal!important}
table.dx-cols th{text-align:left;font-weight:500;color:var(--fg-muted,#57606a);font-size:12px;padding:8px 12px;border-bottom:1px solid var(--border,#e8e8e8);background:var(--bg-panel,#fff);white-space:nowrap}
table.dx-cols td{padding:8px 12px;border-bottom:1px solid var(--border,#e8e8e8);vertical-align:top;color:var(--fg,#333)}
table.dx-cols tr:last-child td{border-bottom:0}
table.dx-cols tbody tr:hover{background:var(--bg-hover,#f0f4f8)}
.dx-c-name{font-family:var(--mono,ui-monospace,monospace);font-weight:500;white-space:nowrap}
.dx-c-type{white-space:nowrap}
.dx-c-def{white-space:nowrap}
.dx-c-type code,.dx-c-def code,.dx-idx code,.dx-refs code{font-family:var(--mono,ui-monospace,monospace);font-size:12px;background:var(--code-inline-bg,rgba(27,31,35,.08));padding:1px 5px;border-radius:4px;border:0;color:var(--code-inline-fg,inherit)}
.dx-c-flag{white-space:nowrap}
.dx-c-null{color:var(--fg-muted,#57606a);font-size:12px;white-space:nowrap}
.dx-c-note{color:var(--fg-muted,#57606a);min-width:140px;max-width:340px;white-space:normal;word-break:break-word;width:340px}
.dx-bdg{display:inline-block;font-size:10px;font-weight:600;line-height:1;padding:3px 5px;border-radius:4px;margin-right:3px;letter-spacing:.02em;white-space:nowrap}
.dx-b-pk{background:rgba(207,34,46,.12);color:#cf222e}
.dx-b-uq{background:rgba(130,80,223,.12);color:#8250df}
.dx-b-nn{background:var(--bg-subtle,#f6f8fa);color:var(--fg-muted,#57606a);border:1px solid var(--border,#e8e8e8)}
.dx-b-inc{background:rgba(3,102,214,.12);color:var(--accent,#0366d6)}
.dx-b-idx{background:var(--bg-subtle,#f6f8fa);color:var(--fg-muted,#57606a);border:1px solid var(--border,#e8e8e8)}
.dx-fk{color:#1a7f37;text-decoration:none;font-family:var(--mono,ui-monospace,monospace);font-size:12px;white-space:nowrap}
.dx-fk:hover{text-decoration:underline}
table.dx-refs a{color:#1a7f37;text-decoration:none;font-family:var(--mono,ui-monospace,monospace);font-size:12px;white-space:nowrap}
table.dx-refs a:hover{text-decoration:underline}
table.dx-refs td:first-child{font-family:var(--mono,ui-monospace,monospace);font-size:12px}
.dx-op{white-space:nowrap;color:var(--fg-muted,#57606a)}
.dx-nil{color:var(--border-strong,#d0d7de)}
.dx-idx{padding:10px 16px 14px;border-top:1px solid var(--border,#e8e8e8);background:var(--bg-panel,#fff)}
.dx-idx h4{margin:0 0 8px;font-size:12px;font-weight:500;color:var(--fg-muted,#57606a)}
.dx-idx ul{margin:0;padding-left:0;list-style:none;font-size:12px;font-family:var(--mono,ui-monospace,monospace);color:var(--fg-muted,#57606a)}
.dx-idx li{padding:2px 0}
.dx-cv-bar{display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-bottom:10px}
.dx-btn{appearance:none;font:inherit;font-size:12px;padding:5px 11px;border:1px solid var(--border,#e8e8e8);background:var(--bg-panel,#fff);color:var(--fg,#333);border-radius:6px;cursor:pointer}
.dx-btn:hover{background:var(--bg-hover,#f0f4f8)}
.dx-q{font:inherit;font-size:12px;padding:5px 10px;border:1px solid var(--border,#e8e8e8);border-radius:6px;background:var(--bg-panel,#fff);color:var(--fg,#333);width:200px}
.dx-zoom{font-size:12px;color:var(--fg-muted,#57606a);font-family:var(--mono,ui-monospace,monospace);min-width:44px}
.dx-hint{font-size:12px;color:var(--fg-muted,#57606a);margin-left:auto}
.dx-cv-wrap{position:relative;height:calc(100vh - 260px);min-height:420px;border:1px solid var(--border,#e8e8e8);border-radius:10px;overflow:hidden;background:var(--bg-subtle,#f6f8fa);cursor:grab}
.dx-cv-wrap.dx-dragging{cursor:grabbing}
.dx-cv-stage{position:absolute;top:0;left:0;transform-origin:0 0;will-change:transform}
.dx-cv-stage svg{display:block;max-width:none}
.dx-cv-stage .dx-tbl-hit{cursor:pointer}
.dx-cv-stage .dx-tbl-hit:hover .dx-hd{filter:brightness(.94)}
.dx-cv-stage .dx-dim{opacity:.18}
/* 画布 SVG 元素：颜色一律走模版变量，换模版时画布与表格一起变 */
.dx-cv-bg{fill:var(--bg-panel,#fff)}
.dx-grp-box{fill:var(--accent,#0366d6);fill-opacity:.04;stroke:var(--accent,#0366d6);stroke-width:1;stroke-dasharray:5 3}
.dx-grp-txt{fill:var(--accent,#0366d6)}
.dx-edge{fill:none;stroke:var(--border-strong,#d0d7de);stroke-width:1.4}
.dx-edge-head{fill:var(--fg-muted,#57606a)}
.dx-node-box{fill:var(--bg-panel,#fff);stroke:var(--border-strong,#d0d7de);stroke-width:1}
.dx-node-hd{fill:var(--bg-subtle,#f6f8fa)}
.dx-node-line{stroke:var(--border,#e8e8e8)}
.dx-node-title{fill:var(--fg-strong,#1f2328)}
.dx-node-cmt{fill:var(--fg-muted,#57606a)}
.dx-node-alias{fill:var(--fg-muted,#57606a)}
.dx-node-col{fill:var(--fg,#333)}
.dx-node-type{fill:var(--fg-faint,#8b949e)}
.dx-node-pk{fill:#cf222e}
/* 画布节点内是 SVG，`fill` 优先级低于任何带 fill 的 CSS 类，
   所以徽章底色写在各 dx-tag-* 上，文字统一用 dx-tag-tx。
   三种颜色与数据字典表格的徽章同源（PK 红 / FK 绿 / UQ 紫），
   同一个概念在两个视图里颜色一致，跨视图扫读不用重新建立映射。 */
.dx-tag{stroke:none}
.dx-tag-pk{fill:#cf222e}
.dx-tag-fk{fill:#1a7f37}
.dx-tag-uq{fill:#8250df}
.dx-tag-tx{fill:#fff}
@media print{.dx-tabs,.dx-cv-bar{display:none}.dx-panel{display:block!important}.dx-cv-wrap{height:auto;overflow:visible}}
"#;

/// 内联脚本：Tab 切换 + 画布缩放平移 + 搜索。
///
/// 全部 id/class 带 `dx-` 前缀，且用 `querySelectorAll` 限定在本次渲染的
/// 容器内——同一页面若嵌多份 dbml 片段（聚合导出场景）也不会互相抢事件。
const JS: &str = r#"
(function(){
  var tabs=document.querySelectorAll('.dx-tab');
  function activate(name){
    var target=null;
    tabs.forEach(function(x){
      var on=x.dataset.dxPanel===name;
      x.classList.toggle('dx-on',on);
      if(on)target=x;
    });
    document.querySelectorAll('.dx-panel').forEach(function(p){p.classList.remove('dx-on')});
    var el=document.getElementById('dx-panel-'+name);
    if(el){el.classList.add('dx-on')}
    return target;
  }
  tabs.forEach(function(t){t.addEventListener('click',function(){
    activate(t.dataset.dxPanel);
    if(t.dataset.dxPanel==='canvas'){fit()}
  })});
  // 深链：#canvas / #refs / #dict 可直接打开对应视图（便于分享具体视图）
  function fromHash(){
    var h=(location.hash||'').replace('#','');
    if(h==='canvas'||h==='refs'||h==='dict'){
      var t=activate(h);
      if(h==='canvas'&&typeof window.__dxFit==='function'){window.__dxFit()}
      if(t){window.scrollTo({top:0,behavior:'auto'})}
    }
  }
  window.addEventListener('hashchange',fromHash);
  fromHash();

  var wrap=document.getElementById('dx-cv-wrap');
  var stage=document.getElementById('dx-cv-stage');
  if(!wrap||!stage)return;
  var scale=1,minS=0.15,maxS=4,tx=0,ty=0;
  var W=+stage.dataset.w||800,H=+stage.dataset.h||600;
  var zoomLabel=document.getElementById('dx-zoom');

  function apply(){
    scale=Math.max(minS,Math.min(maxS,scale));
    stage.style.transform='translate('+tx+'px,'+ty+'px) scale('+scale+')';
    if(zoomLabel){zoomLabel.textContent=Math.round(scale*100)+'%'}
  }
  function fit(){
    var r=wrap.getBoundingClientRect();
    if(!r.width||!W||!H){apply();return}
    var pad=24;
    var s=Math.min((r.width-pad*2)/W,(r.height-pad*2)/H);
    // 可读下限：缩到 60% 以下时表内 10px 的列名会糊成一团，
    // 「适应窗口」反而变成「看不清」。此时不强行塞进窗口，改为
    // 保底 60% 并让容器自己出滚动条——看得清比看得全更重要。
    var READABLE_MIN=0.6;
    if(s<READABLE_MIN){
      s=READABLE_MIN;
      // 保底缩放后画布比容器大：从左上角起排，滚动条接管浏览
      tx=pad;ty=pad;
    }else{
      tx=(r.width-W*s)/2;ty=(r.height-H*s)/2;
    }
    scale=Math.max(minS,Math.min(maxS,s));
    apply();
  }
  window.__dxFit=fit;

  wrap.addEventListener('wheel',function(e){
    e.preventDefault();
    var r=wrap.getBoundingClientRect();
    var mx=e.clientX-r.left,my=e.clientY-r.top;
    var old=scale;
    var k=e.deltaY<0?1.12:1/1.12;
    scale=Math.max(minS,Math.min(maxS,scale*k));
    tx=mx-(mx-tx)*(scale/old);
    ty=my-(my-ty)*(scale/old);
    apply();
  },{passive:false});

  var dragging=false,px=0,py=0,moved=false;
  wrap.addEventListener('mousedown',function(e){
    if(e.target.closest('.dx-tbl-hit')&&!e.shiftKey){return}
    dragging=true;moved=false;px=e.clientX;py=e.clientY;wrap.classList.add('dx-dragging');
  });
  window.addEventListener('mousemove',function(e){
    if(!dragging)return;
    tx+=e.clientX-px;ty+=e.clientY-py;
    px=e.clientX;py=e.clientY;moved=true;apply();
  });
  window.addEventListener('mouseup',function(){
    dragging=false;wrap.classList.remove('dx-dragging');
  });

  var zn=document.getElementById('dx-zin'),zo=document.getElementById('dx-zout'),zf=document.getElementById('dx-fit');
  if(zn)zn.addEventListener('click',function(){var r=wrap.getBoundingClientRect();var cx=r.width/2,cy=r.height/2,old=scale;scale=scale*1.25;tx=cx-(cx-tx)*(scale/old);ty=cy-(cy-ty)*(scale/old);apply()});
  if(zo)zo.addEventListener('click',function(){var r=wrap.getBoundingClientRect();var cx=r.width/2,cy=r.height/2,old=scale;scale=scale/1.25;tx=cx-(cx-tx)*(scale/old);ty=cy-(cy-ty)*(scale/old);apply()});
  if(zf)zf.addEventListener('click',fit);

  var q=document.getElementById('dx-q');
  var hits=stage.querySelectorAll('.dx-tbl-hit');
  if(q){q.addEventListener('input',function(){
    var v=q.value.trim().toLowerCase();
    hits.forEach(function(g){
      var hay=(g.dataset.k||'').toLowerCase();
      g.classList.toggle('dx-dim',v!==''&&hay.indexOf(v)<0);
    });
  })}

  hits.forEach(function(g){
    g.addEventListener('click',function(){
      if(moved)return;
      var id=g.dataset.anchor;
      if(!id)return;
      var tab=document.querySelector('.dx-tab[data-dx-panel="dict"]');
      if(tab)tab.click();
      var card=document.getElementById(id);
      if(card){card.scrollIntoView({behavior:'smooth',block:'start'});
        card.style.transition='box-shadow .4s';card.style.boxShadow='0 0 0 3px rgba(3,102,214,.35)';
        setTimeout(function(){card.style.boxShadow=''},1400);}
    });
  });

  setTimeout(fit,30);
  window.addEventListener('resize',function(){clearTimeout(window.__dxrt);window.__dxrt=setTimeout(fit,150)});
})();
"#;