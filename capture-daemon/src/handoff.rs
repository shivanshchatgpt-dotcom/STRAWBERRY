//! 🍓 Feature 4 — clipboard handoff trigger.
//!
//! A global hotkey is not dependable here: Wayland deliberately has no
//! portable hotkey grab, and the X11 path would need a separate event loop.
//! Worse, a repeated copy of identical text produces **no** clipboard change,
//! so "copy twice" is undetectable by any polling daemon.
//!
//! So the trigger is a short magic token. The flow costs two normal copies:
//!
//!   1. Select the whole chat and copy it  → daemon remembers it.
//!   2. Copy the token `sb!`               → daemon compresses step 1,
//!                                            replaces the clipboard with the
//!                                            handoff packet and notifies.
//!   3. Paste into the other AI.
//!
//! Every step is an ordinary Ctrl+C, needs no permissions, and works
//! identically on Wayland, X11, macOS and Windows.

/// Tokens that mean "compress what I copied before this".
///
/// Deliberately unlikely to be copied by accident, and matched only when the
/// whole clipboard is the token.
pub const TRIGGERS: &[&str] = &["sb!", "!sb", "/sb", "🍓", "sb!!", "strawberry!"];

/// Longest text still eligible to be a trigger. Guards against a real chat
/// that merely happens to start with a token.
const MAX_TRIGGER_CHARS: usize = 12;

/// True when the clipboard holds nothing but a handoff trigger.
pub fn is_trigger(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.chars().count() > MAX_TRIGGER_CHARS {
        return false;
    }
    let lower = t.to_lowercase();
    TRIGGERS.iter().any(|k| lower == *k)
}

/// Shortest text worth remembering as a compression candidate. Below this a
/// "chat" is a stray word and compressing it is meaningless.
const MIN_SOURCE_CHARS: usize = 40;

/// True when text is substantial enough to be worth remembering as the source
/// for the next trigger.
pub fn is_compressible(text: &str) -> bool {
    text.trim().chars().count() >= MIN_SOURCE_CHARS
}

/// Title for a packet built from clipboard text.
///
/// Skips bare role markers ("User:", "Assistant:") and strips an inline role
/// prefix, so a pasted transcript is titled by its first real sentence rather
/// than by the word "User:".
pub fn title_for(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(strawberry_core::rules::strip_role)
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("Clipboard chat");
    let clipped: String = line.chars().take(60).collect();
    if clipped.trim().is_empty() {
        "Clipboard chat".to_string()
    } else {
        clipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_recognized_case_insensitively() {
        assert!(is_trigger("sb!"));
        assert!(is_trigger("  SB!  "));
        assert!(is_trigger("/sb"));
        assert!(is_trigger("🍓"));
        assert!(is_trigger("strawberry!"));
    }

    #[test]
    fn ordinary_text_is_not_a_trigger() {
        assert!(!is_trigger(""));
        assert!(!is_trigger("sb"));
        assert!(!is_trigger("sb! and then a whole lot more text follows here"));
        assert!(!is_trigger("please summarize"));
    }

    #[test]
    fn compressible_needs_real_content() {
        assert!(!is_compressible("hi"));
        assert!(!is_compressible("short note"));
        assert!(is_compressible(
            "User: build me a compressor\nAssistant: we decided to use rules"
        ));
    }

    #[test]
    fn title_uses_first_meaningful_line() {
        assert_eq!(title_for("\n\n  Fix the login bug\nmore"), "Fix the login bug");
        assert_eq!(title_for("   "), "Clipboard chat");
        assert_eq!(title_for(&"x".repeat(100)).chars().count(), 60);
    }

    #[test]
    fn title_skips_bare_role_markers() {
        assert_eq!(
            title_for("User:\nbuild me a compressor\nAssistant:\nsure"),
            "build me a compressor"
        );
        assert_eq!(
            title_for("User: fix the parser please"),
            "fix the parser please"
        );
    }
}
