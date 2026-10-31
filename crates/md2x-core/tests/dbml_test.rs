//! 解析与渲染的回归测试。
//!
//! 每个用例对应一类曾在实现中踩过的真实坑，失败即代表回归。
//!
//! @author yujinping

use md2x_core::dbml::{layout, parser, render};

/// 断言：`serial` 主键被 `int`/`bigint` 外键引用必须能解析。
///
/// 这是 dbml-rs 直接报 "Mismatched foreign key type" 的场景，
/// 也是真实 schema 里最常见的写法，绝不能因为类型不完全相等而失败。
#[test]
fn serial_pk_referenced_by_int_fk() {
    let src = "Table a { id bigserial [pk] }\nTable b { a_id bigint [ref: > a.id] }";
    let sc = parser::parse(src);
    assert_eq!(sc.tables.len(), 2);
    assert_eq!(sc.refs.len(), 1);
    assert_eq!(sc.refs[0].to_table, "a");
}

/// 单行内多列：`{ id int [pk] status int }` 必须解析出两列。
#[test]
fn multiple_columns_on_one_line() {
    let sc = parser::parse("Table a { id int [pk] status int }");
    assert_eq!(sc.tables.len(), 1);
    assert_eq!(sc.tables[0].cols.len(), 2, "单行表应解析出 2 列");
    assert_eq!(sc.tables[0].cols[0].name, "id");
    assert!(sc.tables[0].cols[0].pk);
    assert_eq!(sc.tables[0].cols[1].name, "status");
}

/// 顶层 `Ref:` 语句（关键字后紧跟冒号）。
#[test]
fn top_level_ref_with_colon() {
    let sc = parser::parse("Table a { id int [pk] }\nTable b { a int }\nRef: b.a > a.id");
    assert_eq!(sc.refs.len(), 1);
    assert_eq!(sc.refs[0].from_col, "a");
}

/// 行内 ref 与顶层 Ref 指向同一外键时只保留一条。
#[test]
fn duplicate_inline_and_top_level_ref_is_deduped() {
    let src = "Table a { id int [pk] }\nTable b { a int [ref: > a.id] }\nRef: b.a > a.id";
    let sc = parser::parse(src);
    assert_eq!(sc.refs.len(), 1, "重复表达的外键应去重");
}

/// TableGroup 成员应回填到表的 group 字段。
#[test]
fn table_group_assignment() {
    let src = "TableGroup core { a }\nTable a { id int [pk] }";
    let sc = parser::parse(src);
    assert_eq!(sc.groups.len(), 1);
    assert_eq!(sc.tables[0].group.as_deref(), Some("core"));
}

/// 多行 `'''...'''` 注释必须完整保留，不能只剩最后一行。
#[test]
fn multiline_project_note_is_preserved() {
    let src = "Project p {\n  Note: '''\n  # 标题\n  正文说明\n  '''\n}";
    let sc = parser::parse(src);
    assert!(sc.project_note.contains("标题"), "注释丢失: {:?}", sc.project_note);
    assert!(sc.project_note.contains("正文说明"), "注释丢失: {:?}", sc.project_note);
}

/// Record 块是示例数据，不能被当成表列解析。
#[test]
fn record_block_is_ignored() {
    let src = "Table a { id int [pk]\n status int }\nRecord a {\n 1, 2\n}";
    let sc = parser::parse(src);
    assert_eq!(sc.tables[0].cols.len(), 2, "Record 被误当作列");
}

/// 索引块：复合索引、唯一索引、索引名与类型。
#[test]
fn indexes_are_parsed() {
    let src = "Table a {\n id int [pk]\n b text\n Indexes {\n  (id, b) [name: 'idx_ab', type: btree]\n  b [unique]\n }\n}";
    let sc = parser::parse(src);
    let idx = &sc.tables[0].indexes;
    assert_eq!(idx.len(), 2);
    assert!(idx.iter().any(|i| i.cols.len() == 2 && i.name.as_deref() == Some("idx_ab")));
    assert!(idx.iter().any(|i| i.unique && i.cols == vec!["b"]));
}

/// 小写 `indexes` 块必须被识别。
///
/// DBML 规范写小写，早期实现只认 `Indexes`，小写整块被当成列，
/// 渲染出 `indexes` / `{` / `(a, b)` 三行假字段。
#[test]
fn lowercase_indexes_block_is_recognized() {
    let src = "Table a {\n id int [pk]\n b text\n indexes {\n  (id, b)\n  b [unique]\n }\n}";
    let sc = parser::parse(src);
    assert_eq!(sc.tables[0].indexes.len(), 2, "小写 indexes 块未被识别");
    assert_eq!(sc.tables[0].cols.len(), 2, "索引块内容被误当作列");
}

/// 列级 note 的多种写法都要能取出。
///
/// 真实 schema 里 note 位置不固定（pk 前后、default 后都可能），
/// 内容常带冒号、逗号、方括号，早期按 `,` 粗暴切分时会被截断。
#[test]
fn column_note_variants_are_parsed() {
    let src = "Table t {\n\
        a int [note: '带冒号: http://x.y']\n\
        b int [note: '带逗号, 还有 [方括号]']\n\
        c int [pk, note: '在 pk 之后']\n\
        d int [note: '在 not null 之前', not null]\n\
        e int [Note: '大写 Note']\n\
        f int [note: \"双引号备注\"]\n\
        g int [not null, default: 0, note: '在 default 之后']\n\
    }";
    let sc = parser::parse(src);
    let notes: Vec<String> = sc.tables[0].cols.iter().map(|c| c.note.clone()).collect();
    assert_eq!(notes[0], "带冒号: http://x.y");
    assert_eq!(notes[1], "带逗号, 还有 [方括号]");
    assert_eq!(notes[2], "在 pk 之后");
    assert_eq!(notes[3], "在 not null 之前");
    assert_eq!(notes[4], "大写 Note");
    assert_eq!(notes[5], "双引号备注");
    assert_eq!(notes[6], "在 default 之后");
    // note 不应干扰同一设置块里的其他约束
    assert!(sc.tables[0].cols[2].pk, "note 排在 pk 之后不应吞掉 pk");
    assert!(sc.tables[0].cols[3].not_null, "note 排在前面不应吞掉 not null");
}

