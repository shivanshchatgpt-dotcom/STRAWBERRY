//! 📄 DOCX — Strawberry's offline block-document workspace.
//!
//! COPY → PASTE → UNDERSTAND → ORGANIZE → EDIT.
//!
//! Architecture (spec-compliant, zero new dependencies):
//!   * Block model: typed JSON blocks with stable ids, serde-serializable.
//!   * Smart Paste: deterministic detection (HTML table / CSV-TSV / todo /
//!     tree / LaTeX / code) in Rust — heavy parsing stays native.
//!   * Sanitizer: allowlist-based HTML cleaner (no script/iframe/object/
//!     embed/event-handlers ever survive a paste).
//!   * Storage: SQLite `docx_documents` + FTS5 (existing infra).
//!   * Export: Markdown + HTML + native .json backup, all local.

use serde::{Deserialize, Serialize};

// ─────────────────────────── block model ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Text,
    Heading,
    Table,
    Formula,
    Tree,
    Graph,
    Chart,
    Todo,
    Image,
    Code,
    Divider,
    Callout,
}

impl BlockType {
    pub fn label(&self) -> &'static str {
        match self {
            BlockType::Text => "text",
            BlockType::Heading => "heading",
            BlockType::Table => "table",
            BlockType::Formula => "formula",
            BlockType::Tree => "tree",
            BlockType::Graph => "graph",
            BlockType::Chart => "chart",
            BlockType::Todo => "todo",
            BlockType::Image => "image",
            BlockType::Code => "code",
            BlockType::Divider => "divider",
            BlockType::Callout => "callout",
        }
    }
}

/// Table visual configuration (spec §TABLE REQUIREMENTS).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TableProps {
    #[serde(default = "default_true")]
    pub header_row: bool,
    #[serde(default = "default_border_thickness")]
    pub border_thickness: u32,
    #[serde(default = "default_border_color")]
    pub border_color: String,
    #[serde(default)]
    pub zebra: bool,
    #[serde(default = "default_padding")]
    pub cell_padding: u32,
    #[serde(default = "default_align")]
    pub align: String,
    #[serde(default = "default_true")]
    pub outer_border: bool,
}

fn default_true() -> bool { true }
fn default_border_thickness() -> u32 { 1 }
fn default_border_color() -> String { "#333333".into() }
fn default_padding() -> u32 { 6 }
fn default_align() -> String { "left".into() }

impl Default for TableProps {
    fn default() -> Self {
        Self {
            header_row: true,
            border_thickness: 1,
            border_color: "#333333".into(),
            zebra: false,
            cell_padding: 6,
            align: "left".into(),
            outer_border: true,
        }
    }
}

/// Chart configuration. `source_block_id` keeps the table→chart link alive
/// (spec: changing source data updates the chart).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChartConfig {
    #[serde(default = "default_chart_type")]
    pub chart_type: String, // bar | line | pie | scatter
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub x_label: String,
    #[serde(default)]
    pub y_label: String,
    #[serde(default)]
    pub show_legend: bool,
    #[serde(default)]
    pub source_block_id: Option<String>,
    /// Inline editable annotation overlays.
    #[serde(default)]
    pub annotations: Vec<ChartAnnotation>,
    /// Embedded data rows when no source table (first row = headers).
    #[serde(default)]
    pub data: Vec<Vec<String>>,
}

fn default_chart_type() -> String { "bar".into() }

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            chart_type: "bar".into(),
            title: String::new(),
            x_label: String::new(),
            y_label: String::new(),
            show_legend: false,
            source_block_id: None,
            annotations: vec![],
            data: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChartAnnotation {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default = "default_annotation_size")]
    pub font_size: u32,
}

fn default_annotation_size() -> u32 { 13 }

/// Todo/banner block (spec §TODO/BANNER).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoTask {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default = "default_priority")]
    pub priority: u8, // 0=none 1=low 2=med 3=high
}

fn default_priority() -> u8 { 0 }

/// Editable tree node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub children: Vec<TreeNode>,
    #[serde(default)]
    pub collapsed: bool,
}

/// One document block. `data` holds the typed payload; typed accessors
/// keep the model open to future block kinds without a redesign.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub id: String,
    #[serde(rename = "type")]
    pub block_type: BlockType,
    #[serde(default)]
    pub data: serde_json::Value,
}

impl Block {
    pub fn new(block_type: BlockType, data: serde_json::Value) -> Self {
        Self {
            id: crate::db::new_uuid(),
            block_type,
            data,
        }
    }
}

/// A full document (generation-friendly; storage lives in SQLite).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocxDocument {
    pub id: String,
    pub title: String,
    pub blocks: Vec<Block>,
    pub created_at: String,
    pub updated_at: String,
}

// ─────────────────────────── HTML sanitizer ───────────────────────────

