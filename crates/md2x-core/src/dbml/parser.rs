//! 宽容型 DBML 解析器。
//!
//! 为什么不用现成的 crate：`dbml-rs` 会对外键两端做严格类型相等校验，
//! `id bigserial [pk]` 被 `a_id bigint` 引用时直接报
//! "Mismatched foreign key type" 拒绝解析——而这是真实 schema 里最常见的写法。
//! 出文档是"读"场景，宁可放过也不该因为类型不匹配而整份失败。
//!
//! 本解析器只做词法级扫描，不建完整 AST，也不做语义校验：
//! 遇到不认识的块跳过并继续，保证"总能出文档"。
//!
//! @author yujinping

use super::model::{Column, Enum, Group, IndexDef, Ref, Rel, Schema, Table};

/// 解析入口：任何输入都返回 Schema，解析不了的部分静默跳过。
pub fn parse(src: &str) -> Schema {
    let cleaned = strip_comments(src);
    let mut sc = Schema::default();
    let mut rest = cleaned.as_str();

    while let Some(pos) = find_next_decl(rest) {
        rest = &rest[pos..];
        let trimmed = rest.trim_start();
        rest = trimmed;

        // 各 parse_* 收到的都是「关键字之后」的切片，返回的消耗量也是相对该切片的，
        // 因此回填 rest 时必须加上关键字自身的字节数——否则会切在 UTF-8 字符中间。
        let (consumed, kw_len) = if let Some(body) = rest.strip_prefix("Project") {
            (parse_project(body, &mut sc), "Project".len())
        } else if let Some(body) = rest.strip_prefix("Enum") {
            (parse_enum(body, &mut sc), "Enum".len())
        } else if let Some(body) = rest.strip_prefix("TableGroup") {
            (parse_group(body, &mut sc), "TableGroup".len())
        } else if let Some(body) = rest.strip_prefix("Table") {
            (parse_table(body, &mut sc), "Table".len())
        } else if let Some(body) = rest.strip_prefix("Ref") {
            (parse_ref(body, &mut sc), "Ref".len())
        } else {
            (None, 0)
        };

        match consumed {
            Some(n) if n > 0 => rest = &rest[kw_len + n..],
            // 无法识别：跳过该声明首行，避免死循环
            _ => {
                let n = rest.find('\n').unwrap_or(rest.len().saturating_sub(1));
                rest = &rest[(n + 1).min(rest.len())..];
            }
        }
    }

    // 行内 ref 与顶层 Ref 可能重复表达同一条外键，统一去重
    dedup_refs(&mut sc);

    // 回填 TableGroup 的分组归属。
    // 必须放在整份文件解析完之后：DBML 里 `TableGroup` 惯常写在所有
    // `Table` 之后（官方示例即如此），解析单张表时分组表还没建立，
    // 在那里查会一律查不到，表现为"分组名写在源文件里、页面上却没有"。
    assign_groups(&mut sc);

    sc
}

/// 把 `sc.groups` 里登记的表名回填到对应 `Table.group`。
///
/// 支持限定名（`schema.table`）与裸表名两种写法：DBML 允许
/// `TableGroup g { public.users }`，而 `Table.name` 存的是完整名。
fn assign_groups(sc: &mut Schema) {
    // 先建 表名 → 组名 的映射，避免 O(tables × groups)
    let mut owner: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for g in &sc.groups {
        for t in &g.tables {
            owner.insert(t.clone(), g.name.clone());
        }
    }
    if owner.is_empty() {
        return;
    }
    for t in &mut sc.tables {
        if t.group.is_some() {
            continue;
        }
        // 完整名优先，其次退到裸表名（去掉 schema 前缀）
        let short = t.name.rsplit('.').next().unwrap_or(&t.name);
        if let Some(g) = owner.get(&t.name).or_else(|| owner.get(short)) {
            t.group = Some(g.clone());
        }
    }
}