/// 组合外键 `ref: > a.(x, y)` 应展开为逐列配对。
#[test]
fn composite_ref_is_expanded() {
    let src = "Table a {\n x int\n y int\n Indexes { (x, y) [pk] }\n}\nTable b {\n x int\n y int\n ref: > a.(x, y)\n}";
    let sc = parser::parse(src);
    assert_eq!(sc.refs.len(), 2, "组合外键应展开成 2 条");
}

/// 注释里的 `//` 不能被当作注释剥离（URL 场景）。
#[test]
fn url_in_default_is_not_stripped() {
    let src = "Table a { u varchar [default: 'http://x.y'] }";
    let sc = parser::parse(src);
    let d = sc.tables[0].cols[0].default.clone().unwrap_or_default();
    assert!(d.contains("http://"), "URL 被破坏: {d}");
}

/// 渲染产物必须零外部依赖：不得出现 http(s) 的 src/href 引用。
#[test]
fn rendered_html_has_no_external_assets() {
    let src = "Table a { id int [pk] }";
    let sc = parser::parse(src);
    let html = render::render_body(&sc);
    for bad in ["src=\"http", "href=\"http", "src=\"//", "href=\"//"] {
        assert!(!html.contains(bad), "存在外部资源引用: {bad}");
    }
    assert!(html.contains("<style>"), "样式应内联");
    assert!(html.contains("<script>"), "脚本应内联");
}

/// 画布 SVG 应包含每张表与每条外键的绘制结果。
#[test]
fn canvas_contains_all_tables_and_edges() {
    let src = "Table a { id int [pk] }\nTable b { a int [ref: > a.id] }";
    let sc = parser::parse(src);
    let (svg, w, h) = layout::build_svg(&sc);
    assert_eq!(svg.matches("class=\"dx-tbl-hit\"").count(), 2, "表节点数不符");
    assert!(w > 0.0 && h > 0.0);
    assert!(svg.contains("<svg"), "应产出 SVG");
}

/// 空 schema 不应 panic，应给出可读提示。
#[test]
fn empty_schema_does_not_panic() {
    let sc = parser::parse("");
    let (svg, _, _) = layout::build_svg(&sc);
    assert!(svg.contains("没有可渲染的表"));
    let html = render::render_body(&sc);
    assert!(html.contains("dx-panel-dict"), "应产出字典面板容器");
}

/// HTML 转义：表名/注释里的尖括号不能破坏结构。
#[test]
fn html_escaping() {
    assert_eq!(render::esc("<a>&\"'"), "&lt;a&gt;&amp;&quot;&#39;");
}
// ==================== Markdown 导出 ====================

/// 数据字典式 Markdown：表名、字段表、外键、索引、枚举都要出现。
#[test]
fn markdown_export_contains_dictionary() {
    let src = "Project P {\n database_type: 'PostgreSQL'\n}\n\
Table tenants {\n id bigserial [pk, note: '主键']\n name varchar(20) [not null]\n\
 Indexes {\n  name [unique]\n }\n}\n\
Table users {\n id bigserial [pk]\n tid bigint [ref: > tenants.id]\n}\n\
Enum role {\n admin\n member\n}";
    let md = md2x_core::dbml::to_markdown(src, "示例库");

    assert!(md.starts_with("# 示例库"), "应有一级标题");
    assert!(md.contains("**数据库** PostgreSQL"), "应有数据库类型");
    assert!(md.contains("### tenants"), "应有表小节");
    assert!(md.contains("| `id` | `bigserial` |"), "应有字段行");
    assert!(md.contains("PK"), "标出主键");
    assert!(md.contains("主键"), "字段备注应保留");
    assert!(md.contains("→ `tenants.id`"), "外键应指向父表");
    assert!(md.contains("**索引**"), "应有索引段");
    assert!(md.contains("## 枚举类型"), "应有枚举段");
    assert!(md.contains("`admin`"), "枚举值应保留");
}

/// 备注里的竖线必须转义，否则会把表格列切断。
#[test]
fn markdown_export_escapes_pipe_in_note() {
    let src = "Table t {\n a int [note: 'A | B']\n}";
    let md = md2x_core::dbml::to_markdown(src, "T");
    // 关键断言：出现转义序列，而不是裸竖线把单元格切开
    assert!(md.contains("A \\| B"), "竖线应转义: {md}");
    // 未转义的竖线数应恰好等于表头列数 + 1（7 列 → 8 个）
    let row = md.lines().find(|l| l.contains("`a`")).unwrap();
    let raw = row.matches('|').count() - row.matches("\\|").count();
    assert_eq!(raw, 8, "未转义竖线数应保持 7 列: {row}");
}

