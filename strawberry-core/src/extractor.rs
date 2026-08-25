//! Line-oriented deterministic extraction of technical artifacts from raw
//! chat text. Pure functions only — no I/O, no AI, no network.

use std::collections::HashSet;

use crate::rules as r;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    pub language: String,
    pub code: String,
}

/// An approach that was tried and abandoned, plus the reason when the chat
/// stated one. Negative knowledge: this is what stops the receiving AI from
/// re-proposing a known failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedOption {
    /// The rejection statement itself (clamped).
    pub what: String,
    /// Clause after a reason connector ("because ...", "kyunki ...").
    pub why: Option<String>,
}

/// A hard identifier that must survive compression verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    /// One of: env, port, table, fn, version, path, url.
    pub kind: &'static str,
    pub value: String,
}

impl Identifier {
    /// Storage/render form: `env:HERMES_API_KEY`.
    pub fn tagged(&self) -> String {
        format!("{}:{}", self.kind, self.value)
    }
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
    /// Tried-and-abandoned approaches with reasons.
    pub rejected: Vec<RejectedOption>,
    /// Hard rules the receiving AI must not violate.
    pub constraints: Vec<String>,
    /// Verbatim identifiers: env vars, ports, tables, functions, versions.
    pub identifiers: Vec<Identifier>,
    /// Sections that hit their caps and should carry the "more items" note.
    pub truncated: Vec<String>,
    /// Lines dropped as harness/meta noise. Reported so a compression claim
    /// can be audited instead of trusted.
    pub noise_lines: usize,
    /// Lines demoted as assistant self-narration.
    pub narration_lines: usize,
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

/// Which rejection phrase (if any) fired on this line.
fn rejected_marker(lower_trimmed: &str) -> Option<&'static str> {
    r::REJECTED_KEYWORDS
        .iter()
        .copied()
        .find(|kw| lower_trimmed.contains(kw))
}

fn is_constraint_line(lower_trimmed: &str) -> bool {
    r::CONSTRAINT_KEYWORDS
        .iter()
        .any(|kw| r::contains_keyword(lower_trimmed, kw))
}

