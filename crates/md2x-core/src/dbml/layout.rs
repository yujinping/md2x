//! 画布布局：把 schema 排成可直接内联的 SVG。
//!
//! **布局是选出来的，不是算出来的。** 先把外键依赖压成拓扑层（父表靠左），
//! 再生成一批候选方案（「每层 N 列」的一系列取值 + 网格），对每个候选
//! 真的跑一遍连线路由、用路由结果打分，最后取分最低的。打分看三件事：
//! 有多少条边被迫走画布上下总线（这是「线团」的唯一来源）、跨列距离总和、
//! 以及画布宽高比离容器比例有多远。
//!
//! 为什么不像早期那样写死一串阈值（层高上限 1100px、平均每层表数 < 2.5 就退网格…）：
//! 那些阈值只能拦住当时见过的坏形态。真实 schema 一变形就漏——59 表
//! schema 按层高上限拆成 21 列，55 条边里 44 条挤进顶部总线，排成密不透风
//! 的梳子。改用「生成候选 + 实测打分」后，同一份 schema 是 0 条总线边。
//!
//! 稳定可读仍是硬要求：同一份 DBML 每次生成的图必须完全一致，便于在
//! git 里 review 差异。所以候选生成、装箱、排序全程确定性，不含随机、
//! 不依赖 HashMap 迭代顺序。
//!
//! @author yujinping

use super::model::Schema;
use super::render::esc;
use super::route;

/// 单个表节点的固定宽度。
const NODE_W: f64 = 260.0;
/// 行高（每列一行）。
const ROW_H: f64 = 20.0;
/// 表头高度（只有表名一行时）。
const HD_H: f64 = 30.0;

/// 表头下方再挂一行注释时的额外高度。
///
/// 13 而不是行高 20：注释用 9px 小字，一行 11 高再加 2 的呼吸位就够。
/// 用固定增量而不是「按字数折成两行」，是因为画布是扫读视图——
/// 一眼认出「这是哪张表」比读完注释重要，放不下就截断。
const HD_SUB_H: f64 = 13.0;

/// 画布上表名后面跟的注释最多占多少**显示宽度**。
///
/// 按宽度而不是按字符数：10 个汉字约 95px，10 个英文字符只有 53px——
/// 同样「10 个字」在两种语言下的视觉长度差一倍。表头可用宽度约 236px，
/// 若按字符数截断，中文刚好、英文则明显浪费一半空间、读起来像被砍断。
///
/// 所以预算取 20 个半角单位：中文（全角）算 2 单位 → 恰好 10 个汉字，
/// 与需求「超过 10 个字截断」完全一致；英文（半角）算 1 单位 → 约 20 字符，
/// 既放得下也不会顶到右侧的别名。
const CMT_MAX_W: usize = 20;
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
    head_h(&sc.tables[i]) + sc.tables[i].cols.len() as f64 * ROW_H + 8.0
}

/// 表头高度：有注释行时比常态高出一个副行。
fn head_h(t: &super::model::Table) -> f64 {
    if canvas_comment(t).is_some() {
        HD_H + HD_SUB_H
    } else {
        HD_H
    }
}

/// 取画布上要显示的注释：优先表级注释 `comment`，回退到 `Note`。
///
/// 取表级而不是列级说明，是因为画布表头承载的是「这张表是什么」；
/// 列说明在字典里每行都有，塞进表头只会把表名挤掉。
///
/// 只取首个非空行、去掉 Markdown 装饰标记后再截断：`Note` 常写成
/// 多行块，原样显示会在表头出现 `#` 或 `**`。
fn canvas_comment(t: &super::model::Table) -> Option<String> {
    let raw = if t.comment.trim().is_empty() {
        &t.note
    } else {
        &t.comment
    };
    let line = raw
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())?
        .trim_start_matches('#')
        .trim()
        .trim_matches('*')
        .trim();
    if line.is_empty() {
        return None;
    }
    Some(truncate_cmt(line, CMT_MAX_W))
}

