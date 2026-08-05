use std::collections::HashMap;

#[allow(dead_code)]
pub fn render_html_template(html_body: &str, title: &str) -> String {
    render_html_template_with_metadata(html_body, title, None)
}

pub fn render_html_template_with_metadata(
    html_body: &str,
    title: &str,
    metadata: Option<&HashMap<String, String>>,
) -> String {
    let template = include_str!("../../../templates/mpe.html");
    let github_css = include_str!("../../../templates/assets/github-markdown.min.css");
    let atom_css = include_str!("../../../templates/assets/atom-one-dark.min.css");
    let highlight_js = include_str!("../../../templates/assets/highlight.min.js");

    let (toc_html, body_fixed) = generate_toc(html_body);
    let metadata_html = metadata
        .map(render_skill_metadata_html)
        .unwrap_or_default();

    template
        .replace("{{TITLE}}", title)
        .replace("{{GITHUB_MD_CSS}}", github_css)
        .replace("{{ATOM_ONE_DARK_CSS}}", atom_css)
        .replace("{{HIGHLIGHT_JS}}", highlight_js)
        .replace("{{TOC}}", &toc_html)
        .replace("{{SKILL_METADATA}}", &metadata_html)
        .replace("{{BODY}}", &body_fixed)
}

/// 将 SKILL.md 的元数据渲染为 HTML 卡片
fn render_skill_metadata_html(meta: &HashMap<String, String>) -> String {
    let name = meta.get("name").map(|s| s.as_str()).unwrap_or("");
    let description = meta.get("description").map(|s| s.as_str()).unwrap_or("");

    let mut extra = String::new();
    // 渲染 name/description 之外的可选字段
    for key in ["runAs", "scope", "model", "effort", "allowedTools"] {
        if let Some(val) = meta.get(key) {
            extra.push_str(&format!(
                r#"<span class="skill-meta-item"><span class="skill-meta-label">{}</span><span class="skill-meta-value">{}</span></span>"#,
                key,
                val.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
            ));
        }
    }

    format!(
        r#"<div class="skill-metadata">
  <div class="skill-metadata-header">
    <h1>{name}</h1>
    <span class="skill-badge">Skill</span>
  </div>
  <p class="skill-description">{description}</p>
  <div class="skill-meta-grid">{extra}</div>
</div>"#,
        name = name.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"),
        description = description.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"),
        extra = extra,
    )
}

fn generate_toc(body: &str) -> (String, String) {
    let mut items: Vec<(String, String, String)> = Vec::new(); // (level, id, text)
    let mut result = body.to_string();
    let mut counter = 0u64;
    let mut pos = 0;

    // 逐个查找 h1-h6 标签
    while pos < result.len() {
        let remaining = &result[pos..];
        let mut tag_match = None;

        for level in 1..=6 {
            let open = format!("<h{}", level);
            if let Some(start) = remaining.find(&open) {
                match tag_match {
                    None => tag_match = Some((level, start)),
                    Some((_, existing_start)) if start < existing_start => tag_match = Some((level, start)),
                    _ => {}
                }
            }
        }

        let (level, start) = match tag_match {
            Some(v) => v,
            None => break,
        };

        let abs_start = pos + start;
        let tag_start = abs_start;

        // 找到 closing >
        let after_tag = &result[tag_start..];
        let gt = after_tag.find('>').unwrap();
        let content_start = tag_start + gt + 1;

        // 找到 </hN>
        let end_tag = format!("</h{}>", level);
        let after_content = &result[content_start..];
        let et = after_content.find(&end_tag);
        let content_end = match et {
            Some(e) => content_start + e,
            None => { pos = content_start; continue; }
        };

        let raw_content = &result[content_start..content_end];
        let clean = strip_tags(raw_content);
        let text = clean.trim();
        if !text.is_empty() {
            counter += 1;
            let id = make_id(text, counter);

            // 注入 id 属性到 heading 标签
            let before_gt = &result[tag_start..content_start - 1]; // <hN ...attrs...
            let has_id = before_gt.contains(" id=");
            if !has_id {
                let ins = format!(" id=\"{}\"", id);
                result.insert_str(content_start - 1, &ins);
                // 调整位置偏移
                let shift = ins.len();
                items.push((format!("h{}", level), id, text.to_string()));
                pos = content_end + end_tag.len() + shift;
            } else {
                items.push((format!("h{}", level), id, text.to_string()));
                pos = content_end + end_tag.len();
            }
        } else {
            pos = content_end + end_tag.len();
        }
    }

    // 构建 TOC HTML
    let toc = if items.is_empty() {
        String::new()
    } else {
        let mut t = String::from("<ul class=\"toc\" id=\"toc\">");
        for (level, id, text) in &items {
            let lvl: u32 = level[1..].parse().unwrap_or(1);
            let cls = format!("toc-h{}", lvl.min(6));
            let safe = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            t.push_str(&format!("<li><a href=\"#{}\" class=\"{}\">{}</a></li>", id, cls, safe));
        }
        t.push_str("</ul>");
        t
    };

    (toc, result)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn make_id(text: &str, counter: u64) -> String {
    let id: String = text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '_')
        .collect();
    let id = id.trim().to_lowercase();
    let id: String = id.split_whitespace().collect::<Vec<_>>().join("-");
    if id.is_empty() { format!("heading-{}", counter) } else { id }
}