/// 无备注时用 — 占位，不产生空单元格。
#[test]
fn markdown_export_uses_placeholder_for_empty() {
    let src = "Table t {\n a int\n}";
    let md = md2x_core::dbml::to_markdown(src, "T");
    let row = md.lines().find(|l| l.contains("`a`")).unwrap();
    // 约束、默认值、外键、说明四处均为空，各出一个占位
    assert_eq!(
        row.matches('—').count(),
        4,
        "空列应统一用占位符: {row}"
    );
    assert!(!row.contains("| |"), "不应出现空单元格: {row}");
}

/// 空 schema 的 Markdown 也不应 panic。
#[test]
fn markdown_export_empty_schema() {
    let md = md2x_core::dbml::to_markdown("", "空");
    assert!(md.contains("# 空"));
    assert!(md.contains("**表** 0"));
}

/// HTML 片段不得自带 html/head/body 骨架——它要嵌进模版。
#[test]
fn body_fragment_has_no_document_skeleton() {
    let html = md2x_core::dbml::to_html_body("Table a { id int [pk] }");
    for bad in ["<!doctype", "<html", "<head", "</head", "<body", "</body"] {
        assert!(!html.contains(bad), "片段不应含文档骨架: {bad}");
    }
}

/// 片段样式必须引用模版变量而非写死配色。
#[test]
fn body_fragment_uses_theme_vars() {
    let html = md2x_core::dbml::to_html_body("Table a { id int [pk] }");
    assert!(html.contains("var(--accent"), "应使用模版主色变量");
    assert!(html.contains("var(--bg-panel"), "应使用模版背景变量");
    assert!(html.contains("var(--border"), "应使用模版边框变量");
}