/// 按显示宽度截断，超出部分以 `...` 收尾。
///
/// 全角字符（中日韩文字、全角标点）按 2 个半角单位计，其余按 1。
/// 这是等宽近似，不需要精确排版——目的是中英文在同一预算下
/// 得到相近的视觉长度，而不是让英文只显示一半表头。
///
/// 与 [`truncate`] 的区别：这里超出后保留预算内的字符**再加** `...`，
/// 而 `truncate` 是「总数不超过 max」（吃掉一个字符换省略号）。
/// 注释是给人读的短标签，宁可多三个点的宽度也要保住最后那个字的完整。
fn truncate_cmt(s: &str, budget: usize) -> String {
    let mut w = 0usize;
    let mut out = String::with_capacity(s.len());
    let mut cut = false;
    for c in s.chars() {
        let cw = char_w(c);
        if w + cw > budget {
            cut = true;
            break;
        }
        w += cw;
        out.push(c);
    }
    if cut {
        out.push_str("...");
    }
    out
}

/// 单个字符占几个半角单位。
///
/// 覆盖常用全角区段即可：CJK 统一表意文字、CJK 标点、假名、全角形式。
/// 用 `char::is_ascii` 做快速分流，绝大多数英文注释走第一条分支。
fn char_w(c: char) -> usize {
    if c.is_ascii() {
        return 1;
    }
    let u = c as u32;
    let wide = (0x1100..=0x115F).contains(&u)      // 谚文字母
        || (0x2E80..=0x303E).contains(&u)          // CJK 部首 + 标点
        || (0x3041..=0x33FF).contains(&u)          // 假名 + CJK 兼容
        || (0x3400..=0x4DBF).contains(&u)          // CJK 扩展 A
        || (0x4E00..=0x9FFF).contains(&u)          // CJK 统一表意文字
        || (0xF900..=0xFAFF).contains(&u)          // CJK 兼容表意
        || (0xFE30..=0xFE6F).contains(&u)          // CJK 兼容形式
        || (0xFF00..=0xFF60).contains(&u)          // 全角 ASCII
        || (0xFFE0..=0xFFE6).contains(&u); // 全角符号
    if wide {
        2
    } else {
        1
    }
}

/// 网格列数：取 sqrt(n) 后限幅。
///
/// 上限 10 是因为再宽就必然超过 1.4:1——8 列已是 3120px，横向铺满后
/// 每张表只剩 260px 宽度本该有的可读性，再多就是「看起来表很多」而已。
fn grid_cols_for(n: usize) -> usize {
    ((n as f64).sqrt().ceil() as usize).clamp(1, 10)
}

