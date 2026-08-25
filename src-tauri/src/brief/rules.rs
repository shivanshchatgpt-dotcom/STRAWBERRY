//! Deterministic, rule-based keyword and pattern definitions for the local
//! brief generator. No AI, no network, no external services.

/// Terminal command prefixes (first token of a line).
pub const COMMAND_WORDS: &[&str] = &[
    "npm", "pnpm", "yarn", "pip", "pip3", "python", "python3", "node", "cargo", "git",
    "docker", "flutter", "adb", "make", "gradle",
];

/// Substrings (matched case-insensitively on whole words where possible)
/// that flag a line as error-like.
pub const ERROR_KEYWORDS: &[&str] = &[
    "error", "exception", "traceback", "failed", "failure", "fatal", "undefined",
    "crash", "cannot", "invalid", "unauthorized", "401", "403", "404", "422", "500",
];

/// Keywords that flag decision-like statements.
pub const DECISION_KEYWORDS: &[&str] = &[
    "decided", "final decision", "use this", "avoid", "selected approach",
    "we will", "conclusion", "we'll go with", "going with",
];

/// Keywords that flag action items. `todo` is matched case-insensitively;
/// `next` only when it appears as a leading word like "NEXT:" or "Next steps".
pub const ACTION_KEYWORDS: &[&str] = &[
    "need to", "fix ", "create ", "update ", "delete ", "implement ", "add ",
    "remove ",
];

/// Keywords that make a line high-signal for the key points section.
pub const IMPORTANT_KEYWORDS: &[&str] = &[
    "fix", "bug", "error", "issue", "solution", "implement", "decided", "use",
    "avoid", "because", "problem", "root cause", "steps", "expected", "actual",
    "todo", "task", "important", "note", "architecture", "database", "api",
    "state", "component", "logic", "auth", "login", "payment", "deploy", "build",
    "test", "config", "setup", "install", "create", "update", "delete", "render",
    "route", "schema", "validation", "security", "performance",
];

/// Whole-line fillers (normalized: lowercase, trimmed, punctuation stripped).
pub const FILLER_LINES: &[&str] = &[
    "ok", "okay", "k", "kk", "haan", "hmm", "hmmm", "thanks", "thank you", "thx",
    "shukriya", "bhai", "acha", "accha", "great", "nice", "cool", "wow", "hello",
    "hi", "hey", "yo", "theek", "theek hai", "sahi", "sahi hai", "yes", "yeah",
    "yep", "no problem", "np", "done", "got it", "sure",
];

/// Role prefixes that mark speaker turns. Matched as `"Role:"` at line start.
pub const ROLE_PREFIXES: &[&str] =
    &["User", "Human", "Me", "Assistant", "AI", "Bot", "ChatGPT", "Claude"];

/// File extensions recognized as file paths when preceded by path separators.
pub const PATH_EXTENSIONS: &[&str] = &[
    "py", "js", "jsx", "ts", "tsx", "json", "md", "txt", "html", "css", "scss",
    "java", "kt", "swift", "dart", "sql", "yaml", "yml", "toml", "lock", "log",
];

// Limits -------------------------------------------------------------------

pub const MAX_KEY_POINTS: usize = 100;
pub const MAX_CODE_BLOCKS: usize = 50;
pub const MAX_COMMANDS: usize = 50;
pub const MAX_ERRORS: usize = 50;
pub const MAX_URLS: usize = 50;
pub const MAX_DECISIONS: usize = 50;
pub const MAX_ACTION_ITEMS: usize = 50;
pub const MAX_QUESTIONS: usize = 50;

pub const MORE_NOTE: &str = "More items available in original chat.";

pub const FIRST_IDEA_MAX_CHARS: usize = 240;

// Regexes ------------------------------------------------------------------

use std::sync::OnceLock;
use regex::Regex;

pub fn url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"https?://[^\s<>()\[\]{}"'`]+"#).expect("valid url regex")
    })
}

pub fn path_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let exts = PATH_EXTENSIONS.join("|");
        let pattern = format!(
            r#"(?:[A-Za-z0-9_.\-]+[/\\])+[A-Za-z0-9_.\-]+\.(?:{exts})\b"#
        );
        Regex::new(&pattern).expect("valid path regex")
    })
}

pub fn numbered_list_prefix_len(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i >= bytes.len() {
        return None;
    }
    match bytes[i] {
        b'.' | b')' => Some(i + 1),
        _ => None,
    }
}

