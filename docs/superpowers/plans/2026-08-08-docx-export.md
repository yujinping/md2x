# DOCX 导出功能实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 为 md2x 增加 DOCX 导出：Markdown → 可编辑 .docx，标题映射为 Word 导航窗格层级，正文排版（字体、字号、粗细、颜色、行距、边框、图片、表格、代码高亮）与现有 HTML 模板一致，不依赖本机 Office/WPS。

**架构：** 在 md2x-core 中直接生成 OOXML（zip + XML），不引入 docx-rs（1.x 不支持图片/字体/行距/底纹）。流程：comrak AST → 手写 `document.xml` 片段 + 固定部件（`[Content_Types].xml`、`_rels/.rels`、`word/styles.xml`、`word/_rels/document.xml.rels`、`word/media/*`）→ zip 打包。代码高亮用 syntect + 内嵌 OneDark tmTheme。

**技术栈：** comrak 0.28（已有）、syntect 5.3（default-syntaxes + plist-load，纯 Rust）、zip 8.6（已有）、std 仅此。

---

## 文件结构

| 文件 | 职责 |
|---|---|
| `crates/md2x-core/src/docx/mod.rs` | 入口 `convert_markdown_to_docx`、AST 遍历分发 |
| `crates/md2x-core/src/docx/render.rs` | 节点 → XML 片段（段落、run、标题、列表、表格、图片、代码块） |
| `crates/md2x-core/src/docx/package.rs` | 组装 zip：固定部件 + media |
| `crates/md2x-core/src/docx/image.rs` | 图片解析（路径/下载/尺寸读取） |
| `crates/md2x-core/src/docx/highlight.rs` | syntect 高亮封装 |
| `crates/md2x-core/assets/atom-one-dark.tmTheme` | 代码高亮主题（与 HTML 版 atom-one-dark 同配色） |
| `crates/md2x-core/src/lib.rs` | 导出 `pub mod docx` |
| `crates/md2x-core/src/converter.rs` | 少量函数改 `pub(crate)` 供 docx 复用 |
| `crates/md2x-cli/src/main.rs` | `OutputFormat` 增加 `Docx` |
| `crates/md2x-gui/src/lib.rs` | 新增 `export_html` / `export_pdf` / `export_docx` 命令 |
| `frontend/src/components/Toolbar.vue` | 新增「导出」下拉菜单 |
| `frontend/src/App.vue` | 导出事件处理 |
| `frontend/src/i18n/index.js` | 导出相关文案 |

## 设计常量（HTML → docx 映射）

基准：正文 15px = 11.25pt → `w:sz 22`（11pt）；行距 1.75 → `w:spacing w:line="350" w:lineRule="auto"`；段后距 16px = 240 twips。

| 元素 | 字号(半磅) | 颜色 | 备注 |
|---|---|---|---|
| 正文 | 22 | 333333 | 行距 350、段后 240 |
| h1 | 45 | 继承 | 加粗、段落下边框 eaecef、段前 450 段后 150 |
| h2 | 36 | 继承 | 加粗、段落下边框 eaecef、段前 450 段后 150 |
| h3 | 30 | 继承 | 加粗、段前 420 段后 120 |
| h4 | 26 | 继承 | 加粗、段前 390 段后 90 |
| h5/h6 | 22 | 继承 | 加粗、段前 360 段后 90 |
| 链接 | 22 | 0366d6 | 下划线 |
| 行内代码 | 20 | d63384 | 等宽 Consolas、字符底纹 f0f0f0 |
| 代码块 | 20 | abb2bf | 段落底纹 282c34、行距 320、左右缩进 240 |
| 引用块 | 22 | 6a737d | 左缩进 240、左边框 4px(24 八分之一磅) dfe2e5、背景 fafbfc |
| 表格 | 21 | 继承 | 边框 dfe2e5、表头底纹 f6f8fa 加粗 |
| 任务列表 | 22 | 继承 | ☐/☑ 前缀 |

图片：读取像素宽高，超过内容区宽（6.3 英寸 = 5781600 EMU）等比缩放，居中段落嵌入 `w:drawing`。

---

### 任务 1：OOXML 打包骨架（空文档可生成、可解包）

**文件：**
- 创建：`crates/md2x-core/src/docx/mod.rs`
- 创建：`crates/md2x-core/src/docx/package.rs`
- 修改：`crates/md2x-core/src/lib.rs`

