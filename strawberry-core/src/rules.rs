//! Deterministic keyword, pattern and limit definitions for the STRAWBERRY
//! compression engine. No AI, no network, no external services.

// ---------------------------------------------------------------------------
// Command / error / decision vocabularies
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Negative knowledge: rejected / abandoned approaches
// ---------------------------------------------------------------------------

/// Phrases that mark an approach as tried-and-rejected. This is the highest
/// value signal in a handoff packet: without it the receiving AI happily
/// re-suggests everything that already failed.
///
/// Hinglish variants are included on purpose — real chats are code-switched.
pub const REJECTED_KEYWORDS: &[&str] = &[
    // English
    "didn't work",
    "didnt work",
    "did not work",
    "doesn't work",
    "doesnt work",
    "does not work",
    "not working",
    "won't work",
    "wont work",
    "will not work",
    "instead of",
    "rather than",
    "reverted",
    "revert back",
    "rolled back",
    "roll back",
    "rejected",
    "ruled out",
    "abandoned",
    "scrapped",
    "gave up on",
    "giving up on",
    "no longer using",
    "not using",
    "stopped using",
    "switched from",
    "moved away from",
    "moving away from",
    "turned out to be wrong",
    "was wrong",
    "not viable",
    "not an option",
    "dead end",
    "failed approach",
    "broke everything",
    "made it worse",
    "in favor of",
    "deprecated",
    "tried but",
    "tried it but",
    // Hinglish
    "nahi chala",
    "nhi chala",
    "nahi chalta",
    "nahi chali",
    "kaam nahi kiya",
    "kaam nhi kiya",
    "kaam nahi karta",
    "hata diya",
    "hata do",
    "hata dena",
    "nikaal diya",
    "chhod diya",
    "chod diya",
    "band kar diya",
    "band kar do",
    "galat tha",
    "galat nikla",
    "ulta pad gaya",
    "faayda nahi",
    "fayda nahi",
    "kaam ka nahi",
    "bekar nikla",
    "nahi hua",
    "nhi hua",
    "wapas kar diya",
    "wapas le liya",
    "ki jagah",
];

/// Connectors that introduce the *reason* a thing was rejected. The clause
/// after the connector is the part that actually prevents a repeat mistake.
pub const REASON_CONNECTORS: &[&str] = &[
    " because ",
    " since ",
    " as it ",
    " due to ",
    " reason: ",
    " kyunki ",
    " kyuki ",
    " kyonki ",
    " isliye ",
    " iski wajah se ",
    " wajah se ",
];

// ---------------------------------------------------------------------------
// Constraints
// ---------------------------------------------------------------------------

/// Hard rules the receiving AI must not violate. Deliberately strict: bare
/// words like "only" or "zero" are too common in prose to be useful markers.
pub const CONSTRAINT_KEYWORDS: &[&str] = &[
    "must ",
    "must not",
    "never ",
    "always ",
    "no network",
    "no internet",
    "offline",
    "local-only",
    "local only",
    "local first",
    "local-first",
    "zero llm",
    "no llm",
    "no ai",
    "without ",
    "only use",
    "only allow",
    "don't use",
    "dont use",
    "do not use",
    "requires ",
    "mandatory",
    "not allowed",
    "sirf ",
    "kabhi nahi",
    "zaroori",
    "zaruri",
    "mat ",
    "nahi karna",
    "nhi karna",
];

/// Constraint lines longer than this are prose, not rules.
pub const CONSTRAINT_MAX_CHARS: usize = 200;

// ---------------------------------------------------------------------------
// Noise
// ---------------------------------------------------------------------------

/// Lines that carry zero handoff value and are dropped outright: tool-approval
/// banners, harness chatter and meta-instructions the model wrote to itself.
///
/// Matched case-insensitively anywhere in the line.
pub const NOISE_MARKERS: &[&str] = &[
    "auto-approved",
    "auto approved",
    "matched `bash` rule",
    "matched bash rule",
    "keep the response concise",
    "keep response concise",
    "deserves a short summary",
    "the user speaks",
    "i'll reply in",
    "ill reply in",
    "i will reply in",
    "run with `rust_backtrace",
    "note: run with",
];

/// Phrases that mark a line as the assistant narrating its own next move.
///
/// These lines are *demoted*, not deleted: a decision, error or identifier
/// inside one still counts, but the sentence itself never becomes an action
/// item, constraint or key point. This is the single largest source of waste
/// in a real agent transcript.
pub const NARRATION_MARKERS: &[&str] = &[
    "let me ",
    "let's ",
    "lets check",
    "let me know",
    "i'll start",
    "i'll now",
    "i will now",
    "now i'll",
    "now let me",
    "first let me",
    "first, let me",
    "i'm going to check",
    "im going to check",
    "let us ",
];

