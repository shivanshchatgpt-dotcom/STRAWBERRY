//! Public entry point of the local, rule-based brief generator.

pub mod extractor;
pub mod rules;

use serde::{Deserialize, Serialize};

use self::extractor::Extraction;
use crate::db::models::ChatStats;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub char_count: i64,
    pub word_count: i64,
    pub code_block_count: i64,
    pub error_count: i64,
    pub command_count: i64,
    pub url_count: i64,
}

impl From<Stats> for ChatStats {
    fn from(s: Stats) -> Self {
        Self {
            char_count: s.char_count,
            word_count: s.word_count,
            code_block_count: s.code_block_count,
            error_count: s.error_count,
            command_count: s.command_count,
            url_count: s.url_count,
        }
    }
}

impl From<ChatStats> for Stats {
    fn from(s: ChatStats) -> Self {
        Self {
            char_count: s.char_count,
            word_count: s.word_count,
            code_block_count: s.code_block_count,
            error_count: s.error_count,
            command_count: s.command_count,
            url_count: s.url_count,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GeneratedBrief {
    /// Full markdown brief document.
    pub markdown: String,
    pub first_idea: Option<String>,
    pub stats: Stats,
    /// `(artifact_type, content)` pairs; types match the DB CHECK constraint.
    pub artifacts: Vec<(String, String)>,
}

/// Generate the brief deterministically from raw chat text.
///
/// Pure function of its inputs: same title + text always produce the same
/// output. No AI/LLM/network is involved anywhere in this pipeline.
pub fn generate(title: &str, raw: &str) -> GeneratedBrief {
    let ex: Extraction = extractor::extract(raw);

    let stats = Stats {
        char_count: raw.chars().count() as i64,
        word_count: raw.split_whitespace().count() as i64,
        code_block_count: ex.code_blocks.len() as i64,
        error_count: ex.errors.len() as i64,
        command_count: ex.commands.len() as i64,
        url_count: ex.urls.len() as i64,
    };

    // --- artifacts ----------------------------------------------------------
    let mut artifacts: Vec<(String, String)> = Vec::new();
    for q in &ex.questions {
        artifacts.push(("question".to_string(), q.clone()));
    }
    for kp in &ex.key_points {
        artifacts.push(("answer".to_string(), kp.clone()));
    }
    for h in &ex.headings {
        artifacts.push(("heading".to_string(), h.clone()));
    }
    for cb in &ex.code_blocks {
        let fenced = if cb.language.is_empty() {
            format!("```\n{}\n```", cb.code)
        } else {
            format!("```{}\n{}\n```", cb.language, cb.code)
        };
        artifacts.push(("code".to_string(), fenced));
    }
    for c in &ex.commands {
        artifacts.push(("command".to_string(), c.clone()));
    }
    for e in &ex.errors {
        artifacts.push(("error".to_string(), e.clone()));
    }
    for u in &ex.urls {
        artifacts.push(("url".to_string(), u.clone()));
    }
    for d in &ex.decisions {
        artifacts.push(("decision".to_string(), d.clone()));
    }
    for a in &ex.action_items {
        artifacts.push(("action_item".to_string(), a.clone()));
    }

    // --- markdown ------------------------------------------------------------
    let mut md = String::with_capacity(2048);
    md.push_str("# ");
    md.push_str(title.trim());
    md.push_str("\n");

    if let Some(idea) = &ex.first_idea {
        md.push_str("\n## First Idea\n");
        md.push_str(idea);
        md.push('\n');
    }

    if !ex.questions.is_empty() {
        md.push_str("\n## Questions\n");
        for q in ex.questions.iter().take(rules::MAX_QUESTIONS) {
            md.push_str("- ");
            md.push_str(q);
            md.push('\n');
        }
    }

    if !ex.key_points.is_empty() || !ex.file_paths.is_empty() {
        md.push_str("\n## Answers / Key Points\n");
        for p in ex.key_points.iter().take(rules::MAX_KEY_POINTS) {
            md.push_str("- ");
            md.push_str(p);
            md.push('\n');
        }
        if !ex.file_paths.is_empty() {
            md.push_str("\nFiles referenced:\n");
            for f in dedupe_ref(&ex.file_paths).iter().take(50) {
                md.push_str("- `");
                md.push_str(f);
                md.push_str("`\n");
            }
        }
    }

    if !ex.code_blocks.is_empty() {
        md.push_str("\n## Code Blocks\n");
        for (i, cb) in ex.code_blocks.iter().take(rules::MAX_CODE_BLOCKS).enumerate() {
            if i > 0 {
                md.push('\n');
            }
            if !cb.language.is_empty() {
                md.push_str("```");
                md.push_str(&cb.language);
                md.push('\n');
            } else {
                md.push_str("```\n");
            }
            md.push_str(&cb.code);
            md.push_str("\n```\n");
        }
    }

    if !ex.commands.is_empty() {
        md.push_str("\n## Commands\n");
        for c in ex.commands.iter().take(rules::MAX_COMMANDS) {
            md.push_str("- `");
            md.push_str(c);
            md.push_str("`\n");
        }
    }

    if !ex.errors.is_empty() {
        md.push_str("\n## Errors\n");
        for e in ex.errors.iter().take(rules::MAX_ERRORS) {
            md.push_str("- ");
            md.push_str(e);
            md.push('\n');
        }
    }

    if !ex.decisions.is_empty() {
        md.push_str("\n## Decisions\n");
        for d in ex.decisions.iter().take(rules::MAX_DECISIONS) {
            md.push_str("- ");
            md.push_str(d);
            md.push('\n');
        }
    }

    if !ex.action_items.is_empty() {
        md.push_str("\n## Action Items\n");
        for a in ex.action_items.iter().take(rules::MAX_ACTION_ITEMS) {
            md.push_str("- ");
            md.push_str(a);
            md.push('\n');
        }
    }

    // Truncation notes.
    if !ex.truncated.is_empty() {
        md.push_str("\n> ");
        md.push_str(rules::MORE_NOTE);
        md.push_str(if ex.truncated.len() > 1 { " (multiple sections)" } else { "" });
        md.push('\n');
    }

    md.push_str("\n## Metadata\n");
    md.push_str(&format!("- source: manual/rule-based extraction\n"));
    md.push_str(&format!(
        "- char count: {}\n- word count: {}\n",
        stats.char_count, stats.word_count
    ));
    md.push_str(&format!(
        "- code block count: {}\n- error count: {}\n- command count: {}\n- url count: {}\n",
        stats.code_block_count, stats.error_count, stats.command_count, stats.url_count
    ));

    md.push_str("\n## Note\n");
    md.push_str(
        "This brief is generated locally using rule-based extraction. \
         Original chat is saved separately for full context.\n",
    );

    GeneratedBrief {
        markdown: md,
        first_idea: ex.first_idea.clone(),
        stats,
        artifacts,
    }
}

fn dedupe_ref(items: &[String]) -> Vec<&String> {
    let mut seen = std::collections::HashSet::new();
    items.iter().filter(|i| seen.insert(i.as_str())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_output() {
        let raw = "User:\nWhat is going on with the build?\nAssistant:\nThe build failed.\n```js\nconsole.log(1)\n```";
        let a = generate("T", raw);
        let b = generate("T", raw);
        assert_eq!(a.markdown, b.markdown);
        assert_eq!(a.artifacts, b.artifacts);
    }

    #[test]
    fn sections_present_and_note_always_last_section() {
        let raw = "User:\nFix login bug?\nAssistant:\ndecided to use tokens";
        let g = generate("Login", raw);
        assert!(g.markdown.starts_with("# Login\n"));
        assert!(g.markdown.contains("## First Idea"));
        assert!(g.markdown.contains("## Decisions"));
        assert!(g.markdown.contains("## Note"));
        assert!(g
            .markdown
            .contains("generated locally using rule-based extraction"));
    }

    #[test]
    fn empty_sections_omitted() {
        let g = generate("Empty-ish", "just one plain sentence here");
        assert!(!g.markdown.contains("## Questions"));
        assert!(!g.markdown.contains("## Code Blocks"));
        assert!(!g.markdown.contains("## Errors"));
        assert!(!g.markdown.contains("## Commands"));
    }

    #[test]
    fn stats_are_accurate() {
        let raw = "hello world\n```\ncode\n```";
        let g = generate("S", raw);
        assert_eq!(g.stats.word_count, 5); // hello world ``` code ```
        assert_eq!(g.stats.code_block_count, 1);
        assert_eq!(g.stats.char_count, raw.chars().count() as i64);
    }

    #[test]
    fn limits_add_truncation_note() {
        let mut raw = String::new();
        for i in 0..60 {
            raw.push_str(&format!("decided option number {i} is best because testing\n"));
        }
        let g = generate("Limits", &raw);
        assert!(g.markdown.contains("More items available in original chat."));
        // decisions capped at 50
        let dec_artifacts = g
            .artifacts
            .iter()
            .filter(|(k, _)| k == "decision")
            .count();
        assert_eq!(dec_artifacts, rules::MAX_DECISIONS);
    }
}
