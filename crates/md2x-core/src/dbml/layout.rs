//! 画布布局：把 schema 排成可直接内联的 SVG。
//!
//! 布局用简化的分层力导向：按外键依赖做拓扑分层（父表靠左），
//! 层内垂直居中排列，再按索引顺序横向铺开。不追求最优，追求稳定可读——
//! 同一份 DBML 每次生成的图必须完全一致，便于在 git 里 review 差异。
//!
//! @author yujinping

use super::model::Schema;
use super::render::esc;
use super::route;

/// 单个表节点的固定宽度。
const NODE_W: f64 = 260.0;
/// 行高（每列一行）。
const ROW_H: f64 = 20.0;
/// 表头高度。
const HD_H: f64 = 30.0;

/// 列间横向间距。
const GAP_X: f64 = 120.0;
/// 层间纵向间距。
const GAP_Y: f64 = 56.0;
/// 画布外边距。
const PAD: f64 = 40.0;

/// 列名左侧留给约束徽章（`PK` / `FK` / `UQ`）的槽位宽度。
///
/// 固定槽位而不是「紧贴列名左对齐」：徽章长短不一，紧贴会让所有列名
/// 起点参差不齐，一列看下去像锯齿；固定槽位后列名永远从同一 x 起排。
///
/// 44 = 左内边距 8 + 徽章 17 + 间隙 3 + 徽章 17 - 1，刚好容纳两枚
/// （`PK`+`UQ` 这类组合必须能同时放下，否则第二个会压住列名）。
const TAG_W: f64 = 44.0;

/// 单枚徽章的宽度（三个标签都是两字符，等宽即可）。
const TAG_BW: f64 = 17.0;

/// 较小的约束徽章字号。
const TAG_FS: f64 = 8.0;

/// 列的约束徽章（PK / FK / UQ），最多两个。
///
/// 画布上一个字段能承载的信息有限，超过两个徽章就会挤掉列名本身，
/// 因此按「主键 > 外键 > 唯一」的优先级截断——前两者表达结构关系，
/// 唯一约束通常能从索引区看到。`INC`（自增）刻意不放进画布：
/// 它几乎总与 PK 同时出现，占位却在视觉上不提供新信息。
fn col_tags(c: &super::model::Column, is_fk: bool) -> Vec<&'static str> {
    let mut v = Vec::with_capacity(2);
    if c.pk {
        v.push("PK");
    }
    if is_fk {
        v.push("FK");
    }
    if c.unique {
        v.push("UQ");
    }
    v.truncate(2);
    v
}

/// 表的排布结果。
struct Placed {
    /// 表在 schema.tables 里的下标
    idx: usize,
    /// 列槽位号：同一列内所有节点 x 相同，路由靠它判断「是否跨列」
    col: usize,
    /// 左上角坐标
    x: f64,
    y: f64,
    /// 高度
    h: f64,
}

/// 表节点高度：表头 + 每列一行 + 底部留白。
fn node_h(sc: &Schema, i: usize) -> f64 {
    HD_H + sc.tables[i].cols.len() as f64 * ROW_H + 8.0
}

/// 分层结果是否「足够好看」，不值得为它放弃分层布局。
///
/// 分层布局给出自上而下的依赖流，可读性最好，但有两种退化形态：
/// 1. **纯链式**：每层只剩 1 张表，画布变成 1×n 的细长条（宽高比 > 2）；
/// 2. **单表**：n == 1 时谈不上布局，随它去。
///
/// 只在 n <= 3 时放宽——两三张表的链式长条还能一眼读完，
/// 4 张以上就明显该摆成网格了。
fn use_grid_layers_ok(sc: &Schema, layer: &[usize], max_layer: usize, n: usize) -> bool {
    if n <= 3 {
        return true;
    }
    // 估算分层结果的宽高：每层一列（不考虑子列，判据只需量级）
    let w = PAD * 2.0 + (max_layer + 1) as f64 * (NODE_W + GAP_X);
    let tallest = (0..n).map(|i| node_h(sc, i)).fold(0.0f64, f64::max);
    let max_per_layer = (0..=max_layer)
        .map(|l| layer.iter().filter(|x| **x == l).count())
        .max()
        .unwrap_or(1);
    let h = PAD * 2.0 + max_per_layer as f64 * (tallest + GAP_Y);
    // 宽高比 > 2.0 视为「明显偏横向」，改用网格更接近方形
    w / h.max(1.0) <= 2.0
}

