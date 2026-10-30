use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// 一篇被聚合的 Markdown 文档
#[derive(Debug, Clone)]
pub struct DocEntry {
    /// 绝对路径（用于读取内容与解析相对链接）
    pub path: PathBuf,
    /// 相对文件夹根的路径，统一用 `/` 分隔（用于展示与生成锚点前缀）
    pub rel_path: String,
    /// 展示标题：优先取 front matter 的 title，其次用文件名
    pub title: String,
    /// 锚点前缀，形如 `doc-0-`；同时作为该文档 section 的 DOM id
    pub anchor_prefix: String,
    /// 该文档 section 的 DOM id（等于 anchor_prefix 去掉结尾 `-`）
    pub section_id: String,
}

/// 聚合结果：文档列表 + 相对路径到锚点的索引
pub struct Collected {
    /// 按展示顺序排列的文档（深度优先拍平，与 GUI 文件树一致）
    pub docs: Vec<DocEntry>,
    /// 相对路径（`/` 分隔）→ section id，用于链接重写时查表
    pub by_rel: HashMap<String, String>,
}

/// 递归收集文件夹内的所有 Markdown 文档，按默认顺序排列。
///
/// 默认顺序见 [`collect_with_config`]。
pub fn collect(root: &Path) -> Collected {
    collect_with_config(root, &[], &[])
}

/// 递归收集文件夹内的所有 Markdown 文档，并可按用户自定义顺序重排。
///
/// 等价于不带排除项的 [`collect_with_config`]，保留此函数是为了让只关心
/// 顺序的调用方（CLI、DOCX 拼接）不必构造完整配置。
pub fn collect_with_order(root: &Path, order: &[String]) -> Collected {
    collect_with_config(root, order, &[])
}

/// 递归收集文件夹内的 Markdown 文档，支持自定义同层顺序与排除项。
///
/// `order` 是用户保存的**同层顺序**，元素为相对 `root` 的 `/` 分隔路径
/// （目录项也占一个位置，因此可以把文件拖到目录之前）。空切片即默认顺序。
///
/// `exclude` 是被排除的相对路径，既可以是文件，也可以是目录（目录连同
/// 其下所有内容一并排除）。文件树里这些节点显示为灰色，导出时不参与。
///
/// 遍历规则（与 GUI 文件树 `scan_tree` 完全一致，保证「看到什么就导出什么」）：
/// - 本层条目按 [`SiblingOrder`] 排序：用户 `order` 里出现过的按其在列表里的
///   位置排，未出现的排在后面；
/// - 都未排序时**目录在前、文件在后**，各自按名称排序；
/// - 顶层 `README.md` 默认置顶（`docs/README.md` 视作普通文档，
///   否则读者会把子目录索引误当成全文入口）；
/// - 随后按排序结果深度优先拍平：遇到目录就地深入，遇到文件就地输出。
///
/// `order` / `exclude` 里已不存在的路径（文件被删/改名）自然被忽略。
pub fn collect_with_config(root: &Path, order: &[String], exclude: &[String]) -> Collected {
    let ord = SiblingOrder::new(order, top_readme_rel(root));
    let mut files = Vec::new();
    walk_depth_first(root, root, &mut files, 0, &ord, exclude);

    let mut docs = Vec::with_capacity(files.len());
    let mut by_rel = HashMap::with_capacity(files.len());
    for (i, path) in files.iter().enumerate() {
        let rel_path = relative_path(path, root);
        let section_id = format!("doc-{}", i);
        let title = read_title(path);
        docs.push(DocEntry {
            path: path.clone(),
            rel_path: rel_path.clone(),
            title,
            anchor_prefix: format!("{}-", section_id),
            section_id: section_id.clone(),
        });
        by_rel.insert(rel_path, section_id);
    }

    Collected { docs, by_rel }
}