- [ ] **步骤 1：编写失败的测试**（`docx/mod.rs` 内 `#[cfg(test)]`）

```rust
#[test]
fn empty_docx_is_valid_zip_with_required_parts() {
    let dir = std::env::temp_dir().join(format!("md2x-docx-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join("empty.docx");
    let content = super::markdown_to_docx_content("# 标题", std::path::Path::new("test.md")).unwrap();
    super::write_docx(&content, &dst).unwrap();

    let file = std::fs::File::open(&dst).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let names: Vec<String> = zip.file_names().map(|s| s.to_string()).collect();
    for required in ["[Content_Types].xml", "_rels/.rels", "word/document.xml", "word/styles.xml"] {
        assert!(names.iter().any(|n| n == required), "缺少部件 {required}: {names:?}");
    }
    let doc = zip.by_name("word/document.xml").unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut doc, &mut xml).unwrap();
    assert!(xml.contains("Heading1"), "标题应使用 Heading1 样式");
    std::fs::remove_dir_all(&dir).ok();
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p md2x-core docx` 预期：编译失败（`docx` 模块不存在）

- [ ] **步骤 3：实现 `docx/mod.rs` 与 `package.rs` 骨架**

```rust
// docx/mod.rs
pub mod package;

use crate::error::MpeError;
use std::path::Path;

pub struct DocxContent {
    pub document_xml: String,
    pub media: Vec<(String, Vec<u8>, String)>, // (文件名, 字节, content_type)
}

pub fn markdown_to_docx_content(md: &str, md_file: &Path) -> Result<DocxContent, MpeError> {
    // 骨架：先渲染标题，正文渲染后续任务补充
    let body = render_body(md, md_file)?;
    Ok(DocxContent { document_xml: document_wrapper(&body), media: Vec::new() })
}

pub fn write_docx(content: &DocxContent, dst: &Path) -> Result<(), MpeError> {
    package::write_package(content, dst)
}

pub fn convert_markdown_to_docx(md: &str, md_file: &Path, dst: &Path) -> Result<(), MpeError> {
    let content = markdown_to_docx_content(md, md_file)?;
    write_docx(&content, dst)
}
```

