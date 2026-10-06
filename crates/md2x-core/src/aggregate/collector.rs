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
    /// 按展示顺序排列的文档（README 优先，其余按相对路径排序）
    pub docs: Vec<DocEntry>,
    /// 相对路径（`/` 分隔）→ section id，用于链接重写时查表
    pub by_rel: HashMap<String, String>,
}

/// 递归收集文件夹内的所有 Markdown 文档。
///
/// 排序规则：README / INDEX / index 等入口文件排在最前（顶层优先），
/// 其余按相对路径字典序，保证导出结果稳定可复现。
pub fn collect(root: &Path) -> Collected {
    let mut files = Vec::new();
    scan_dir(root, &mut files, 0);

    // 入口文件优先：文件名（忽略大小写）为 readme / index 的排在最前
    files.sort_by(|a, b| {
        let a_entry = is_entry_file(a, root);
        let b_entry = is_entry_file(b, root);
        match (a_entry, b_entry) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => rel_of(a, root).cmp(&rel_of(b, root)),
        }
    });

    let mut docs = Vec::with_capacity(files.len());
    let mut by_rel = HashMap::with_capacity(files.len());
    for (i, path) in files.iter().enumerate() {
        let rel_path = rel_of(path, root);
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

/// 判断是否为入口文件（readme / index），且位于根目录或仅隔一层
fn is_entry_file(path: &Path, root: &Path) -> bool {
    let rel = rel_of(path, root);
    // 只认顶层与一级子目录下的入口文件，避免深层目录的 index 抢占首位
    let depth = rel.matches('/').count();
    if depth > 1 {
        return false;
    }
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    stem == "readme" || stem == "index"
}

/// 递归扫描目录，收集 `.md` / `.markdown` 文件
///
/// `depth` 用于防御：跳过过深的目录（如 node_modules 嵌套），
/// 也避免符号链接成环导致的无限递归。
fn scan_dir(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 12 {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        // 符号链接不跟随：既能避免成环，也能避免跳到用户意外的大目录
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // 跳过隐藏目录与常见依赖/产物目录
            if name.starts_with('.') || matches!(name.as_ref(), "node_modules" | "target") {
                continue;
            }
            scan_dir(&path, out, depth + 1);
        } else if file_type.is_file() && is_markdown(&path) {
            out.push(path);
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("md") | Some("markdown")
    )
}

/// 计算相对 root 的路径，统一 `/` 分隔
fn rel_of(path: &Path, root: &Path) -> String {
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

    #[test]
    fn 递归收集嵌套目录的md() {
        let d = tmpdir("nested");
        write(d.as_path(), "a.md", "# A");
        write(d.as_path(), "sub/b.md", "# B");
        write(d.as_path(), "sub/deep/c.markdown", "# C");
        write(d.as_path(), "sub/ignore.txt", "not md");

        let c = collect(d.as_path());
        let mut names: Vec<&str> = c.docs.iter().map(|x| x.rel_path.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["a.md", "sub/b.md", "sub/deep/c.markdown"]);
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
        assert_eq!(
            c.docs[0].title, "R",
            "应取正文首个 h1 作为标题"
        );
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
    fn 识别md与markdown() {
        assert!(is_markdown(Path::new("a.md")));
        assert!(is_markdown(Path::new("a.MD")));
        assert!(is_markdown(Path::new("a.markdown")));
        assert!(!is_markdown(Path::new("a.txt")));
        assert!(!is_markdown(Path::new("a.mdx")));
    }
}
