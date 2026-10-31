//! 画布连线路由：正交折线 + 走廊避让。
//!
//! **为什么不用贝塞尔**：早期实现从源节点直连目标节点，控制点取两点 x 的中点。
//! 一旦两节点跨列（`roles → tenants`，中间隔着 `users`），控制点就落在中间
//! 那一列上，曲线鼓进去横穿整张表。更糟的是**同列**的两个节点
//! （`api_keys.owner_id → users.id`，两张表都在 x=420）：`x1 <= x2` 成立，
//! 于是从源表**右**边缘出线、却要往左走到目标，整条线从源表自己身上穿回去。
//!
//! **现在的模型**只用两种通道，且都不经过任何节点框：
//!
//! - **列间走廊**：相邻列之间恒有 `GAP_X` 的空隙，中线两侧各 60px 内无节点。
//!   同列/相邻列的边都在这条走廊里竖直行走。
//! - **顶部与底部总线**：跨多列时没有任何一条走廊能直达（水平段必然横穿中间
//!   整列），改用画布上下预留的横向车道。车道区在布局阶段就被让出来，节点
//!   整体下移/上移，所以水平段永远不会压到任何表。
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// 同列：绕到该列右侧走廊走 U 形（源和目标 x 完全相同，无法直连）
    SameCol,
    /// 相邻列：在两者之间的那条走廊里直接走
    Adjacent,
    /// 跨列：上半区走顶部总线，下半区走底部总线
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
            let mid_y = (a.y + e.from_dy + b.y + e.to_dy) / 2.0;
            if mid_y <= mid {
                Kind::Bus { top: true }
            } else {
                Kind::Bus { top: false }
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
        Box { col, x, y, h }
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
    fn cross_column_edge_goes_through_bus_and_avoids_middle_table() {
        // roles(第2列) → tenants(第0列)，中间第1列有 users 挡着
        let boxes = [
            bx(0, 40.0, 40.0, 98.0),
            bx(1, 420.0, 40.0, 118.0),
            bx(2, 800.0, 40.0, 98.0),
        ];
        let edges = [Edge {
            from: 2,
            to: 0,
            from_dy: 40.0,
            to_dy: 40.0,
        }];
        let p = plan(&boxes, &edges, 90.0);
        assert_eq!(p.top_lanes, 1, "跨列边应占用一条顶部车道");
        let top = top_reserved(&p);
        // 平移后重新 emit
        let moved: Vec<Box> = boxes
            .iter()
            .map(|b| Box { y: b.y + top, ..*b })
            .collect();
        let d = &emit(&moved, &edges, &p, 260.0, 120.0, 500.0)[0];
        assert_eq!(path_hits_box(d, &moved, 260.0), None, "path={d}");
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