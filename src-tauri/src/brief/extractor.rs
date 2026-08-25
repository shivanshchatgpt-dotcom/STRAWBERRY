//! Line-oriented deterministic extraction of technical artifacts from raw
//! chat text. Pure functions only — no I/O, no AI, no network.

use std::collections::HashSet;

use super::rules as r;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    pub language: String,
    pub code: String,
}

#[derive(Debug, Clone, Default)]
pub struct Extraction {
    pub first_idea: Option<String>,
    pub questions: Vec<String>,
    pub key_points: Vec<String>,
    pub headings: Vec<String>,
    pub code_blocks: Vec<CodeBlock>,
    pub commands: Vec<String>,
    pub errors: Vec<String>,
    pub urls: Vec<String>,
    pub file_paths: Vec<String>,
    pub decisions: Vec<String>,
    pub action_items: Vec<String>,
    /// Sections that hit their caps and should carry the "more items" note.
    pub truncated: Vec<String>,
}

/// Split the text into (code blocks, non-code lines) preserving order of lines.
fn segment_code_blocks(raw: &str) -> (Vec<CodeBlock>, Vec<&str>) {
    let mut blocks: Vec<CodeBlock> = Vec::new();
    let mut plain_lines: Vec<&str> = Vec::new();
    let mut in_code = false;
    let mut current_lang = String::new();
    let mut current_lines: Vec<&str> = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if !in_code {
                in_code = true;
                current_lang = trimmed.trim_start_matches('`').trim().to_string();
                current_lines.clear();
            } else {
                in_code = false;
                blocks.push(CodeBlock {
                    language: current_lang.clone(),
                    code: current_lines.join("\n"),
                });
                current_lines.clear();
            }
            continue;
        }
        if in_code {
            current_lines.push(line);
        } else {
            plain_lines.push(line);
        }
    }
    // Unterminated fence: flush what we have so content is never lost.
    if in_code && !current_lines.is_empty() {
        blocks.push(CodeBlock {
            language: current_lang,
            code: current_lines.join("\n"),
        });
    }
    (blocks, plain_lines)
}

fn is_command_line(line: &str) -> Option<String> {
    let t = line.trim();
    if let Some(rest) = t.strip_prefix("$ ") {
        let rest = rest.trim();
        if !rest.is_empty() {
            return Some(rest.to_string());
        }
    }
    let first_word = t.split_whitespace().next()?.to_lowercase();
    if r::COMMAND_WORDS.contains(&first_word.as_str()) {
        // Require at least a second token or a flag to avoid matching prose
        // like "git is a tool".
        if t.split_whitespace().count() >= 2 {
            return Some(t.to_string());
        }
    }
    None
}

fn is_error_line(lower_trimmed: &str) -> bool {
    for kw in r::ERROR_KEYWORDS {
        if kw.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            // status codes: match anywhere as token-ish
            if lower_trimmed.contains(kw) {
                return true;
            }
        } else if r::contains_word(lower_trimmed, kw) {
            return true;
        }
    }
    false
}

fn is_decision_line(lower_trimmed: &str) -> bool {
    r::DECISION_KEYWORDS
        .iter()
        .any(|kw| r::contains_keyword(lower_trimmed, kw))
}

fn is_action_line(lower_trimmed: &str) -> bool {
    if lower_trimmed.contains("todo") || lower_trimmed.starts_with("next") {
        return true;
    }
    r::ACTION_KEYWORDS
        .iter()
        .any(|kw| r::contains_keyword(lower_trimmed, kw))
}

fn importance_score(lower_content: &str) -> usize {
    r::IMPORTANT_KEYWORDS
        .iter()
        .filter(|kw| r::contains_keyword(lower_content, kw))
        .count()
}

/// Truncate at a word boundary with an ellipsis.
fn clamp(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().take(max_chars).collect();
    match cut.rfind(' ') {
        Some(i) if i > max_chars / 2 => format!("{} …", &cut[..i]),
        _ => format!("{cut} …"),
    }
}