（`render_body`、`document_wrapper` 及 `package.rs` 的 `write_package` 按下方“OOXML 固定部件模板”实现，标题渲染先支持 `# 标题`。）

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p md2x-core docx` 预期：1 passed

- [ ] **步骤 5：Commit**

```bash
git add crates/md2x-core/src/lib.rs crates/md2x-core/src/docx
git commit -m "feat(core): docx 打包骨架（任务 1/13）"
```

### 任务 2：段落与行内文本（粗体、斜体、删除线、链接、行内代码、换行）

**文件：**
- 创建：`crates/md2x-core/src/docx/render.rs`

- [ ] **步骤 1：编写失败的测试**（`docx/mod.rs` 内）

```rust
#[test]
fn paragraph_renders_runs_and_inline_styles() {
    let content = super::markdown_to_docx_content(
        "普通 **粗体** *斜体* ~~删除~~ `code` [链接](https://example.com) 换行",
        std::path::Path::new("test.md"),
    ).unwrap();
    assert!(content.document_xml.contains("<w:b/>"));
    assert!(content.document_xml.contains("<w:i/>"));
    assert!(content.document_xml.contains("<w:strike/>"));
    assert!(content.document_xml.contains("d63384")); // 行内代码颜色
    assert!(content.document_xml.contains("w:hyperlink"));
    assert!(content.document_xml.contains("0366d6")); // 链接颜色
    assert!(content.document_xml.contains("eastAsia")); // 中文字体
    assert!(content.document_xml.contains("w:line=\"350\"")); // 行距 1.75
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p md2x-core docx` 预期：FAIL（断言不满足或编译失败）

- [ ] **步骤 3：实现 `render.rs` 的段落与行内渲染**

要点：
- `render_inline(children) -> String`：递归处理 Text/Emph/Strong/Strikethrough/Code/Link/SoftBreak/LineBreak，生成 `<w:r><w:rPr>…</w:rPr><w:t xml:space="preserve">…</w:t></w:r>`；
- `CharacterProperty` 组装：字号 22、字体 `w:rFonts w:ascii="Segoe UI" w:eastAsia="PingFang SC, Microsoft YaHei"`（rFonts 手写）、颜色、加粗/斜体/删除线；
- 行内代码：等宽 Consolas + 颜色 d63384 + `w:shd` 字符底纹 f0f0f0；
- 链接：`<w:hyperlink r:id="rIdLINK"><w:r>…` 并生成对应 rels（图片任务前先支持外链 rel，任务 8 统一收口 rels）；
- XML 转义：`& < >`；文本用 `xml:space="preserve"`。

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p md2x-core docx` 预期：2 passed

- [ ] **步骤 5：Commit**

```bash
git add crates/md2x-core/src/docx
git commit -m "feat(core): docx 段落与行内文本渲染（任务 2/13）"
```

### 任务 3：标题 → Word 导航窗格层级

**文件：** `crates/md2x-core/src/docx/render.rs`、`crates/md2x-core/src/docx/package.rs`

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn headings_use_heading_styles_and_outline() {
    let content = super::markdown_to_docx_content("# H1\n\n## H2\n\n### H3", std::path::Path::new("t.md")).unwrap();
    for (lvl, sz) in [("Heading1", 45usize), ("Heading2", 36usize), ("Heading3", 30usize)] {
        assert!(content.document_xml.contains(&format!("w:val=\"{lvl}\"")));
    }
    // styles.xml 需包含 Heading 样式定义与 outlineLvl
    let dir = std::env::temp_dir().join("md2x-docx-styles");
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join("h.docx");
    super::write_docx(&content, &dst).unwrap();
    let file = std::fs::File::open(&dst).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut styles = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("word/styles.xml").unwrap(), &mut styles).unwrap();
    assert!(styles.contains("Heading1"));
    assert!(styles.contains("outlineLvl"));
    std::fs::remove_dir_all(&dir).ok();
}
```

- [ ] **步骤 2：运行测试验证失败**（预期 FAIL）
- [ ] **步骤 3：实现标题渲染与 styles.xml**

标题段落：`<w:p><w:pPr><w:pStyle w:val="HeadingN"/>…</w:pPr>…</w:p>`；styles.xml 中 Heading1-6 定义（`w:name w:val="heading N"`、`w:outlineLvl`、`w:basedOn`、`w:next`）。

- [ ] **步骤 4：运行测试验证通过**（预期 3 passed）
- [ ] **步骤 5：Commit**

```bash
git add crates/md2x-core/src/docx
git commit -m "feat(core): docx 标题样式与导航层级（任务 3/13）"
```

### 任务 4：列表（无序、有序、任务列表）

**文件：** `crates/md2x-core/src/docx/render.rs`、`crates/md2x-core/src/docx/package.rs`

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn lists_render_bullets_numbers_and_tasks() {
    let content = super::markdown_to_docx_content(
        "- 甲\n- 乙\n\n1. 一\n2. 二\n\n- [x] 完成\n- [ ] 未完成",
        std::path::Path::new("t.md"),
    ).unwrap();
    assert!(content.document_xml.contains("w:numId"));
    assert!(content.document_xml.contains("☑"));
    assert!(content.document_xml.contains("☐"));
}
```

- [ ] **步骤 2：运行测试验证失败**（预期 FAIL）
- [ ] **步骤 3：实现列表渲染 + numbering.xml**

numbering.xml 固定模板：抽象编号 `abstractNum` 两个（bullet、decimal），实例 `num` 两个；列表项段落加 `w:numPr`（`w:ilvl` 按嵌套深度）；任务列表用 ☐/☑ 文本前缀（run 前加符号，去掉 numPr 或保留 bullet 无符号）。

- [ ] **步骤 4：运行测试验证通过**（预期 4 passed）
- [ ] **步骤 5：Commit**

```bash
git add crates/md2x-core/src/docx
git commit -m "feat(core): docx 列表与任务列表渲染（任务 4/13）"
```

### 任务 5：代码块（底纹 + syntect 高亮）

