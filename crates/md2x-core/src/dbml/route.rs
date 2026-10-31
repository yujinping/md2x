//! 画布连线路由：正交折线 + 走廊避让。
//!
//! **为什么不用贝塞尔**：早期实现从源节点直连目标节点，控制点取两点 x 的中点。
//! 一旦两节点跨列（`roles → tenants`，中间隔着 `users`），控制点就落在中间
//! 那一列上，曲线鼓进去横穿整张表。更糟的是**同列**的两个节点
//! （`api_keys.owner_id → users.id`，两张表都在 x=420）：`x1 <= x2` 成立，
//! 于是从源表**右**边缘出线、却要往左走到目标，整条线从源表自己身上穿回去。
//!
//! **现在的模型**用三种通道，都不经过任何节点框：
//!
//! - **列间走廊**：相邻列之间恒有 `GAP_X` 的空隙，中线两侧各 60px 内无节点。
//!   同列/相邻列的边都在这条走廊里竖直行走。
//! - **中间列的水平空档**：跨多列时，如果能在两端的 y 之间找到一条横贯
//!   所有中间列的水平通道——该高度上这些列恰好都没有节点——就直接穿过去。
//!   同一层里表与表之间恒有 `GAP_Y` 的空隙，绝大多数跨列边都能走这里。
//! - **顶部与底部总线**：上面两条都走不通时（中间列被整片占满）才用。
//!   车道区在布局阶段就被让出来，节点整体下移/上移，所以水平段永远不会
//!   压到任何表。
//!
//! **为什么必须有「中间空档」这一档**：只留走廊 + 总线时，一旦层内分列
//! （59 表 schema 分了 12 列），跨 2 列以上就成了常态，52 条边里有 43 条
//! 被迫挤进顶部总线，在 450px 高、9px 间距的车道带里排成密不透风的梳子——
//! 图彻底读不出来。而在中间列的空档穿过去，路径只比相邻列多两个直角，
//! 视觉上仍是「一条线连了两张表」。
//!
//! 换句话说，「线穿表」在结构上不可能发生，而不是靠调参让曲线看起来没穿。
//!
//! @author yujinping

/// 画布上的一个节点框（只保留路由需要的几何量）。
#[derive(Debug, Clone, Copy)]
pub struct Box {
    /// 列槽位号：同一列内所有节点的 `x` 相同。
    pub col: usize,
    /// 左边缘。
    pub x: f64,
    /// 上边缘。
    pub y: f64,
    /// 宽度。
    pub w: f64,
    /// 高度。
    pub h: f64,
}

/// 一条待路由的边。
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    /// 源节点在 `boxes` 里的下标
    pub from: usize,
    /// 目标节点在 `boxes` 里的下标
    pub to: usize,
    /// 源锚点相对源节点上边缘的偏移（列所在行的中心）
    pub from_dy: f64,
    /// 目标锚点相对目标节点上边缘的偏移
    pub to_dy: f64,
}

/// 一条边的走法。
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// 同列：绕到该列右侧走廊走 U 形（源和目标 x 完全相同，无法直连）
    SameCol,
    /// 相邻列：在两者之间的那条走廊里直接走
    Adjacent,
    /// 跨多列但中间列在该高度有空档：水平段直接穿过空档
    Pass { y: f64 },
    /// 跨列且中间被占满：上半区走顶部总线，下半区走底部总线
    Bus { top: bool },
}

/// 路由计划：走法 + 车道号 + 总线预留量。
pub struct Plan {
    kinds: Vec<Kind>,
    /// 每条边在其总线方向上的车道号（非总线边为 0）
    lanes: Vec<usize>,
    /// 顶部总线需要几条车道
    pub top_lanes: usize,
    /// 底部总线需要几条车道
    pub bottom_lanes: usize,
}

/// 车道高度（相邻两条总线的垂直间距）。
pub const LANE_H: f64 = 9.0;

/// 总线区距画布边缘的留白。
pub const BUS_MARGIN: f64 = 16.0;