/// Line-initial acknowledgements and progress reports. Same demotion rule.
pub const NARRATION_PREFIXES: &[&str] = &[
    "okay",
    "ok ",
    "ok.",
    "ok,",
    "good.",
    "good,",
    "good ",
    "sure!",
    "sure,",
    "sure.",
    "exactly.",
    "exactly,",
    "no problem",
    "great,",
    "great.",
    "great!",
    "alright",
    "perfect",
    "nice,",
    "nice.",
    "i see",
    "hmm,",
    "hmm.",
    "that worked",
    "verified.",
    "verified,",
    "build succeeded",
    "theek hai",
    "samajh gaya",
    "main samajh",
];

// ---------------------------------------------------------------------------
// Structural vocabularies
// ---------------------------------------------------------------------------

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

/// All-caps tokens that look like env vars but are ordinary prose markers.
pub const ENV_VAR_DENYLIST: &[&str] = &[
    "TODO_LIST", "NOTE_TO_SELF", "READ_ME", "AS_IS", "AKA_", "CREATE_TABLE",
    "INSERT_INTO", "PRIMARY_KEY", "FOREIGN_KEY", "NOT_NULL", "IF_NOT",
];

/// Suffixes that strongly imply "this all-caps token is configuration".
pub const ENV_VAR_SUFFIXES: &[&str] = &[
    "_KEY", "_TOKEN", "_SECRET", "_URL", "_URI", "_HOST", "_PORT", "_PATH",
    "_DIR", "_HOME", "_ENV", "_MODE", "_ID", "_NAME", "_PASSWORD", "_PASS",
    "_USER", "_DB", "_DATABASE", "_REGION", "_BUCKET", "_ENDPOINT", "_MODEL",
    "_TIMEOUT", "_LEVEL", "_DISPLAY", "_CONFIG", "_FILE", "_ROOT",
];

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

pub const MAX_KEY_POINTS: usize = 100;
pub const MAX_CODE_BLOCKS: usize = 50;
pub const MAX_COMMANDS: usize = 50;
pub const MAX_ERRORS: usize = 50;
pub const MAX_URLS: usize = 50;
pub const MAX_DECISIONS: usize = 50;
pub const MAX_ACTION_ITEMS: usize = 50;
pub const MAX_QUESTIONS: usize = 50;
pub const MAX_REJECTED: usize = 50;
pub const MAX_CONSTRAINTS: usize = 30;
pub const MAX_IDENTIFIERS: usize = 120;

pub const MORE_NOTE: &str = "More items available in original chat.";

pub const FIRST_IDEA_MAX_CHARS: usize = 240;

// ---------------------------------------------------------------------------
// Regexes
// ---------------------------------------------------------------------------

use regex::Regex;
use std::sync::OnceLock;

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
            r#"(?:[A-Za-z0-9_.\-~]+[/\\])+[A-Za-z0-9_.\-]+\.(?:{exts})\b"#
        );
        Regex::new(&pattern).expect("valid path regex")
    })
}

/// SCREAMING_SNAKE_CASE tokens with at least one underscore.
pub fn env_var_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+\b").expect("valid env regex")
    })
}

/// Ports: `localhost:1420`, `127.0.0.1:3080`, `port 11434`, `PORT=8080`.
pub fn port_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\bports?\b)\s*[:=]?\s*(\d{2,5})\b")
            .expect("valid port regex")
    })
}

/// SQL object names from statements that define or address a table directly.
pub fn table_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:create\s+(?:virtual\s+)?table(?:\s+if\s+not\s+exists)?|insert\s+into|alter\s+table|drop\s+table(?:\s+if\s+exists)?|create\s+index(?:\s+if\s+not\s+exists)?)\s+([A-Za-z_][A-Za-z0-9_]*)",
        )
        .expect("valid table regex")
    })
}

/// Function/method definitions across common languages.
pub fn fn_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(?:fn|def|function|func)\s+([A-Za-z_][A-Za-z0-9_]*)")
            .expect("valid fn regex")
    })
}