/// 同层排序器：把「用户自定义顺序 + 默认规则」编译成一个可复用的比较器。
///
/// 抽成独立类型而不是留在 `collect_with_config` 里，是因为**导出收集**与
/// **GUI 文件树**必须共用同一套排序规则，否则「树上看到的顺序」与「导出产物
/// 的顺序」会静默漂移——而两边持有的数据结构不同（`PathBuf` / `TreeNode`），
/// 只能共用比较逻辑，不能共用整段遍历代码。
///
/// 搜索顺序沿用文件管理器的通行心智：目录与文件在同一层**交错**排列，
/// 而不是「所有目录都在前、所有文件都在后」——只有允许交错，用户把
/// `README.md` 拖到 `docs/` 之上时导出的顺序才与树里看到的完全一致。
pub struct SiblingOrder {
    /// 用户顺序：相对路径 → 下标
    rank: HashMap<String, usize>,
    /// 顶层 README 的相对路径（未被用户显式排序时置顶）
    top_readme: Option<String>,
}

impl SiblingOrder {
    /// `top_readme` 传顶层 README 的相对路径（无则为 None）。
    pub fn new(order: &[String], top_readme: Option<String>) -> Self {
        Self {
            rank: order
                .iter()
                .enumerate()
                .map(|(i, p)| (p.clone(), i))
                .collect(),
            top_readme,
        }
    }

    /// 比较同层两个条目。`bool` 位表示是否为目录。
    pub fn cmp(&self, a: (&str, bool), b: (&str, bool)) -> Ordering {
        match (self.rank.get(a.0), self.rank.get(b.0)) {
            (Some(x), Some(y)) => return x.cmp(y),
            // 显式排过序的一律排在未排序的前面：用户只调了部分条目时，
            // 那些被手动指定的必须保持相对位置，不能让默认规则插进来打乱
            (Some(_), None) => return Ordering::Less,
            (None, Some(_)) => return Ordering::Greater,
            (None, None) => {}
        }
        // 置顶判定先于「目录在前」：用户对 README 的期待是「合集的第一篇」，
        // 若被同级目录压到后面，标题页就排在了正文之后。
        // 文件树用的是同一个比较器，因此树里 README 也在目录之上，两边仍一致
        let a_top = self.top_readme.as_deref() == Some(a.0);
        let b_top = self.top_readme.as_deref() == Some(b.0);
        match (a_top, b_top) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        // 都未排序：目录在前、文件在后（文件管理器的默认约定）
        if a.1 != b.1 {
            return b.1.cmp(&a.1);
        }
        name_lower(a.0).cmp(&name_lower(b.0))
    }
}

/// 深度优先遍历：按同层顺序就地展开——遇到目录立刻深入，遇到文件立刻输出。
///
/// 这一函数与 GUI 的 `scan_tree` 是同一套顺序约定的两侧实现：树的扁平化结果
/// 等于这里的输出。任何一侧单独改动（例如把目录统一挪到最前）都会让
/// 「拖动树节点排导出顺序」这个前提失效。
fn walk_depth_first(
    dir: &Path,
    root: &Path,
    out: &mut Vec<PathBuf>,
    depth: usize,
    ord: &SiblingOrder,
    exclude: &[String],
) {
    // 深度上限与 scan_tree 保持一致，避免异常深的目录把栈打满
    if depth > 12 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    // 相对路径先算好再排序：比较函数会被调用 O(n log n) 次，
    // 在里面现算 rel 会反复分配字符串
    let mut entries: Vec<(String, PathBuf, bool)> = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        // 符号链接不跟随：既能避免成环，也能避免跳到用户意外的大目录
        if ft.is_symlink() {
            continue;
        }
        let rel = relative_path(&path, root);
        // 用户排除优先于一切：被排除的目录不再深入，被排除的文件不进产物
        if is_excluded(&rel, exclude) {
            continue;
        }
        if ft.is_dir() {
            let name = entry.file_name();
            if should_skip_dir(&name.to_string_lossy()) {
                continue;
            }
            entries.push((rel, path, true));
        } else if ft.is_file() && is_markdown(&path) {
            entries.push((rel, path, false));
        }
    }

    entries.sort_by(|a, b| ord.cmp((&a.0, a.2), (&b.0, b.2)));

    for (_, path, is_dir) in entries {
        if is_dir {
            walk_depth_first(&path, root, out, depth + 1, ord, exclude);
        } else {
            out.push(path);
        }
    }
}