/// 找到下一个顶层声明的起始偏移。
fn find_next_decl(s: &str) -> Option<usize> {
    let keys = ["Project", "Enum", "TableGroup", "Table", "Ref"];
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // 只在行首（允许前导空白）匹配关键字
        let line_start = i == 0 || bytes[i - 1] == b'\n';
        if line_start {
            let mut j = i;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            for k in keys {
                if s[j..].starts_with(k) {
                    // 关键字后必须是空白、`{` 或 `:`（`Ref:` 的冒号直接跟关键字）
                    let after = s.as_bytes().get(j + k.len()).copied();
                    if matches!(
                        after,
                        Some(b' ') | Some(b'\t') | Some(b'{') | Some(b':') | Some(b'\r') | Some(b'\n')
                    ) {
                        // TableGroup 的 T 也匹配 Table 前缀，需让它优先
                        if k == "Table" && s[j..].starts_with("TableGroup") {
                            continue;
                        }
                        return Some(j);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// 去掉 `//` 行注释，但不误伤字符串里的 `//`（如 `default: 'http://x'`）。
fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut quote: Option<char> = None;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
                continue;
            }
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => {
                quote = Some(c);
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                // 吞到行尾
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// 取 `{ ... }` 体，返回 (体内文本, 消耗字节数)。
fn take_brace_body(s: &str) -> Option<(String, usize)> {
    let open = s.find('{')?;
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'\'' | b'"' | b'`' => quote = Some(b),
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((s[open + 1..i].to_string(), i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// 解析 `[a, b: 'x']` 形式的设置串，按顶层逗号切分。
fn split_settings(s: &str) -> Vec<String> {
    let inner = s.trim();
    let inner = inner
        .strip_prefix('[')
        .and_then(|x| x.strip_suffix(']'))
        .unwrap_or(inner);
    split_top(inner, ',')
}

/// 按顶层分隔符切分，尊重引号与括号嵌套。
fn split_top(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            buf.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    buf.push(n);
                }
                continue;
            }
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => {
                quote = Some(c);
                buf.push(c);
            }
            '(' | '[' => {
                depth += 1;
                buf.push(c);
            }
            ')' | ']' => {
                depth -= 1;
                buf.push(c);
            }
            c if c == sep && depth == 0 => {
                out.push(buf.trim().to_string());
                buf.clear();
            }
            _ => buf.push(c),
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

/// 去掉包裹引号，`'''x'''` 也处理。
fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 6 && t.starts_with("'''") && t.ends_with("'''") {
        // 多行 note 在源文件里是按缩进对齐写的，剥掉引号后每行还带着一层
        // 文件缩进（常见 4~8 个空格）。这层缩进必须去掉：
        //   - Markdown 里 4 空格开头是「缩进代码块」，`**粗体**` 和 `` `code` ``
        //     在代码块内不解析，导出 DOCX 时原样变成正文里的星号和反引号；
        //   - HTML 片段里会白白多出前导空白，首行还会因 flex/对齐偏移。
        // 只去公共前缀缩进（dedent）即可，理由见 `dedent` 的注释：
        // 逐行 trim_start 会把 note 里真有结构的嵌套列表压平。
        return dedent(t[3..t.len() - 3].trim_matches('\n'));
    }
    if t.len() >= 2 {
        let f = t.chars().next().unwrap();
        let l = t.chars().last().unwrap();
        if f == l && matches!(f, '\'' | '"' | '`') {
            return dedent(&t[1..t.len() - 1]);
        }
    }
    // 走到这里的是「已由 `split_props` 剥掉 `'''` 的多行 note」以及无引号的裸值。
    // 单行值因为没有前导空白（上游已 trim），dedent 是空操作；
    // 多行值则要靠这里去掉每行统一的那层文件缩进。
    dedent(t)
}

/// 去掉多行文本的公共前导缩进（dedent）。
///
/// 只按「所有非空行的最小缩进」整体左移，因此相对缩进（嵌套列表、代码块）
/// 会被保留。空行保持空行——补空格反而会让 Markdown 把它当硬换行。
///
/// **首行不参与缩进统计**：调用方已对整段做过 `trim()`，首行的缩进信息
/// 在那一刻就丢了（`'''` 后紧跟换行时首行本就没有缩进）。若把它算进
/// 最小值，min 会恒为 0，整个函数变成空操作——这正是多行 note 缩进
/// 一直没被去掉的原因。
fn dedent(s: &str) -> String {
    let mut it = s.split('\n');
    let first = it.next().unwrap_or("");
    let rest: Vec<&str> = it.collect();
    let min = rest
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    if min == 0 {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    out.push_str(first);
    for l in rest {
        out.push('\n');
        if l.trim().is_empty() {
            continue;
        }
        // 缩进不足 min 的行（写入方手抖）直接截到空，不 panic
        out.push_str(l.get(min..).unwrap_or(""));
    }
    out
}

/// 解析 Project 块。
fn parse_project(s: &str, sc: &mut Schema) -> Option<usize> {
    let (body, used) = take_brace_body(s)?;
    // 头部可能带名字：`Project foo {`
    let head = &s[..s.find('{')?];
    // 入参已剥掉 `Project` 关键字，head 就是 `名字` 部分
    let name = head.trim();
    if !name.is_empty() {
        sc.project = Some(unquote(name));
    }
    // 按"键: 值"解析，但 `'''...'''` 多行 note 必须整体取出，
    // 逐行切分公司只会留下最后一行（典型症状：注释只剩一个句号）。
    for (k, v) in split_props(&body) {
        match k.to_ascii_lowercase().as_str() {
            "database_type" => sc.db_type = Some(unquote(&v)),
            "note" => sc.project_note = unquote(&v),
            _ => {}
        }
    }
    Some(used)
}

/// 把 `{ k: v  k: '''多行''' }` 这类属性块拆成 (键, 值) 列表。
///
/// 与逐行 split_once 的区别：值里的 `'''...'''`、引号字符串、括号都视为整体，
/// 内部换行不会中断。属性之间靠「换行 + 新键」或纯换行分隔。
fn split_props(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = body.chars().peekable();
    let mut key = String::new();
    let mut val = String::new();
    let mut stage = 0; // 0=读键 1=读值
    let mut quote: Option<char> = None;
    let mut triple = false;

    let flush = |out: &mut Vec<(String, String)>, key: &mut String, val: &mut String, stage: &mut i32| {
        if *stage == 1 && !key.trim().is_empty() {
            out.push((key.trim().to_string(), val.trim().to_string()));
        }
        key.clear();
        val.clear();
        *stage = 0;
    };

    while let Some(c) = chars.next() {
        // 三引号块
        if triple {
            if c == '\'' && body_has_triple(chars.clone()) {
                // 吞掉剩余两个引号
                chars.next();
                chars.next();
                triple = false;
                continue;
            }
            val.push(c);
            continue;
        }
        if let Some(q) = quote {
            val.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    val.push(n);
                }
                continue;
            }
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' if stage == 1 && chars.peek() == Some(&'\'') => {
                // 可能是三引号起始
                let mut look = chars.clone();
                look.next();
                if look.peek() == Some(&'\'') {
                    chars.next();
                    chars.next();
                    triple = true;
                    continue;
                }
                quote = Some('\'');
                val.push(c);
            }
            '\'' | '"' if stage == 1 => {
                quote = Some(c);
                val.push(c);
            }
            ':' if stage == 0 => stage = 1,
            '\n' | '\r' if stage == 1 => {
                // 换行后若紧接「新键:」则收尾，否则视为值内换行
                let mut look = chars.clone();
                while matches!(look.peek(), Some(' ') | Some('\t') | Some('\n') | Some('\r')) {
                    look.next();
                }
                // 前瞻：连续标识符后跟冒号 → 新属性
                let mut id = String::new();
                while let Some(&x) = look.peek() {
                    if x.is_alphanumeric() || x == '_' {
                        id.push(x);
                        look.next();
                    } else {
                        break;
                    }
                }
                if look.peek() == Some(&':') && !id.is_empty() {
                    flush(&mut out, &mut key, &mut val, &mut stage);
                }
            }
            _ => {
                if stage == 0 {
                    key.push(c);
                } else {
                    val.push(c);
                }
            }
        }
    }
    flush(&mut out, &mut key, &mut val, &mut stage);
    out
}

/// 判断后续是否紧跟 `''`（三引号收尾）。
fn body_has_triple(mut it: std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    it.next() == Some('\'') && it.next() == Some('\'')
}

/// 解析 Enum 块。
fn parse_enum(s: &str, sc: &mut Schema) -> Option<usize> {
    let brace = s.find('{')?;
    // 入参已剥掉 `Enum` 关键字
    let head = s[..brace].trim();
    let (body, used) = take_brace_body(s)?;
    let mut e = Enum {
        name: unquote(head.rsplit('.').next().unwrap_or(head)),
        values: Vec::new(),
    };
    for raw in body.lines() {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        // 值可带 [note: '...']
        let (val, note) = match t.find('[') {
            Some(i) => {
                let settings = split_settings(&t[i..]);
                let note = settings
                    .iter()
                    .find_map(|x| {
                        let (k, v) = x.split_once(':')?;
                        (k.trim().eq_ignore_ascii_case("note")).then(|| unquote(v))
                    })
                    .unwrap_or_default();
                (t[..i].trim(), note)
            }
            None => (t, String::new()),
        };
        if !val.is_empty() {
            e.values.push((unquote(val), note));
        }
    }
    sc.enums.push(e);
    Some(used)
}

/// 解析 TableGroup 块。
fn parse_group(s: &str, sc: &mut Schema) -> Option<usize> {
    let brace = s.find('{')?;
    // 入参已剥掉 `TableGroup` 关键字
    let head = s[..brace].trim();
    let (body, used) = take_brace_body(s)?;
    let mut g = Group {
        name: unquote(head.rsplit('.').next().unwrap_or(head)),
        note: String::new(),
        tables: Vec::new(),
    };
    for raw in body.lines() {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(v) = t.strip_prefix("Note") {
            g.note = unquote(v.trim_start_matches(':').trim());
            continue;
        }
        // 成员既可以逐行写，也可以单行逗号分隔（`{ users, posts }`）。
        // 两种都要认：只按换行切分的话，单行写法会得到一整个
        // "users, posts" 当成表名，`TableGroup` 于是静默失效。
        for piece in t.split(',') {
            // 成员可带注释：`users [note: '...']`
            let p = piece.trim();
            if p.is_empty() {
                continue;
            }
            let name = match p.find('[') {
                Some(i) => p[..i].trim(),
                None => p,
            };
            if !name.is_empty() {
                let n = unquote(name);
                if !n.is_empty() && !g.tables.contains(&n) {
                    g.tables.push(n);
                }
            }
        }
    }
    sc.groups.push(g);
    Some(used)
}

/// 解析 Table 块：列、行内 ref、Indexes、Note。
fn parse_table(s: &str, sc: &mut Schema) -> Option<usize> {
    let brace = s.find('{')?;
    // 入参已剥掉 `Table` 关键字，header 形如 `users as U [headercolor: ...]`
    let header = s[..brace].trim();
    let (mut body, used) = take_brace_body(s)?;

    // 先剔除 Record 块（示例数据，不属于表结构），否则会被当成列解析
    while let Some(ri) = find_keyword(&body, "Record") {
        match take_brace_body(&body[ri..]) {
            Some((_, rused)) => {
                let nb = format!("{}{}", &body[..ri], &body[ri + rused..]);
                body = nb;
            }
            None => break,
        }
    }

    // 表头：`name` / `name as alias` / `schema.name`，其后可能跟 [settings]
    let (name_part, table_settings) = match header.find('[') {
        Some(i) => (
            header[..i].trim(),
            Some(split_settings(&header[i..])),
        ),
        None => (header, None),
    };
    let (name_part, alias) = match name_part.to_ascii_lowercase().find(" as ") {
        Some(i) => (
            name_part[..i].trim(),
            Some(unquote(&name_part[i + 4..])),
        ),
        None => (name_part, None),
    };

    let mut t = Table {
        name: unquote(name_part),
        alias,
        note: String::new(),
        color: None,
        group: None,
        cols: Vec::new(),
        indexes: Vec::new(),
        pending_inline: Vec::new(),
        pending_table_ref: Vec::new(),
    };

    if let Some(settings) = table_settings {
        for st in settings {
            if let Some((k, v)) = st.split_once(':') {
                match k.trim().to_ascii_lowercase().as_str() {
                    "headercolor" | "color" => t.color = Some(unquote(v)),
                    "note" => t.note = unquote(v),
                    _ => {}
                }
            }
        }
    }

    // 独立提取 Indexes 块，剩余部分再按列解析
    if let Some(idx_start) = find_keyword(&body, "Indexes") {
        if let Some((ib, iused)) = take_brace_body(&body[idx_start..]) {
            for item in split_top(&ib, '\n') {
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                let (cols_part, settings) = match item.find('[') {
                    Some(i) => (item[..i].trim(), Some(split_settings(&item[i..]))),
                    None => (item, None),
                };
                let cols_part = cols_part
                    .strip_prefix('(')
                    .and_then(|x| x.strip_suffix(')'))
                    .unwrap_or(cols_part);
                let cols: Vec<String> = split_top(cols_part, ',')
                    .into_iter()
                    .map(|c| unquote(&c))
                    .filter(|c| !c.is_empty())
                    .collect();
                let mut idx = IndexDef {
                    cols,
                    ..Default::default()
                };
                if let Some(ss) = settings {
                    for st in ss {
                        if let Some((k, v)) = st.split_once(':') {
                            match k.trim().to_ascii_lowercase().as_str() {
                                "name" => idx.name = Some(unquote(v)),
                                "type" => idx.ty = Some(unquote(v)),
                                _ => {}
                            }
                        } else if st.trim().eq_ignore_ascii_case("pk")
                            || st.trim().eq_ignore_ascii_case("primary key")
                        {
                            idx.pk = true;
                        } else if st.trim().eq_ignore_ascii_case("unique") {
                            idx.unique = true;
                        }
                    }
                }
                if !idx.cols.is_empty() {
                    t.indexes.push(idx);
                }
            }
            let abs_end = idx_start + iused;
            let mut nb = String::with_capacity(body.len());
            nb.push_str(&body[..idx_start]);
            nb.push_str(&body[abs_end..]);
            body = nb;
        }
    }

    // 列解析：DBML 允许「一行一列」和「一行多列」两种写法
    // （`id int [pk] status int`）。按语句切分而非按行切分，
    // 一条语句 = 列名 + 类型 + 可选设置块。
    for stmt in split_statements(&body) {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        let lower = stmt.to_ascii_lowercase();
        if lower.starts_with("note") && stmt[4..].trim_start().starts_with(':') {
            let v = stmt[4..].trim_start().trim_start_matches(':');
            t.note = unquote(v);
            continue;
        }
        // 表内 `ref: > a.(x, y)`：引用方是本表的组合列
        if lower.starts_with("ref") && stmt[3..].trim_start().starts_with(':') {
            let expr = stmt[3..].trim_start().trim_start_matches(':').trim();
            if let Some((rel, tt, tcols)) = parse_ref_expr_multi(expr) {
                // 本表参与列：从左侧 `dummy` 无法判断，改为整表级 ref，
                // 由 Indexes 中的 PK 顺序回填；此处先记下目标，稍后配对
                t.pending_table_ref.push((rel, tt, tcols));
            }
            continue;
        }
        let _ = parse_column(stmt, &mut t);
    }

    // 表级 ref 的引用方列：取本表 PK 索引的列顺序，没有则取前 N 列
    let pk_cols: Vec<String> = t
        .indexes
        .iter()
        .find(|i| i.pk)
        .map(|i| i.cols.clone())
        .unwrap_or_else(|| t.cols.iter().take(2).map(|c| c.name.clone()).collect());
    for (rel, tt, tcols) in std::mem::take(&mut t.pending_table_ref) {
        let n = tcols.len().min(pk_cols.len());
        for k in 0..n {
            sc.refs.push(Ref {
                from_table: t.name.clone(),
                from_col: pk_cols[k].clone(),
                to_table: tt.clone(),
                to_col: tcols[k].clone(),
                rel,
                on_delete: None,
                on_update: None,
            });
        }
    }

    // 分组归属**不在这里回填**：`TableGroup` 在 DBML 里通常写在所有 Table
    // 之后（官方文档的示例就是这种顺序），此时 `sc.groups` 还是空的，
    // 在这里查等于永远查不到，分组会静默丢失（表现为「写了 TableGroup
    // 但页面上没有分组标题」）。统一改到 `parse()` 末尾扫一遍。

    // 行内 ref 收口成统一 Ref 表，供画布与表格共同消费
    for (col, (rel, tt, tc)) in std::mem::take(&mut t.pending_inline) {
        sc.refs.push(Ref {
            from_table: t.name.clone(),
            from_col: col,
            to_table: tt,
            to_col: tc,
            rel,
            on_delete: None,
            on_update: None,
        });
    }

    sc.tables.push(t);
    Some(used)
}

/// 去掉重复 ref：同一对 (子表,子列,父表,父列) 只保留第一条。
///
/// DBML 里行内 `[ref: > a.id]` 和独立的 `Ref: b.c > a.id` 表达同一件事，
/// 真实文件两种写法常同时出现（生成器与手写混用）。不去重会让画布上
/// 出现两条完全重合的连线，看起来像渲染 bug。
fn dedup_refs(sc: &mut Schema) {
    let mut seen = std::collections::HashSet::new();
    sc.refs.retain(|r| {
        seen.insert((
            r.from_table.clone(),
            r.from_col.clone(),
            r.to_table.clone(),
            r.to_col.clone(),
        ))
    });
}

/// 把表体切成"列语句"序列。
///
/// DBML 里一列形如 `<名> <类型> [设置]`，既可能各占一行，也可能挤在一行。
/// 判定一条语句结束的依据：设置块 `[...]` 闭合后，或遇到换行。
/// 括号/引号内的空白与换行不切分，避免 `varchar(64)`、`'''多行注释'''` 被拆开。
fn split_statements(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut in_settings = false;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            buf.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    buf.push(n);
                }
                continue;
            }
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => {
                quote = Some(c);
                buf.push(c);
            }
            '(' => {
                depth += 1;
                buf.push(c);
            }
            ')' => {
                depth -= 1;
                buf.push(c);
            }
            '[' => {
                in_settings = true;
                depth += 1;
                buf.push(c);
            }
            ']' => {
                depth -= 1;
                buf.push(c);
                if in_settings && depth == 0 && !buf.trim().is_empty() {
                    // 设置块收尾即一条语句结束：提交并等下一个名字
                    out.push(buf.trim().to_string());
                    buf.clear();
                    in_settings = false;
                    // 跳过紧随其后的空白
                    while matches!(chars.peek(), Some(' ') | Some('\t')) {
                        chars.next();
                    }
                }
            }
            '\n' | '\r' if depth == 0 => {
                if !buf.trim().is_empty() {
                    out.push(buf.trim().to_string());
                    buf.clear();
                }
            }
            _ => buf.push(c),
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

/// 查找关键字位置（行首、后接空白）。
///
/// 大小写不敏感：DBML 规范写 `indexes`，而生成器与手写稿常写 `Indexes`，
/// 早期版本严格区分大小写，导致小写 `indexes { ... }` 无法识别，
/// 整块索引声明被当作列解析、渲染成 `indexes` / `{` / `(a, b)` 三行假字段。
fn find_keyword(s: &str, kw: &str) -> Option<usize> {
    let mut offset = 0;
    for line in s.split_inclusive('\n') {
        let t = line.trim_start();
        // 按字节比较，避免在中文备注的多字节字符处切片 panic
        if t.len() >= kw.len() && t.as_bytes()[..kw.len()].eq_ignore_ascii_case(kw.as_bytes()) {
            let after = t.as_bytes().get(kw.len()).copied();
            if matches!(after, Some(b' ') | Some(b'\t') | Some(b'{') | Some(b'\r') | Some(b'\n') | None)
            {
                return Some(offset + (line.len() - t.len()));
            }
        }
        offset += line.len();
    }
    None
}

/// 解析单行列定义，成功则 push 进表。
fn parse_column(line: &str, t: &mut Table) -> Option<()> {
    // 切出设置段
    let (head, settings) = match line.find('[') {
        Some(i) => (&line[..i], Some(split_settings(&line[i..]))),
        None => (line, None),
    };
    let head = head.trim();
    // 名 + 类型：以第一个空白切分；类型可含空格（`double precision`）与括号
    let mut it = head.splitn(2, char::is_whitespace);
    let name = it.next()?.trim();
    if name.is_empty() {
        return None;
    }
    let ty = it.next().unwrap_or("").trim();
    if ty.is_empty() {
        return None;
    }

    let mut c = Column {
        name: unquote(name),
        ty: ty.to_string(),
        ..Default::default()
    };

    if let Some(ss) = settings {
        for st in ss {
            let trimmed = st.trim();
            let (k, v) = match trimmed.split_once(':') {
                Some((k, v)) => (k.trim(), Some(v.trim())),
                None => (trimmed, None),
            };
            match (k.to_ascii_lowercase().as_str(), v) {
                ("pk" | "primary key", None) => c.pk = true,
                ("not null", None) => c.not_null = true,
                ("null", None) => c.not_null = false,
                ("unique", None) => c.unique = true,
                ("increment", None) => c.increment = true,
                ("note", Some(v)) => c.note = unquote(v),
                ("default", Some(v)) => c.default = Some(unquote(v)),
                ("ref", Some(v)) => {
                    // 行内引用：`> tenants.id` / `< x.y`
                    if let Some(r) = parse_ref_expr(v) {
                        t.pending_inline.push((c.name.clone(), r));
                    }
                }
                _ => {}
            }
        }
    }

    t.cols.push(c);
    Some(())
}

/// 剥掉基数符号，返回 (基数, 剩余表达式)。
fn split_rel(v: &str) -> Option<(Rel, &str)> {
    if let Some(r) = v.strip_prefix("<>") {
        Some((Rel::ManyToMany, r))
    } else if let Some(r) = v.strip_prefix('>') {
        Some((Rel::ManyToOne, r))
    } else if let Some(r) = v.strip_prefix('<') {
        Some((Rel::OneToMany, r))
    } else {
        v.strip_prefix('-').map(|r| (Rel::OneToOne, r))
    }
}

/// 解析引用表达式（支持组合列），返回 (基数, 目标表, 目标列列表)。
fn parse_ref_expr_multi(v: &str) -> Option<(Rel, String, Vec<String>)> {
    let v = v.trim();
    let (rel, rest) = split_rel(v)?;
    let (tbl, cols) = split_side(rest);
    if cols.is_empty() || tbl.is_empty() {
        return None;
    }
    Some((rel, tbl, cols))
}

/// 解析引用表达式，返回 (基数, 目标表, 目标列)。
fn parse_ref_expr(v: &str) -> Option<(Rel, String, String)> {
    let v = v.trim();
    let (rel, rest) = split_rel(v)?;
    let target = rest.trim();
    let (tbl, col) = target.rsplit_once('.')?;
    Some((rel, unquote(tbl), unquote(col.trim_matches(|c| c == '(' || c == ')'))))
}

/// 解析顶层 `Ref:` 语句。
fn parse_ref(s: &str, sc: &mut Schema) -> Option<usize> {
    let line_end = s.find('\n').unwrap_or(s.len());
    // 入参已是 `Ref` 之后的内容，只需去掉冒号
    let line = s[..line_end].trim();
    let body = line.strip_prefix(':').unwrap_or(line).trim();

    // 设置段（delete/update）置于末尾
    let (expr, settings) = match body.find('[') {
        Some(i) => (body[..i].trim(), Some(split_settings(&body[i..]))),
        None => (body, None),
    };

    let (rel, on_delete, on_update) = {
        let mut rel = Rel::ManyToOne;
        let mut del = None;
        let mut upd = None;
        if let Some(ss) = settings {
            for st in ss {
                if let Some((k, v)) = st.split_once(':') {
                    match k.trim().to_ascii_lowercase().as_str() {
                        "delete" => del = Some(unquote(v)),
                        "update" => upd = Some(unquote(v)),
                        _ => {}
                    }
                }
            }
        }
        // 表达式两侧的基数符号
        if let Some(i) = expr.find("<>") {
            rel = Rel::ManyToMany;
            push_ref(sc, &expr[..i], &expr[i + 2..], rel, del.clone(), upd.clone());
        } else if let Some(i) = find_rel_op(expr) {
            let (op, idx, len) = i;
            rel = op;
            push_ref(sc, &expr[..idx], &expr[idx + len..], rel, del.clone(), upd.clone());
        }
        (rel, del, upd)
    };
    let _ = (rel, on_delete, on_update);
    Some(line_end)
}

/// 找到关系运算符的位置（跳过括号内的）。
fn find_rel_op(s: &str) -> Option<(Rel, usize, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"<>") {
            return Some((Rel::ManyToMany, i, 2));
        }
        match b[i] {
            b'>' => return Some((Rel::ManyToOne, i, 1)),
            b'<' => return Some((Rel::OneToMany, i, 1)),
            b'-' => return Some((Rel::OneToOne, i, 1)),
            _ => {}
        }
        i += 1;
    }
    None
}

/// 把 `a.(x, y) > b.(p, q)` 展开成逐列 refs 追加进 schema。
fn push_ref(
    sc: &mut Schema,
    lhs: &str,
    rhs: &str,
    rel: Rel,
    on_delete: Option<String>,
    on_update: Option<String>,
) {
    let (lt, lcols) = split_side(lhs);
    let (rt, rcols) = split_side(rhs);
    // 多对多存一次，其余逐列配对
    if lcols.len() > 1 || rcols.len() > 1 {
        let n = lcols.len().min(rcols.len());
        for k in 0..n {
            sc.refs.push(Ref {
                from_table: lt.clone(),
                from_col: lcols[k].clone(),
                to_table: rt.clone(),
                to_col: rcols[k].clone(),
                rel,
                on_delete: on_delete.clone(),
                on_update: on_update.clone(),
            });
        }
    } else if let (Some(l), Some(r)) = (lcols.first(), rcols.first()) {
        sc.refs.push(Ref {
            from_table: lt,
            from_col: l.clone(),
            to_table: rt,
            to_col: r.clone(),
            rel,
            on_delete,
            on_update,
        });
    }
}

/// 拆 `table.(a, b)` 为 (表名, 列列表)。
fn split_side(s: &str) -> (String, Vec<String>) {
    let s = s.trim();
    if let Some(i) = s.find(".(") {
        let tbl = unquote(s[..i].trim());
        let inner = s[i + 2..].trim_end_matches(')');
        let cols = split_top(inner, ',')
            .into_iter()
            .map(|c| unquote(&c))
            .collect();
        (tbl, cols)
    } else if let Some((t, c)) = s.rsplit_once('.') {
        (unquote(t), vec![unquote(c)])
    } else {
        (unquote(s), Vec::new())
    }
}