/// Allowlist sanitizer (spec §SECURITY). Strips every tag outside the list,
/// every attribute except `src` on img (and forces it to a safe scheme),
/// every script/iframe/object/embed/event-handler form. Deterministic,
/// dependency-free.
pub fn sanitize_html(input: &str) -> String {
    let allowed = [
        "p", "br", "b", "i", "strong", "em", "u", "s", "code", "pre", "blockquote",
        "table", "thead", "tbody", "tr", "td", "th",
        "ul", "ol", "li", "sup", "sub", "mark", "h1", "h2", "h3", "h4", "h5", "h6",
        "span", "div", "a", "img",
    ];
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        if rest.starts_with("<!--") {
            // comment: drop through matching close
            if let Some(end) = rest.find("-->") {
                rest = &rest[end + 3..];
            } else {
                break;
            }
            continue;
        }
        let close = match rest.find('>') {
            Some(c) => c,
            None => break,
        };
        let tag_src = &rest[..=close];
        let inner = tag_src
            .trim_start_matches('<')
            .trim_end_matches('>')
            .trim_start_matches('/');
        let is_closing = rest.starts_with("</");
        let tag_name = inner
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();

        if allowed.contains(&tag_name.as_str()) {
            let rendered = if is_closing { format!("</{tag_name}>") } else { format!("<{tag_name}>") };
            // Keep the tag but strip attributes except safe img src / a href.
            if tag_name == "img" && !is_closing {
                let url = extract_attr(tag_src, "src").unwrap_or_default();
                if url.starts_with("data:image/")
                    || url.starts_with("https://")
                    || url.starts_with("http://")
                {
                    out.push_str(&format!("<img src=\"{url}\" alt=\"\">"));
                }
            } else if tag_name == "a" && !is_closing {
                let url = extract_attr(tag_src, "href").unwrap_or_default();
                if url.starts_with("https://") || url.starts_with("http://") {
                    out.push_str(&format!("<a href=\"{url}\">"));
                } else {
                    out.push_str("<a>");
                }
            } else {
                out.push_str(&rendered);
            }
        }
        // Disallowed tags are dropped entirely (their children were plain
        // text in `out` already since we only skip the tag itself).
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(r#"{attr}\s*=\s*["']([^"']*)["']"#)).ok()?;
    re.captures(tag).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
}

// ─────────────────────────── smart paste ───────────────────────────

/// What the clipboard offered (frontend collects formats; Rust decides).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteInput {
    pub html: Option<String>,
    pub text: Option<String>,
}

/// Deterministic detection + conversion into blocks.
///
/// Priority (spec: prefer structured over plain):
///   1. HTML containing <table> → table blocks (+ paragraphs around them)
///   2. HTML lists → rich text
///   3. TSV/CSV/pipe tables → table
///   4. Todo syntax → todo block
///   5. Markdown/indented tree → tree block
///   6. LaTeX-ish math → formula
///   7. Fenced/indented code heuristics → code
///   8. Plain text → text block
pub fn parse_paste(input: &PasteInput) -> Vec<Block> {
    if let Some(html) = input.html.as_deref().filter(|h| !h.trim().is_empty()) {
        let blocks = parse_html_smart(html);
        if !blocks.is_empty() {
            return blocks;
        }
    }
    if let Some(text) = input.text.as_deref().filter(|t| !t.trim().is_empty()) {
        return parse_plain_text(text);
    }
    Vec::new()
}

/// HTML route: sanitize, then split into table segments vs prose.
fn parse_html_smart(html: &str) -> Vec<Block> {
    let clean = sanitize_html(html);
    let mut blocks = Vec::new();
    // Extract <table>…</table> spans; prose between them becomes text blocks.
    let table_re = regex::Regex::new(r"(?is)<table[^>]*>(.*?)</table>").unwrap();
    let mut cursor = 0usize;
    for cap in table_re.captures_iter(&clean) {
        let whole = cap.get(0).unwrap();
        let prose = &clean[cursor..whole.start()];
        let trimmed = prose.trim();
        if !trimmed.is_empty() {
            blocks.push(Block::new(
                BlockType::Text,
                serde_json::json!({ "html": sanitize_html(trimmed) }),
            ));
        }
        let rows = html_table_to_rows(whole.as_str());
        if !rows.is_empty() {
            blocks.push(make_table_block(rows));
        }
        cursor = whole.end();
    }
    let tail = clean[cursor..].trim();
    if !tail.is_empty() {
        blocks.push(Block::new(
            BlockType::Text,
            serde_json::json!({ "html": tail }),
        ));
    }
    blocks
}

/// Parse a sanitized <table> into string rows (cells stripped of tags).
fn html_table_to_rows(table_html: &str) -> Vec<Vec<String>> {
    let cell_re = regex::Regex::new(r"(?is)<t[dh][^>]*>(.*?)</t[dh]>").unwrap();
    let mut rows: Vec<Vec<String>> = Vec::new();
    // Split rows on <tr>.
    let tr_re = regex::Regex::new(r"(?is)<tr[^>]*>(.*?)</tr>").unwrap();
    for tr in tr_re.captures_iter(table_html) {
        let mut row = Vec::new();
        for cell in cell_re.captures_iter(tr.get(1).unwrap().as_str()) {
            row.push(strip_tags(cell.get(1).unwrap().as_str()).trim().to_string());
        }
        if !row.is_empty() {
            rows.push(row);
        }
    }
    // Fallback for tables without <tr> wrappers (rare malformed pastes).
    if rows.is_empty() {
        let mut row = Vec::new();
        for cell in cell_re.captures_iter(table_html) {
            row.push(strip_tags(cell.get(1).unwrap().as_str()).trim().to_string());
        }
        if !row.is_empty() {
            rows.push(row);
        }
    }
    rows
}