/// Pinned dependency versions: `rusqlite@0.31`, `tauri 2.0.3`, `version 1.77`.
pub fn version_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b([a-z][a-z0-9_.\-]*)\s*(?:@|\bversion\s+|\bv)\s*[\^~]?(\d+\.\d+(?:\.\d+)?)\b",
        )
        .expect("valid version regex")
    })
}

// ---------------------------------------------------------------------------
// Line helpers
// ---------------------------------------------------------------------------

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
            && line
                .trim_start()
                .get(..prefix.len())
                .map(|s| s.eq_ignore_ascii_case(&prefix))
                .unwrap_or(false)
        {
            return Some(role);
        }
    }
    None
}

/// Strip a leading role prefix ("User:") from a line, returning the remainder.
pub fn strip_role(line: &str) -> &str {
    if role_of(line).is_some() {
        let idx = line.find(':').map(|i| i + 1).unwrap_or(0);
        return line[idx..].trim_start();
    }
    line
}

/// Case-insensitive whole-word-ish containment check.
///
/// A needle ending in a space is prefix-style ("fix ", "mat "): the core word
/// must appear as a whole word, so Hinglish `"mat "` does not fire on English
/// "matched".
pub fn contains_keyword(haystack_lower: &str, needle: &str) -> bool {
    if needle.ends_with(' ') {
        return contains_word(haystack_lower, &needle[..needle.len() - 1]);
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

/// Conservative token estimate: ~3.5 characters per token, rounded up.
///
/// Deliberately pessimistic. Code, identifiers and Hinglish tokenize worse
/// than English prose, so a budget built on this figure does not overflow.
pub fn est_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    if chars == 0 {
        return 0;
    }
    (chars * 2 + 6) / 7
}

/// True when the line is pure harness/meta noise and should be dropped whole.
pub fn is_noise_line(lower_trimmed: &str) -> bool {
    NOISE_MARKERS.iter().any(|m| lower_trimmed.contains(m))
}

/// True when the line is the assistant narrating its own next move.
///
/// Callers demote rather than delete: identifiers and errors inside a narration
/// line are still real, but the sentence must not occupy a handoff slot.
pub fn is_narration_line(lower_trimmed: &str) -> bool {
    if NARRATION_PREFIXES
        .iter()
        .any(|p| lower_trimmed.starts_with(p))
    {
        return true;
    }
    NARRATION_MARKERS.iter().any(|m| lower_trimmed.contains(m))
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
        assert!(!contains_word("terror at midnight", "error"));
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

    #[test]
    fn env_vars_matched() {
        let re = env_var_regex();
        let hits: Vec<&str> = re
            .find_iter("set HERMES_CUSTOM_API_KEY and XDG_DATA_HOME now")
            .map(|m| m.as_str())
            .collect();
        assert_eq!(hits, vec!["HERMES_CUSTOM_API_KEY", "XDG_DATA_HOME"]);
        assert!(!re.is_match("NOTHING here lowercase"));
    }

    #[test]
    fn ports_matched() {
        let re = port_regex();
        let got: Vec<String> = re
            .captures_iter("bind localhost:1420 then 127.0.0.1:3080 and port 11434")
            .map(|c| c[1].to_string())
            .collect();
        assert_eq!(got, vec!["1420", "3080", "11434"]);
    }

    #[test]
    fn tables_and_fns_and_versions() {
        let t: Vec<String> = table_regex()
            .captures_iter("CREATE TABLE IF NOT EXISTS chat_artifacts (x); INSERT INTO chats VALUES")
            .map(|c| c[1].to_string())
            .collect();
        assert_eq!(t, vec!["chat_artifacts", "chats"]);

        let f: Vec<String> = fn_regex()
            .captures_iter("fn build_handoff() and def compress(x) plus function run()")
            .map(|c| c[1].to_string())
            .collect();
        assert_eq!(f, vec!["build_handoff", "compress", "run"]);

        let v: Vec<String> = version_regex()
            .captures_iter("rusqlite@0.31 and rust version 1.77")
            .map(|c| format!("{}={}", &c[1], &c[2]))
            .collect();
        assert!(v.contains(&"rusqlite=0.31".to_string()));
        assert!(v.contains(&"rust=1.77".to_string()));
    }

    #[test]
    fn token_estimate_is_pessimistic() {
        // 7 chars -> 2 tokens (ceil(7/3.5))
        assert_eq!(est_tokens("abcdefg"), 2);
        assert_eq!(est_tokens(""), 0);
        // never underestimates a realistic English word count
        let text = "the quick brown fox jumps over the lazy dog";
        assert!(est_tokens(text) >= 9);
    }
}