pub fn extract(raw: &str) -> Extraction {
    let (code_blocks_raw, plain_lines) = segment_code_blocks(raw);

    let mut out = Extraction::default();

    // --- dedup helpers -----------------------------------------------------
    let mut seen_lines: HashSet<String> = HashSet::new();
    let mut seen_questions: HashSet<String> = HashSet::new();
    let mut seen_urls: HashSet<String> = HashSet::new();
    let mut seen_paths: HashSet<String> = HashSet::new();
    let mut seen_blocks: HashSet<String> = HashSet::new();

    // --- code blocks (dedupe exact bodies, preserve order) ------------------
    let mut total_unique_blocks = 0usize;
    for block in code_blocks_raw {
        let key = block.code.trim().to_string();
        if key.is_empty() || seen_blocks.contains(&key) {
            continue;
        }
        seen_blocks.insert(key);
        total_unique_blocks += 1;
        if out.code_blocks.len() < r::MAX_CODE_BLOCKS {
            out.code_blocks.push(block);
        }
    }
    if total_unique_blocks > r::MAX_CODE_BLOCKS {
        out.truncated.push("code".to_string());
    }

    // --- line pass ----------------------------------------------------------
    struct KeyPointCandidate {
        text: String,
    }
    let mut candidates: Vec<KeyPointCandidate> = Vec::new();
    let mut first_idea: Option<String> = None;
    let mut last_role_was_user = false;

    for line in plain_lines.iter() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            last_role_was_user = false;
            continue;
        }
        if r::is_filler_line(trimmed) {
            continue;
        }

        let role = r::role_of(trimmed);
        let content = r::strip_role(trimmed).trim().to_string();
        let user_speaking =
            matches!(role, Some("User") | Some("Human") | Some("Me"));
        if content.is_empty() {
            last_role_was_user = user_speaking;
            continue;
        }

        // First idea: first meaningful line directly after a user turn.
        if first_idea.is_none() && last_role_was_user && !content.ends_with(':') {
            first_idea = Some(clamp(&content, r::FIRST_IDEA_MAX_CHARS));
        }

        // Exact-duplicate suppression (normalized).
        let dup_key = content.to_lowercase();
        if !seen_lines.insert(dup_key.clone()) {
            last_role_was_user = user_speaking;
            continue;
        }

        // URLs.
        for m in r::url_regex().find_iter(trimmed) {
            let url = m.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']);
            if seen_urls.insert(url.to_string()) && out.urls.len() < r::MAX_URLS {
                out.urls.push(url.to_string());
            }
        }

        // File paths.
        for m in r::path_regex().find_iter(trimmed) {
            if seen_paths.insert(m.as_str().to_string()) {
                out.file_paths.push(m.as_str().to_string());
            }
        }

        let lower_trimmed = trimmed.to_lowercase();
        let lower_content = content.to_lowercase();

        // Commands.
        if let Some(cmd) = is_command_line(trimmed) {
            if out.commands.len() < r::MAX_COMMANDS {
                out.commands.push(cmd);
            }
        }

        // Errors (skip pure question lines to reduce noise).
        if is_error_line(&lower_trimmed) && !content.ends_with('?') {
            if out.errors.len() < r::MAX_ERRORS {
                out.errors.push(content.clone());
            }
        }

        // Decisions / actions.
        if is_decision_line(&lower_trimmed) && out.decisions.len() < r::MAX_DECISIONS {
            out.decisions.push(clamp(&content, 300));
        }
        if is_action_line(&lower_trimmed)
            && out.action_items.len() < r::MAX_ACTION_ITEMS
        {
            out.action_items.push(clamp(&content, 300));
        }

        // Questions.
        if content.ends_with('?') && content.len() >= 8 {
            if seen_questions.insert(lower_content.clone())
                && out.questions.len() < r::MAX_QUESTIONS
            {
                out.questions.push(clamp(&content, 300));
            }
        }

        // Structural elements and scored candidates for key points.
        let structural_heading = trimmed.starts_with('#');
        let structural_bullet =
            (trimmed.starts_with("- ") || trimmed.starts_with("* "))
                && content.len() > 3;
        let structural_numbered = r::numbered_list_prefix_len(trimmed)
            .map(|prefix_len| trimmed.len() > prefix_len + 2)
            .unwrap_or(false);

        if structural_heading {
            let heading_text = trimmed.trim_start_matches('#').trim().to_string();
            if !heading_text.is_empty() {
                out.headings.push(heading_text);
            }
        }

        let structural_list = structural_bullet || structural_numbered;
        let score = importance_score(&lower_content);
        let keep = structural_heading || structural_list || score > 0;

        if keep {
            let text = if structural_heading || structural_list {
                content.trim_start_matches('#').trim().to_string()
            } else {
                clamp(&content, 300)
            };
            if !text.is_empty() {
                candidates.push(KeyPointCandidate { text });
            }
        }

        last_role_was_user = user_speaking;
    }

    // First-idea fallback: first meaningful non-role line overall.
    if first_idea.is_none() {
        for line in &plain_lines {
            let t = line.trim();
            if t.is_empty() || r::is_filler_line(t) || r::role_of(t).is_some() {
                continue;
            }
            let c = r::strip_role(t).trim();
            if c.is_empty() {
                continue;
            }
            first_idea = Some(clamp(c, r::FIRST_IDEA_MAX_CHARS));
            break;
        }
    }
    out.first_idea = first_idea;

    // --- assemble key points: document order, dedupe, cap -------------------
    let mut seen_points: HashSet<String> = HashSet::new();
    for cand in candidates {
        if out.key_points.len() >= r::MAX_KEY_POINTS {
            out.truncated.push("key points".to_string());
            break;
        }
        let key = cand.text.to_lowercase();
        if seen_points.insert(key) {
            out.key_points.push(cand.text);
        }
    }

    // Caps notes.
    if out.commands.len() >= r::MAX_COMMANDS {
        out.truncated.push("commands".to_string());
    }
    if out.errors.len() >= r::MAX_ERRORS {
        out.truncated.push("errors".to_string());
    }
    if out.urls.len() >= r::MAX_URLS {
        out.truncated.push("urls".to_string());
    }
    if out.decisions.len() >= r::MAX_DECISIONS {
        out.truncated.push("decisions".to_string());
    }
    if out.action_items.len() >= r::MAX_ACTION_ITEMS {
        out.truncated.push("action items".to_string());
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_code_with_language() {
        let raw = "User:\nhow to serve?\n\nAssistant:\nRun this:\n```bash\nnpm run dev\n```\ndone";
        let ex = extract(raw);
        assert_eq!(ex.code_blocks.len(), 1);
        assert_eq!(ex.code_blocks[0].language, "bash");
        assert_eq!(ex.code_blocks[0].code, "npm run dev");
    }

    #[test]
    fn unterminated_fence_is_flushed() {
        let ex = extract("```python\nprint(1)\n");
        assert_eq!(ex.code_blocks.len(), 1);
        assert_eq!(ex.code_blocks[0].language, "python");
    }

    #[test]
    fn duplicate_code_blocks_removed() {
        let raw = "```\na=1\n```\ntext\n```\na=1\n```";
        let ex = extract(raw);
        assert_eq!(ex.code_blocks.len(), 1);
    }

    #[test]
    fn detects_commands() {
        let ex = extract("$ npm install\ncargo build --release\nplease make me a sandwich");
        assert!(ex.commands.contains(&"npm install".to_string()));
        assert!(ex.commands.contains(&"cargo build --release".to_string()));
        assert!(!ex.commands.iter().any(|c| c.contains("sandwich")));
    }

    #[test]
    fn detects_errors_and_skips_fillers() {
        let raw = "User:\nok\nthanks\nAssistant:\nThe build failed with error E0499";
        let ex = extract(raw);
        assert!(!ex.errors.is_empty());
        assert!(ex.errors.iter().any(|e| e.contains("failed")));
    }

    #[test]
    fn first_idea_from_user_turn() {
        let raw = "User:\nHow do I fix the login redirect loop?\nAssistant:\nCheck your auth guard config.";
        let ex = extract(raw);
        assert_eq!(
            ex.first_idea.as_deref(),
            Some("How do I fix the login redirect loop?")
        );
    }

    #[test]
    fn decisions_and_actions_detected() {
        let raw = "We decided to use zustand.\nTODO: write integration tests.";
        let ex = extract(raw);
        assert!(ex.decisions.iter().any(|d| d.contains("zustand")));
        assert!(ex.action_items.iter().any(|a| a.contains("integration tests")));
    }

    #[test]
    fn urls_collected_once() {
        let ex = extract("visit https://example.com/a then https://example.com/a again");
        assert_eq!(ex.urls.len(), 1);
    }

    #[test]
    fn file_paths_found() {
        let ex = extract("open src/lib/api.ts and styles/global.css");
        assert_eq!(ex.file_paths.len(), 2);
    }
}