pub fn strip_tags(html: &str) -> String {
    let mut s = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        s.push_str(&rest[..lt]);
        rest = &rest[lt..];
        match rest.find('>') {
            Some(gt) => rest = &rest[gt + 1..],
            None => {
                rest = "";
                break;
            }
        }
    }
    s.push_str(rest);
    decode_entities(&s)
}

/// Minimal entity decoding for common pastes.
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

// ── plain-text detectors ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Delim {
    Tab,
    Pipe,
    Comma,
    Semicolon,
    None,
}

/// Heuristic table detection (spec: tab > pipe > semicolon > comma).
fn detect_table(text: &str) -> Option<(Vec<Vec<String>>, Delim)> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 {
        return None;
    }
    for (delim, name) in [
        (Delim::Tab, '\t'),
        (Delim::Pipe, '|'),
        (Delim::Semicolon, ';'),
        (Delim::Comma, ','),
    ] {
        let counts: Vec<usize> = lines.iter().map(|l| l.matches(name).count()).collect();
        let first = counts[0];
        if first == 0 {
            continue;
        }
        let consistent = counts.iter().all(|c| *c == first);
        if consistent && first >= 1 && lines.len() >= 2 {
            let rows: Vec<Vec<String>> = lines
                .iter()
                .map(|l| {
                    l.split(name as char)
                        .map(|c| c.trim().to_string())
                        .collect()
                })
                .collect();
            // Require at least 2 columns in every row.
            if rows.iter().all(|r| r.len() >= 2) {
                return Some((rows, delim));
            }
        }
    }
    None
}

fn is_todo_text(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_start();
        let tl = t.to_lowercase();
        t.starts_with("[ ]")
            || t.starts_with("[x]")
            || tl.starts_with("todo:")
            || tl.starts_with("todo ")
            || (t.starts_with("- [ ]") )
            || (t.starts_with("- [x]"))
    })
}

/// Indented/markdown tree detection: ≥2 levels of consistent indentation or
/// nested markdown bullets.
fn detect_tree(text: &str) -> Option<TreeNode> {
    let lines: Vec<&str> = text.lines().collect();
    let useful: Vec<&&str> = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .filter(|l| !is_todo_line(l))
        .collect();
    if useful.len() < 3 {
        return None;
    }
    // Markdown bullets with nesting?
    let bullet_re = regex::Regex::new(r"^(\s*)[-*•]\s+(.*)$").unwrap();
    let all_bullets = useful.iter().all(|l| bullet_re.is_match(l.trim_end()));
    // Consistent indentation (2+ spaces per level) without bullets?
    let indented = useful.iter().any(|l| l.starts_with("  ") || l.starts_with('\t'));
    if !all_bullets && !indented {
        return None;
    }

    // Build the tree with an indentation stack. The stack holds the ACTUAL
    // tree nodes (the same allocation chain), so parent links are real —
    // no detached copies.
    let norm_depth = |lead: &str| -> usize {
        // tabs count as one level; every 2 spaces one level.
        let tabs = lead.matches('\t').count();
        let spaces = lead.matches(' ').count();
        tabs + spaces / 2
    };

    let mut stack: Vec<(usize, TreeNode)> = Vec::new();
    // Deferred children: (parent_stack_len, node) — attach once the parent's
    // final position is known. Simplest correct approach: build children
    // bottom-up by carrying nodes in a recursive pass.

    // Strategy: recursive descent over the line list using depth boundaries.
    struct Ctx<'a> {
        lines: &'a [&'a &'a str],
        bullet_re: regex::Regex,
        norm_depth: fn(&str) -> usize,
    }
    let ctx = Ctx {
        lines: &useful,
        bullet_re: regex::Regex::new(r"^(\s*)[-*•]\s+(.*)$").unwrap(),
        norm_depth: norm_depth,
    };

    // Parse children of depth `min_depth` starting at index i; returns
    // (nodes, next_index).
    fn parse_siblings(ctx: &Ctx, i: usize, depth: usize) -> (Vec<TreeNode>, usize) {
        let mut nodes: Vec<TreeNode> = Vec::new();
        let mut idx = i;
        while idx < ctx.lines.len() {
            let line = *ctx.lines[idx];
            let lead_len = line.len() - line.trim_start().len();
            let lead = &line[..lead_len];
            let d = (ctx.norm_depth)(lead);
            let mut text_of = line.trim().to_string();
            if let Some(c) = ctx.bullet_re.captures(line) {
                text_of = c.get(2).unwrap().as_str().trim().to_string();
            }
            if text_of.is_empty() {
                idx += 1;
                continue;
            }
            if d < depth {
                break; // caller (a shallower level) consumes it
            }
            if d > depth {
                // Unexpected deeper jump without a parent at `depth`:
                // treat as child of the previous sibling.
                if let Some(prev) = nodes.last_mut() {
                    let (children, next) = parse_siblings(ctx, idx, d);
                    prev.children.extend(children);
                    idx = next;
                    continue;
                }
                // No previous sibling — tolerate by promoting to this depth.
                let (children, next) = parse_siblings(ctx, idx, d);
                nodes.extend(children);
                idx = next;
                continue;
            }
            // d == depth: a sibling.
            let (children, next) = parse_siblings(ctx, idx + 1, depth + 1);
            nodes.push(TreeNode {
                id: crate::db::new_uuid(),
                text: text_of,
                children,
                collapsed: false,
            });
            idx = next;
        }
        (nodes, idx)
    }

    let (roots, _) = parse_siblings(&ctx, 0, 0);
    match roots.len() {
        0 => None,
        1 => Some(roots.into_iter().next().unwrap()),
        _ => {
            // Multiple top-level lines → wrap under a synthetic Root.
            Some(TreeNode {
                id: crate::db::new_uuid(),
                text: "Root".into(),
                children: roots,
                collapsed: false,
            })
        }
    }
}

