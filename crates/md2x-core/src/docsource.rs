//! 文档源类型：扩展名 → 渲染器，以及按类型分派的统一入口。
//!
//! 设计意图：把「什么文件能打开、用什么方式渲染」从散落各处的
//! `is_markdown()` 判断收敛成一张表。新增一种可渲染类型（比如 `.mdx`
//! 按 Markdown 处理、`.dbml` 按数据库 schema 处理）只需要在前端设置里
//! 加一行映射，不需要改 Rust 代码。
//!
//! 两层的分离：
//! - [`Renderer`] 是**能力**，编译期固定（Markdown / Dbml / PlainText）；
//! - [`TypeMap`] 是**配置**，运行期可变（把哪些扩展名交给哪个能力）。
//!
//! 这样「支持的类型可扩展」不会变成「渲染能力必须在运行期可插拔」——
//! 后者需要动态库或脚本引擎，对当前需求是过度设计。
//!
//! @author yujinping

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// 内置渲染能力。编译期固定，配置只能把扩展名映射到其中之一。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Renderer {
    /// Markdown：走 comrak + Mermaid + 外部图表工具
    Markdown,
    /// DBML：解析为数据字典表格 + 可缩放画布
    Dbml,
    /// 纯文本：整体包在 `<pre>` 里，保留空白与缩进
    PlainText,
}

impl Renderer {
    /// 稳定标识串，用于配置存档与前端交互。
    pub fn id(self) -> &'static str {
        match self {
            Renderer::Markdown => "markdown",
            Renderer::Dbml => "dbml",
            Renderer::PlainText => "plain_text",
        }
    }

    /// 展示名（用于设置界面兜底，前端通常走 i18n）。
    pub fn label(self) -> &'static str {
        match self {
            Renderer::Markdown => "Markdown",
            Renderer::Dbml => "DBML",
            Renderer::PlainText => "纯文本",
        }
    }

    /// 从标识串还原；未知串返回 None，由调用方决定回退策略。
    pub fn from_id(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "markdown" | "md" => Some(Renderer::Markdown),
            "dbml" => Some(Renderer::Dbml),
            "plain_text" | "text" | "plain" => Some(Renderer::PlainText),
            _ => None,
        }
    }

    /// 全部能力，供设置界面列出下拉选项。
    pub fn all() -> &'static [Renderer] {
        &[Renderer::Markdown, Renderer::Dbml, Renderer::PlainText]
    }

    /// 该能力下，是否允许导出 DOCX。
    ///
    /// DBML 走 `dbml → Markdown → DOCX` 中转，所以也算支持；
    /// 纯文本没有结构化内容，导出 Word 没有意义，前端据此隐藏菜单项。
    pub fn supports_docx(self) -> bool {
        matches!(self, Renderer::Markdown | Renderer::Dbml)
    }

    /// 默认扩展开启的渲染能力清单。
    pub fn default_types() -> Vec<TypeRule> {
        vec![
            TypeRule {
                ext: "md".into(),
                renderer: Renderer::Markdown,
                enabled: true,
            },
            TypeRule {
                ext: "markdown".into(),
                renderer: Renderer::Markdown,
                enabled: true,
            },
            TypeRule {
                ext: "dbml".into(),
                renderer: Renderer::Dbml,
                enabled: true,
            },
        ]
    }
}

/// 一条「扩展名 → 渲染器」规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeRule {
    /// 扩展名，不带点，比较时忽略大小写
    pub ext: String,
    /// 交给哪个渲染器
    pub renderer: Renderer,
    /// 是否启用。关掉的规则既不在文件树显示，也不能打开
    pub enabled: bool,
}

/// 渲染类型配置：扩展名 → 渲染器。
///
/// 用 `BTreeMap` 而不是 `HashMap`：序列化结果稳定有序，
/// 配置文件进 git 时不会因为哈希顺序变化产生无意义的 diff。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeMap {
    /// 全部规则，键为小写扩展名（不带点）
    rules: BTreeMap<String, TypeRule>,
}