/// 网格布局：列数取 sqrt(n) 后限幅，行距按最高表对齐——
/// 统一行高让格子对齐，比逐行自适应更像「图」而不是拼贴。
fn layout_grid(sc: &Schema) -> Vec<Placed> {
    let n = sc.tables.len();
    let cols = ((n as f64).sqrt().ceil() as usize).clamp(1, 10);
    let cell_w = NODE_W + GAP_X;
    let row_h = placed_row_height(sc) + GAP_Y;
    (0..n)
        .map(|i| {
            let c = i % cols;
            let x = PAD + c as f64 * cell_w;
            Placed {
                idx: i,
                col: col_slot(x),
                x,
                y: PAD + (i / cols) as f64 * row_h,
                h: node_h(sc, i),
            }
        })
        .collect()
}

/// 分层布局：按拓扑层分列，层内竖向堆叠，超过 `MAX_COL_H` 就在该层内另起子列。
///
/// 折行时 x 必须同步右移，否则后续表会叠加在前面的表上
/// （60 表压测时出现整片重叠，即为此处漏算）。
fn layout_layers(sc: &Schema, layer: &[usize], max_layer: usize) -> Vec<Placed> {
    const MAX_COL_H: f64 = 1100.0;
    let n = sc.tables.len();
    let mut sub_cols = vec![1usize; max_layer + 1];
    let mut slot: Vec<(usize, usize, f64)> = vec![(0, 0, 0.0); n];
    for (l, sc_l) in sub_cols.iter_mut().enumerate() {
        let members: Vec<usize> = (0..n).filter(|i| layer[*i] == l).collect();
        let mut y = PAD;
        let mut sub = 0usize;
        for (k, i) in members.iter().enumerate() {
            let h = node_h(sc, *i);
            if y + h > PAD + MAX_COL_H && k > 0 {
                sub += 1;
                y = PAD;
            }
            slot[*i] = (l, sub, y);
            y += h + GAP_Y;
        }
        *sc_l = sub + 1;
    }
    let mut layer_x = vec![PAD; max_layer + 1];
    for l in 1..=max_layer {
        layer_x[l] = layer_x[l - 1] + sub_cols[l - 1] as f64 * (NODE_W + GAP_X);
    }
    slot.iter()
        .copied()
        .enumerate()
        .map(|(i, (l, sub, y))| {
            let x = layer_x[l] + sub as f64 * (NODE_W + GAP_X);
            Placed {
                idx: i,
                // 子列也算独立列：折行后第二子列与第一子列之间隔着 NODE_W+GAP_X，
                // 路由把它们当「相邻列」用中间那条走廊，是成立的。
                col: col_slot(x),
                x,
                y,
                h: node_h(sc, i),
            }
        })
        .collect()
}

/// 由 x 坐标反推列槽位号。
///
/// 两种布局都是「列槽位 × (NODE_W + GAP_X)」的整数倍网格，因此从 x 反推
/// 比在布局时逐处手工传递 col 更可靠——布局里每加一条产出路径都得记得
/// 带上 col，漏一处就会让路由把跨列边当作同列处理，直接穿表。
fn col_slot(x: f64) -> usize {
    (((x - PAD) / (NODE_W + GAP_X)).round()).max(0.0) as usize
}

