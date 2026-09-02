//! 🔒 Deterministic privacy screening for captured text.
//!
//! One reusable boundary shared by every capture path (the clipboard daemon
//! today; screen/OCR/file/chat capabilities later). Before ANY persistence —
//! SQLite rows, FTS indexes, raw files on disk, future events or memories —
//! text must pass through [`PrivacyPolicy::evaluate`].
//!
//! Design rules (foundation-hardening spec):
//! - Pure and deterministic: no clocks, no network, no randomness, no AI.
//! - Decisions carry a reason and a match count — **never the matched value**.
//!   A decision is safe to log, print, or put in a report.
//! - High-confidence secret material (private-key headers, known vendor key
//!   formats, JWTs, bearer headers, credentialed URLs) blocks short captures
//!   outright and is redacted out of longer ones.
//! - The softer password-like heuristic (a label such as `password`/`api_key`
//!   followed by a value) blocks short captures and redacts just the labeled
//!   span in longer ones.
//! - Bare OTP/2FA-style short codes are deliberately NOT flagged: they are
//!   indistinguishable from ordinary numbers in code and prose, so detecting
//!   them "reliably" is not possible and false positives would be worse than
//!   the gap. Labeled assignments (`otp=123456`) are still caught by the
//!   password-like heuristic.
//!
//! This is a screening boundary, not cryptography and not a secrecy guarantee:
//! it keeps obvious credential material out of persistent storage.

use std::fmt;

use regex::Regex;

/// What the policy decided should happen with a capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyAction {
    /// No secret-like material detected; persist the text as-is.
    Allow,
    /// Secret-like spans exist inside otherwise useful text; persist the
    /// output of [`PrivacyPolicy::redact`] instead of the original.
    Redact,
    /// The capture is (mostly) secret material; persist nothing at all.
    Block,
}

/// Why a decision was reached. Carries no sensitive data by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyReason {
    /// PEM private-key header (`-----BEGIN … PRIVATE KEY-----`).
    PrivateKey,
    /// Known vendor key/token shape: OpenAI `sk-…`, GitHub `gh?_…`,
    /// AWS `AKIA…`, Google `AIza…`, Slack `xox?_…`.
    ApiKey,
    /// `Authorization: Bearer …` style header value.
    BearerToken,
    /// Structured JWT (`eyJ….eyJ….sig`).
    Jwt,
    /// URL with embedded credentials (`scheme://user:pass@host`).
    CredentialUrl,
    /// Heuristic label+value pair, e.g. `password=hunter2boi`.
    PasswordLike,
    /// A user-configured extra pattern matched.
    UserPattern,
    /// Capture exceeds the configured maximum length.
    TooLarge,
}

impl PrivacyReason {
    /// Short stable label safe for logs and reports.
    pub fn label(self) -> &'static str {
        match self {
            PrivacyReason::PrivateKey => "private_key",
            PrivacyReason::ApiKey => "api_key",
            PrivacyReason::BearerToken => "bearer_token",
            PrivacyReason::Jwt => "jwt",
            PrivacyReason::CredentialUrl => "credential_url",
            PrivacyReason::PasswordLike => "password_like",
            PrivacyReason::UserPattern => "user_pattern",
            PrivacyReason::TooLarge => "too_large",
        }
    }
}

impl fmt::Display for PrivacyReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// The verdict for one candidate capture.
///
/// `reason` and `matches` explain WHY without ever holding the secret text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyDecision {
    /// What to do with the capture.
    pub action: PrivacyAction,
    /// `Some` whenever the action is not `Allow`.
    pub reason: Option<PrivacyReason>,
    /// How many secret-like spans were found (0 when allowed).
    /// Never their content.
    pub matches: usize,
}

impl PrivacyDecision {
    /// True when the original text may be persisted unchanged.
    pub fn is_allowed(&self) -> bool {
        self.action == PrivacyAction::Allow
    }

    /// True when nothing at all should be persisted.
    pub fn is_blocked(&self) -> bool {
        self.action == PrivacyAction::Block
    }

    /// True when [`PrivacyPolicy::redact`] must be applied before persisting.
    pub fn needs_redaction(&self) -> bool {
        self.action == PrivacyAction::Redact
    }