fn is_todo_line(l: &str) -> bool {
    let t = l.trim_start();
    t.starts_with("[ ]") || t.starts_with("[x]") || t.starts_with("- [ ]") || t.starts_with("- [x]")
}

fn looks_like_latex(text: &str) -> bool {
    let t = text.trim();
    if t.len() < 4 {
        return false;
    }
    let indicators = [
        "\\frac", "\\sum", "\\int", "\\sqrt", "\\alpha", "\\beta", "\\gamma",
        "\\pi", "\\infty", "\\partial", "\\Delta", "\\cdot", "\\times",
        "^{", "_{", "\\begin{", "\\end{", "\\mathbb", "\\mathcal",
    ];
    indicators.iter().any(|i| t.contains(i)) && t.lines().count() <= 5
}

fn looks_like_code(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 2 {
        return false;
    }
    let codey = lines
        .iter()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("fn ") || t.starts_with("let ") || t.starts_with("use ")
                || t.starts_with("def ") || t.starts_with("class ")
                || t.starts_with("import ") || t.starts_with("function ")
                || t.starts_with("const ") || t.starts_with("var ")
                || t.starts_with("return ") || t.starts_with("if (") || t.starts_with("for (")
                || t.contains(" => ") || t.contains("::") || t.contains("();")
                || t.contains("() {")
        })
        .count();
    codey * 2 >= lines.len()
}

fn make_table_block(rows: Vec<Vec<String>>) -> Block {
    Block::new(
        BlockType::Table,
        serde_json::json!({ "rows": rows, "props": TableProps::default() }),
    )
}

/// Plain-text smart paste.
pub fn parse_plain_text(text: &str) -> Vec<Block> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // 1. Table first (strongest structured signal).
    if let Some((rows, _)) = detect_table(trimmed) {
        return vec![make_table_block(rows)];
    }

    // 2. Todo lines.
    if is_todo_text(trimmed) {
        let mut tasks: Vec<TodoTask> = Vec::new();
        for line in trimmed.lines() {
            let t = line.trim();
            let done = t.starts_with("[x]") || t.starts_with("- [x]");
            let text_of = t
                .trim_start_matches("- [x]")
                .trim_start_matches("- [ ]")
                .trim_start_matches("[x]")
                .trim_start_matches("[ ]")
                .trim_start_matches("TODO:")
                .trim_start_matches("todo:")
                .trim();
            if text_of.is_empty() {
                continue;
            }
            tasks.push(TodoTask {
                id: crate::db::new_uuid(),
                text: text_of.to_string(),
                done,
                priority: 0,
            });
        }
        if !tasks.is_empty() {
            return vec![Block::new(
                BlockType::Todo,
                serde_json::json!({
                    "bannerText": "Tasks",
                    "bannerColor": "#1d2434",
                    "textColor": "#ffffff",
                    "bannerHeight": 52,
                    "tasks": tasks,
                }),
            )];
        }
    }

    // 3. Code (before tree: indented code looks like a tree otherwise).
    if looks_like_code(trimmed) {
        return vec![Block::new(
            BlockType::Code,
            serde_json::json!({ "code": trimmed, "language": "" }),
        )];
    }

    // 4. Tree.
    if let Some(tree) = detect_tree(trimmed) {
        return vec![Block::new(
            BlockType::Tree,
            serde_json::json!({ "root": tree }),
        )];
    }

    // 5. LaTeX.
    if looks_like_latex(trimmed) {
        return vec![Block::new(
            BlockType::Formula,
            serde_json::json!({ "latex": trimmed, "display": true }),
        )];
    }

    // 6. Default: text block (split very long pastes into paragraphs).
    let paragraphs: Vec<&str> = trimmed.split("\n\n").collect();
    if paragraphs.len() > 1 {
        return paragraphs
            .iter()
            .filter(|p| !p.trim().is_empty())
            .map(|p| {
                Block::new(
                    BlockType::Text,
                    serde_json::json!({ "html": format!("<p>{}</p>", escape_html(p.trim())) }),
                )
            })
            .collect();
    }
    vec![Block::new(
        BlockType::Text,
        serde_json::json!({ "html": format!("<p>{}</p>", escape_html(trimmed)) }),
    )]
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ─────────────────────────── plain-text projection (search) ───────────────────────────