impl Default for TypeMap {
    fn default() -> Self {
        let mut rules = BTreeMap::new();
        for r in Renderer::default_types() {
            rules.insert(r.ext.to_ascii_lowercase(), r);
        }
        Self { rules }
    }
}

impl TypeMap {
    /// 从规则列表构造。
    ///
    /// 清洗策略：
    /// - 扩展名去点、去空白、转小写，空串丢弃；
    /// - 非法字符（路径分隔符、通配符）丢弃，避免构造出能遍历目录的假扩展名；
    /// - 重复扩展名后出现的覆盖先出现的——顺序即优先级，符合直觉。
    pub fn from_rules(rules: Vec<TypeRule>) -> Self {
        let mut map = BTreeMap::new();
        for r in rules {
            let ext = normalize_ext(&r.ext);
            if ext.is_empty() {
                continue;
            }
            map.insert(
                ext.clone(),
                TypeRule {
                    ext,
                    renderer: r.renderer,
                    enabled: r.enabled,
                },
            );
        }
        Self { rules: map }
    }

    /// 取全部规则（有序，供设置界面渲染）。
    pub fn rules(&self) -> Vec<TypeRule> {
        self.rules.values().cloned().collect()
    }

    /// 按扩展名查规则；未配置或已禁用都返回 None。
    pub fn renderer_for_ext(&self, ext: &str) -> Option<Renderer> {
        let e = normalize_ext(ext);
        self.rules
            .get(&e)
            .filter(|r| r.enabled)
            .map(|r| r.renderer)
    }

    /// 按路径查规则。
    pub fn renderer_for(&self, path: &Path) -> Option<Renderer> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        self.renderer_for_ext(ext)
    }

    /// 路径是否属于可渲染类型（即：是否该出现在文件树里）。
    pub fn is_supported(&self, path: &Path) -> bool {
        self.renderer_for(path).is_some()
    }

    /// 全部启用的扩展名（不带点）。前端打开对话框的过滤器用得上。
    pub fn enabled_exts(&self) -> Vec<String> {
        self.rules
            .values()
            .filter(|r| r.enabled)
            .map(|r| r.ext.clone())
            .collect()
    }
}

/// 按渲染器把源文本转成等价的 Markdown。
///
/// 这是「DBML → md → docx」这类链路的**短路实现点**：中间 Markdown
/// 只作为内存里的字符串存在，不落盘。GUI 的 `export_docx` / `export_markdown`
/// 与 CLI 的 `--format docx` 都走这里，两处产出因此不会漂移。
///
/// - Markdown：原样返回；
/// - DBML：转数据字典式 Markdown（表格化字段清单）；
/// - 纯文本：包成围栏代码块，避免下游把内容当成结构解析。
pub fn to_markdown(renderer: Renderer, src: &str, title: &str) -> String {
    match renderer {
        Renderer::Markdown => src.to_string(),
        Renderer::Dbml => crate::dbml::to_markdown(src, title),
        Renderer::PlainText => format!("```text\n{}\n```\n", src.trim_end()),
    }
}