    /// Human-safe one-line summary (contains no captured text).
    pub fn summary(&self) -> String {
        match self.reason {
            Some(r) => format!("{}: {} ({} span(s))", self.action.label(), r.label(), self.matches),
            None => format!("{} (0 spans)", self.action.label()),
        }
    }
}

impl PrivacyAction {
    /// Short stable label safe for logs and reports.
    pub fn label(self) -> &'static str {
        match self {
            PrivacyAction::Allow => "allow",
            PrivacyAction::Redact => "redact",
            PrivacyAction::Block => "block",
        }
    }
}

/// A capture shorter than this is treated as "mostly the secret itself"
/// for the password-like heuristic: block it instead of redacting.
const SHORT_PASSWORD_CAPTURE: usize = 120;
/// A capture longer than this is treated as a real note/chat that merely
/// contains a secret: redact the spans instead of dropping everything.
const NOTE_CAPTURE: usize = 400;
/// Default maximum bytes for any single capture (matches the daemon gate).
pub const DEFAULT_MAX_CAPTURE_LEN: usize = 200_000;

/// The reusable screening boundary.
///
/// Clone-cheap to construct once and reuse for the process lifetime.
pub struct PrivacyPolicy {
    private_key: Regex,
    // Redaction-only companion: swallows the whole PEM block (body included).
    private_key_block: Regex,
    api_key: Regex,
    bearer: Regex,
    jwt: Regex,
    cred_url: Regex,
    password_like: Regex,
    /// User-supplied extra patterns (label used only for debugging).
    user_patterns: Vec<(String, Regex)>,
    max_len: usize,
}

impl Default for PrivacyPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivacyPolicy {
    /// Built-in detectors with a conservative default length limit.
    pub fn new() -> Self {
        Self {
            private_key: Regex::new(r"-----BEGIN[ A-Z0-9]*PRIVATE KEY-----").unwrap(),
            private_key_block: Regex::new(
                r"(?s)-----BEGIN[ A-Z0-9]*PRIVATE KEY-----.*?(-----END[A-Z0-9 ]*PRIVATE KEY-----|\z)",
            )
            .unwrap(),
            api_key: Regex::new(
                r"sk-(?:proj-|ant-)?[A-Za-z0-9_-]{20,}|gh[a-z]_[A-Za-z0-9]{30,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35}|xox[a-z]-[A-Za-z0-9-]{10,}",
            )
            .unwrap(),
            bearer: Regex::new(r"(?i)bearer\s+[A-Za-z0-9._~+/=-]{20,}").unwrap(),
            jwt: Regex::new(r"eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{4,}").unwrap(),
            cred_url: Regex::new(r"[A-Za-z][A-Za-z0-9+.-]*://[^\s/:@]+:[^\s/@]+@").unwrap(),
            password_like: Regex::new(
                r#"(?i)\b(api[_-]?key|apikey|secret|access[_-]?token|refresh[_-]?token|token|password|passwd|pwd|pass|otp|2fa)\b["']?\s*(?:is\s*)?[:=]>?\s*(?:"[^"]{6,}"|'[^']{6,}'|[A-Za-z0-9_.\-+/=]*[0-9][A-Za-z0-9_.\-+/=]{3,})"#,
            )
            .unwrap(),
            user_patterns: Vec::new(),
            max_len: DEFAULT_MAX_CAPTURE_LEN,
        }
    }

    /// Override the maximum capture length (bytes).
    pub fn with_max_len(mut self, max_len: usize) -> Self {
        self.max_len = max_len;
        self
    }

    /// Add a user-configured extra regex pattern. Invalid patterns return an
    /// error naming the pattern's *position* — never echoed back with capture
    /// data, but the pattern text itself is user-supplied config, so it is
    /// safe to include in the error.
    pub fn add_extra_pattern(&mut self, pattern: &str) -> Result<(), String> {
        let re = Regex::new(pattern)
            .map_err(|e| format!("extra pattern {pattern:?} is not a valid regex: {e}"))?;
        self.user_patterns.push((pattern.to_string(), re));
        Ok(())
    }