/// Derive searchable plain text from blocks.
pub fn blocks_to_plain_text(blocks: &[Block]) -> String {
    let mut out: Vec<String> = Vec::new();
    for b in blocks {
        match b.block_type {
            BlockType::Text | BlockType::Heading | BlockType::Callout => {
                if let Some(html) = b.data.get("html").and_then(|v| v.as_str()) {
                    out.push(strip_tags(html));
                }
            }
            BlockType::Table | BlockType::Chart | BlockType::Graph => {
                if let Some(rows) = b.data.get("rows").and_then(|v| v.as_array()) {
                    for row in rows {
                        let cells: Vec<String> = row
                            .as_array()
                            .map(|r| {
                                r.iter()
                                    .filter_map(|c| c.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        out.push(cells.join(" "));
                    }
                }
                if let Some(title) = b.data.get("title").and_then(|v| v.as_str()) {
                    if !title.is_empty() {
                        out.push(title.to_string());
                    }
                }
            }
            BlockType::Todo => {
                if let Some(tasks) = b.data.get("tasks").and_then(|v| v.as_array()) {
                    for t in tasks {
                        if let Some(text) = t.get("text").and_then(|v| v.as_str()) {
                            out.push(text.to_string());
                        }
                    }
                }
                if let Some(bt) = b.data.get("bannerText").and_then(|v| v.as_str()) {
                    out.push(bt.to_string());
                }
            }
            BlockType::Tree => {
                if let Some(root) = b.data.get("root") {
                    collect_tree_text(root, &mut out);
                }
            }
            BlockType::Formula => {
                if let Some(latex) = b.data.get("latex").and_then(|v| v.as_str()) {
                    out.push(latex.to_string());
                }
            }
            BlockType::Code => {
                if let Some(code) = b.data.get("code").and_then(|v| v.as_str()) {
                    out.push(code.to_string());
                }
            }
            BlockType::Image => {
                if let Some(alt) = b.data.get("alt").and_then(|v| v.as_str()) {
                    out.push(alt.to_string());
                }
            }
            BlockType::Divider => {}
        }
    }
    out.join("\n")
}

fn collect_tree_text(node: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(text) = node.get("text").and_then(|v| v.as_str()) {
        out.push(text.to_string());
    }
    if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
        for c in children {
            collect_tree_text(c, out);
        }
    }
}

// ─────────────────────────── export ───────────────────────────

/// Export blocks to Markdown (spec: portable where practical).
pub fn blocks_to_markdown(blocks: &[Block], title: &str) -> String {
    let mut md = format!("# {title}\n\n");
    for b in blocks {
        match b.block_type {
            BlockType::Heading => {
                let level = b.data.get("level").and_then(|v| v.as_u64()).unwrap_or(2);
                let text = b
                    .data
                    .get("html")
                    .and_then(|v| v.as_str())
                    .map(strip_tags)
                    .unwrap_or_default();
                md.push_str(&"#{".repeat(1) .to_string());
                md.truncate(md.len() - 0);
                md.push_str(&format!("{} {}{}\n\n", "#".repeat(level as usize), text, ""));
            }
            BlockType::Text | BlockType::Callout => {
                let html = b.data.get("html").and_then(|v| v.as_str()).unwrap_or("");
                let text = html_to_markdown(html);
                if b.block_type == BlockType::Callout {
                    md.push_str(&format!("> {text}\n\n"));
                } else {
                    md.push_str(&format!("{text}\n\n"));
                }
            }
            BlockType::Table => {
                if let Some(rows) = b.data.get("rows").and_then(|v| v.as_array()) {
                    let grid: Vec<Vec<String>> = rows
                        .iter()
                        .map(|r| {
                            r.as_array()
                                .map(|cells| {
                                    cells
                                        .iter()
                                        .filter_map(|c| c.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default()
                        })
                        .collect();
                    if !grid.is_empty() {
                        let cols = grid.iter().map(|r| r.len()).max().unwrap_or(0);
                        for (i, row) in grid.iter().enumerate() {
                            md.push('|');
                            for c in 0..cols {
                                let cell = row.get(c).cloned().unwrap_or_default();
                                md.push_str(&format!(" {} |", cell.replace('|', "\\|")));
                            }
                            md.push('\n');
                            if i == 0 {
                                md.push('|');
                                for _ in 0..cols {
                                    md.push_str(" --- |");
                                }
                                md.push('\n');
                            }
                        }
                        md.push('\n');
                    }
                }
            }
            BlockType::Todo => {
                if let Some(tasks) = b.data.get("tasks").and_then(|v| v.as_array()) {
                    for t in tasks {
                        let done = t.get("done").and_then(|v| v.as_bool()).unwrap_or(false);
                        let text = t.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        md.push_str(&format!("- [{}] {}\n", if done { "x" } else { " " }, text));
                    }
                    md.push('\n');
                }
            }
            BlockType::Tree => {
                if let Some(root) = b.data.get("root") {
                    tree_to_markdown(root, 0, &mut md);
                    md.push('\n');
                }
            }
            BlockType::Code => {
                let code = b.data.get("code").and_then(|v| v.as_str()).unwrap_or("");
                let lang = b.data.get("language").and_then(|v| v.as_str()).unwrap_or("");
                md.push_str(&format!("```{lang}\n{code}\n```\n\n"));
            }
            BlockType::Formula => {
                let latex = b.data.get("latex").and_then(|v| v.as_str()).unwrap_or("");
                md.push_str(&format!("$${latex}$$\n\n"));
            }
            BlockType::Divider => md.push_str("---\n\n"),
            BlockType::Chart | BlockType::Graph => {
                let title = b.data.get("title").and_then(|v| v.as_str()).unwrap_or("Chart");
                let rows = b.data.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let mut block = Block { id: String::new(), block_type: BlockType::Table, data: serde_json::json!({ "rows": rows }) };
                let _ = &mut block;
                md.push_str(&format!("**{title}** (chart — data below)\n\n"));
                let grid: Vec<Vec<String>> = rows
                    .iter()
                    .map(|r| {
                        r.as_array()
                            .map(|cells| {
                                cells
                                    .iter()
                                    .filter_map(|c| c.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect();
                if !grid.is_empty() {
                    let cols = grid.iter().map(|r| r.len()).max().unwrap_or(0);
                    for (i, row) in grid.iter().enumerate() {
                        md.push('|');
                        for c in 0..cols {
                            md.push_str(&format!(" {} |", row.get(c).cloned().unwrap_or_default()));
                        }
                        md.push('\n');
                        if i == 0 {
                            md.push('|');
                            for _ in 0..cols {
                                md.push_str(" --- |");
                            }
                            md.push('\n');
                        }
                    }
                    md.push('\n');
                }
            }
            BlockType::Image => {
                let alt = b.data.get("alt").and_then(|v| v.as_str()).unwrap_or("");
                md.push_str(&format!("![{alt}](image)\n\n"));
            }
        }
    }
    md
}

fn tree_to_markdown(node: &serde_json::Value, depth: usize, md: &mut String) {
    if let Some(text) = node.get("text").and_then(|v| v.as_str()) {
        md.push_str(&format!("{}- {}\n", "  ".repeat(depth), text));
    }
    if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
        for c in children {
            tree_to_markdown(c, depth + 1, md);
        }
    }
}

/// Very small inline-HTML → markdown normalizer for export.
fn html_to_markdown(html: &str) -> String {
    let s = html
        .replace("<b>", "**").replace("</b>", "**")
        .replace("<strong>", "**").replace("</strong>", "**")
        .replace("<i>", "*").replace("</i>", "*")
        .replace("<em>", "*").replace("</em>", "*")
        .replace("<code>", "`").replace("</code>", "`")
        .replace("<br>", "\n");
    let s = s.replace("</p>\n<p>", "\n\n");
    let s = s.replace("<p>", "").replace("</p>", "");
    let s = s.replace("<h1>", "# ").replace("<h2>", "## ").replace("<h3>", "### ");
    let s = s.replace("</h1>", "").replace("</h2>", "").replace("</h3>", "");
    strip_tags(&s)
}

/// Export blocks to standalone sanitized HTML.
pub fn blocks_to_html(blocks: &[Block], title: &str) -> String {
    let mut body = String::new();
    for b in blocks {
        match b.block_type {
            BlockType::Text | BlockType::Heading | BlockType::Callout => {
                body.push_str(&sanitize_html(
                    b.data.get("html").and_then(|v| v.as_str()).unwrap_or(""),
                ));
            }
            BlockType::Table => {
                if let Some(rows) = b.data.get("rows").and_then(|v| v.as_array()) {
                    body.push_str("<table>");
                    for (i, row) in rows.iter().enumerate() {
                        body.push_str("<tr>");
                        if let Some(cells) = row.as_array() {
                            for c in cells {
                                let tag = if i == 0 { "th" } else { "td" };
                                body.push_str(&format!(
                                    "<{t}>{}</{t}>",
                                    c.as_str().unwrap_or(""),
                                    t = tag
                                ));
                            }
                        }
                        body.push_str("</tr>");
                    }
                    body.push_str("</table>");
                }
            }
            BlockType::Code => {
                body.push_str(&format!(
                    "<pre><code>{}</code></pre>",
                    escape_html(b.data.get("code").and_then(|v| v.as_str()).unwrap_or(""))
                ));
            }
            BlockType::Formula => {
                body.push_str(&format!(
                    "<div class=\"math\">{}</div>",
                    escape_html(b.data.get("latex").and_then(|v| v.as_str()).unwrap_or(""))
                ));
            }
            BlockType::Todo => {
                body.push_str("<ul class=\"todo\">");
                if let Some(tasks) = b.data.get("tasks").and_then(|v| v.as_array()) {
                    for t in tasks {
                        let done = t.get("done").and_then(|v| v.as_bool()).unwrap_or(false);
                        body.push_str(&format!(
                            "<li>{} {}</li>",
                            if done { "☑" } else { "☐" },
                            t.get("text").and_then(|v| v.as_str()).unwrap_or("")
                        ));
                    }
                }
                body.push_str("</ul>");
            }
            BlockType::Tree => {
                body.push_str("<ul class=\"tree\">");
                if let Some(root) = b.data.get("root") {
                    tree_to_html(root, &mut body);
                }
                body.push_str("</ul>");
            }
            BlockType::Divider => body.push_str("<hr>"),
            BlockType::Chart | BlockType::Graph => {
                body.push_str(&format!(
                    "<p><b>{}</b> (chart data in JSON export)</p>",
                    b.data.get("title").and_then(|v| v.as_str()).unwrap_or("Chart")
                ));
            }
            BlockType::Image => body.push_str("<p>[image]</p>"),
        }
    }
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{}</title>\
<style>body{{font-family:system-ui;max-width:860px;margin:40px auto;padding:0 16px}}\
table{{border-collapse:collapse}}td,th{{border:1px solid #333;padding:6px 10px}}\
pre{{background:#f4f4f4;padding:12px;border-radius:8px}}</style></head><body>{}</body></html>",
        escape_html(title),
        body
    )
}

fn tree_to_html(node: &serde_json::Value, out: &mut String) {
    out.push_str("<li>");
    out.push_str(&escape_html(node.get("text").and_then(|v| v.as_str()).unwrap_or("")));
    if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
        if !children.is_empty() {
            out.push_str("<ul>");
            for c in children {
                tree_to_html(c, out);
            }
            out.push_str("</ul>");
        }
    }
    out.push_str("</li>");
}

// ─────────────────────────── tests ───────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitizer ──────────────────────────────────────────────────

    #[test]
    fn sanitizer_strips_scripts_and_handlers() {
        let dirty = "<p onclick=\"evil()\">hi</p><script>alert(1)</script><iframe src=\"x\"></iframe><b>keep</b>";
        let clean = sanitize_html(dirty);
        assert!(!clean.contains("script"), "got: {clean}");
        assert!(!clean.contains("iframe"), "got: {clean}");
        assert!(!clean.contains("onclick"), "got: {clean}");
        assert!(clean.contains("<b>keep</b>"), "safe tags survive: {clean}");
        assert!(clean.contains("hi"));
    }

    #[test]
    fn sanitizer_blocks_javascript_urls() {
        let clean = sanitize_html("<a href=\"javascript:evil()\">x</a><img src=\"data:image/png;base64,AAAA\">");
        assert!(!clean.contains("javascript"), "got: {clean}");
        assert!(clean.contains("data:image/png"), "safe img kept: {clean}");
    }

    #[test]
    fn sanitizer_keeps_table_structure() {
        let clean = sanitize_html("<table><tr><td style=\"color:red\">a</td></tr></table>");
        assert!(clean.contains("<table>") && clean.contains("<td>"), "got: {clean}");
        assert!(!clean.contains("style"), "attributes stripped: {clean}");
    }

    // ── smart paste: html ───────────────────────────────────────────

    #[test]
    fn html_table_paste_makes_table_block() {
        let blocks = parse_paste(&PasteInput {
            html: Some("<p>Intro</p><table><tr><td>Name</td><td>Marks</td></tr><tr><td>Amit</td><td>85</td></tr></table>".into()),
            text: Some("Intro Name Marks Amit 85".into()),
        });
        assert_eq!(blocks.len(), 2, "prose + table");
        assert_eq!(blocks[0].block_type, BlockType::Text);
        assert_eq!(blocks[1].block_type, BlockType::Table);
        let rows = blocks[1].data["rows"].as_array().unwrap();
        assert_eq!(rows[0][0].as_str().unwrap(), "Name");
        assert_eq!(rows[1][1].as_str().unwrap(), "85");
    }

    // ── smart paste: plain text tables ──────────────────────────────

    #[test]
    fn tsv_paste_makes_table() {
        let blocks = parse_plain_text("Name\tMarks\nAmit\t85\nNeha\t91");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_type, BlockType::Table);
        assert_eq!(blocks[0].data["rows"][1][0].as_str().unwrap(), "Amit");
    }

    #[test]
    fn csv_paste_makes_table() {
        let blocks = parse_plain_text("Name,Marks\nAmit,85\nNeha,91");
        assert_eq!(blocks[0].block_type, BlockType::Table);
        assert_eq!(blocks[0].data["rows"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn pipe_table_paste_makes_table() {
        let blocks = parse_plain_text("Name | Marks\nAmit | 85\nNeha | 91");
        assert_eq!(blocks[0].block_type, BlockType::Table);
    }

    #[test]
    fn single_paragraph_is_text_not_table() {
        let blocks = parse_plain_text("Just one line, with a comma but no structure.");
        assert_eq!(blocks[0].block_type, BlockType::Text);
    }

    // ── smart paste: todo ───────────────────────────────────────────

    #[test]
    fn todo_paste_makes_todo_block() {
        let blocks = parse_plain_text("- [ ] Study chapter 4\n- [x] Revise formulas\nTODO: Practice problems");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_type, BlockType::Todo);
        let tasks = blocks[0].data["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 3);
        assert!(!tasks[0]["done"].as_bool().unwrap());
        assert!(tasks[1]["done"].as_bool().unwrap());
        assert_eq!(tasks[2]["text"].as_str().unwrap(), "Practice problems");
    }

    // ── smart paste: tree ───────────────────────────────────────────

    #[test]
    fn indented_text_pastes_as_tree() {
        let blocks = parse_plain_text(
            "Physics\n  Mechanics\n    Laws of Motion\n  Optics\n    Mirrors",
        );
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_type, BlockType::Tree);
        let root = &blocks[0].data["root"];
        assert_eq!(root["text"].as_str().unwrap(), "Physics");
        assert_eq!(root["children"].as_array().unwrap().len(), 2);
        assert_eq!(root["children"][0]["text"].as_str().unwrap(), "Mechanics");
        assert_eq!(root["children"][0]["children"][0]["text"].as_str().unwrap(), "Laws of Motion");
    }

    #[test]
    fn markdown_bullets_paste_as_tree() {
        let blocks = parse_plain_text("- Physics\n  - Mechanics\n  - Optics");
        assert_eq!(blocks[0].block_type, BlockType::Tree);
        assert_eq!(blocks[0].data["root"]["children"].as_array().unwrap().len(), 2);
    }

    // ── smart paste: math/code ──────────────────────────────────────

    #[test]
    fn latex_paste_makes_formula() {
        let blocks = parse_plain_text("\\frac{a}{b} + \\sqrt{c^2}");
        assert_eq!(blocks[0].block_type, BlockType::Formula);
        assert!(blocks[0].data["latex"].as_str().unwrap().contains("\\frac"));
    }

    #[test]
    fn code_paste_makes_code_block() {
        let blocks = parse_plain_text("fn main() {\n    let x = 42;\n    println!(\"{}\", x);\n}");
        assert_eq!(blocks[0].block_type, BlockType::Code);
    }

    // ── plain text projection ───────────────────────────────────────

    #[test]
    fn plain_text_projection_collects_all_types() {
        let blocks = vec![
            Block::new(BlockType::Text, serde_json::json!({"html": "<p>hello world</p>"})),
            make_table_block(vec![vec!["a".into(), "b".into()], vec!["1".into(), "2".into()]]),
            Block::new(BlockType::Todo, serde_json::json!({"bannerText": "T", "tasks": [{"id":"1","text":"task one","done":false,"priority":0}]})),
        ];
        let pt = blocks_to_plain_text(&blocks);
        assert!(pt.contains("hello world"));
        assert!(pt.contains("a b"));
        assert!(pt.contains("task one"));
    }

    // ── export ─────────────────────────────────────────────────────

    #[test]
    fn markdown_export_round_trips_structure() {
        let blocks = vec![
            make_table_block(vec![vec!["Name".into(), "Marks".into()], vec!["Amit".into(), "85".into()]]),
            Block::new(BlockType::Todo, serde_json::json!({"bannerText": "T", "bannerColor": "#000", "textColor": "#fff", "bannerHeight": 52, "tasks": [{"id":"1","text":"study","done":false,"priority":0}]})),
        ];
        let md = blocks_to_markdown(&blocks, "Test Doc");
        assert!(md.contains("# Test Doc"));
        assert!(md.contains("| Name |"));
        assert!(md.contains("- [ ] study"));
    }

    #[test]
    fn html_export_is_sanitized() {
        let blocks = vec![Block::new(
            BlockType::Text,
            serde_json::json!({"html": "<p onclick=\"x\">ok</p>"}),
        )];
        let html = blocks_to_html(&blocks, "T");
        assert!(!html.contains("onclick"), "got: {html}");
        assert!(html.contains("ok"));
    }

    // ── block model ────────────────────────────────────────────────

    #[test]
    fn blocks_serialize_with_stable_shape() {
        let b = make_table_block(vec![vec!["x".into()]]);
        let j = serde_json::to_string(&b).unwrap();
        assert!(j.contains("\"type\":\"table\""));
        let back: Block = serde_json::from_str(&j).unwrap();
        assert_eq!(back.block_type, BlockType::Table);
        assert_eq!(back.id, b.id);
    }

    #[test]
    fn empty_paste_is_empty() {
        assert!(parse_paste(&PasteInput::default()).is_empty());
        assert!(parse_plain_text("   ").is_empty());
    }

    #[test]
    fn smart_paste_prefers_html_over_text() {
        let blocks = parse_paste(&PasteInput {
            html: Some("<table><tr><td>structured</td></tr></table>".into()),
            text: Some("structured".into()),
        });
        assert_eq!(blocks[0].block_type, BlockType::Table, "HTML wins");
    }
}