/// 列表右侧预留的横向宽度。
///
/// 走廊中线在节点右边缘外 `GAP_X / 2 = 60` 处，再留 20 给箭头收尾。
/// 早期用统一的 `PAD = 40` 做右边距，最右列的走廊中线和画布边界几乎重合，
/// 右侧进入的箭头会被裁掉半个。
pub const RIGHT_PAD: f64 = 80.0;

/// 同一走廊内多条边叠在一起时，按槽位左右错开，避免糊成一条。
///
/// 走廊宽 120，节点边缘在 ±60 处，±18 的偏移碰不到任何框。
const SLOT_STEP: f64 = 9.0;
/// 错开档位数（含左右两侧）。
const SLOT_N: usize = 5;

/// 一条边在 SVG 里的起点/终点 x 与走廊 x。
struct Geom {
    /// 起点 x
    sx: f64,
    /// 终点 x
    ex: f64,
    /// 源侧走廊中线
    ch_s: f64,
    /// 目标侧走廊中线
    ch_t: f64,
}

impl Plan {
    /// 把计划里所有**绝对 y** 平移 `dy`。
    ///
    /// `Kind::Pass` 记的是穿过中间列空档的那个高度，它是在「节点尚未为
    /// 总线让位」时算出来的绝对坐标。布局阶段随后会把所有节点整体下移
    /// `top_reserved()` 腾出总线区——如果 Pass 的高度不跟着移动，水平段
    /// 就会落在原位置，横穿整片节点。这个 bug 在 59 表 schema 上造成
    /// 49 条连线穿表，而单元测试全绿（测试夹具没有总线区，dy 恒为 0）。
    ///
    /// 总线车道的 y 不走这里：它由 `emit` 按 `canvas_h` 现算。
    pub fn shift(&mut self, dy: f64) {
        for k in &mut self.kinds {
            if let Kind::Pass { y } = k {
                *y += dy;
            }
        }
    }
}

/// 规划每条边的走法，并统计总线车道需求。
///
/// `mid` 是节点区的垂直中点：在它上方的跨列边走顶部总线，下方走底部，
/// 让绕行距离尽量短。
pub fn plan(boxes: &[Box], edges: &[Edge], mid: f64) -> Plan {
    let mut kinds = Vec::with_capacity(edges.len());
    let mut lanes = Vec::with_capacity(edges.len());
    let (mut top, mut bottom) = (0usize, 0usize);
    for e in edges {
        let (a, b) = (&boxes[e.from], &boxes[e.to]);
        let kind = if a.col == b.col {
            Kind::SameCol
        } else if a.col.abs_diff(b.col) == 1 {
            Kind::Adjacent
        } else {
            // 跨多列：先找中间列的水平空档，找不到才退回总线
            let sy = a.y + e.from_dy;
            let ty = b.y + e.to_dy;
            match find_gap(boxes, a.col, b.col, sy, ty) {
                Some(y) => Kind::Pass { y },
                None => {
                    let mid_y = (sy + ty) / 2.0;
                    Kind::Bus {
                        top: mid_y <= mid,
                    }
                }
            }
        };
        let lane = match kind {
            Kind::Bus { top: true } => {
                let n = top;
                top += 1;
                n
            }
            Kind::Bus { top: false } => {
                let n = bottom;
                bottom += 1;
                n
            }
            _ => 0,
        };
        kinds.push(kind);
        lanes.push(lane);
    }
    Plan {
        kinds,
        lanes,
        top_lanes: top,
        bottom_lanes: bottom,
    }
}