**文件：**
- 创建：`crates/md2x-core/src/docx/highlight.rs`
- 创建：`crates/md2x-core/assets/atom-one-dark.tmTheme`

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn code_block_has_dark_shading_and_highlight_colors() {
    let content = super::markdown_to_docx_content("```rust\nfn main() {}\n```", std::path::Path::new("t.md")).unwrap();
    assert!(content.document_xml.contains("282C34")); // 代码块底纹
    assert!(content.document_xml.contains("abb2bf")); // 前景
    assert!(content.document_xml.contains("C678DD")); // 关键字高亮色
}
```

- [ ] **步骤 2：运行测试验证失败**（预期 FAIL）
- [ ] **步骤 3：实现 highlight.rs + tmTheme + 代码块段落**

`highlight.rs`：`SyntaxSet::load_defaults_newlines()` + 内嵌 tmTheme（`include_str!`）构造 `ThemeSet`；`highlight_code(lang, code) -> Vec<(String, String)>`（文本、颜色）；无语言/未知语言回退纯文本。
代码块段落：段落底纹 282c34、左右缩进 240、行距 320；每个 token 一个 run（颜色、等宽 Consolas、字号 20）。
tmTheme 基于 atom-one-dark 配色：background #282c34、foreground #abb2bf；comment→#5c6370、keyword→#c678dd、string→#98c379、number/constant→#d19a66、function/entity.name.function→#61aeee、type/storage.type→#e06c75 等。

- [ ] **步骤 4：运行测试验证通过**（预期 5 passed）
- [ ] **步骤 5：Commit**

```bash
git add crates/md2x-core/src/docx crates/md2x-core/assets
git commit -m "feat(core): docx 代码块底纹与 syntect 高亮（任务 5/13）"
```

### 任务 6：引用块与分割线

**文件：** `crates/md2x-core/src/docx/render.rs`

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn blockquote_and_hr_render() {
    let content = super::markdown_to_docx_content("> 引用\n\n---", std::path::Path::new("t.md")).unwrap();
    assert!(content.document_xml.contains("6a737d")); // 引用灰字
    assert!(content.document_xml.contains("w:left")); // 左边框
    assert!(content.document_xml.contains("fafbfc")); // 引用背景
    assert!(content.document_xml.contains("eaecef")); // 分割线颜色
}
```

- [ ] **步骤 2：运行测试验证失败**（预期 FAIL）
- [ ] **步骤 3：实现引用块与分割线**

引用块：段落左缩进 240、`w:pBdr` left（sz=24 八分之一磅、color dfe2e5）、背景 fafbfc、文字 6a737d；内部内容递归渲染。
分割线：空段落 + `w:pBdr` bottom（eaecef、sz=4）。

- [ ] **步骤 4：运行测试验证通过**（预期 6 passed）
- [ ] **步骤 5：Commit**

```bash
git add crates/md2x-core/src/docx
git commit -m "feat(core): docx 引用块与分割线（任务 6/13）"
```

### 任务 7：表格