/// 类名统一 `dx-` 前缀，避免与 markdown 正文样式互相污染。
#[test]
fn body_fragment_classes_are_prefixed() {
    let html = md2x_core::dbml::to_html_body("Table a { id int [pk] }");
    // 抽出 class="..." 里的每个类名，逐个检查前缀
    let mut checked = 0;
    for seg in html.split("class=\"").skip(1) {
        let Some(end) = seg.find('"') else { continue };
        for cls in seg[..end].split_whitespace() {
            assert!(
                cls.starts_with("dx-"),
                "类名缺少 dx- 前缀: {cls}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "应至少有类名可检查");
}

/// 纯链式 4 表：分层会排成 1×4 的长条，必须改走网格。
///
/// 回归点：曾按「表数 > 12」才切网格，4 张表串成 4 层时产出 1480×298 的横条，
/// 适应窗口后缩到 52%，表内 10px 列名全糊，上下还空着两大片。
#[test]
fn chain_schema_uses_grid_not_horizontal_strip() {
    let src = "\
Table a { id int [pk] }
Table b { id int [pk] a_id int [ref: > a.id] }
Table c { id int [pk] b_id int [ref: > b.id] }
Table d { id int [pk] c_id int [ref: > c.id] }";
    let sc = parser::parse(src);
    assert_eq!(sc.tables.len(), 4);
    let (_, w, h) = layout::build_svg(&sc);
    let ratio = w / h;
    // 退化分层布局是 1480×298（比例 4.97）；网格布局是 720×292（比例 2.47）。
    // 3.0 是两者之间的分界，能区分「横条」与「正常铺开」。
    assert!(
        ratio < 3.0,
        "纯链式 schema 不应排成横条，实际 {w}x{h} 宽高比 {ratio:.2}"
    );
}

/// 星型 schema（一父多子）分层效果好，应保持分层而非退化成网格。
///
/// 反向断言：宽高比闸门不能把真正需要「从左到右依赖流」的图也推翻。
#[test]
fn star_schema_keeps_layer_layout() {
    let src = "\
Table hub { id int [pk] }
Table s1 { id int [pk] hub_id int [ref: > hub.id] }
Table s2 { id int [pk] hub_id int [ref: > hub.id] }
Table s3 { id int [pk] hub_id int [ref: > hub.id] }";
    let sc = parser::parse(src);
    let (_, w, h) = layout::build_svg(&sc);
    // 2 层 × 2 列：宽度大于高度，但比例在合理区间内
    assert!(w > h, "星型 schema 应横向铺开，实际 {w}x{h}");
    assert!(w / h < 2.5, "不应拉成长条，实际 {w}x{h}");
}

/// 单表也要有合理画布，不能因为「层数 1」触发异常分支。
#[test]
fn single_table_canvas_is_sane() {
    let sc = parser::parse("Table only { id int [pk] name varchar }");
    let (_, w, h) = layout::build_svg(&sc);
    assert!(w > 260.0 && h > 40.0, "单表画布过小: {w}x{h}");
    // 单表节点宽 260、高 ~78，加上四周 PAD(40) 即 340×158，比例 2.15 属正常
    assert!(w / h < 3.0, "单表不应是极端长条: {w}x{h}");
}

/// 表别名以 `as x` 形式右对齐渲染，且不越过节点宽度。
#[test]
fn table_alias_is_rendered_with_as_prefix() {
    let sc = parser::parse("Table users as U { id int [pk] }");
    let (svg, w, _) = layout::build_svg(&sc);
    assert!(svg.contains(">as U<"), "别名应带 as 前缀: 未找到 'as U'");
    // 右对齐锚点在节点内边距上
    for seg in svg.split("<text").filter(|s| s.contains("as U")) {
        assert!(seg.contains("text-anchor=\"end\""), "别名应右对齐: {seg}");
    }
    // 别名 x 坐标不得超出画布
    assert!(w >= 260.0);
}

/// 长别名以右对齐锚点渲染，文字向左延伸，不会被写死的 x 挤出节点外。
///
/// 回归点：原代码写 `x = NODE_W - 40`（即 220）且未设 text-anchor，
/// 别名按左对齐向右铺，长别名直接冲出节点右边框。改为 text-anchor="end"
/// 锚在右内边距（NODE_W - 12 = 248）后，文字从锚点往左排，永远不越界。
#[test]
fn long_alias_stays_inside_node() {
    let sc = parser::parse("Table very_long_table_name_here as VERYLONG { id int [pk] }");
    let (svg, _, _) = layout::build_svg(&sc);
    let seg = svg
        .split("<text")
        .find(|s| s.contains("VERYLONG"))
        .expect("应渲染长别名");
    assert!(
        seg.contains("text-anchor=\"end\""),
        "长别名必须右对齐，否则向右溢出节点: {seg}"
    );
    // 锚点 x 落在节点右内边距上（左侧 PAD=40 + NODE_W-12=248 → 288）
    let x: f64 = seg
        .split("x=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .and_then(|s| s.parse().ok())
        .expect("别名应有 x 坐标");
    let node_right = 40.0 + 260.0 - 12.0;
    assert!(
        (x - node_right).abs() < 0.5,
        "右对齐锚点应在节点右内边距 {node_right}，实际 {x}"
    );
}

/// 多行 `Note: '''...'''` 必须去公共缩进。
///
/// 曾出的问题：note 每行带着源文件的 4 个空格被原样写进 Markdown，
/// CommonMark 把「4 空格开头」判成缩进代码块，代码块内不解析行内格式，
/// 于是 `dbml → md → docx` 的 Word 正文里出现裸的 `**API Key**` 和
/// 反引号。HTML 链路因为 `md_inline` 逐行处理恰好躲过了这个坑，
/// 只有 DOCX 暴露出来——正是「同一个中间体喂两个消费者」才照出的差异。
#[test]
fn multi_line_note_is_dedented() {
    let src = "Project demo {\n  Note: '''\n    # 标题\n\n    存放 **API Key**。\n    '''\n}";
    let sc = parser::parse(src);
    assert!(
        !sc.project_note.contains("    存放"),
        "note 不应保留源文件缩进: {:?}",
        sc.project_note
    );
    assert!(
        sc.project_note.contains("存放 **API Key**。"),
        "行内 Markdown 语法必须保留，供下游解析: {:?}",
        sc.project_note
    );
    assert!(sc.project_note.contains("# 标题"), "标题行不应丢");
}

/// note 里的相对缩进（嵌套结构）必须保留，不能被 dedent 压平。
#[test]
fn note_keeps_relative_indentation() {
    let src = "Project demo {\n  Note: '''\n    第一段\n\n    - 列表项\n      - 子项\n    '''\n}";
    let sc = parser::parse(src);
    assert!(
        sc.project_note.contains("  - 子项"),
        "嵌套列表的相对缩进必须保留: {:?}",
        sc.project_note
    );
}

/// `dbml → md` 短路产物里，多行 note 不能变成缩进代码块。
///
/// 直接断言最终 Markdown，而不是只断言 note 字段：这条链路是
/// `to_markdown` 拼装出来的，note 之外任何一处补缩进都会重新踩坑。
#[test]
fn markdown_shortcut_does_not_emit_indented_code_block() {
    let src = "Project demo {\n  Note: '''\n    存放 **API Key**。\n    '''\n}\nTable t { id int [pk] }";
    let md = md2x_core::dbml::to_markdown(src, "demo");
    for line in md.lines() {
        // 引用块前缀 `> ` 之后跟 4 空格，就是缩进代码块
        let body = line.trim_start_matches('>');
        let body = body.strip_prefix(' ').unwrap_or(body);
        assert!(
            !body.starts_with("    "),
            "Markdown 不应出现 4 空格缩进块，否则行内格式不解析: {line:?}"
        );
    }
    assert!(md.contains("**API Key**"), "粗体标记应保留给下游渲染");
}

/// 数据字典表格必须自带「不被模板通用表格样式压垮」的样式。
///
/// 这两条是配套的，缺一条就退化：
/// - 模版对 `.markdown-body table` 有 `display:block!important`（为了正文表格
///   横向滚动），不按回去的话 `<table>` 不再是表格，列宽不再按内容分配；
/// - 表格若被限制在容器宽度内（`width:100%` / `max-width:100%`），浏览器为
///   塞下 7 列会压扁列宽，于是 `varchar(255)` 在括号处断行、`created_at`
///   断成 `created_a|t`，`nowrap` 也救不回来。
///
/// 正确形态是 `width:max-content` + `max-width:none`，超宽交给外层滚动容器。
#[test]
fn dictionary_table_style_wins_over_template_defaults() {
    let src = "Table t { id int [pk] }";
    let body = render::render_body(&parser::parse(src));
    assert!(
        body.contains(".markdown-body table.dx-cols{display:table!important"),
        "必须用更高特异性把 display 按回 table"
    );
    assert!(
        body.contains("width:max-content!important"),
        "表格宽度必须按内容撑开，否则列会被压扁导致词中断行"
    );
    assert!(
        body.contains("max-width:none!important"),
        "不能给表格设 max-width，否则 nowrap 也挡不住断行"
    );
    // 滚动容器存在，超宽表格才能在卡片内横向滚动而不撑破布局
    assert!(
        body.contains("<div class=\"dx-tw\"><table class=\"dx-cols\">"),
        "表格应包在横向滚动容器里"
    );
    assert!(body.contains(".dx-tw{width:100%;overflow-x:auto"), "缺少滚动容器样式");
}

/// note 里的 `# 标题` 不能原样露出井号。
#[test]
fn note_heading_is_rendered_not_shown_as_hash() {
    let src = "Project d {\n  Note: '''\n    # 网关库\n    正文\n    '''\n}\nTable t { id int }";
    let body = render::render_body(&parser::parse(src));
    assert!(body.contains("dx-nh"), "note 标题应有专属样式类");
    assert!(
        !body.contains("># 网关库<") && !body.contains("<br># 网关库"),
        "不应把赤裸的 # 输出到正文"
    );
}

/// note 里的行内代码要按模板变量上色，不能落到模板的固定浅灰底。
#[test]
fn note_inline_code_uses_template_vars() {
    let src = "Project d {\n  Note: '''带 `tenant_id` 的说明'''\n}\nTable t { id int }";
    let body = render::render_body(&parser::parse(src));
    // 找到 note 的样式规则，确认它明确指定了背景与前景变量
    let seg = body
        .split(".dx-pnote code,.dx-tnote code{")
        .nth(1)
        .expect("应有 note 内行内代码的样式规则");
    let rule = seg.split('}').next().unwrap();
    assert!(rule.contains("var(--code-inline-bg"), "应使用模板的背景变量: {rule}");
    assert!(rule.contains("var(--fg"), "应使用模板的前景变量: {rule}");
}

/// `TableGroup` 写在 `Table` 之后时，分组归属仍要回填成功。
///
/// DBML 官方示例就是「先所有 Table，最后 TableGroup」的顺序。曾把回填
/// 放在 `parse_table` 内部查 `sc.groups`——解析单张表时分组表还是空的，
/// 一律查不到，表现为「源文件里写了 TableGroup、页面上却没有分组标题」。
#[test]
fn table_group_after_tables_still_assigns() {
    let src = "Table users { id int [pk] }\nTable posts { id int [pk] }\n\
               TableGroup 业务 { users, posts }";
    let sc = parser::parse(src);
    assert_eq!(sc.groups.len(), 1, "分组应被解析");
    assert_eq!(sc.groups[0].name, "业务");
    assert_eq!(sc.groups[0].tables.len(), 2, "单行逗号分隔的成员要拆开");
    for t in &sc.tables {
        assert_eq!(
            t.group.as_deref(),
            Some("业务"),
            "{} 的 group 应回填为「业务」",
            t.name
        );
    }
}

/// 逐行写的分组同样要工作（TokenGateway 这类真实 schema 的写法）。
#[test]
fn table_group_multiline_members_still_assign() {
    let src = "TableGroup 核心 {\n    users\n    posts\n}\n\
               Table users { id int [pk] }\nTable posts { id int [pk] }";
    let sc = parser::parse(src);
    assert_eq!(sc.groups[0].tables.len(), 2);
    assert_eq!(sc.tables[0].group.as_deref(), Some("核心"));
}

/// 分组名要真的出现在 HTML 里，而不是只停在模型层。
#[test]
fn group_name_appears_in_rendered_html() {
    let src = "Table users { id int [pk] }\nTableGroup 业务 { users }";
    let body = render::render_body(&parser::parse(src));
    assert!(body.contains("业务"), "分组标题应出现在 HTML 中");
    assert!(body.contains("dx-gsec"), "分组标题应使用专属样式类");
}

/// 画布节点必须标出 PK / FK / UQ，而不是只用一条红条表示主键。
///
/// 画布是「看结构关系」的视图：缺了 FK 标识，只能靠连线的落点去猜
/// 哪个字段是外键。这里同时断言三种徽章都出现、且颜色类各不相同。
#[test]
fn canvas_marks_pk_fk_uq_on_columns() {
    let src = "Table users { id int [pk] }\n\
               Table profile {\n  uid int [ref: > users.id, unique]\n}";
    let body = render::render_body(&parser::parse(src));
    for cls in ["dx-tag-pk", "dx-tag-fk", "dx-tag-uq"] {
        assert!(body.contains(cls), "画布缺少 {cls} 徽章");
    }
    // 三种徽章各自独立的填充色（红 / 绿 / 紫），不能共用
    for (cls, color) in [
        (".dx-tag-pk{fill:#cf222e}", "#cf222e"),
        (".dx-tag-fk{fill:#1a7f37}", "#1a7f37"),
        (".dx-tag-uq{fill:#8250df}", "#8250df"),
    ] {
        assert!(body.contains(cls), "缺少 {color} 徽章样式");
    }
}

/// 一张表里只要有一列带徽章，所有列名都从同一 x 起排。
///
/// 曾按「这一列有没有徽章」逐行决定列名起点：有徽章的从槽位右端开始、
/// 没徽章的贴左边距，同一张表里列名上下错开整整一个槽位，呈锯齿状，
/// 扫读时眼睛要不断重新找起点。
#[test]
fn canvas_column_names_share_one_left_edge() {
    // users 的 id 是 PK、tenant_id 是 FK；email / display 没有徽章。
    // 节点在 SVG 里是 `<g class="dx-tbl-hit">` 包起来的，所以按 g 切段，
    // 在**同一张表内**比较列名起点——跨表比较没有意义（每张表 x 不同）。
    let src = "Table tenants { id int [pk] }\n\
               Table users {\n  id int [pk]\n  tenant_id int [ref: > tenants.id]\n\
               \n  email varchar\n  display varchar\n}";
    let body = render::render_body(&parser::parse(src));

    // 找到含 email 的那个节点段（即 users）
    let seg = body
        .split("<g class=\"dx-tbl-hit\"")
        .find(|s| s.contains("dx-node-col\">email"))
        .expect("应能找到 users 节点");

    let mut xs: Vec<f64> = Vec::new();
    for part in seg.split("<text x=\"").skip(1) {
        let Some(num) = part.split('"').next().and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        if part.contains("dx-node-col\">") {
            xs.push(num);
        }
    }
    assert_eq!(xs.len(), 4, "users 应有四个列名: {xs:?}");
    let first = xs[0];
    for x in &xs {
        assert!(
            (x - first).abs() < 0.5,
            "同一张表的列名起点必须一致，实际 {xs:?}（首个 {first}）"
        );
    }
}

/// 画布表头在表名下方显示表级注释，且中英文都支持。
///
/// 用户要求的原话是「中英文表名需要把 COMMENT 的名字也加上」——
/// 表名本身可能是英文（`subscriptions`），可读性靠注释补；
/// 注释也可能是中文。两种来源（`Note:` / `[comment: '...']`）都要认。
#[test]
fn canvas_shows_table_comment_below_name() {
    let src = "Table plans [comment: 'Plans'] {\n id int [pk]\n}\n\
               Table subs {\n id int [pk]\n Note: '租户订阅记录'\n}";
    let body = render::render_body(&parser::parse(src));
    assert!(
        body.contains("class=\"dx-node-cmt\">Plans<"),
        "英文 comment 应显示在画布表头"
    );
    assert!(
        body.contains("class=\"dx-node-cmt\">租户订阅记录<"),
        "中文 Note 应显示在画布表头"
    );
}

/// 超过 10 个汉字的注释必须截断并加 `...`，否则会顶出节点边界。
///
/// 中文按全角计 2 个单位、预算 20 单位，等价于 10 个汉字。
#[test]
fn canvas_comment_truncated_to_ten_chars_with_ellipsis() {
    let long = "一二三四五六七八九十十一十二";
    let src = format!("Table t {{\n id int [pk]\n Note: '{long}'\n}}");
    let body = render::render_body(&parser::parse(&src));
    assert!(
        body.contains("class=\"dx-node-cmt\">一二三四五六七八九十...<"),
        "超长中文注释应截断为前 10 字 + ..."
    );
}

/// 英文字符只占 1 单位，预算内可显示约 20 个字符。
///
/// 这一条是为了防止「按字符数截断」的回归：若按 10 个**字符**截，
/// 英文注释会在 10 个字母处被砍断，白白浪费半个表头的宽度。
#[test]
fn canvas_comment_gives_ascii_more_room_than_cjk() {
    // 19 个字符，未超 20 单位预算 → 完整显示
    let short_en = "Subscription plans";
    let src = format!("Table t [comment: '{short_en}'] {{\n id int [pk]\n}}");
    let body = render::render_body(&parser::parse(&src));
    assert!(
        body.contains(&format!("class=\"dx-node-cmt\">{short_en}<")),
        "19 个 ASCII 字符在 20 单位预算内，不应被截断"
    );

    // 30 个字符 → 截断到 20
    let long_en = "abcdefghijklmnopqrstuvwxyz1234";
    let src = format!("Table t [comment: '{long_en}'] {{\n id int [pk]\n}}");
    let body = render::render_body(&parser::parse(&src));
    assert!(
        body.contains("class=\"dx-node-cmt\">abcdefghijklmnopqrst...<"),
        "ASCII 超长注释应截断为前 20 字符 + ..."
    );
}

/// 恰好 10 个字不截断：边界不能多打三个点。
#[test]
fn canvas_comment_exactly_ten_chars_is_not_truncated() {
    let exact = "一二三四五六七八九十";
    let src = format!("Table t {{\n id int [pk]\n Note: '{exact}'\n}}");
    let body = render::render_body(&parser::parse(&src));
    assert!(
        body.contains(&format!("class=\"dx-node-cmt\">{exact}<")),
        "10 字整不应截断"
    );
}

/// 注释里的 Markdown 装饰不应漏到画布表头上。
///
/// 多行 `Note: '''# 标题 ...'''` 是 DBML 的常见写法，
/// 原样显示会在表头出现一个赤裸的 `#`。
#[test]
fn canvas_comment_strips_markdown_markers() {
    let src = "Table t {\n id int [pk]\n Note: '''\n    # 租户主表\n\n    详细说明\n '''\n}";
    let body = render::render_body(&parser::parse(src));
    assert!(
        body.contains("class=\"dx-node-cmt\">租户主表<"),
        "应取首个非空行并去掉 # 前缀"
    );
    assert!(!body.contains("dx-node-cmt\">#"), "表头不应出现裸井号");
}

/// 连线不得穿过任何表节点。
///
/// 这是「线穿表」的机械复现：造一个三列场景，让最右表引用最左表，
/// 中间那张表正好挡在直线路径上。旧实现用贝塞尔直连，曲线必然穿中间表。
#[test]
fn canvas_edges_never_cross_table_boxes() {
    // 三张表分在三个列槽位，c 引用 a，b 挡在中间
    let src = "Table a { id int [pk] }\n\
               Table b { id int [pk] }\n\
               Table c { id int [pk] a_id int [ref: > a.id] }";
    let sc = parser::parse(src);
    let (svg, _, _) = layout::build_svg(&sc);

    // 收集节点框
    let mut boxes: Vec<(f64, f64, f64, f64)> = Vec::new();
    for seg in svg.split("<g class=\"dx-tbl-hit\"").skip(1) {
        let Some(rect) = seg.split("<rect ").nth(1) else {
            continue;
        };
        let get = |k: &str| -> Option<f64> {
            let i = rect.find(&format!("{k}=\""))?;
            let rest = &rect[i + k.len() + 2..];
            rest.split('"').next()?.parse().ok()
        };
        if let (Some(x), Some(y), Some(w), Some(h)) =
            (get("x"), get("y"), get("width"), get("height"))
        {
            boxes.push((x, y, w, h));
        }
    }
    assert_eq!(boxes.len(), 3, "应渲染三张表");

    // 逐条路径采样，任何一点落进框内（内缩 1px 容差）即失败
    let mut crossed = Vec::new();
    for d in extract_edge_paths(&svg) {
        for (x, y) in sample_polyline(&d) {
            for (bx, by, bw, bh) in &boxes {
                if x > bx + 1.0 && x < bx + bw - 1.0 && y > by + 1.0 && y < by + bh - 1.0 {
                    crossed.push(d.clone());
                }
            }
        }
    }
    assert!(crossed.is_empty(), "连线穿过了表节点: {crossed:?}");
}

/// 同列的两张表之间的外键，不得从源表身体里穿回去。
///
/// 旧实现用 `x1 <= x2` 判断出线侧，同列时两者 x 相同，恒判为「从右侧出」，
/// 于是线从源表右边缘出发、却要往左走到目标，整条线横穿源表自身。
#[test]
fn same_column_edge_does_not_cross_its_own_table() {
    // 网格布局下相邻索引的表同列（8 张表 → 3 列，index 0 和 3 同列）
    let mut src = String::new();
    for i in 0..9 {
        src.push_str(&format!("Table t{i} {{ id int [pk] }}\n"));
    }
    // t3 与 t0 同在 col 0，让 t3 引用 t0
    src.push_str("Ref: t3.id > t0.id");
    let sc = parser::parse(&src);
    let (svg, _, _) = layout::build_svg(&sc);

    let mut boxes: Vec<(f64, f64, f64, f64)> = Vec::new();
    for seg in svg.split("<g class=\"dx-tbl-hit\"").skip(1) {
        let Some(rect) = seg.split("<rect ").nth(1) else {
            continue;
        };
        let get = |k: &str| -> Option<f64> {
            let i = rect.find(&format!("{k}=\""))?;
            let rest = &rect[i + k.len() + 2..];
            rest.split('"').next()?.parse().ok()
        };
        if let (Some(x), Some(y), Some(w), Some(h)) =
            (get("x"), get("y"), get("width"), get("height"))
        {
            boxes.push((x, y, w, h));
        }
    }
    for d in extract_edge_paths(&svg) {
        for (x, y) in sample_polyline(&d) {
            for (bx, by, bw, bh) in &boxes {
                assert!(
                    !(x > bx + 1.0 && x < bx + bw - 1.0 && y > by + 1.0 && y < by + bh - 1.0),
                    "连线 {d} 穿过了表框 ({bx},{by},{bw},{bh})"
                );
            }
        }
    }
}

/// 从画布 SVG 里取出所有连线的 `d`。
fn extract_edge_paths(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find("<path d=\"") {
        rest = &rest[i + 9..];
        let Some(j) = rest.find('"') else { break };
        let d = &rest[..j];
        // 只有连线带 class="dx-edge"，箭头 marker 里的 path 没有
        if rest[j..].starts_with("\" class=\"dx-edge\"") {
            out.push(d.to_string());
        }
    }
    out
}

/// 按 1px 步长采样折线（本实现产出的全是正交段，采样不会漏判）。
fn sample_polyline(d: &str) -> Vec<(f64, f64)> {
    let toks: Vec<&str> = d.split_whitespace().collect();
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let mut p = (0.0f64, 0.0f64);
    let mut i = 0;
    while i < toks.len() {
        let t = toks[i];
        if let Some(v) = t.strip_prefix('M') {
            let x: f64 = v.parse().unwrap_or(0.0);
            let y: f64 = toks.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            p = (x, y);
            pts.push(p);
            i += 2;
        } else if let Some(v) = t.strip_prefix('H') {
            p.0 = v.parse().unwrap_or(p.0);
            pts.push(p);
            i += 1;
        } else if let Some(v) = t.strip_prefix('V') {
            p.1 = v.parse().unwrap_or(p.1);
            pts.push(p);
            i += 1;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        let n = ((x1 - x0).abs() + (y1 - y0).abs()).ceil() as usize + 1;
        for s in 0..=n {
            let t = s as f64 / n as f64;
            out.push((x0 + (x1 - x0) * t, y0 + (y1 - y0) * t));
        }
    }
    out
}

/// 大 schema 的画布不能退化成「顶部梳状线团」。
///
/// 这是 59 表真实 schema（token-gateway）暴露的问题：早期布局把每层限高
/// 1100px、超了就另起子列，59 张表被摊成 21 列，55 条边里 44 条被迫走顶部
/// 总线，在 400px 高的区域里排成密不透风的横线，图彻底读不出来。
///
/// 现在把「总线边数」压到极低（实测 59 表为 0 条），以及画布宽高比接近容器。
/// 用 40 张表 + 茂密外键构造一个规模相当、结构相似的夹具。
#[test]
fn large_schema_does_not_pile_edges_onto_top_bus() {
    let mut src = String::new();
    // 6 张主表（父），34 张子表分成两批挂到不同主表上
    for i in 0..6 {
        src.push_str(&format!("Table p{i} {{\n  id int [pk]\n  name varchar\n}}\n"));
    }
    for i in 0..34 {
        let host = i % 6;
        src.push_str(&format!(
            "Table c{i} {{\n  id int [pk]\n  p_id int [ref: > p{host}.id]\n  name varchar\n  created_at timestamp\n}}\n"
        ));
    }
    let sc = parser::parse(&src);
    assert_eq!(sc.tables.len(), 40);
    let (svg, w, h) = layout::build_svg(&sc);

    // 宽高比：不能是横条（4.78:1 那种）也不能是竖条
    let ratio = w / h;
    assert!(
        ratio > 0.5 && ratio < 3.0,
        "大 schema 画布形状失控: {w}x{h} 比例 {ratio:.2}"
    );

    // 统计走顶部/底部总线的边：水平段落在所有表框的上下之外
    let boxes = extract_node_boxes(&svg);
    let (min_y, max_y) = boxes.iter().fold((f64::MAX, f64::MIN), |(lo, hi), b| {
        (lo.min(b.1), hi.max(b.1 + b.3))
    });
    let mut bus = 0usize;
    for d in extract_edge_paths(&svg) {
        let pts = sample_vertices(&d);
        // 找长水平段（跨 >100px）的 y
        let lane = pts
            .windows(2)
            .filter(|w2| (w2[0].1 - w2[1].1).abs() < 0.1 && (w2[0].0 - w2[1].0).abs() > 100.0)
            .map(|w2| w2[0].1)
            .next_back();
        if let Some(y) = lane {
            if y < min_y - 1.0 || y > max_y + 1.0 {
                bus += 1;
            }
        }
    }
    assert!(
        bus <= 6,
        "走总线的边过多（{bus} 条），画布会出现梳状线团"
    );
}

/// 孤立表（无任何外键）必须单独成块，不能混进拓扑层里把那一列撑高。
///
/// 真实 schema 里 14 张孤立表曾全被归入层 0，把它从 3287px 顶到 9412px，
/// 最终把画布拉成 1980×9492 的竖条。
///
/// 夹具用「一条 4 层依赖链 + 16 张孤立表」。这个测试**不代表**当前实现
/// 一定走 `layout_layers` 的孤立表分支——自适应选型可能直接选网格，
/// 那样两种实现都能通过。它守的是下限：不论走哪条路径，20 张表的画布
/// 都不能退化成一个窄列。
#[test]
fn isolated_tables_are_laid_out_in_a_separate_block() {
    let mut src = String::from(
        "Table a { id int [pk] }\n\
         Table b { id int [pk] a_id int [ref: > a.id] }\n\
         Table c { id int [pk] b_id int [ref: > b.id] }\n\
         Table d { id int [pk] c_id int [ref: > c.id] }\n",
    );
    for i in 0..16 {
        src.push_str(&format!(
            "Table lone{i} {{\n  id int [pk]\n  v1 varchar\n  v2 varchar\n  v3 varchar\n}}\n"
        ));
    }
    let sc = parser::parse(&src);
    let (svg, w, h) = layout::build_svg(&sc);
    let ratio = w / h;
    assert!(
        ratio > 0.5,
        "孤立表把画布拉成了竖条: {w}x{h} 比例 {ratio:.2}"
    );

    // 依赖链的 4 张表必须落在 4 个不同列（拓扑分层的基本要求）
    let boxes = extract_node_boxes(&svg);
    let mut cols: Vec<f64> = Vec::new();
    for b in &boxes {
        if !cols.iter().any(|x| (x - b.0).abs() < 1.0) {
            cols.push(b.0);
        }
    }
    assert!(
        cols.len() >= 4,
        "依赖链被压进了 {} 列，分层信息丢失",
        cols.len()
    );
}

/// 从画布 SVG 里取出所有节点框的 (x, y, w, h)。
fn extract_node_boxes(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    let mut out = Vec::new();
    for seg in svg.split("<g class=\"dx-tbl-hit\"").skip(1) {
        let Some(rect) = seg.split("<rect ").nth(1) else {
            continue;
        };
        let get = |k: &str| -> Option<f64> {
            let i = rect.find(&format!("{k}=\""))?;
            let rest = &rect[i + k.len() + 2..];
            rest.split('"').next()?.parse().ok()
        };
        if let (Some(x), Some(y), Some(w), Some(h)) =
            (get("x"), get("y"), get("width"), get("height"))
        {
            out.push((x, y, w, h));
        }
    }
    out
}

/// 解析折线的顶点序列（本模块产出的只有 M/H/V）。
fn sample_vertices(d: &str) -> Vec<(f64, f64)> {
    let toks: Vec<&str> = d.split_whitespace().collect();
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let mut p = (0.0f64, 0.0f64);
    let mut i = 0;
    while i < toks.len() {
        let t = toks[i];
        if let Some(v) = t.strip_prefix('M') {
            p = (
                v.parse().unwrap_or(0.0),
                toks.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0.0),
            );
            pts.push(p);
            i += 2;
        } else if let Some(v) = t.strip_prefix('H') {
            p.0 = v.parse().unwrap_or(p.0);
            pts.push(p);
            i += 1;
        } else if let Some(v) = t.strip_prefix('V') {
            p.1 = v.parse().unwrap_or(p.1);
            pts.push(p);
            i += 1;
        } else {
            i += 1;
        }
    }
    pts
}