/// Split a rejection line into (statement, reason) at the first reason
/// connector.
///
/// The reason clause is *removed* from the statement: keeping it in both
/// places would pay for the same tokens twice, which is the exact waste this
/// crate exists to remove.
///
/// Searches case-insensitively but slices the original text so the output
/// stays readable.
fn split_reason(content: &str) -> (String, Option<String>) {
    let lower = content.to_lowercase();
    let mut best: Option<(usize, usize)> = None;
    for c in r::REASON_CONNECTORS {
        if let Some(idx) = lower.find(c) {
            if best.map(|(b, _)| idx < b).unwrap_or(true) {
                best = Some((idx, idx + c.len()));
            }
        }
    }
    let Some((at, after)) = best else {
        return (content.to_string(), None);
    };
    let (Some(head), Some(tail)) = (content.get(..at), content.get(after..)) else {
        return (content.to_string(), None);
    };
    let why = tail.trim().trim_end_matches(['.', ',', ';', '!']);
    let head = head.trim().trim_end_matches([',', ';']);
    // Too short on either side means the connector was incidental prose.
    if why.chars().count() < 3 || head.chars().count() < 3 {
        return (content.to_string(), None);
    }
    (clamp(head, 240), Some(clamp(why, 160)))
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

fn looks_like_env_var(token: &str) -> bool {
    if token.chars().count() < 5 {
        return false;
    }
    if r::ENV_VAR_DENYLIST.contains(&token) {
        return false;
    }
    let underscores = token.bytes().filter(|b| *b == b'_').count();
    if underscores >= 2 {
        return true;
    }
    r::ENV_VAR_SUFFIXES.iter().any(|s| token.ends_with(s))
}

/// Collect verbatim identifiers from one line of text (code or prose).
fn collect_identifiers(
    line: &str,
    seen: &mut HashSet<String>,
    out: &mut Vec<Identifier>,
) {
    let mut push = |kind: &'static str, value: String| {
        if out.len() >= r::MAX_IDENTIFIERS {
            return;
        }
        let key = format!("{kind}:{value}");
        if seen.insert(key) {
            out.push(Identifier { kind, value });
        }
    };

    for m in r::env_var_regex().find_iter(line) {
        let tok = m.as_str();
        if looks_like_env_var(tok) {
            push("env", tok.to_string());
        }
    }
    for c in r::port_regex().captures_iter(line) {
        if let Some(p) = c.get(1) {
            let n: u32 = p.as_str().parse().unwrap_or(0);
            if (1..=65_535).contains(&n) {
                push("port", p.as_str().to_string());
            }
        }
    }
    for c in r::table_regex().captures_iter(line) {
        if let Some(t) = c.get(1) {
            push("table", t.as_str().to_string());
        }
    }
    for c in r::fn_regex().captures_iter(line) {
        if let Some(f) = c.get(1) {
            push("fn", f.as_str().to_string());
        }
    }
    for c in r::version_regex().captures_iter(line) {
        if let (Some(name), Some(ver)) = (c.get(1), c.get(2)) {
            push("version", format!("{}@{}", name.as_str(), ver.as_str()));
        }
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
    let mut seen_rejected: HashSet<String> = HashSet::new();
    let mut seen_constraints: HashSet<String> = HashSet::new();
    let mut seen_identifiers: HashSet<String> = HashSet::new();

    // --- code blocks (dedupe exact bodies, preserve order) ------------------
    let mut total_unique_blocks = 0usize;
    for block in code_blocks_raw {
        let key = block.code.trim().to_string();
        if key.is_empty() || seen_blocks.contains(&key) {
            continue;
        }
        seen_blocks.insert(key);
        total_unique_blocks += 1;
        // Identifiers inside code are the most reliable ones.
        for line in block.code.lines() {
            collect_identifiers(line, &mut seen_identifiers, &mut out.identifiers);
        }
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
        let user_speaking = matches!(role, Some("User") | Some("Human") | Some("Me"));
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

        let lower_trimmed = trimmed.to_lowercase();
        let lower_content = content.to_lowercase();

        // Harness banners and meta-instructions carry no handoff value at all.
        if r::is_noise_line(&lower_content) {
            out.noise_lines += 1;
            last_role_was_user = user_speaking;
            continue;
        }

        // Assistant self-narration ("Let me check the config…") is demoted:
        // identifiers and errors inside it are still real, but the sentence
        // itself must never occupy a handoff slot.
        let narration = r::is_narration_line(&lower_content);
        if narration {
            out.narration_lines += 1;
        }

        // Identifiers (prose lines too: env vars and ports appear in prose).
        collect_identifiers(trimmed, &mut seen_identifiers, &mut out.identifiers);

        // URLs.
        for m in r::url_regex().find_iter(trimmed) {
            let url = m
                .as_str()
                .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']);
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

        // Commands.
        if let Some(cmd) = is_command_line(trimmed) {
            if out.commands.len() < r::MAX_COMMANDS {
                out.commands.push(cmd);
            }
        }

        // Errors. Skip questions, and skip narration: "Let me investigate that
        // error" is a log line, not the error itself.
        if !narration && is_error_line(&lower_trimmed) && !content.ends_with('?') {
            if out.errors.len() < r::MAX_ERRORS {
                out.errors.push(content.clone());
            }
        }

        // Rejected approaches. Checked before decisions so a line like
        // "decided to use X instead of Y" lands in both, but questions are
        // never rejections.
        let is_question = content.ends_with('?');
        let rejection = if is_question {
            None
        } else {
            rejected_marker(&lower_content)
        };
        if rejection.is_some() {
            let (what, why) = split_reason(&content);
            let what = clamp(&what, 240);
            if seen_rejected.insert(what.to_lowercase())
                && out.rejected.len() < r::MAX_REJECTED
            {
                out.rejected.push(RejectedOption { what, why });
            }
        }

        // Constraints: short imperative rules only, and never self-narration.
        if !is_question
            && !narration
            && content.chars().count() <= r::CONSTRAINT_MAX_CHARS
            && is_constraint_line(&lower_content)
        {
            let c = clamp(&content, r::CONSTRAINT_MAX_CHARS);
            if seen_constraints.insert(c.to_lowercase())
                && out.constraints.len() < r::MAX_CONSTRAINTS
            {
                out.constraints.push(c);
            }
        }

        // Decisions / actions. Narration lines are skipped: "Let me update the
        // Cargo.toml" is a log entry, not a decision or an action item.
        if !narration
            && is_decision_line(&lower_trimmed)
            && out.decisions.len() < r::MAX_DECISIONS
        {
            out.decisions.push(clamp(&content, 300));
        }
        if !narration
            && is_action_line(&lower_trimmed)
            && out.action_items.len() < r::MAX_ACTION_ITEMS
        {
            out.action_items.push(clamp(&content, 300));
        }

        // Questions.
        if is_question && content.len() >= 8 {
            if seen_questions.insert(lower_content.clone())
                && out.questions.len() < r::MAX_QUESTIONS
            {
                out.questions.push(clamp(&content, 300));
            }
        }

        // Structural elements and scored candidates for key points.
        let structural_heading = trimmed.starts_with('#');
        let structural_bullet =
            (trimmed.starts_with("- ") || trimmed.starts_with("* ")) && content.len() > 3;
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
        let keep = !narration && (structural_heading || structural_list || score > 0);

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
    if out.rejected.len() >= r::MAX_REJECTED {
        out.truncated.push("rejected".to_string());
    }
    if out.constraints.len() >= r::MAX_CONSTRAINTS {
        out.truncated.push("constraints".to_string());
    }
    if out.identifiers.len() >= r::MAX_IDENTIFIERS {
        out.truncated.push("identifiers".to_string());
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
        let raw =
            "User:\nHow do I fix the login redirect loop?\nAssistant:\nCheck your auth guard config.";
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
        assert!(ex
            .action_items
            .iter()
            .any(|a| a.contains("integration tests")));
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

    // --- feature 2: rejected options ---------------------------------------

    #[test]
    fn rejected_english_with_reason() {
        let ex = extract("We tried polling every 100ms but it didn't work because the CPU spiked.");
        assert_eq!(ex.rejected.len(), 1);
        assert!(ex.rejected[0].what.contains("polling"));
        assert_eq!(ex.rejected[0].why.as_deref(), Some("the CPU spiked"));
    }

    #[test]
    fn rejected_hinglish_with_reason() {
        let ex = extract("Wayland clipboard API nahi chala kyunki permission nahi mili");
        assert_eq!(ex.rejected.len(), 1);
        assert!(ex.rejected[0].what.contains("Wayland"));
        assert_eq!(ex.rejected[0].why.as_deref(), Some("permission nahi mili"));
    }

    #[test]
    fn rejected_without_reason_has_none() {
        let ex = extract("Reverted the sqlx migration.");
        assert_eq!(ex.rejected.len(), 1);
        assert!(ex.rejected[0].why.is_none());
    }

    #[test]
    fn rejected_ignores_questions() {
        let ex = extract("Why didn't work the parser?");
        assert!(ex.rejected.is_empty());
    }

    #[test]
    fn rejected_deduped() {
        let ex = extract("sqlx didn't work\nsomething else\nSQLX didn't work");
        assert_eq!(ex.rejected.len(), 1);
    }

    #[test]
    fn switched_from_and_in_favor_of_detected() {
        let ex = extract(
            "Switched from sqlx to rusqlite.\nDropped Electron in favor of Tauri because bundle size.",
        );
        assert_eq!(ex.rejected.len(), 2);
        assert_eq!(ex.rejected[1].why.as_deref(), Some("bundle size"));
    }

    #[test]
    fn constraints_detected_and_prose_rejected() {
        let ex = extract(
            "Everything must stay offline.\nSirf Rust use karna hai.\nThis is a long friendly paragraph that merely mentions we could always consider many different possible approaches over time and never really commits to any specific rule at all whatsoever in practice today.",
        );
        assert!(ex.constraints.iter().any(|c| c.contains("offline")));
        assert!(ex.constraints.iter().any(|c| c.contains("Sirf Rust")));
        assert!(!ex.constraints.iter().any(|c| c.contains("friendly paragraph")));
    }

    // --- feature 3: identifiers --------------------------------------------

    #[test]
    fn identifiers_env_port_table_fn_version() {
        let raw = "\
Set HERMES_CUSTOM_API_KEY in the env.
Dev server binds localhost:1420 strictly.
```sql
CREATE TABLE IF NOT EXISTS chat_artifacts (id TEXT);
```
```rust
fn build_handoff() {}
```
We pin rusqlite@0.31 exactly.";
        let ex = extract(raw);
        let tagged: Vec<String> = ex.identifiers.iter().map(|i| i.tagged()).collect();
        assert!(tagged.contains(&"env:HERMES_CUSTOM_API_KEY".to_string()));
        assert!(tagged.contains(&"port:1420".to_string()));
        assert!(tagged.contains(&"table:chat_artifacts".to_string()));
        assert!(tagged.contains(&"fn:build_handoff".to_string()));
        assert!(tagged.contains(&"version:rusqlite@0.31".to_string()));
    }

    #[test]
    fn identifiers_deduped_and_denylisted() {
        let ex = extract("XDG_DATA_HOME twice XDG_DATA_HOME plus CREATE_TABLE noise");
        let envs: Vec<&Identifier> =
            ex.identifiers.iter().filter(|i| i.kind == "env").collect();
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].value, "XDG_DATA_HOME");
    }

    #[test]
    fn single_word_caps_not_env_var() {
        let ex = extract("This IMPORTANT thing matters");
        assert!(!ex.identifiers.iter().any(|i| i.kind == "env"));
    }

    #[test]
    fn env_var_with_known_suffix_and_one_underscore() {
        let ex = extract("export API_KEY=abc123");
        assert!(ex
            .identifiers
            .iter()
            .any(|i| i.kind == "env" && i.value == "API_KEY"));
    }

    #[test]
    fn extraction_is_deterministic() {
        let raw = "Tried sqlx, didn't work because async overhead. Use rusqlite@0.31 on localhost:1420.";
        let a = extract(raw);
        let b = extract(raw);
        assert_eq!(a.rejected, b.rejected);
        assert_eq!(a.identifiers, b.identifiers);
    }
}
