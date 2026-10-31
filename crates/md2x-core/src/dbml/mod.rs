//! DBML → 文档（HTML 片段 / Markdown）。
//!
//! 三个子模块各管一件事：
//! - [`model`]：解析后的中间模型，刻意不做类型校验；
//! - [`parser`]：宽容型词法扫描器，遇到不认识的块跳过而不整份失败；
//! - [`layout`]：把 schema 排成可直接内联的 SVG；
//! - [`route`]：画布连线的正交路由，保证路径不穿任何节点框；
//! - [`render`]：表格与画布的 HTML 片段，配色走模板 CSS 变量。
//!
//! 对外只暴露两个入口：`to_html_body` 与 `to_markdown`，
//! 分别对应「预览/导出 HTML」与「导出 Markdown / DOCX」两条链路。
//!
//! @author yujinping

pub mod layout;
pub mod model;
pub mod parser;
pub mod render;
pub mod route;

pub use model::Schema;

/// 解析 DBML 源码。
pub fn parse(src: &str) -> Schema {
    parser::parse(src)
}

/// DBML 源码 → 可嵌入模板 `{{BODY}}` 的 HTML 片段。
pub fn to_html_body(src: &str) -> String {
    render::render_body(&parser::parse(src))
}

/// DBML 源码 → 数据字典式 Markdown。
///
/// 用途一：右键「导出 Markdown」，产物可直接进 git 做文档评审。
/// 用途二：DOCX 导出的中转——`markdown → docx` 这条链路已经成熟
/// （`docx::convert_markdown_to_docx` 内已有表格渲染），
/// 为 DBML 单独写一份 OOXML 生成器属于重复建设。
pub fn to_markdown(src: &str, title: &str) -> String {
    let sc = parser::parse(src);
    let mut out = String::with_capacity(src.len() * 2);
    let (tbl_n, col_n, fk_n) = sc.stats();

    out.push_str(&format!("# {}\n\n", title.trim()));
    let mut meta = Vec::new();
    if let Some(p) = &sc.project {
        meta.push(format!("**项目** {}", p));
    }
    if let Some(d) = &sc.db_type {
        meta.push(format!("**数据库** {}", d));
    }
    meta.push(format!("**表** {tbl_n}"));
    meta.push(format!("**字段** {col_n}"));
    meta.push(format!("**外键** {fk_n}"));
    out.push_str(&meta.join(" · "));
    out.push_str("\n\n");

    if !sc.project_note.trim().is_empty() {
        out.push_str("> ");
        out.push_str(&sc.project_note.trim().replace('\n', "\n> "));
        out.push_str("\n\n");
    }

    // 与 HTML 版一致：按分组归类，未分组的放最后
    let mut order: Vec<(String, Vec<&model::Table>)> = Vec::new();
    for g in &sc.groups {
        let ts: Vec<&model::Table> = sc
            .tables
            .iter()
            .filter(|t| t.group.as_deref() == Some(&g.name))
            .collect();
        if !ts.is_empty() {
            order.push((g.name.clone(), ts));
        }
    }
    let loose: Vec<&model::Table> = sc.tables.iter().filter(|t| t.group.is_none()).collect();
    if !loose.is_empty() {
        order.push(("未分组".to_string(), loose));
    }

    let show_group_title = !(order.len() == 1 && order[0].0 == "未分组");
    for (gname, tables) in order {
        if show_group_title {
            out.push_str(&format!("## {}\n\n", gname));
        }
        for t in tables {
            write_table_md(&mut out, &sc, t);
        }
    }

    if !sc.enums.is_empty() {
        out.push_str("## 枚举类型\n\n");
        for e in &sc.enums {
            out.push_str(&format!("### {}\n\n", e.name));
            if e.values.iter().all(|(_, n)| n.is_empty()) {
                // 全无注释时用一个行内列表，比空表格紧凑得多
                let vals: Vec<String> = e
                    .values
                    .iter()
                    .map(|(v, _)| format!("`{}`", v))
                    .collect();
                out.push_str(&vals.join(" · "));
                out.push_str("\n\n");
            } else {
                out.push_str("| 值 | 说明 |\n| --- | --- |\n");
                for (v, n) in &e.values {
                    out.push_str(&format!(
                        "| `{}` | {} |\n",
                        v,
                        if n.is_empty() { "—" } else { n.as_str() }
                    ));
                }
                out.push('\n');
            }
        }
    }

    if !sc.refs.is_empty() {
        out.push_str("## 引用关系\n\n");
        out.push_str("| 子表.字段 | 关系 | 父表.字段 | ON DELETE | ON UPDATE |\n");
        out.push_str("| --- | --- | --- | --- | --- |\n");
        for r in &sc.refs {
            out.push_str(&format!(
                "| `{}.{}` | {} {} | `{}.{}` | {} | {} |\n",
                r.from_table,
                r.from_col,
                r.rel.symbol(),
                r.rel.label(),
                r.to_table,
                r.to_col,
                r.on_delete.as_deref().unwrap_or("—"),
                r.on_update.as_deref().unwrap_or("—")
            ));
        }
        out.push('\n');
    }

    out
}

/// 写单张表的 Markdown 段。
fn write_table_md(out: &mut String, sc: &Schema, t: &model::Table) {
    let short = t.name.rsplit('.').next().unwrap_or(&t.name);
    let alias = t
        .alias
        .as_ref()
        .map(|a| format!(" （别名 `{a}`）"))
        .unwrap_or_default();
    out.push_str(&format!("### {}{}\n\n", t.name, alias));

    if !t.note.trim().is_empty() {
        out.push_str("> ");
        out.push_str(&t.note.trim().replace('\n', "\n> "));
        out.push_str("\n\n");
    }

    out.push_str("| 字段 | 类型 | 约束 | 可空 | 默认值 | 外键 | 说明 |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for c in &t.cols {
        let mut flags = Vec::new();
        if c.pk {
            flags.push("PK");
        }
        if c.unique {
            flags.push("UQ");
        }
        if c.increment {
            flags.push("INC");
        }
        if c.not_null {
            flags.push("NN");
        }
        let fk = sc
            .fk_of(&t.name, &c.name)
            .or_else(|| sc.fk_of(short, &c.name))
            .map(|(tt, tc, _)| format!("→ `{tt}.{tc}`"))
            .unwrap_or_else(|| "—".into());
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | {} |\n",
            c.name,
            c.ty,
            if flags.is_empty() {
                "—".to_string()
            } else {
                flags.join(" ")
            },
            if c.not_null { "NOT NULL" } else { "NULL" },
            c.default
                .as_ref()
                .map(|d| format!("`{d}`"))
                .unwrap_or_else(|| "—".into()),
            fk,
            if c.note.is_empty() {
                "—".to_string()
            } else {
                // 表格单元格里的 | 必须转义，否则会把列切断
                c.note.replace('|', "\\|").replace('\n', " ")
            }
        ));
    }
    out.push('\n');

    if !t.indexes.is_empty() {
        out.push_str("**索引**\n\n");
        for i in &t.indexes {
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
                .map(|n| format!(" `{n}`"))
                .unwrap_or_default();
            let ty = i.ty.as_ref().map(|x| format!(" *{x}*")).unwrap_or_default();
            out.push_str(&format!(
                "- {} ({}){}{}\n",
                kind,
                i.cols.join(", "),
                name,
                ty
            ));
        }
        out.push('\n');
    }
}