/// 网格布局：列数取 sqrt(n) 后限幅，行距按最高表对齐——
/// 统一行高让格子对齐，比逐行自适应更像「图」而不是拼贴。
///
/// 网格按 `sc.tables` 原始顺序填充，忽略依赖关系。它给出的是**索引视图**：
/// 找表方便，但看不出谁是父谁是子。所以只在分层布局畸变时才用它。
fn layout_grid(sc: &Schema) -> Vec<Placed> {
    let n = sc.tables.len();
    let cols = grid_cols_for(n);
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

/// 理想的画布宽高比。
///
/// **为什么是 1.4**：画布容器是横向的（860 × 560，比例 1.54），画布比例
/// 接近容器比例时 fit 之后两个方向都恰好铺满，不浪费空间、也不需要滚动。
/// 取 1.4 略小于 1.54，让纵向多留一点余量给高个子表。
///
/// 这是**选型判据**，不是裁剪标准：真实布局不可能精确命中，判据只用来
/// 在几个候选里挑离它最近的。
const TARGET_RATIO: f64 = 1.4;

/// 画布容器的最小可用尺寸（px）。
///
/// 默认模版正文宽 860px，画布容器就是 860 × (100vh - 260)。高度取一个
/// 偏保守的 560（1080 屏下 1080-260=820，扣掉标签栏与工具栏约 250）。
///
/// 这两个数只用于注释与列预算推算，**不参与绝对尺寸评分**——用户可能
/// 拖宽窗口、可能用 `--full-width`，按绝对宽度硬判会做错决策。
const VIEW_W: f64 = 860.0;
#[allow(dead_code)]
const VIEW_H: f64 = 560.0;

/// 保底缩放比（与 render.rs 的 `READABLE_MIN` 同一个数）。
const READABLE_SCALE: f64 = 0.6;

/// 画布尺寸的合理上限：0.6 缩放下刚好铺满容器。
///
/// 只用来定「列预算试到哪儿停」，不作评分依据——大 schema 必然超出，
/// 惩罚它只会选出一个既看不清、线又乱的方案。
const MAX_W: f64 = VIEW_W / READABLE_SCALE; // 1433

/// 把一组成员按目标高度贪心装箱成若干列，返回 `[列][(表下标, 列内 y)]`。
///
/// 贪心而非最优平衡：分列目的是控制画布高度，几像素的列高差看不出来，
/// 而最优平衡要排序、会打乱 DBML 里的原始表序——同一份文件两次渲染
/// 顺序必须一致，否则 git diff 里画布天天变。
fn pack_columns(
    members: &[usize],
    target_h: f64,
    h_of: impl Fn(usize) -> f64,
) -> Vec<Vec<(usize, f64)>> {
    let mut cols: Vec<Vec<(usize, f64)>> = Vec::new();
    let mut cur: Vec<(usize, f64)> = Vec::new();
    let mut y = 0.0;
    for &i in members {
        let h = h_of(i);
        // 单张表就超过目标高度时不换列——换也放不下，只会多出一个空列
        if !cur.is_empty() && y + h > target_h {
            cols.push(std::mem::take(&mut cur));
            y = 0.0;
        }
        cur.push((i, y));
        y += h + GAP_Y;
    }
    if !cur.is_empty() {
        cols.push(cur);
    }
    cols
}

/// 分层布局：依赖流按拓扑层分列，孤立表单独成块挂在右侧。
///
/// **分列依据是总面积，不是固定高度上限。** 早期版本给每层写死 1100px 上限，
/// 超了就横向另起子列——在真实 schema 上是灾难：59 张表按拓扑只有 5 层
/// （层 0 有 28 张、层 1 有 21 张），限高把这两层摊成 10 个和 6 个子列，
/// 画布拉成 8060px 宽的细长条。宽度一爆，绝大多数外键都变成跨列边，
/// 只能挤到顶部/底部总线上，44 条线在 400px 高的区域里排成梳子。
///
/// 反过来「每层恒占一列」同样是灾难：同一份 schema 变成 1980×9492 的
/// 竖条。两个极端都是因为**决策依据是局部量**（单层高度）而画布好坏是
/// **整体量**（宽高比）。
///
/// 现在的做法先在整图层面定目标：总列高 ÷ 列距 得到「把所有表摊成一条
/// 竖列需要多高」，再按 1.4:1 反解出需要的总列数，最后按各层高度占比
/// 把列数分给每层。层高的绝对数字不再参与决策，只有层与层的相对比例
/// 起作用——这正是跨层依赖流该有的权重。
///
/// 孤立表（无任何外键）必须抽出来：它们没有依赖关系，混在层 0 里只是
/// 把该列撑高（本例 14 张孤立表把层 0 的列高从 3287 顶到 9412），
/// 却不贡献任何结构信息。单独成一两块窄列挂在右侧，扫读时已知可跳过。
fn layout_layers(
    sc: &Schema,
    layer: &[usize],
    max_layer: usize,
    isolated: &[bool],
    extra_cols: usize,
) -> Vec<Placed> {
    let n = sc.tables.len();
    let h_of = |i: usize| node_h(sc, i);

    let mut members: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    let mut iso: Vec<usize> = Vec::new();
    for i in 0..n {
        if isolated[i] {
            iso.push(i);
        } else {
            members[layer[i]].push(i);
        }
    }
    let layer_h: Vec<f64> = members
        .iter()
        .map(|m| m.iter().map(|&i| h_of(i) + GAP_Y).sum())
        .collect();
    let total_ch: f64 = layer_h.iter().sum();
    let iso_ch: f64 = iso.iter().map(|&i| h_of(i) + GAP_Y).sum();
    let pitch = NODE_W + GAP_X;

    // 每层至少一列；`extra_cols` 是按全局宽高比算出来的额外列，按各层高度占比分配
    let mut cols_per_layer = vec![1usize; max_layer + 1];
    if extra_cols > 0 && total_ch > 0.0 {
        let shares: Vec<f64> = layer_h
            .iter()
            .map(|h| h / total_ch * extra_cols as f64)
            .collect();
        let mut given = 0usize;
        let mut rest: Vec<(f64, usize)> = Vec::with_capacity(shares.len());
        for (l, s) in shares.iter().enumerate() {
            let g = s.floor() as usize;
            cols_per_layer[l] += g;
            given += g;
            rest.push((s - s.floor(), l));
        }
        rest.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (j, (_, l)) in rest.iter().enumerate() {
            if j >= extra_cols.saturating_sub(given) {
                break;
            }
            cols_per_layer[*l] += 1;
        }
    }

    // 逐层装箱：目标列高按该层分到的列数均分
    let mut placed: Vec<Placed> = Vec::with_capacity(n);
    let mut slot = 0usize;
    for l in 0..=max_layer {
        if members[l].is_empty() {
            continue;
        }
        let target = layer_h[l] / cols_per_layer[l] as f64;
        for col in pack_columns(&members[l], target, h_of) {
            let x = PAD + slot as f64 * pitch;
            for (i, y) in col {
                placed.push(Placed {
                    idx: i,
                    col: slot,
                    x,
                    y: PAD + y,
                    h: h_of(i),
                });
            }
            slot += 1;
        }
    }

    // 孤立表块：给它们一个**独立的、自成方块的**高度，不跟随依赖流区。
    //
    // 早期让块高对齐依赖流区（`flow_h - PAD`），在候选评估里捅了大漏：
    // 依赖流变矮时孤立表块仍被拉到同高，于是孤立表被拆成许多只装一两张表
    // 的矮列，列数暴涨、单条边要跨的列数暴涨，路由成本被这一个局部决策
    // 顶到天上（实测 extra=1 时成本 4235，而 extra=0 只 22），最后选型
    // 只能退回竖条。
    //
    // 现在按「块自身摊成接近方形」定高度：4 列的宽度对应 4 列的高度，
    // 孤立表之间没有连线，形状怎么摆都不影响路由，别去干扰主体布局。
    if !iso.is_empty() {
        let n_cols = ((iso_ch / (4.0 * pitch)).sqrt().round() as usize).clamp(1, 4);
        let target = (iso_ch / n_cols as f64).max(400.0);
        for col in pack_columns(&iso, target, h_of) {
            let x = PAD + slot as f64 * pitch;
            for (i, y) in col {
                placed.push(Placed {
                    idx: i,
                    col: slot,
                    x,
                    y: PAD + y,
                    h: h_of(i),
                });
            }
            slot += 1;
        }
    }

    placed
}

/// 由一个布局方案估出的路由成本：**越小越好**。
///
/// 三个量按重要性叠加，权重来自实测（59 表 schema 上各方案的实际差别）：
///
/// 1. **总线边数**（权重 10）：每多一条挤进顶部/底部车道的线，画布就多一道
///    横贯全宽的折线。这是「线团」的唯一来源，必须压到最低——它是主导项。
/// 2. **跨列距离总和**（权重 1）：跨 8 列和跨 2 列都要花一条车道，但前者
///    横穿整张画布、后者只跨两列，视觉负担差得多。
/// 3. **宽高比偏离**（权重 3）：纯按前两项选会一路选到「每层一列」，
///    得到 2280×5619 的竖条——线是漂亮了，可 fit 进窗口后字全糊掉。
///    这一项把画布拉回可用形状。
///
/// 之所以能这样算，是因为 `route::plan` 本来就只依赖节点几何，不需要真的
/// 生成 SVG 字符串——单次估算成本约 O(边数 × 中间列节点数)，59 表下
/// 十几毫秒，一次性开销可以接受。
fn route_cost(sc: &Schema, placed: &[Placed], find: &dyn Fn(&str) -> Option<usize>) -> f64 {
    let geo: Vec<route::Box> = placed
        .iter()
        .map(|p| route::Box {
            col: p.col,
            x: p.x,
            y: p.y,
            w: NODE_W,
            h: p.h,
        })
        .collect();
    let mut edges: Vec<route::Edge> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for r in &sc.refs {
        let (Some(f), Some(t)) = (find(&r.from_table), find(&r.to_table)) else {
            continue;
        };
        if f == t || !seen.insert((f.min(t), f.max(t))) {
            continue;
        }
        let fy = head_h(&sc.tables[f]) + col_row(sc, f, &r.from_col) * ROW_H + ROW_H / 2.0;
        let ty = head_h(&sc.tables[t]) + col_row(sc, t, &r.to_col) * ROW_H + ROW_H / 2.0;
        edges.push(route::Edge {
            from: i_of(placed, f),
            to: i_of(placed, t),
            from_dy: fy,
            to_dy: ty,
        });
    }
    let mid = placed
        .iter()
        .map(|p| p.y + p.h / 2.0)
        .fold(0.0f64, |a, b| a + b)
        / placed.len().max(1) as f64;
    let plan = route::plan(&geo, &edges, mid);

    // 总线车道数 = 必须横贯全宽的线数
    let bus = plan.top_lanes + plan.bottom_lanes;
    // 跨列距离：col 差值之和（相邻列不花车道，但从第 2 列起就要）
    let span: usize = edges
        .iter()
        .map(|e| geo[e.from].col.abs_diff(geo[e.to].col).saturating_sub(1))
        .sum();

    let w = placed.iter().map(|p| p.x + NODE_W).fold(0.0f64, f64::max) + route::RIGHT_PAD;
    let h = placed.iter().map(|p| p.y + p.h).fold(0.0f64, f64::max) + PAD;

    // 形状惩罚：**宽高比**，不是绝对尺寸。
    //
    // 早期想「把画布压进一屏」而惩罚绝对尺寸，实测发现这是伪目标：
    // 59 张表 × 260px 节点宽本身就占地数千像素，任何布局都塞不进
    // 860×560 的容器。用户靠缩放控件和滚动看细节，这是既定事实。
    //
    // 真正能优化的是**形状**：为什么 4.78:1 的横条读不出来？因为 fit 之后
    // 纵向还剩大片空白（浪费），而横向细线挤在一起。为什么 0.41:1 的竖条
    // 也不适？因为容器是横向的。1.4:1 附近两者都不浪费——这是唯一
    // 稳定可判据的目标。
    let ratio = w / h.max(1.0);
    let dev = (ratio / TARGET_RATIO).max(TARGET_RATIO / ratio);
    let shape_pen = dev.powi(2) * 40.0;

    bus as f64 * 10.0 + span as f64 + shape_pen
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

    // 孤立表：没有任何外键连到它、它也不连出去。
    //
    // 必须单独识别，不能让它们混进层 0（详见 `layout_layers` 的注释）。
    // 自引用（`parent_id > id`）不算连通——它连的是自己，对分层没有贡献。
    let isolated: Vec<bool> = {
        let mut touched = vec![false; n];
        for r in &sc.refs {
            if let (Some(f), Some(t)) = (find(&r.from_table), find(&r.to_table)) {
                if f != t {
                    touched[f] = true;
                    touched[t] = true;
                }
            }
        }
        touched.iter().map(|x| !x).collect()
    };

    // ---- 布局选型：用真实路由成本挑，不靠猜 ----
    //
    // 布局好坏本质上取决于「连线的走法有多绕」：跨列边越多，挤进总线车道的
    // 线越多，画布顶部就越像一把梳子。而这个量**只有把节点真排一遍、
    // 把边真规划一遍才知道**。所以这里不写任何启发式阈值，直接对几个候选
    // 方案各算一次路由成本，取最小的那个。
    //
    // 候选 = 网格 + 「每层 1 列」到「每层多列」的一串列预算。列预算的
    // 上限由可读宽度推出（再宽就必须横向滚动），不会漫无边际地试。
    let grid_cols = grid_cols_for(n);
    let grid_ratio = {
        let row_h = placed_row_height(sc) + GAP_Y;
        let w = PAD * 2.0 + grid_cols as f64 * (NODE_W + GAP_X);
        let h = PAD * 2.0 + row_h * (n as f64 / grid_cols as f64).ceil();
        w / h.max(1.0)
    };

    // 列预算从 0 一路试到「画布明显过宽」为止。
    //
    // 上限用 MAX_W 的两倍：再宽下去任何惩罚项都救不回来，试了也是浪费。
    // `extra` 是**额外列总数**，不是每层加一列——extra=1 意味着只给
    // 最高的那一层多分一个子列。
    let max_extra = {
        let by_w = ((MAX_W * 2.0) / (NODE_W + GAP_X)).floor() as usize;
        by_w.saturating_sub(max_layer + 1)
    };

    // 对每个候选做一次「布局 → 路由规划」，用返回值打分
    let mut best: Option<(f64, Vec<Placed>)> = None;
    for extra in 0..=max_extra {
        let p = layout_layers(sc, &layer, max_layer, &isolated, extra);
        let cost = route_cost(sc, &p, &find);
        if best.as_ref().map(|(c, _)| cost < *c).unwrap_or(true) {
            best = Some((cost, p));
        }
    }
    // 网格作为对照：只有它明显更好时才赢（分层给出依赖流，形状相同就优先分层）
    let grid_placed = layout_grid(sc);
    let grid_cost = route_cost(sc, &grid_placed, &find) * 0.75;
    let placed: Vec<Placed> = match best {
        Some((c, _)) if grid_cost < c => grid_placed,
        Some((_, p)) => p,
        None => grid_placed,
    };
    let _ = grid_ratio;

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
            let fy = head_h(&sc.tables[f]) + col_row(sc, f, &r.from_col) * ROW_H + ROW_H / 2.0;
            let ty = head_h(&sc.tables[t]) + col_row(sc, t, &r.to_col) * ROW_H + ROW_H / 2.0;
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
            w: NODE_W,
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
    // 计划里记的穿行高度是绝对 y，必须跟着节点一起下移，否则水平段会
    // 落在原位置横穿节点（详见 `Plan::shift` 的注释）。
    let mut plan = plan;
    plan.shift(top_pad);
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
        let hh = head_h(t);
        let cmt = canvas_comment(t);
        svg.push_str(&format!(
            "<g class=\"dx-tbl-hit\" data-anchor=\"dx-t-{anc}\" data-k=\"{key}\">",
            anc = esc(&t.name.replace('.', "--").replace(' ', "_")),
            key = esc(&format!(
                "{} {} {}",
                t.name,
                cmt.clone().unwrap_or_default(),
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
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{NODE_W}\" height=\"{hh:.1}\" fill=\"{}\"/>",
                p.x,
                p.y,
                esc(c)
            )),
            None => svg.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{NODE_W}\" height=\"{hh:.1}\" class=\"dx-node-hd\"/>",
                p.x, p.y
            )),
        }
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" class=\"dx-node-line\"/>",
            p.x,
            p.y + hh,
            p.x + NODE_W,
            p.y + hh
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"13\" font-weight=\"600\" class=\"dx-node-title\">{}</text>",
            p.x + 12.0,
            p.y + 19.0,
            esc(short)
        ));
        // 表名后的注释：中英文表名都要带上 COMMENT 名字，超出 10 字截断。
        //
        // 放在**第二行**而不是表名右侧同一行：表名本身可能就很长
        // （`subscriptions` 这类 13 字符的英文表名已经占掉半个表头），
        // 同行并排会把两者都压成省略号。副行还能容纳中文注释的自然长度，
        // 且不改变表名行的视觉重量——表名始终是表头的主信息。
        if let Some(c) = &cmt {
            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9.5\" class=\"dx-node-cmt\">{}</text>",
                p.x + 12.0,
                p.y + HD_H + 9.5,
                esc(c)
            ));
        }
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
            let cy = p.y + hh + ci as f64 * ROW_H + 14.0;
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
        .map(|t| head_h(t) + t.cols.len() as f64 * ROW_H + 8.0)
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