/// Normalize a line for filler comparison: lowercase, trim, strip edges of
/// common punctuation.
pub fn normalize_line(line: &str) -> String {
    line.trim()
        .trim_matches(|c: char| matches!(c, '.' | ',' | '!' | '?' | ':' | ';' | '"' | '\''))
        .to_lowercase()
}

pub fn is_filler_line(line: &str) -> bool {
    let n = normalize_line(line);
    if n.is_empty() || n.len() <= 2 && !n.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }
    FILLER_LINES.contains(&n.as_str())
}

pub fn role_of(line: &str) -> Option<&'static str> {
    for role in ROLE_PREFIXES {
        let prefix = format!("{role}:");
        if line.trim_start().len() >= prefix.len()
            && line.trim_start().get(..prefix.len()).map(|s| s.eq_ignore_ascii_case(&prefix)).unwrap_or(false)
        {
            return Some(role);
        }
    }
    None
}

/// Strip a leading role prefix ("User:") from a line, returning the remainder.
pub fn strip_role<'a>(line: &'a str) -> &'a str {
    if role_of(line).is_some() {
        let idx = line.find(':').map(|i| i + 1).unwrap_or(0);
        return line[idx..].trim_start();
    }
    line
}

/// Case-insensitive whole-word-ish containment check.
pub fn contains_keyword(haystack_lower: &str, needle: &str) -> bool {
    if needle.ends_with(' ') {
        // prefix-style keyword like "fix " — require word boundary after
        let core = &needle[..needle.len() - 1];
        return haystack_lower
            .match_indices(core)
            .any(|(i, _)| {
                let before_ok = i == 0
                    || !haystack_lower[..i]
                        .chars()
                        .last()
                        .map(|c| c.is_alphanumeric())
                        .unwrap_or(false);
                before_ok
            });
    }
    haystack_lower.contains(needle)
}

pub fn contains_word(haystack_lower: &str, word: &str) -> bool {
    let mut start = 0usize;
    while let Some(pos) = haystack_lower[start..].find(word) {
        let abs = start + pos;
        let end = abs + word.len();
        let before_ok = abs == 0
            || !haystack_lower[..abs]
                .chars()
                .last()
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false);
        let after_ok = end >= haystack_lower.len()
            || !haystack_lower[end..]
                .chars()
                .next()
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false);
        if before_ok && after_ok {
            return true;
        }
        start = abs + word.len().max(1);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filler_detection() {
        assert!(is_filler_line("OK"));
        assert!(is_filler_line("  Thank you! "));
        assert!(is_filler_line("acha"));
        assert!(is_filler_line("theek hai."));
        assert!(!is_filler_line("We need to fix the parser"));
    }

    #[test]
    fn roles_detected_and_stripped() {
        assert_eq!(role_of("User: hello"), Some("User"));
        assert_eq!(role_of("assistant: hi"), Some("Assistant"));
        assert_eq!(role_of("Claude: sure"), Some("Claude"));
        assert_eq!(role_of("plain line"), None);
        assert_eq!(strip_role("User: build it"), "build it");
        assert_eq!(strip_role("no role here"), "no role here");
    }

    #[test]
    fn keywords_match_words_not_substrings() {
        assert!(contains_word("this is an error message", "error"));
        assert!(!contains_word("terror at midnight", "error")); // substring, not word
        assert!(contains_keyword("npm install failed", "failed"));
        assert!(contains_keyword("fix this now", "fix "));
        assert!(!contains_keyword("prefixfix should not match", "fix "));
        assert!(contains_word("expected output", "expected"));
        assert!(!contains_word("unexpected result", "actual"));
    }

    #[test]
    fn urls_and_paths() {
        let url_re = url_regex();
        let found: Vec<&str> = url_re
            .find_iter("see https://doc.rust-lang.org/book and http://example.com/x.")
            .map(|m| m.as_str())
            .collect();
        assert_eq!(found.len(), 2);

        let path_re = path_regex();
        assert!(path_re.is_match("edit src/main.py please"));
        assert!(path_re.is_match("check C:\\proj\\app\\src\\lib\\util.ts"));
        assert!(!path_re.is_match("just a word"));
    }

    #[test]
    fn numbered_lists() {
        assert_eq!(numbered_list_prefix_len("1. do this"), Some(2));
        assert_eq!(numbered_list_prefix_len("12) other"), Some(3));
        assert_eq!(numbered_list_prefix_len("not numbered"), None);
    }
}
