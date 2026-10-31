//! DBML 解析后的中间模型。
//!
//! 设计意图：刻意只保留"文档化"需要的字段，不做类型合法性校验。
//! DBML 是给人写文档的 DSL，`serial` 主键被 `int` 外键引用是常态，
//! 任何"类型必须完全相等"的强校验都会把合法 schema 判死。

/// 一列（字段）。
#[derive(Debug, Clone, Default)]
pub struct Column {
    /// 列名
    pub name: String,
    /// 原始类型串，保留 `varchar(64)` 这类参数
    pub ty: String,
    /// 是否主键
    pub pk: bool,
    /// 是否 NOT NULL（DBML 默认可空）
    pub not_null: bool,
    /// 是否唯一
    pub unique: bool,
    /// 是否自增
    pub increment: bool,
    /// 默认值原文，无则 None
    pub default: Option<String>,
    /// 列注释
    pub note: String,
}

/// 索引定义。
#[derive(Debug, Clone, Default)]
pub struct IndexDef {
    /// 参与的列
    pub cols: Vec<String>,
    /// 是否主键索引
    pub pk: bool,
    /// 是否唯一索引
    pub unique: bool,
    /// 索引名
    pub name: Option<String>,
    /// 索引类型（btree / hash …）
    pub ty: Option<String>,
}

/// 一张表。
#[derive(Debug, Clone, Default)]
pub struct Table {
    /// 表名，`schema.table` 形式时保留 schema
    pub name: String,
    /// `Table x as A` 里的别名
    pub alias: Option<String>,
    /// 表注释（`Note:` 块或多行文本）
    pub note: String,
    /// 表头 `[comment: '...']` 里的短注释。
    ///
    /// 与 `note` 分开存：真实 schema 里两者常同时存在且用途不同——
    /// `note` 是给人读的完整说明（可多行、带 Markdown），
    /// `comment` 是紧贴表名的一行短标签（画布表头用）。
    pub comment: String,
    /// `[headercolor: #xxx]`
    pub color: Option<String>,
    /// 所属 TableGroup 名
    pub group: Option<String>,
    /// 字段
    pub cols: Vec<Column>,
    /// 索引
    pub indexes: Vec<IndexDef>,
    /// 行内 `[ref: > x.y]` 暂存：列定义解析完后统一收口成 Ref
    pub pending_inline: Vec<(String, (Rel, String, String))>,
    /// 表级 `ref: > a.(x, y)` 暂存：(基数, 目标表, 目标列列表)
    pub pending_table_ref: Vec<(Rel, String, Vec<String>)>,
}

/// 引用基数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rel {
    /// 多对一 `>`
    ManyToOne,
    /// 一对多 `<`
    OneToMany,
    /// 一对一 `-`
    OneToOne,
    /// 多对多 `<>`
    ManyToMany,
}

impl Rel {
    /// 展示用符号。
    pub fn symbol(&self) -> &'static str {
        match self {
            Rel::ManyToOne => ">",
            Rel::OneToMany => "<",
            Rel::OneToOne => "-",
            Rel::ManyToMany => "<>",
        }
    }
    /// 展示用中文说明。
    pub fn label(&self) -> &'static str {
        match self {
            Rel::ManyToOne => "多对一",
            Rel::OneToMany => "一对多",
            Rel::OneToOne => "一对一",
            Rel::ManyToMany => "多对多",
        }
    }
}

/// 一条外键引用，已展开为单列对。
#[derive(Debug, Clone)]
pub struct Ref {
    /// 引用方（子表）表名
    pub from_table: String,
    /// 引用方列名
    pub from_col: String,
    /// 被引用（父表）表名
    pub to_table: String,
    /// 被引用列名
    pub to_col: String,
    /// 基数
    pub rel: Rel,
    /// ON DELETE 行为
    pub on_delete: Option<String>,
    /// ON UPDATE 行为
    pub on_update: Option<String>,
}

/// 枚举类型。
#[derive(Debug, Clone, Default)]
pub struct Enum {
    /// 枚举名
    pub name: String,
    /// (值, 注释)
    pub values: Vec<(String, String)>,
}

/// TableGroup：逻辑分组，用于画布分区。
#[derive(Debug, Clone, Default)]
pub struct Group {
    /// 组名
    pub name: String,
    /// 组注释
    pub note: String,
    /// 成员表名
    pub tables: Vec<String>,
}

/// 整个 schema。
#[derive(Debug, Clone, Default)]
pub struct Schema {
    /// `Project xxx { }` 的名字
    pub project: Option<String>,
    /// `database_type`
    pub db_type: Option<String>,
    /// 项目级注释（支持 markdown 原文）
    pub project_note: String,
    /// 全部表
    pub tables: Vec<Table>,
    /// 全部枚举
    pub enums: Vec<Enum>,
    /// 全部分组
    pub groups: Vec<Group>,
    /// 全部外键
    pub refs: Vec<Ref>,
}

impl Schema {
    /// 按名字查表（含别名回退）。
    #[allow(dead_code)]
    pub fn find_table(&self, name: &str) -> Option<&Table> {
        self.tables
            .iter()
            .find(|t| t.name == name || t.name.rsplit('.').next() == Some(name))
            .or_else(|| {
                self.tables
                    .iter()
                    .find(|t| t.alias.as_deref() == Some(name))
            })
    }

    /// 取某列的外键目标（表, 列, 基数）。
    pub fn fk_of(&self, table: &str, col: &str) -> Option<(&str, &str, Rel)> {
        self.refs
            .iter()
            .find(|r| r.from_table == table && r.from_col == col)
            .map(|r| (r.to_table.as_str(), r.to_col.as_str(), r.rel))
    }

    /// 统计：表数、字段数、外键数。
    pub fn stats(&self) -> (usize, usize, usize) {
        (
            self.tables.len(),
            self.tables.iter().map(|t| t.cols.len()).sum(),
            self.refs.len(),
        )
    }
}