/// 路径是否命中排除列表。
///
/// 命中规则：完全相等（文件被排除），或是某个被排除目录的后代
/// （排除目录时连同内容一起排除，无需把每个子文件都写进列表）。
pub fn is_excluded(rel: &str, exclude: &[String]) -> bool {
    exclude.iter().any(|e| {
        rel == e
            || (rel.len() > e.len()
                && rel.starts_with(e.as_str())
                && rel.as_bytes().get(e.len()) == Some(&b'/'))
    })
}

/// 顶层 README 的相对路径（不区分大小写匹配文件名主干）。
///
/// 刻意只看**顶层**：`docs/README.md` 是那个目录的索引页，把它顶到整个
/// 合集最前面会让读者误以为那是全文入口。`index.md` 也不再特殊对待——
/// 用户在多数场景下用 README 表达入口意图。
///
/// 公开是为了让 GUI 文件树构造同一个 [`SiblingOrder`]，两处共用置顶规则。
pub fn top_readme_rel(root: &Path) -> Option<String> {
    let rd = std::fs::read_dir(root).ok()?;
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_markdown(Path::new(&name)) {
            continue;
        }
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name);
        if stem.eq_ignore_ascii_case("readme") {
            return Some(name);
        }
    }
    None
}

/// 取路径最后一个分量的名称（小写），用于稳定排序。
///
/// 与 `scan_tree` 的 `to_lowercase` 口径一致：两侧若一处按全路径、
/// 一处按名称，同层排序结果会不同。
fn name_lower(rel: &str) -> String {
    rel.rsplit('/').next().unwrap_or(rel).to_lowercase()
}

/// 是否应跳过该目录名（隐藏目录、依赖/产物目录）。
///
/// 抽成公开函数是为了让 GUI 的文件树（`scan_tree`）与导出收集共用同一份规则：
/// 两处各写一份会漂移，导致「树里能看到的 md 导不出来」这类困惑。
pub fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target")
}

/// 是否为待收集的 Markdown 文件：**只认 `.md`**。
///
/// 早期版本同时接受 `.markdown`，但文件树（`scan_tree`）只认 `.md`，
/// 两处不一致会让「树里没有、却出现在导出产物里」的情况无法解释。
/// 归一为 `.md` 单一后缀后，浏览与导出看到的是同一批文件。
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

/// 计算相对 root 的路径，统一 `/` 分隔
pub fn relative_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// 读取文档标题：优先 front matter 的 title，其次取首个 h1，最后退回文件名
fn read_title(path: &Path) -> String {
    let fallback = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();

    let Ok(md) = std::fs::read_to_string(path) else {
        return fallback;
    };

    // front matter 的 title 优先（复用现有解析器，口径与单文件导出一致）
    if let (Some(meta), _) = crate::converter::parse_front_matter(&md) {
        if let Some(t) = meta.get("title") {
            if !t.trim().is_empty() {
                return t.trim().to_string();
            }
        }
    }

    // 其次取正文中首个 h1
    for line in md.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("# ") {
            let t = rest.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }

    fallback
}