**文件：** `crates/md2x-core/src/docx/render.rs`

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn table_renders_with_borders_and_header_shading() {
    let content = super::markdown_to_docx_content(
        "| 名称 | 数量 |\n| --- | ---: |\n| 苹果 | 3 |",
        std::path::Path::new("t.md"),
    ).unwrap();
    assert!(content.document_xml.contains("<w:tbl>"));
    assert!(content.document_xml.contains("dfe2e5")); // 边框色
    assert!(content.document_xml.contains("f6f8fa")); // 表头底纹
    assert!(content.document_xml.contains("w:jc w:val=\"right\"")); // 右对齐列
}
```

- [ ] **步骤 2：运行测试验证失败**（预期 FAIL）
- [ ] **步骤 3：实现表格渲染**

`<w:tbl>` + `<w:tblPr>`（`w:tblBorders` 单线 dfe2e5、`w:tblCellMar` 上下 120 / 左右 180）+ `<w:tblGrid>`（等宽列）+ 行/单元格；表头行 `w:shd f6f8fa` + 加粗；对齐按 `TableAlignment` 映射（center/right）。

- [ ] **步骤 4：运行测试验证通过**（预期 7 passed）
- [ ] **步骤 5：Commit**

```bash
git add crates/md2x-core/src/docx
git commit -m "feat(core): docx 表格渲染（任务 7/13）"
```

### 任务 8：图片（路径解析、尺寸、media、rels）

**文件：**
- 创建：`crates/md2x-core/src/docx/image.rs`
- 修改：`crates/md2x-core/src/converter.rs`（少量函数改 `pub(crate)`）

- [ ] **步骤 1：编写失败的测试**（先生成一张 1x1 PNG 到临时目录）

```rust
#[test]
fn image_embeds_into_media_and_drawing() {
    let dir = std::env::temp_dir().join("md2x-docx-img");
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("pixel.png");
    // 1x1 透明 PNG（base64 解码）
    let b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    use base64::Engine as _;
    std::fs::write(&png, base64::engine::general_purpose::STANDARD.decode(b64).unwrap()).unwrap();
    let md = format!("![像素]({})", png.display());
    let content = super::markdown_to_docx_content(&md, &png).unwrap();
    assert_eq!(content.media.len(), 1);
    let dir2 = std::env::temp_dir().join("md2x-docx-img-out");
    std::fs::create_dir_all(&dir2).unwrap();
    let dst = dir2.join("img.docx");
    super::write_docx(&content, &dst).unwrap();
    let file = std::fs::File::open(&dst).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let names: Vec<String> = zip.file_names().map(|s| s.to_string()).collect();
    assert!(names.iter().any(|n| n.starts_with("word/media/")), "缺少 media: {names:?}");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&dir2).ok();
}
```

（需要 `base64` dev-dependency，或用项目内 `converter::base64_encode` 的逆过程手写解码——测试直接用 base64 crate 更简单。）

- [ ] **步骤 2：运行测试验证失败**（预期 FAIL）
- [ ] **步骤 3：实现 image.rs + 渲染图片段落**

`image.rs`：
- `resolve_image_bytes(src, md_file) -> Option<(Vec<u8>, &'static str)>`：复用 `converter` 的 `find_hugo_root` / `is_resolvable_path`（改 `pub(crate)`）；本地读文件，远程 `download_image_bytes`（新增，`download_image_as_data_uri` 改为调用它）；失败返回 None（跳过，保留 alt 文本）。
- `image_dimensions(bytes, mime) -> Option<(u32, u32)>`：解析 PNG(IHDR)/JPEG(SOF)/GIF(头)/BMP(头)/WebP(VP8/VP8L) 尺寸。
- 缩放：宽 > 5781600 EMU 时等比缩放。

渲染：居中段落 + `w:drawing`（wp:inline + a:graphic + pic:pic，r:embed 指向 rId）；media 列表与 `document.xml.rels` 同步生成；`[Content_Types].xml` 加扩展名 default。

- [ ] **步骤 4：运行测试验证通过**（预期 8 passed）
- [ ] **步骤 5：Commit**

```bash
git add crates/md2x-core/src/docx crates/md2x-core/src/converter.rs crates/md2x-core/Cargo.toml
git commit -m "feat(core): docx 图片嵌入（任务 8/13）"
```

### 任务 9：入口集成（含 frontmatter、软换行、HTML 块兜底）

**文件：** `crates/md2x-core/src/docx/mod.rs`

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn frontmatter_stripped_and_mixed_doc_renders() {
    let content = super::markdown_to_docx_content(
        "---\ntitle: 演示\n---\n\n# 标题\n\n- 项一\n- 项二\n\n```rust\nlet x = 1;\n```",
        std::path::Path::new("t.md"),
    ).unwrap();
    assert!(!content.document_xml.contains("title: 演示"));
    assert!(content.document_xml.contains("项一"));
}
```

- [ ] **步骤 2：运行测试验证失败**（预期 FAIL）
- [ ] **步骤 3：实现入口完整遍历**

`markdown_to_docx_content`：剥离 frontmatter（复用 `converter::parse_front_matter`）→ `parse_document` → 遍历 Document 子节点分发到 render.rs；HtmlBlock/HtmlInline 忽略；Math/脚注保留文本兜底；未知节点跳过。

- [ ] **步骤 4：运行测试验证通过**（预期 9 passed）
- [ ] **步骤 5：Commit**

```bash
git add crates/md2x-core/src/docx
git commit -m "feat(core): docx 入口集成与 frontmatter 处理（任务 9/13）"
```

### 任务 10：CLI 支持 `--format docx`

**文件：** `crates/md2x-cli/src/main.rs`

- [ ] **步骤 1：编写/更新 CLI 行为**（在 `match cli.format` 增加 Docx 分支，无单元测试框架，用编译 + 实际运行验证）
- [ ] **步骤 2：实现**

```rust
OutputFormat::Docx => {
    let docx_path = path.with_extension("docx");
    md2x_core::docx::convert_markdown_to_docx(body_md, path, &docx_path)?;
}
```

同时更新 `Cli` 的 `--format` 帮助文本为 `pdf, html, png or docx`。

- [ ] **步骤 3：验证**：`cargo run -p md2x-cli -- README.md --format docx` 预期：生成 `README.docx`；`unzip -l README.docx` 包含 document.xml/styles.xml；用 `libreoffice --headless --convert-to pdf README.docx`（若本机有）或人工打开验证。
- [ ] **步骤 4：Commit**

```bash
git add crates/md2x-cli/src/main.rs
git commit -m "feat(cli): 支持 --format docx（任务 10/13）"
```

### 任务 11：GUI 导出命令（export_html / export_pdf / export_docx）

**文件：** `crates/md2x-gui/src/lib.rs`

- [ ] **步骤 1：实现三个命令并注册**

```rust
#[tauri::command]
fn export_html(s: State<AppState>) -> Result<String, String> { /* 返回默认文件名，前端弹保存框 */ }

#[tauri::command]
fn export_pdf(s: State<AppState>) -> Result<String, String> { /* 复用现有 HTML 渲染 → chrome::generate_pdf 到 dst */ }

#[tauri::command]
fn export_docx(dst: String, s: State<AppState>) -> Result<(), String> {
    // 读 current_file → md2x_core::docx::convert_markdown_to_docx
}
```

设计：前端弹保存框拿到 dst 后调用 `export_html(dst)` / `export_pdf(dst)` / `export_docx(dst)`，三个命令签名统一为 `(dst, state)`。

- [ ] **步骤 2：注册** 加入 `invoke_handler` 列表
- [ ] **步骤 3：验证**：`cargo check -p md2x-gui` 通过
- [ ] **步骤 4：Commit**

```bash
git add crates/md2x-gui/src/lib.rs
git commit -m "feat(gui): 新增导出 html/pdf/docx 命令（任务 11/13）"
```

### 任务 12：前端「导出」下拉菜单

**文件：** `frontend/src/components/Toolbar.vue`、`frontend/src/App.vue`、`frontend/src/i18n/index.js`

- [ ] **步骤 1：实现 Toolbar 导出菜单**

在「预览 PDF」右侧新增「导出 ▾」按钮（`hasFile` 禁用），点击展开子菜单：HTML / PDF / DOCX，点击外部关闭；emit `export-doc` 事件（参数格式）。

- [ ] **步骤 2：实现 App.vue 处理**

`onExportDoc(format)`：弹保存框（扩展名对应 html/pdf/docx）→ `invoke('export_html' | 'export_pdf' | 'export_docx', { dst })` → StatusBar 成功/失败提示。

- [ ] **步骤 3：补充 i18n 文案**（btnExport、exportHtml、exportPdf、exportDocx、statusExporting、statusExportReady、statusExportError）
- [ ] **步骤 4：验证**：`cd frontend && pnpm build` 通过
- [ ] **步骤 5：Commit**

```bash
git add frontend/src
git commit -m "feat(gui): 前端导出菜单（HTML/PDF/DOCX）（任务 12/13）"
```

### 任务 13：全量验证与示例文档

- [ ] **步骤 1：全量测试**：`cargo test` 全部通过（记录输出）
- [ ] **步骤 2：全量构建**：`cargo build` exit 0；`cd frontend && pnpm build` exit 0
- [ ] **步骤 3：端到端验证**：用仓库 `README.md` 生成 `README.docx`，`unzip -l` 检查部件与 media；用 LibreOffice headless 转 PDF（若可用）或打开抽查标题导航、代码块、表格、图片
- [ ] **步骤 4：更新 README**（新增 docx 输出格式说明）
- [ ] **步骤 5：Commit**

```bash
git add README.md
git commit -m "docs: 更新 README 支持 docx 输出（任务 13/13）"
```

---

## 自检

- 规格覆盖：导航窗格（任务 3）、正文排版（任务 2-7）、图片（任务 8）、CLI/GUI/前端（任务 10-12）、验证（任务 13）✓
- 占位符扫描：无 TODO/待定 ✓
- 类型一致性：`markdown_to_docx_content` / `write_docx` / `DocxContent` 贯穿全部任务 ✓