/// 生成 (svg, 宽, 高)。
pub fn build_svg(sc: &Schema) -> (String, f64, f64) {
    let n = sc.tables.len();
    if n == 0 {
        return (
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"400\" height=\"120\"><text x=\"20\" y=\"60\" font-size=\"14\" fill=\"#59636e\">没有可渲染的表</text></svg>".to_string(),
            400.0,
            120.0,
        );
    }

    // 计算每张表的层号：被引用多的表靠左（父表在左，子表在右）
    let mut layer = vec![0usize; n];
    let name_to_idx: Vec<(String, usize)> = sc
        .tables
        .iter()
        .enumerate()
        .map(|(i, t)| (t.name.clone(), i))
        .collect();
    let find = |name: &str| -> Option<usize> {
        name_to_idx
            .iter()
            .find(|(tn, _)| tn == name || tn.rsplit('.').next() == Some(name))
            .map(|(_, i)| *i)
    };
    // 迭代松弛，最多 n 轮，避免环导致死循环
    for _ in 0..n {
        let mut changed = false;
        for r in &sc.refs {
            if let (Some(f), Some(t)) = (find(&r.from_table), find(&r.to_table)) {
                if layer[f] <= layer[t] && f != t {
                    layer[f] = layer[t] + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    let max_layer = *layer.iter().max().unwrap_or(&0);

    // 布局选型：分层布局只在"层数适中、每层有若干表"时可读——它给出漂亮的
    // 从左到右依赖流。但链式外键会让层数接近表数（10 张表串成 10 层），
    // 横向拉成几千像素的长条，任何缩放都读不清。
    //
    // 判据用「平均每层表数」：低于 2.5 说明分层退化成一条线，
    // 此时改用网格，保证画布接近方形。这个阈值来自实测——
    // 4 表 3 层的正常 schema 平均 1.3 但表少，仍应走分层；
    // 60 表 10 层平均 6.0，分层其实可读，真正不可读的是 10 层 × 6 张
    // 叠在同一列限高里形成的多子列横条，故再叠加一个总宽度闸门。
    let avg_per_layer = n as f64 / (max_layer + 1) as f64;
    // 表少时（n <= 12）分层通常更好看，但**纯链式** schema 会退化成
    // 一条 1×n 的长条（例如 4 张表串成 4 层 → 1480×298，宽高比 5:1）。
    // 那种图 fit 进窗口后缩到 50% 上下，字全糊了，而画布上下还空着两大片。
    // 所以不能只看表数，要看**分层结果本身的宽高比**。
    let layer_ratio_too_wide = !use_grid_layers_ok(sc, &layer, max_layer, n);

    let use_grid = (n > 12 && (avg_per_layer < 2.5 || (max_layer + 1) > 6))
        || layer_ratio_too_wide;

    let placed: Vec<Placed> = if use_grid {
        layout_grid(sc)
    } else {
        layout_layers(sc, &layer, max_layer)
    };

    // ---- 连线路由（两遍）----
    //
    // 第一遍：用「尚未为总线让位」的节点位置规划走法，只为了知道需要几条车道；
    // 第二遍：按车道数把节点整体下移/上移腾出总线区，再真正生成路径。
    //
    // 为什么必须分两遍：跨列边只能走顶部或底部总线（中间隔着整列节点，
    // 没有任何走廊能直达），而总线的高度又取决于有**多少条**跨列边。
    // 先定尺寸再规划就会漏掉这部分高度，总线被画在画布外面。
    let mut raw: Vec<(usize, usize, f64, f64)> = Vec::new();
    {
        // 同一对表只画一条：两个方向都画会得到两条几乎重合的线
        let mut seen = std::collections::HashSet::new();
        for r in &sc.refs {
            let (Some(f), Some(t)) = (find(&r.from_table), find(&r.to_table)) else {
                continue;
            };
            // 自引用（`parent_id > id`）只会在表内部绕一小圈，
            // 视觉上几乎总是压在表身上，收益远小于噪声，不画。
            if f == t {
                continue;
            }
            if !seen.insert((f.min(t), f.max(t))) {
                continue;
            }
            let fy = HD_H + col_row(sc, f, &r.from_col) * ROW_H + ROW_H / 2.0;
            let ty = HD_H + col_row(sc, t, &r.to_col) * ROW_H + ROW_H / 2.0;
            raw.push((f, t, fy, ty));
        }
    }

    // 节点区的纵向中点，用来决定跨列边走顶部还是底部总线（取近的一侧）
    let mid_y = {
        let lo = placed.iter().map(|p| p.y).fold(f64::MAX, f64::min);
        let hi = placed
            .iter()
            .map(|p| p.y + p.h)
            .fold(f64::MIN, f64::max);
        (lo + hi) / 2.0
    };

    let geo: Vec<route::Box> = placed
        .iter()
        .map(|p| route::Box {
            col: p.col,
            x: p.x,
            y: p.y,
            h: p.h,
        })
        .collect();
    let edges: Vec<route::Edge> = raw
        .iter()
        .map(|(f, t, fy, ty)| route::Edge {
            from: i_of(&placed, *f),
            to: i_of(&placed, *t),
            from_dy: *fy,
            to_dy: *ty,
        })
        .collect();
    let plan = route::plan(&geo, &edges, mid_y);

    // 腾出总线区：节点整体下移「顶部车道高度」，底部由总高度自动让出
    let top_pad = route::top_reserved(&plan);
    let bot_pad = route::bottom_reserved(&plan);
    let geo: Vec<route::Box> = geo
        .iter()
        .map(|b| route::Box {
            y: b.y + top_pad,
            ..*b
        })
        .collect();
    let placed: Vec<Placed> = placed
        .into_iter()
        .map(|p| Placed {
            y: p.y + top_pad,
            ..p
        })
        .collect();

    // 最右列的走廊中线在节点右边缘外 60px，右边距必须容下它
    let total_w = placed
        .iter()
        .map(|p| p.x + NODE_W)
        .fold(0.0f64, f64::max)
        + route::RIGHT_PAD;
    let total_h = placed
        .iter()
        .map(|p| p.y + p.h)
        .fold(0.0f64, f64::max)
        + bot_pad.max(PAD);

    // 画布尺寸取整，避免浮点尾巴
    let (w, h) = (total_w.ceil(), total_h.ceil());

    let mut svg = String::with_capacity(4096 + n * 900);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"-apple-system,Segoe UI,Noto Sans SC,sans-serif\">"
    ));
    // 底色与箭头。
    //
    // 所有填充/描边都走 CSS 类而不是写死颜色：片段会被塞进任意模版的
    // `.markdown-body`，配色必须跟随模版的 CSS 变量，否则换模版后
    // 表格是新的、画布还是旧的，一眼就能看出是两套东西拼起来的。
    svg.push_str("<defs><marker id=\"dx-ah\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path class=\"dx-edge-head\" d=\"M0 0L10 5L0 10z\"/></marker></defs>");
    svg.push_str("<rect class=\"dx-cv-bg\" width=\"100%\" height=\"100%\"/>");

    // 分组底色：TableGroup 画成虚线分区，把成员表框在一起
    for g in &sc.groups {
        let members: Vec<&Placed> = placed
            .iter()
            .filter(|p| {
                let tn = &sc.tables[p.idx].name;
                g.tables.iter().any(|x| x == tn)
            })
            .collect();
        if members.len() < 2 {
            continue;
        }
        let x1 = members.iter().map(|p| p.x).fold(f64::MAX, f64::min) - 14.0;
        let y1 = members.iter().map(|p| p.y).fold(f64::MAX, f64::min) - 28.0;
        let x2 = members
            .iter()
            .map(|p| p.x + NODE_W)
            .fold(f64::MIN, f64::max)
            + 14.0;
        let y2 = members.iter().map(|p| p.y + p.h).fold(f64::MIN, f64::max) + 14.0;
        svg.push_str(&format!(
            "<rect x=\"{x1:.1}\" y=\"{y1:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"10\" class=\"dx-grp-box\"/>",
            x2 - x1,
            y2 - y1
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" font-weight=\"600\" class=\"dx-grp-txt\">{}</text>",
            x1 + 8.0,
            y1 + 15.0,
            esc(&g.name)
        ));
    }

    // 先画连线（在节点之下）
    //
    // 路径由 `route` 模块生成：正交折线，只走列间走廊与顶部/底部总线，
    // 结构上不可能穿过任何节点框（见该模块顶部注释）。
    let paths = route::emit(&geo, &edges, &plan, NODE_W, GAP_X, h);
    for d in paths {
        svg.push_str(&format!(
            "<path d=\"{d}\" class=\"dx-edge\" marker-end=\"url(#dx-ah)\"/>"
        ));
    }

    // 再画节点
    for p in &placed {
        let t = &sc.tables[p.idx];
        let short = t.name.rsplit('.').next().unwrap_or(&t.name);
        svg.push_str(&format!(
            "<g class=\"dx-tbl-hit\" data-anchor=\"dx-t-{anc}\" data-k=\"{key}\">",
            anc = esc(&t.name.replace('.', "--").replace(' ', "_")),
            key = esc(&format!(
                "{} {}",
                t.name,
                t.cols.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(" ")
            ))
        ));
        // 外框
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{NODE_W}\" height=\"{:.1}\" rx=\"8\" class=\"dx-node-box\"/>",
            p.x, p.y, p.h
        ));
        // 表头底：用 rect 而非 path——path 的 z 收尾方向容易让浏览器把填充
        // 判成未闭合区域，表现为表头变成透明、表名像浮在空白里。
        //
        // 有 headercolor 时用 DBML 指定的色，否则用类名交给主题变量。
        // 自定义色是用户在 DBML 里明确指定的，理当优先于主题。
        match &t.color {
            Some(c) => svg.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{NODE_W}\" height=\"{HD_H}\" fill=\"{}\"/>",
                p.x,
                p.y,
                esc(c)
            )),
            None => svg.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{NODE_W}\" height=\"{HD_H}\" class=\"dx-node-hd\"/>",
                p.x, p.y
            )),
        }
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" class=\"dx-node-line\"/>",
            p.x,
            p.y + HD_H,
            p.x + NODE_W,
            p.y + HD_H
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"13\" font-weight=\"600\" class=\"dx-node-title\">{}</text>",
            p.x + 12.0,
            p.y + 19.0,
            esc(short)
        ));
        if let Some(a) = &t.alias {
            // 用 `as x` 而不是裸别名：单独一个 `U` 在表头右侧会被读成表名或缩写，
            // 加上 as 才明确是「别名」。右对齐锚点固定在节点右内边距上，
            // 别名长短都不会越过标题（原来写死 x = NODE_W-40，长别名会左溢出压住表名）。
            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" text-anchor=\"end\" class=\"dx-node-alias\">as {}</text>",
                p.x + NODE_W - 12.0,
                p.y + 19.0,
                esc(a)
            ));
        }

        // 列名统一起点：整张表只要有任意一列带徽章，所有列名都从槽位右端起排。
        //
        // 不能按「这一列有没有徽章」逐行决定——那样有徽章的从槽位右端开始、
        // 没徽章的贴左内边距，同一张表里列名起点上下错开整整一个槽位，
        // 一列看下去是锯齿，扫读时眼睛要不断重新找起点。
        // 槽位空着就空着，对齐比省那点横向空间重要。
        let table_has_tags = t.cols.iter().any(|c| {
            let fk = sc.fk_of(&t.name, &c.name).is_some() || sc.fk_of(short, &c.name).is_some();
            c.pk || fk || c.unique
        });
        let name_x = if table_has_tags {
            p.x + 8.0 + TAG_W
        } else {
            p.x + 10.0
        };

        // 列
        for (ci, c) in t.cols.iter().enumerate() {
            let cy = p.y + HD_H + ci as f64 * ROW_H + 14.0;
            // 该列是否为外键：行内 ref 与顶层 Ref 已在解析期收口到 sc.refs，
            // 这里只需按 (表, 列) 查一次。表名既可能是完整名也可能是裸名
            // （`schema.table` vs `table`），两种都试，与字典表格的口径一致。
            let is_fk = sc.fk_of(&t.name, &c.name).is_some()
                || sc.fk_of(short, &c.name).is_some();
            let tags = col_tags(c, is_fk);

            // 约束徽章：小方块 + 白字，颜色按约束类型区分（见 CSS dx-tag-*）。
            //
            // 原来这里只有一条 3px 的左侧红条表示 PK，FK / UQ 在画布上
            // 完全不可见——而画布恰恰是「看结构关系」的视图，缺了 FK 标识
            // 就只能靠连线的落点去猜哪个字段是外键。改成徽章后三种约束
            // 都能一眼读出，且不占用列名行的纵向空间（行高不变）。
            let mut tx = p.x + 8.0;
            for tag in &tags {
                svg.push_str(&format!(
                    "<rect x=\"{tx:.1}\" y=\"{:.1}\" width=\"{TAG_BW}\" height=\"11\" rx=\"2.5\" class=\"dx-tag dx-tag-{}\"/>",
                    cy - 8.6,
                    tag.to_ascii_lowercase()
                ));
                svg.push_str(&format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{TAG_FS}\" font-weight=\"700\" text-anchor=\"middle\" class=\"dx-tag-tx\">{tag}</text>",
                    tx + TAG_BW / 2.0,
                    cy - 0.6
                ));
                tx += TAG_BW + 3.0;
            }

            svg.push_str(&format!(
                "<text x=\"{name_x:.1}\" y=\"{cy:.1}\" font-size=\"11\" font-family=\"ui-monospace,Menlo,monospace\" class=\"dx-node-col\">{}</text>",
                esc(&c.name)
            ));
            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{cy:.1}\" font-size=\"10\" font-family=\"ui-monospace,Menlo,monospace\" class=\"dx-node-type\" text-anchor=\"end\">{}</text>",
                p.x + NODE_W - 10.0,
                esc(&truncate(&c.ty, 18))
            ));
        }
        svg.push_str("</g>");
    }

    svg.push_str("</svg>");
    (svg, w, h)
}

/// 网格布局用的行高：取所有表里最高的那张，让网格行距一致。
fn placed_row_height(sc: &Schema) -> f64 {
    sc.tables
        .iter()
        .map(|t| HD_H + t.cols.len() as f64 * ROW_H + 8.0)
        .fold(0.0f64, f64::max)
}

/// 某列在表内的行号（找不到时按 0）。
fn col_row(sc: &Schema, t: usize, col: &str) -> f64 {
    sc.tables[t]
        .cols
        .iter()
        .position(|c| c.name == col)
        .unwrap_or(0) as f64
}

/// 表下标 → 它在 `placed` 里的下标。
///
/// `placed` 与 `sc.tables` 顺序一致（两种布局都按 0..n 生成），所以两者
/// 相同；但路由接口用的是 `placed` 下标，显式转换一次以免日后布局改成
/// 重排顺序时出隐蔽错位。
fn i_of(placed: &[Placed], table_idx: usize) -> usize {
    placed
        .iter()
        .position(|p| p.idx == table_idx)
        .unwrap_or(0)
}

/// 按字符数截断（中文按 2 宽处理不重要，只求不溢出）。
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut o: String = s.chars().take(max.saturating_sub(1)).collect();
    o.push('…');
    o
}