    /// Screen one candidate capture.
    ///
    /// Deterministic policy:
    /// - over `max_len` → Block(`TooLarge`);
    /// - any high-confidence secret (private key, vendor key format, bearer,
    ///   JWT, credentialed URL, user pattern) → Block when the capture is
    ///   short (≤ [`NOTE_CAPTURE`] chars, i.e. it is mostly the secret),
    ///   otherwise Redact;
    /// - password-like heuristic → Block when short
    ///   (≤ [`SHORT_PASSWORD_CAPTURE`] chars), otherwise Redact;
    /// - otherwise Allow.
    pub fn evaluate(&self, text: &str) -> PrivacyDecision {
        if text.len() > self.max_len {
            return PrivacyDecision {
                action: PrivacyAction::Block,
                reason: Some(PrivacyReason::TooLarge),
                matches: 0,
            };
        }
        let chars = text.chars().count();

        let high_confidence: [(PrivacyReason, &Regex); 5] = [
            (PrivacyReason::PrivateKey, &self.private_key),
            (PrivacyReason::ApiKey, &self.api_key),
            (PrivacyReason::BearerToken, &self.bearer),
            (PrivacyReason::Jwt, &self.jwt),
            (PrivacyReason::CredentialUrl, &self.cred_url),
        ];

        let mut total = 0usize;
        let mut first: Option<PrivacyReason> = None;
        for (reason, re) in high_confidence {
            let n = re.find_iter(text).count();
            if n > 0 {
                total += n;
                if first.is_none() {
                    first = Some(reason);
                }
            }
        }
        for (_label, re) in &self.user_patterns {
            let n = re.find_iter(text).count();
            if n > 0 {
                total += n;
                if first.is_none() {
                    first = Some(PrivacyReason::UserPattern);
                }
            }
        }
        if total > 0 {
            let action = if chars <= NOTE_CAPTURE {
                PrivacyAction::Block
            } else {
                PrivacyAction::Redact
            };
            return PrivacyDecision { action, reason: first, matches: total };
        }

        let pw = self.password_like.find_iter(text).count();
        if pw > 0 {
            let action = if chars <= SHORT_PASSWORD_CAPTURE {
                PrivacyAction::Block
            } else {
                PrivacyAction::Redact
            };
            return PrivacyDecision { action, reason: Some(PrivacyReason::PasswordLike), matches: pw };
        }

        PrivacyDecision { action: PrivacyAction::Allow, reason: None, matches: 0 }
    }