/// 扩展名归一：去点、去空白、转小写；含路径分隔符或通配符的直接判为非法。
fn normalize_ext(raw: &str) -> String {
    let s = raw.trim().trim_start_matches('.').to_ascii_lowercase();
    if s.is_empty() {
        return String::new();
    }
    // 只允许字母、数字、下划线、连字符、加号：
    // 覆盖 `.md` / `.dbml` / `.c++` 这类真实扩展名，同时挡掉 `../x`、`*` 之类的构造
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+'))
    {
        return String::new();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_map_covers_md_and_dbml() {
        let m = TypeMap::default();
        assert_eq!(m.renderer_for_ext("md"), Some(Renderer::Markdown));
        assert_eq!(m.renderer_for_ext("MD"), Some(Renderer::Markdown));
        assert_eq!(m.renderer_for_ext("markdown"), Some(Renderer::Markdown));
        assert_eq!(m.renderer_for_ext("dbml"), Some(Renderer::Dbml));
        assert_eq!(m.renderer_for_ext("txt"), None);
    }

    #[test]
    fn ext_with_dot_and_case_is_normalized() {
        let m = TypeMap::from_rules(vec![TypeRule {
            ext: ".DBML".into(),
            renderer: Renderer::Dbml,
            enabled: true,
        }]);
        assert_eq!(m.renderer_for_ext("dbml"), Some(Renderer::Dbml));
        assert_eq!(m.renderer_for_ext("Dbml"), Some(Renderer::Dbml));
    }

    #[test]
    fn disabled_rule_is_not_matched() {
        let m = TypeMap::from_rules(vec![TypeRule {
            ext: "dbml".into(),
            renderer: Renderer::Dbml,
            enabled: false,
        }]);
        assert_eq!(m.renderer_for_ext("dbml"), None);
        assert!(m.enabled_exts().is_empty());
    }

    #[test]
    fn illegal_ext_is_dropped() {
        let m = TypeMap::from_rules(vec![
            TypeRule {
                ext: "../etc/passwd".into(),
                renderer: Renderer::Markdown,
                enabled: true,
            },
            TypeRule {
                ext: "*".into(),
                renderer: Renderer::Markdown,
                enabled: true,
            },
            TypeRule {
                ext: "  ".into(),
                renderer: Renderer::Markdown,
                enabled: true,
            },
        ]);
        assert!(m.rules().is_empty(), "非法扩展名应被丢弃");
    }

    #[test]
    fn later_rule_overrides_earlier() {
        let m = TypeMap::from_rules(vec![
            TypeRule {
                ext: "x".into(),
                renderer: Renderer::Markdown,
                enabled: true,
            },
            TypeRule {
                ext: "x".into(),
                renderer: Renderer::PlainText,
                enabled: true,
            },
        ]);
        assert_eq!(m.renderer_for_ext("x"), Some(Renderer::PlainText));
    }

    #[test]
    fn renderer_roundtrips_through_id() {
        for r in Renderer::all() {
            assert_eq!(Renderer::from_id(r.id()), Some(*r), "{} 无法往返", r.id());
        }
        assert_eq!(Renderer::from_id("nope"), None);
    }

    #[test]
    fn dbml_supports_docx_via_markdown() {
        assert!(Renderer::Dbml.supports_docx());
        assert!(Renderer::Markdown.supports_docx());
        assert!(!Renderer::PlainText.supports_docx());
    }

    #[test]
    fn path_lookup_by_extension() {
        let m = TypeMap::default();
        assert_eq!(
            m.renderer_for(Path::new("/a/b/schema.dbml")),
            Some(Renderer::Dbml)
        );
        assert_eq!(
            m.renderer_for(Path::new("/a/b/readme.md")),
            Some(Renderer::Markdown)
        );
        assert_eq!(m.renderer_for(Path::new("/a/b/noext")), None);
        assert!(!m.is_supported(Path::new("/a/b/x.png")));
    }

    #[test]
    fn markdown_shortcut_passes_through() {
        let src = "# 标题\n\n正文";
        assert_eq!(to_markdown(Renderer::Markdown, src, "t"), src);
    }

    #[test]
    fn dbml_shortcut_produces_dictionary() {
        let src = "Table users {\n  id int [pk, note: '主键']\n  name varchar\n}";
        let md = to_markdown(Renderer::Dbml, src, "示例");
        // 必须是 Markdown 表格形态，而不是原始 DBML 文本。
        // 字段名在字典表里是行内代码（`id`），断言要带上反引号——
        // 只断言 `| id |` 会漏掉真实产物。
        assert!(md.contains("# 示例"), "缺少标题: {md}");
        assert!(md.contains("| 字段 | 类型 | 约束 |"), "缺少表头: {md}");
        assert!(md.contains("| `id` | `int` | PK |"), "缺少字段行: {md}");
        assert!(md.contains("主键"), "字段备注丢失: {md}");
        assert!(!md.contains("Table users {"), "不应把 DBML 原文塞进来");
    }

    #[test]
    fn plaintext_shortcut_wraps_in_fence() {
        let md = to_markdown(Renderer::PlainText, "a\nb\n\n", "t");
        assert_eq!(md, "```text\na\nb\n```\n");
    }
}