/// 把任意相对路径解析为「相对 root 的 `/` 分隔路径」，消解 `.` 与 `..`。
///
/// 用于把 `[a](./sub/b.md)`、`[a](../b.md)` 这类链接映射回收集时的键。
/// `..` 越过根目录时不再上浮（`root` 本身视为边界），避免算出目录外路径。
pub fn normalize_rel(from_rel_dir: &str, link: &str) -> String {
    let mut stack: Vec<&str> = from_rel_dir
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    for seg in link.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("md2x-agg-{}", tag));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn names(c: &Collected) -> Vec<&str> {
        c.docs.iter().map(|x| x.rel_path.as_str()).collect()
    }

    #[test]
    fn 递归收集嵌套目录的md() {
        let d = tmpdir("nested");
        write(d.as_path(), "a.md", "# A");
        write(d.as_path(), "sub/b.md", "# B");
        write(d.as_path(), "sub/deep/c.md", "# C");
        write(d.as_path(), "sub/ignore.txt", "not md");

        let c = collect(d.as_path());
        let mut got = names(&c);
        got.sort();
        assert_eq!(got, vec!["a.md", "sub/b.md", "sub/deep/c.md"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 只收集md后缀且忽略markdown() {
        // 设计变更：早期同时收 .md/.markdown，但文件树只认 .md，
        // 两处不一致会让「树里没有、产物里却有」无法解释，故统一为 .md
        let d = tmpdir("onlymd");
        write(d.as_path(), "a.md", "# A");
        write(d.as_path(), "b.markdown", "# B");
        write(d.as_path(), "c.MD", "# C");

        let c = collect(d.as_path());
        let mut got = names(&c);
        got.sort();
        assert_eq!(got, vec!["a.md", "c.MD"], ".markdown 不应被收集");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn readme排在首位() {
        let d = tmpdir("readme");
        write(d.as_path(), "zebra.md", "# Z");
        write(d.as_path(), "README.md", "# R");
        write(d.as_path(), "apple.md", "# A");

        let c = collect(d.as_path());
        assert_eq!(c.docs[0].rel_path, "README.md");
        assert_eq!(c.docs[0].title, "R", "应取正文首个 h1 作为标题");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 跳过隐藏目录与依赖目录() {
        let d = tmpdir("skip");
        write(d.as_path(), "keep.md", "# K");
        write(d.as_path(), ".git/x.md", "# hidden");
        write(d.as_path(), "node_modules/pkg/y.md", "# dep");

        let c = collect(d.as_path());
        assert_eq!(c.docs.len(), 1);
        assert_eq!(c.docs[0].rel_path, "keep.md");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 每篇文档拿到唯一锚点前缀() {
        let d = tmpdir("anchor");
        write(d.as_path(), "a.md", "# A");
        write(d.as_path(), "b.md", "# B");
        write(d.as_path(), "c.md", "# C");

        let c = collect(d.as_path());
        let ids: Vec<&str> = c.docs.iter().map(|x| x.section_id.as_str()).collect();
        // 去重后数量不变即说明唯一
        let mut uniq = ids.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(ids.len(), uniq.len(), "section_id 必须互不相同: {:?}", ids);
        assert_eq!(c.docs[0].anchor_prefix, "doc-0-");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 相对路径索引可用于查表() {
        let d = tmpdir("index");
        write(d.as_path(), "sub/b.md", "# B");

        let c = collect(d.as_path());
        let sid = c.by_rel.get("sub/b.md").unwrap();
        assert_eq!(*sid, c.docs[0].section_id);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 标题优先取front_matter() {
        let d = tmpdir("fm");
        write(
            d.as_path(),
            "a.md",
            "---\ntitle: 来自元数据\n---\n\n# 来自h1",
        );
        let c = collect(d.as_path());
        assert_eq!(c.docs[0].title, "来自元数据");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 无标题时退回文件名() {
        let d = tmpdir("nofm");
        write(d.as_path(), "plain.md", "只有正文，没有标题");
        let c = collect(d.as_path());
        assert_eq!(c.docs[0].title, "plain");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 消解当前目录前缀() {
        assert_eq!(normalize_rel("", "b.md"), "b.md");
        assert_eq!(normalize_rel("sub", "b.md"), "sub/b.md");
        assert_eq!(normalize_rel("sub", "./b.md"), "sub/b.md");
    }

    #[test]
    fn 消解上级目录() {
        assert_eq!(normalize_rel("sub/deep", "../b.md"), "sub/b.md");
        assert_eq!(normalize_rel("sub", "../b.md"), "b.md");
        assert_eq!(normalize_rel("sub/deep", "../../b.md"), "b.md");
    }

    #[test]
    fn 越过根目录不再上浮() {
        // root 是边界，.. 不应产出目录外路径
        assert_eq!(normalize_rel("", "../b.md"), "b.md");
        assert_eq!(normalize_rel("sub", "../../b.md"), "b.md");
    }

    #[test]
    fn 识别md后缀为唯一入口() {
        assert!(is_markdown(Path::new("a.md")));
        assert!(is_markdown(Path::new("a.MD")));
        assert!(
            !is_markdown(Path::new("a.markdown")),
            ".markdown 应被排除，与文件树规则保持一致"
        );
        assert!(!is_markdown(Path::new("a.txt")));
        assert!(!is_markdown(Path::new("a.mdx")));
    }

    #[test]
    fn 自定义顺序生效() {
        let d = tmpdir("order");
        write(d.as_path(), "README.md", "# 索引");
        write(d.as_path(), "req.md", "# 需求");
        write(d.as_path(), "design.md", "# 设计");

        // 默认顺序：README 置顶，其余字典序
        let c = collect(d.as_path());
        assert_eq!(names(&c), vec!["README.md", "design.md", "req.md"]);

        // 手动顺序：需求在前、README 被拖到中间
        let order = vec![
            "req.md".to_string(),
            "README.md".to_string(),
            "design.md".to_string(),
        ];
        let c = collect_with_order(d.as_path(), &order);
        assert_eq!(names(&c), vec!["req.md", "README.md", "design.md"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 顺序中缺失的新文档追加到末尾() {
        // 用户排好序后又新增 api.md：新文件不应插队，也不应冲掉既有顺序
        let d = tmpdir("ordernew");
        write(d.as_path(), "a.md", "# A");
        write(d.as_path(), "b.md", "# B");
        write(d.as_path(), "new.md", "# N");

        let order = vec!["b.md".to_string(), "a.md".to_string()];
        let c = collect_with_order(d.as_path(), &order);
        assert_eq!(names(&c), vec!["b.md", "a.md", "new.md"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 顺序中已删除的文档被忽略() {
        let d = tmpdir("orderdel");
        write(d.as_path(), "a.md", "# A");

        let order = vec!["gone.md".to_string(), "a.md".to_string()];
        let c = collect_with_order(d.as_path(), &order);
        assert_eq!(c.docs.len(), 1);
        assert_eq!(c.docs[0].rel_path, "a.md");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 只有顶层readme置顶() {
        // docs/README.md 是子目录索引，不该被误认为全文入口
        let d = tmpdir("subreadme");
        write(d.as_path(), "README.md", "# 顶层");
        write(d.as_path(), "docs/README.md", "# 子索引");
        write(d.as_path(), "zzz.md", "# Z");

        let c = collect(d.as_path());
        assert_eq!(
            names(&c),
            vec!["README.md", "docs/README.md", "zzz.md"],
            "顶层 README 置顶，其余按「目录在前 + 名称」"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 跳过名单覆盖三类依赖目录() {
        assert!(should_skip_dir(".git"));
        assert!(should_skip_dir(".hidden"));
        assert!(should_skip_dir("node_modules"));
        assert!(should_skip_dir("target"));
        assert!(!should_skip_dir("docs"));
        assert!(!should_skip_dir("src"));
    }

    #[test]
    fn 顺序与文件树深度优先一致() {
        // 核心保障：树里看到的排列必须等于导出顺序，
        // 否则「拖动树就能排导出顺序」这个前提不成立。
        // 树里 docs 与文件同层时目录在前，所以先展开 sub 再输出本层文件
        let d = tmpdir("dfs");
        write(d.as_path(), "a.md", "# A");
        write(d.as_path(), "z.md", "# Z");
        write(d.as_path(), "sub/x.md", "# X");

        let c = collect(d.as_path());
        assert_eq!(names(&c), vec!["sub/x.md", "a.md", "z.md"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 目录与文件可在同层交错排序() {
        // 树里文件和目录是平铺一排的，用户把文件拖到目录之上必须生效，
        // 否则「拖动即所见」只对纯文件列表成立
        let d = tmpdir("interleave");
        write(d.as_path(), "a.md", "# A");
        write(d.as_path(), "z.md", "# Z");
        write(d.as_path(), "sub/x.md", "# X");

        let order = vec![
            "z.md".to_string(),
            "sub".to_string(),
            "a.md".to_string(),
        ];
        let c = collect_with_config(d.as_path(), &order, &[]);
        assert_eq!(names(&c), vec!["z.md", "sub/x.md", "a.md"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 同层自定义顺序生效且不影响层级() {
        let d = tmpdir("sibling");
        write(d.as_path(), "a.md", "# A");
        write(d.as_path(), "b.md", "# B");
        write(d.as_path(), "sub/x.md", "# X");

        // 只把同层的 b 提到 a 前面；sub 未出现在 order 里，
        // 按「显式排过的在前」排到两者之后
        let order = vec!["b.md".to_string(), "a.md".to_string()];
        let c = collect_with_config(d.as_path(), &order, &[]);
        assert_eq!(names(&c), vec!["b.md", "a.md", "sub/x.md"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 排除单个文件() {
        let d = tmpdir("exclfile");
        write(d.as_path(), "keep.md", "# K");
        write(d.as_path(), "skip.md", "# S");

        let c = collect_with_config(d.as_path(), &[], &["skip.md".to_string()]);
        assert_eq!(names(&c), vec!["keep.md"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 排除目录连同其内容一并排除() {
        let d = tmpdir("excldir");
        write(d.as_path(), "keep.md", "# K");
        write(d.as_path(), "old/a.md", "# A");
        write(d.as_path(), "old/deep/b.md", "# B");

        let c = collect_with_config(d.as_path(), &[], &["old".to_string()]);
        assert_eq!(names(&c), vec!["keep.md"], "目录排除应递归生效");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 排除前缀相近的兄弟目录不被误伤() {
        // 排除 old 不应连带排除 old2：只认分隔符边界
        let d = tmpdir("exclprefix");
        write(d.as_path(), "old/a.md", "# A");
        write(d.as_path(), "old2/b.md", "# B");

        let c = collect_with_config(d.as_path(), &[], &["old".to_string()]);
        assert_eq!(names(&c), vec!["old2/b.md"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn 排序器在目录与文件之间保持目录优先() {
        // 直接对比较器下断言：这是 collect 与 scan_tree 共用的唯一真相源
        let ord = SiblingOrder::new(&[], None);
        assert_eq!(ord.cmp(("sub", true), ("a.md", false)), Ordering::Less);
        assert_eq!(ord.cmp(("b", true), ("a", true)), Ordering::Greater);
    }

    #[test]
    fn 排序器里显式顺序压过目录优先() {
        let order = vec!["a.md".to_string(), "sub".to_string()];
        let ord = SiblingOrder::new(&order, None);
        assert_eq!(ord.cmp(("a.md", false), ("sub", true)), Ordering::Less);
    }

    #[test]
    fn 排序器把顶层readme置顶但可被用户覆盖() {
        let ord = SiblingOrder::new(&[], Some("README.md".to_string()));
        assert_eq!(ord.cmp(("README.md", false), ("a.md", false)), Ordering::Less);
        // 用户把它拖到 a.md 之后：显式顺序优先于置顶规则
        let order = vec!["a.md".to_string(), "README.md".to_string()];
        let ord = SiblingOrder::new(&order, Some("README.md".to_string()));
        assert_eq!(
            ord.cmp(("README.md", false), ("a.md", false)),
            Ordering::Greater
        );
    }

    #[test]
    fn 顶层readme识别不区分大小写() {
        let d = tmpdir("readmecase");
        write(d.as_path(), "readme.md", "# R");
        write(d.as_path(), "a.md", "# A");
        assert_eq!(top_readme_rel(d.as_path()).as_deref(), Some("readme.md"));
        let _ = std::fs::remove_dir_all(&d);
    }
}