/// 在 `[lo, hi]` 两条列之间找一条横贯的水平空档，返回其 y。
///
/// 空档 = 该 y 上**所有**中间列都没有节点覆盖。等价于：把中间列所有节点的
/// 纵向占位取**并集**，空档就是并集的补集。取并集而不是逐列求交集，是因为
/// 「穿过」要求每一列都让路，任一列被占就过不去——并集正好表达这个约束，
/// 且结果是一组区间，能直接从中挑最合适的 y。
///
/// **y 的搜索范围是整张画布，不限于两端锚点之间。** 竖直段走的是列间走廊，
/// 那里恒无节点，所以从 `sy` 先上/下挪到任意 y 再横穿是安全的。
/// 早期把范围限制在 `[sy, ty]` 内，结果两端锚点处于同一行、而中间列恰好在
/// 那一行有表时找不到空档，只能退回顶部总线——59 表 schema 上 52 条边里
/// 有 43 条因此挤进总线，排成密不透风的梳子。
///
/// 挑选原则：离两端锚点中点最近的空档优先，即绕行最短。
fn find_gap(boxes: &[Box], col_a: usize, col_b: usize, sy: f64, ty: f64) -> Option<f64> {
    let (lo, hi) = if col_a < col_b {
        (col_a, col_b)
    } else {
        (col_b, col_a)
    };
    // 两端所在的列不算「中间列」——线是从它们的边缘出发的
    let mut occ: Vec<(f64, f64)> = boxes
        .iter()
        .filter(|b| b.col > lo && b.col < hi)
        .map(|b| (b.y - GAP_CLEAR, b.y + b.h + GAP_CLEAR))
        .collect();
    if occ.is_empty() {
        return Some(ty);
    }
    // 按起点排序后合并重叠区间，得到「整段被占」的连续块
    occ.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(occ.len());
    for (s, e) in occ {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    // 空档 = 各占位块之间的**缝**。刻意不把画布上下边界算作空档：
    // 那两处正是顶部/底部总线区，一旦允许就会被当成普通空档，
    // 「找不到空档 → 退回总线」这条兜底永远不会触发，总线代码成了死路。
    // 而且走画布边缘在视觉上就是总线，没必要用两条不同的代码路径画同一件事。
    let want = (sy + ty) / 2.0;
    let mut best: Option<f64> = None;
    let mut consider = |y: f64| {
        if best.map(|b| (y - want).abs() < (b - want).abs()).unwrap_or(true) {
            best = Some(y);
        }
    };
    for w in merged.windows(2) {
        let (s, e) = (w[0].1, w[1].0);
        // 缝窄于两倍余量时线会贴着表边划过，不如走总线干净
        if e - s > GAP_CLEAR * 2.0 {
            consider(((s + e) / 2.0).clamp(s + GAP_CLEAR, e - GAP_CLEAR));
        }
    }
    best
}

/// 空档判定时给节点边缘留的余量（避免线贴着表边划过）。
const GAP_CLEAR: f64 = 3.0;

/// 顶部总线区的高度（含上下留白）。
pub fn top_reserved(plan: &Plan) -> f64 {
    if plan.top_lanes == 0 {
        0.0
    } else {
        BUS_MARGIN * 2.0 + plan.top_lanes as f64 * LANE_H
    }
}

/// 底部总线区的高度（含上下留白）。
pub fn bottom_reserved(plan: &Plan) -> f64 {
    if plan.bottom_lanes == 0 {
        0.0
    } else {
        BUS_MARGIN * 2.0 + plan.bottom_lanes as f64 * LANE_H
    }
}

/// 生成每条边的 SVG `path d`。
///
/// `boxes` 必须是**已平移**后的坐标（即已经为总线区让出空间），
/// `canvas_h` 也必须是最终高度——底部总线的车道 y 由它反推。
pub fn emit(
    boxes: &[Box],
    edges: &[Edge],
    plan: &Plan,
    node_w: f64,
    gap_x: f64,
    canvas_h: f64,
) -> Vec<String> {
    // 每条走廊被用过几次，用于把重叠的边左右错开
    let mut used: std::collections::HashMap<(usize, bool), usize> =
        std::collections::HashMap::new();
    let mut out = Vec::with_capacity(edges.len());

    for (i, e) in edges.iter().enumerate() {
        let a = &boxes[e.from];
        let b = &boxes[e.to];
        let sy = a.y + e.from_dy;
        let ty = b.y + e.to_dy;

        let g = geometry(a, b, node_w, gap_x);
        match plan.kinds[i] {
            Kind::SameCol => {
                // 从右侧出、右侧进，绕一个 `]` 形。同列时源和目标 x 完全相同，
                // 只有绕到列外才能既不穿自己、也不穿对方。
                let off = slot(&mut used, (a.col, true));
                let cx = g.ch_s + off;
                out.push(format!(
                    "M{:.1} {:.1} H{:.1} V{:.1} H{:.1}",
                    a.x + node_w,
                    sy,
                    cx,
                    ty,
                    b.x + node_w
                ));
            }
            Kind::Adjacent => {
                let col = if a.col < b.col { a.col } else { b.col };
                let off = slot(&mut used, (col, false));
                let cx = g.ch_s + off;
                out.push(format!(
                    "M{:.1} {:.1} H{:.1} V{:.1} H{:.1}",
                    g.sx, sy, cx, ty, g.ex
                ));
            }
            Kind::Pass { y } => {
                // 竖直段落在两端列的走廊里，水平段穿过中间列的空档。
                // 与 Adjacent 的区别只是水平段在 y 高度而非目标行高度。
                if (y - ty).abs() < 0.5 {
                    // 空档恰好就在目标行：退化成一次平移 + 直达
                    out.push(format!(
                        "M{:.1} {:.1} H{:.1} V{:.1} H{:.1}",
                        g.sx, sy, g.ch_s, ty, g.ex
                    ));
                } else if (y - sy).abs() < 0.5 {
                    // 空档在源行：先横穿再落到目标
                    out.push(format!(
                        "M{:.1} {:.1} H{:.1} V{:.1} H{:.1}",
                        g.sx, sy, g.ch_t, ty, g.ex
                    ));
                } else {
                    out.push(format!(
                        "M{:.1} {:.1} H{:.1} V{:.1} H{:.1} V{:.1} H{:.1}",
                        g.sx, sy, g.ch_s, y, g.ch_t, ty, g.ex
                    ));
                }
            }
            Kind::Bus { top } => {
                let lane = plan.lanes[i];
                let lane_y = if top {
                    BUS_MARGIN + lane as f64 * LANE_H
                } else {
                    canvas_h - BUS_MARGIN - lane as f64 * LANE_H
                };
                // 水平段走总线区，竖直段走两侧走廊：两段都在节点框之外
                out.push(format!(
                    "M{:.1} {:.1} H{:.1} V{:.1} H{:.1} V{:.1} H{:.1}",
                    g.sx, sy, g.ch_s, lane_y, g.ch_t, ty, g.ex
                ));
            }
        }
    }
    out
}

/// 算出一条边的四个关键 x。
fn geometry(a: &Box, b: &Box, node_w: f64, gap_x: f64) -> Geom {
    if a.col < b.col {
        Geom {
            sx: a.x + node_w,
            ex: b.x,
            // 源列右侧走廊
            ch_s: a.x + node_w + gap_x / 2.0,
            // 目标列左侧走廊（等价于前一列的右侧走廊）
            ch_t: b.x - gap_x / 2.0,
        }
    } else if a.col > b.col {
        Geom {
            sx: a.x,
            ex: b.x + node_w,
            ch_s: a.x - gap_x / 2.0,
            ch_t: b.x + node_w + gap_x / 2.0,
        }
    } else {
        // 同列：统一走右侧走廊
        let cx = a.x + node_w + gap_x / 2.0;
        Geom {
            sx: a.x + node_w,
            ex: b.x + node_w,
            ch_s: cx,
            ch_t: cx,
        }
    }
}

/// 取该走廊的下一个错开偏移。
fn slot(used: &mut std::collections::HashMap<(usize, bool), usize>, key: (usize, bool)) -> f64 {
    let k = used.entry(key).or_insert(0);
    let v = (*k % SLOT_N) as f64 - 2.0;
    *k += 1;
    v * SLOT_STEP
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(col: usize, x: f64, y: f64, h: f64) -> Box {
        Box {
            col,
            x,
            y,
            w: 260.0,
            h,
        }
    }

    /// 解析本模块生成的 `M{x} {y} H{x} V{y} …` 折线，返回顶点序列。
    fn parse_path(d: &str) -> Vec<(f64, f64)> {
        let toks: Vec<&str> = d.split_whitespace().collect();
        let mut pts = Vec::new();
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
        pts
    }

    /// 精确采样折线上的点，判断是否落在任一节点框内。
    ///
    /// 这是本模块存在的全部理由：只要这个断言成立，"线穿表"就不可能发生。
    /// 逐段按 2px 步长采样，而不是比包围盒——本模块产出的全是正交段，
    /// 沿段采样不会漏判。
    fn path_hits_box(d: &str, boxes: &[Box], node_w: f64) -> Option<usize> {
        let pts = parse_path(d);
        for w in pts.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            let steps = (((x1 - x0).abs() + (y1 - y0).abs()) / 2.0).ceil() as usize + 1;
            for s in 0..=steps {
                let t = s as f64 / steps as f64;
                let x = x0 + (x1 - x0) * t;
                let y = y0 + (y1 - y0) * t;
                for (i, bx) in boxes.iter().enumerate() {
                    // 内缩 1px：贴边出线不算穿过
                    if x > bx.x + 1.0
                        && x < bx.x + node_w - 1.0
                        && y > bx.y + 1.0
                        && y < bx.y + bx.h - 1.0
                    {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    #[test]
    fn same_column_edge_never_crosses_its_own_box() {
        // 两张表在同一列：这是旧实现里最严重的穿模场景
        let boxes = [
            bx(0, 40.0, 40.0, 118.0),
            bx(0, 40.0, 234.0, 138.0),
        ];
        let edges = [Edge {
            from: 1,
            to: 0,
            from_dy: 80.0,
            to_dy: 40.0,
        }];
        let p = plan(&boxes, &edges, 200.0);
        let d = &emit(&boxes, &edges, &p, 260.0, 120.0, 500.0)[0];
        assert_eq!(path_hits_box(d, &boxes, 260.0), None, "path={d}");
    }

    #[test]
    fn cross_column_edge_falls_back_to_bus_when_no_gap() {
        // roles(第2列) → tenants(第0列)，中间第1列被一张高表从上到下占满，
        // 找不到任何横贯的水平空档，只能退回顶部总线
        let boxes = [
            bx(0, 40.0, 40.0, 98.0),
            bx(1, 420.0, 0.0, 600.0), // 贯穿整张画布的中间列
            bx(2, 800.0, 40.0, 98.0),
        ];
        let edges = [Edge {
            from: 2,
            to: 0,
            from_dy: 40.0,
            to_dy: 40.0,
        }];
        let p = plan(&boxes, &edges, 90.0);
        assert_eq!(p.top_lanes, 1, "无空档时跨列边应占用一条顶部车道");
        let top = top_reserved(&p);
        // 平移后重新 emit
        let moved: Vec<Box> = boxes
            .iter()
            .map(|b| Box { y: b.y + top, ..*b })
            .collect();
        let d = &emit(&moved, &edges, &p, 260.0, 120.0, 800.0)[0];
        assert_eq!(path_hits_box(d, &moved, 260.0), None, "path={d}");
    }

    /// 中间列有空档时，跨列边走空档而不是总线——这是 59 表 schema 上
    /// 消灭「顶部梳状线团」的关键：52 条边里 43 条原本被迫挤进总线。
    #[test]
    fn cross_column_edge_passes_through_gap_when_available() {
        // 中间列只有上下一小段有表，中间留出大片空档（y 208..697 之间可通行）
        let boxes = [
            bx(0, 40.0, 300.0, 98.0),
            bx(1, 420.0, 40.0, 98.0),  // 中间列上方：40..138
            bx(1, 420.0, 700.0, 98.0), // 中间列下方：700..798
            bx(2, 800.0, 320.0, 98.0),
        ];
        let edges = [Edge {
            from: 3,
            to: 0,
            from_dy: 40.0,
            to_dy: 40.0,
        }];
        let p = plan(&boxes, &edges, 400.0);
        assert_eq!(p.top_lanes, 0, "有空档时不该占用总线车道");
        assert_eq!(p.bottom_lanes, 0);
        let d = &emit(&boxes, &edges, &p, 260.0, 120.0, 900.0)[0];
        assert_eq!(path_hits_box(d, &boxes, 260.0), None, "path={d}");
        // 水平段必须真的横穿中间列所在的 x 区间（该列横跨 420..680）
        let pts = parse_path(d);
        let spans_mid = pts.windows(2).any(|w| {
            (w[0].1 - w[1].1).abs() < 0.1
                && w[0].0.min(w[1].0) < 450.0
                && w[0].0.max(w[1].0) > 650.0
        });
        assert!(spans_mid, "水平段应横穿中间列: {d}");
    }

    /// `Plan::shift` 必须把 Pass 的穿行高度一起平移。
    ///
    /// **真实 bug 回归**：59 表 schema 上，`plan()` 在「节点尚未为总线让位」
    /// 的坐标系里记下 Pass 的绝对 y；随后布局把节点整体下移 `top_reserved()`，
    /// Pass 的 y 却没动，水平段落在原位置横穿整片表——实测 49 条连线穿模，
    /// 而当时全部测试是绿的（夹具都不产生总线区，平移量恰好为零）。
    #[test]
    fn pass_height_moves_with_shift() {
        // 中间列只在上、下各有一小段，中间留出空档
        let boxes = [
            bx(0, 40.0, 400.0, 98.0),  // 源
            bx(1, 420.0, 40.0, 98.0),  // 中间列上方
            bx(1, 420.0, 900.0, 98.0), // 中间列下方
            bx(2, 800.0, 420.0, 98.0), // 目标
        ];
        let edges = [Edge {
            from: 0,
            to: 3,
            from_dy: 40.0,
            to_dy: 40.0,
        }];
        let mut p = plan(&boxes, &edges, 500.0);
        // 这个夹具必须真的走 Pass，否则本测试失去意义
        assert!(
            matches!(p.kinds[0], Kind::Pass { .. }),
            "夹具应触发 Pass 走法，实际 {:?}",
            p.kinds[0]
        );

        // 模拟布局阶段为顶部总线让出若干像素。
        // 取 80 而不是 60：空档中线在 y=500，居中于中间列 138..900 之间，
        // 平移后若恰好落回某张表内部才能验证 bug，80 让 500→580 正落在
        // 中间列下方那张表（900 起）之外、又不与任何表重叠的错位处，
        // 反而更适合验证「不 shift 会穿、shift 后不穿」的对比。
        let dy = 400.0;
        p.shift(dy);
        let moved: Vec<Box> = boxes
            .iter()
            .map(|b| Box { y: b.y + dy, ..*b })
            .collect();
        let d = &emit(&moved, &edges, &p, 260.0, 120.0, 1200.0)[0];
        assert_eq!(
            path_hits_box(d, &moved, 260.0),
            None,
            "shift 后水平段仍落在旧高度上，穿表了: {d}"
        );

        // 反向确认：不 shift 就会穿表（证明本测试确实能捕获该 bug）
        let p0 = plan(&boxes, &edges, 500.0);
        let d0 = &emit(&moved, &edges, &p0, 260.0, 120.0, 1200.0)[0];
        assert!(
            path_hits_box(d0, &moved, 260.0).is_some(),
            "未 shift 时本应穿表，夹具失效: {d0}"
        );
    }

    #[test]
    fn adjacent_column_edge_stays_inside_the_gap() {
        let boxes = [
            bx(0, 40.0, 40.0, 138.0),
            bx(1, 420.0, 234.0, 118.0),
        ];
        let edges = [Edge {
            from: 1,
            to: 0,
            from_dy: 60.0,
            to_dy: 100.0,
        }];
        let p = plan(&boxes, &edges, 200.0);
        assert_eq!(p.top_lanes, 0);
        let d = &emit(&boxes, &edges, &p, 260.0, 120.0, 500.0)[0];
        assert_eq!(path_hits_box(d, &boxes, 260.0), None, "path={d}");
    }

    #[test]
    fn overlapping_edges_in_one_channel_are_offset() {
        let boxes = [
            bx(0, 40.0, 40.0, 138.0),
            bx(1, 420.0, 40.0, 138.0),
        ];
        let edges = [
            Edge { from: 1, to: 0, from_dy: 40.0, to_dy: 40.0 },
            Edge { from: 1, to: 0, from_dy: 60.0, to_dy: 60.0 },
        ];
        let p = plan(&boxes, &edges, 100.0);
        let ds = emit(&boxes, &edges, &p, 260.0, 120.0, 400.0);
        assert_ne!(ds[0], ds[1], "同一走廊的两条边不应完全重合");
        for d in &ds {
            assert_eq!(path_hits_box(d, &boxes, 260.0), None, "path={d}");
        }
    }
}