    /// Replace every secret-like span with `[REDACTED]`.
    ///
    /// For private keys the whole PEM block (base64 body included) is
    /// removed, not just the header line.
    pub fn redact(&self, text: &str) -> String {
        let mut out = self.private_key_block.replace_all(text, "[REDACTED]").into_owned();
        for re in [&self.api_key, &self.bearer, &self.jwt, &self.cred_url, &self.password_like] {
            out = re.replace_all(&out, "[REDACTED]").into_owned();
        }
        for (_label, re) in &self.user_patterns {
            out = re.replace_all(&out, "[REDACTED]").into_owned();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── High-confidence detectors ──────────────────────────────────────────

    #[test]
    fn blocks_synthetic_openai_style_key() {
        let key = "sk-abc123DEF456ghi789JKL012";
        let d = PrivacyPolicy::new().evaluate(key);
        assert_eq!(d.action, PrivacyAction::Block);
        assert_eq!(d.reason, Some(PrivacyReason::ApiKey));
        assert_eq!(d.matches, 1);
    }

    #[test]
    fn blocks_synthetic_github_token() {
        let d = PrivacyPolicy::new().evaluate(&format!("ghp_{}", "a1B2c3D4e5".repeat(3)));
        assert_eq!(d.reason, Some(PrivacyReason::ApiKey));
        assert_eq!(d.action, PrivacyAction::Block);
    }

    #[test]
    fn blocks_synthetic_aws_key() {
        let d = PrivacyPolicy::new().evaluate("aws_access_key_id = AKIAIOSFODNN7EXAMPLE");
        assert!(matches!(d.reason, Some(PrivacyReason::ApiKey | PrivacyReason::PasswordLike)));
        assert_eq!(d.action, PrivacyAction::Block);
    }

    #[test]
    fn blocks_bearer_token_header() {
        let d = PrivacyPolicy::new().evaluate("Authorization: Bearer abcdef1234567890XYZxyzab");
        assert_eq!(d.reason, Some(PrivacyReason::BearerToken));
        assert_eq!(d.action, PrivacyAction::Block);
    }

    #[test]
    fn blocks_jwt() {
        // Synthetic JWT-shaped string (not a real token).
        let jwt = format!("eyJ{}{}.eyJ{}{}.Sf4Ke0sig1", "hbGciOi".repeat(2), "x9", "zdWIiOi".repeat(2), "y7");
        let d = PrivacyPolicy::new().evaluate(&jwt);
        assert_eq!(d.reason, Some(PrivacyReason::Jwt));
        assert_eq!(d.action, PrivacyAction::Block);
    }

    #[test]
    fn blocks_private_key_material() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0123456789\n-----END RSA PRIVATE KEY-----";
        let d = PrivacyPolicy::new().evaluate(pem);
        assert_eq!(d.reason, Some(PrivacyReason::PrivateKey));
        assert_eq!(d.action, PrivacyAction::Block);
    }

    #[test]
    fn blocks_credentialed_url() {
        let d = PrivacyPolicy::new().evaluate("postgres://admin:s3cretpass@localhost:5432/mydb");
        assert_eq!(d.reason, Some(PrivacyReason::CredentialUrl));
        assert_eq!(d.action, PrivacyAction::Block);
    }

    #[test]
    fn long_text_with_secret_is_redacted_not_dropped() {
        // > NOTE_CAPTURE chars: a real note that merely contains a key.
        let filler = "This is an ordinary engineering note about the release process and deploy steps. ".repeat(10);
        let text = format!("{filler}\nTemporary staging key: sk-abc123DEF456ghi789JKL012\nMore prose follows here.");
        let p = PrivacyPolicy::new();
        let d = p.evaluate(&text);
        assert_eq!(d.action, PrivacyAction::Redact);
        assert_eq!(d.reason, Some(PrivacyReason::ApiKey));
        let red = p.redact(&text);
        assert!(!red.contains("sk-abc123DEF456ghi789JKL012"), "redaction must remove the key");
        assert!(red.contains("release process"), "redaction must keep the surrounding text");
        assert!(red.contains("[REDACTED]"));
    }

    #[test]
    fn redaction_removes_whole_private_key_block() {
        let text = format!(
            "Deploy note prose. {} body prose that continues after the key. {}",
            "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE KEY-----",
            "End of note."
        );
        let p = PrivacyPolicy::new();
        let red = p.redact(&text);
        assert!(!red.contains("b3BlbnNzaC1rZXktdjE"), "PEM body must be redacted, not just the header");
        assert!(red.contains("Deploy note prose."));
    }

    // ── Password-like heuristic ────────────────────────────────────────────

    #[test]
    fn short_password_like_assignment_is_blocked() {
        let d = PrivacyPolicy::new().evaluate("The password is: hunter2boi");
        assert_eq!(d.action, PrivacyAction::Block);
        assert_eq!(d.reason, Some(PrivacyReason::PasswordLike));
    }

    #[test]
    fn short_labeled_api_key_assignment_is_blocked() {
        let d = PrivacyPolicy::new().evaluate("api_key: \"Zx9wYhQ2mLvB7kDp\"");
        assert_eq!(d.action, PrivacyAction::Block);
        assert!(matches!(d.reason, Some(PrivacyReason::ApiKey | PrivacyReason::PasswordLike)));
    }

    #[test]
    fn long_note_with_password_is_redacted() {
        let note = "Meeting notes: we discussed the migration plan for the user table, index strategy, and the deployment window. ".repeat(3)
            + "Login password=hunter2boi for the test box. "
            + "Action items follow: review indexes, run migration on staging, then production.";
        let p = PrivacyPolicy::new();
        let d = p.evaluate(&note);
        assert_eq!(d.action, PrivacyAction::Redact);
        assert_eq!(d.reason, Some(PrivacyReason::PasswordLike));
        let red = p.redact(&note);
        assert!(!red.contains("hunter2boi"));
        assert!(red.contains("migration plan"));
    }

    #[test]
    fn otp_labeled_assignment_is_caught() {
        let d = PrivacyPolicy::new().evaluate("otp=482913");
        assert_eq!(d.action, PrivacyAction::Block);
        assert_eq!(d.reason, Some(PrivacyReason::PasswordLike));
    }

    // ── Ordinary content must flow through (no false positives) ─────────────

    #[test]
    fn allows_ordinary_code() {
        let code = "fn main() {\n    let total = compute(42);\n    println!(\"hi {}\");\n}\n";
        let d = PrivacyPolicy::new().evaluate(code);
        assert!(d.is_allowed());
        assert_eq!(d.reason, None);
    }

    #[test]
    fn allows_env_indirection() {
        // Deliberate false-negative-safe pass-through: no digits in the value.
        let d = PrivacyPolicy::new().evaluate("const apiKey = process.env.API_KEY;");
        assert!(d.is_allowed());
    }

    #[test]
    fn allows_ordinary_url() {
        let d = PrivacyPolicy::new().evaluate("https://example.com/docs?q=hello+world#section");
        assert!(d.is_allowed());
    }

    #[test]
    fn allows_url_with_bare_userinfo() {
        // `user@host` without a password component is not a credential leak.
        let d = PrivacyPolicy::new().evaluate("https://octocat@github.com/org/repo/pulls");
        assert!(d.is_allowed());
    }

    #[test]
    fn allows_ordinary_prose_and_unicode() {
        let prose = "パスワードは秘密です — обычный текст, nothing secret here. Diwali ki taiyari chal rahi hai!";
        let d = PrivacyPolicy::new().evaluate(prose);
        assert!(d.is_allowed());
    }

    #[test]
    fn allows_bearer_placeholder() {
        let d = PrivacyPolicy::new().evaluate("Authorization: Bearer ${token}");
        assert!(d.is_allowed());
    }

    #[test]
    fn allows_port_only_connection_string() {
        // `localhost:6379` has no userinfo password — must not match.
        let d = PrivacyPolicy::new().evaluate("redis://localhost:6379/0");
        assert!(d.is_allowed());
    }

    // ── User-configured extra patterns ────────────────────────────────────

    #[test]
    fn user_pattern_blocks_and_redacts() {
        let mut p = PrivacyPolicy::new();
        p.add_extra_pattern(r"COMPANY-CONFIDENTIAL-\d+").unwrap();

        let d = p.evaluate("COMPANY-CONFIDENTIAL-42");
        assert_eq!(d.action, PrivacyAction::Block);
        assert_eq!(d.reason, Some(PrivacyReason::UserPattern));

        let note = format!("{} {}", "Project retro notes and follow-ups, lots of ordinary prose here. ".repeat(12), "Tagged COMPANY-CONFIDENTIAL-99 internally.");
        let d2 = p.evaluate(&note);
        assert_eq!(d2.action, PrivacyAction::Redact);
        assert!(!p.redact(&note).contains("COMPANY-CONFIDENTIAL-99"));
    }

    #[test]
    fn invalid_extra_pattern_is_rejected_without_panicking() {
        let mut p = PrivacyPolicy::new();
        assert!(p.add_extra_pattern("(unclosed").is_err());
        // Policy stays usable with the built-in detectors only.
        assert!(p.evaluate("ordinary text").is_allowed());
    }

    // ── Length limit ───────────────────────────────────────────────────────

    #[test]
    fn oversized_capture_is_blocked() {
        let p = PrivacyPolicy::new().with_max_len(100);
        let d = p.evaluate(&"x".repeat(101));
        assert_eq!(d.reason, Some(PrivacyReason::TooLarge));
        assert!(d.is_blocked());
    }

    // ── Decisions never leak the secret ───────────────────────────────────

    #[test]
    fn decision_debug_and_summary_never_contain_the_secret() {
        let key = "sk-superSecretValue987654321";
        let d = PrivacyPolicy::new().evaluate(key);
        let dbg = format!("{d:?}");
        let summary = d.summary();
        assert!(!dbg.contains("superSecretValue"));
        assert!(!summary.contains("superSecretValue"));
        assert!(summary.contains("block"));
        assert!(summary.contains("api_key"));
    }
}
