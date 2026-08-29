//! 🎯 Alpha Hunter — detects free AI model "alphas" from legitimate public
//! sources, verifies them with a real API call, and emits copy-paste config.
//!
//! Design rules:
//! - Deterministic keyword detection only — no LLM, no scraping behind logins.
//! - Sources: HackerNews (Algolia), Reddit JSON API, GitHub Search API,
//!   HuggingFace public API, OpenRouter public API, Product Hunt RSS.
//! - Network is opt-in: callers must check `app_meta.alpha_hunter_enabled`.
//! - Never panics; every fetch error degrades to an empty result.

use serde::{Deserialize, Serialize};

/// One raw item pulled from a source, before detection.
#[derive(Debug, Clone)]
pub struct RawItem {
    pub source: &'static str,
    pub title: String,
    pub url: Option<String>,
    pub body: Option<String>,
}

/// A detected candidate, ready for DB insertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub source: String,
    pub title: String,
    pub url: Option<String>,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub base_url: Option<String>,
    pub score: i64,
}

// ================================================================ sources ==

/// Fetch recent HackerNews stories matching alpha-ish queries.
pub fn fetch_hn() -> Vec<RawItem> {
    let mut out = Vec::new();
    let queries = [
        "free%20AI%20API",
        "free%20tier%20LLM",
        "free%20model",
        "free%20OpenAI%20alternative",
        "free%20API%20key",
        "free%20inference",
    ];
    for q in queries {
        let url = format!(
            "https://hn.algolia.com/api/v1/search_by_date?query={q}&tags=story&hitsPerPage=15&numericFilters=points>3"
        );
        let resp = match ureq::get(&url)
            .timeout(std::time::Duration::from_secs(8))
            .call()
        {
            Ok(r) => r,
            Err(_) => continue,
        };
        let json: serde_json::Value = match resp.into_json() {
            Ok(j) => j,
            Err(_) => continue,
        };
        if let Some(hits) = json["hits"].as_array() {
            for hit in hits {
                let title = hit["title"].as_str().unwrap_or("").to_string();
                if title.is_empty() {
                    continue;
                }
                let url = hit["url"]
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| hit["objectID"].as_str().map(|id| format!("https://news.ycombinator.com/item?id={id}")));
                out.push(RawItem {
                    source: "hackernews",
                    title,
                    url,
                    body: None,
                });
            }
        }
    }
    out
}

/// Fetch hot posts from relevant subreddits via the public JSON API.
pub fn fetch_reddit() -> Vec<RawItem> {
    let mut out = Vec::new();
    let subs = [
        "LocalLLaMA",
        "singularity",
        "OpenRouter",
        "ChatGPT",
        "artificial",
        "AutoGPT",
        "LLM",
        "MachineLearning",
        "DataHacker",
        "SideProject",
    ];
    for sub in &subs[..6] {
        let url = format!("https://www.reddit.com/r/{sub}/hot.json?limit=25");
        let resp = match ureq::get(&url)
            .timeout(std::time::Duration::from_secs(8))
            .set("User-Agent", "strawberry-alpha-hunter/1.0 (local research tool)")
            .call()
        {
            Ok(r) => r,
            Err(_) => continue,
        };
        let json: serde_json::Value = match resp.into_json() {
            Ok(j) => j,
            Err(_) => continue,
        };
        if let Some(children) = json["data"]["children"].as_array() {
            for child in children {
                let d = &child["data"];
                let title = d["title"].as_str().unwrap_or("").to_string();
                if title.is_empty() {
                    continue;
                }
                let body = d["selftext"].as_str().map(|s| s.chars().take(2000).collect());
                let url = d["url"]
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| d["permalink"].as_str().map(|p| format!("https://www.reddit.com{}", p)));
                out.push(RawItem {
                    source: "reddit",
                    title,
                    url,
                    body,
                });
            }
        }
    }
    out
}

/// Fetch OpenRouter's public model list and flag `:free` variants.
pub fn fetch_openrouter() -> Vec<RawItem> {
    let resp = match ureq::get("https://openrouter.ai/api/v1/models")
        .timeout(std::time::Duration::from_secs(8))
        .call()
    {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let json: serde_json::Value = match resp.into_json() {
        Ok(j) => j,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    if let Some(models) = json["data"].as_array() {
        for m in models {
            let id = m["id"].as_str().unwrap_or("");
            if id.ends_with(":free") {
                let name = m["name"].as_str().unwrap_or(id).to_string();
                out.push(RawItem {
                    source: "openrouter",
                    title: format!("FREE model on OpenRouter: {name} ({id})"),
                    url: Some(format!("https://openrouter.ai/models/{id}")),
                    body: Some(id.to_string()),
                });
            }
        }
    }
    out
}

/// Search GitHub for repos about free AI APIs / providers / free-tier tools.
pub fn fetch_github() -> Vec<RawItem> {
    let mut out = Vec::new();
    let queries = [
        "free AI API provider",
        "free LLM inference API",
        "free OpenAI alternative",
        "free API key AI",
        "free tier LLM",
    ];
    for q in queries {
        let url = format!(
            "https://api.github.com/search/repositories?q={q}&sort=updated&per_page=10"
        );
        let resp = match ureq::get(&url)
            .timeout(std::time::Duration::from_secs(8))
            .set("Accept", "application/vnd.github.v3+json")
            .call()
        {
            Ok(r) => r,
            Err(_) => continue,
        };
        let json: serde_json::Value = match resp.into_json() {
            Ok(j) => j,
            Err(_) => continue,
        };
        if let Some(items) = json["items"].as_array() {
            for item in items {
                let title = item["full_name"].as_str().unwrap_or("").to_string();
                let desc = item["description"].as_str().unwrap_or("").to_string();
                let url = item["html_url"].as_str().map(|s| s.to_string());
                let body = format!("{} {}", title, desc);
                out.push(RawItem {
                    source: "github",
                    title: format!("GitHub: {title}"),
                    url,
                    body: Some(body),
                });
            }
        }
    }
    out
}

/// Fetch HuggingFace models with free inference / free tags.
pub fn fetch_huggingface() -> Vec<RawItem> {
    let mut out = Vec::new();
    let urls = [
        "https://huggingface.co/api/models?search=free&limit=30",
        "https://huggingface.co/api/models?search=inference&limit=30",
        "https://huggingface.co/api/models?search=chat&limit=30",
    ];
    for url in &urls {
        let resp = match ureq::get(url)
            .timeout(std::time::Duration::from_secs(8))
            .call()
        {
            Ok(r) => r,
            Err(_) => continue,
        };
        let json: serde_json::Value = match resp.into_json() {
            Ok(j) => j,
            Err(_) => continue,
        };
        if let Some(models) = json.as_array() {
            for m in models {
                let id = m["id"].as_str().unwrap_or("");
                let tags = m["tags"].as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }).unwrap_or_default();
                if tags.contains("free") || tags.contains("free-inference-api") || id.contains("free") {
                    out.push(RawItem {
                        source: "huggingface",
                        title: format!("HF free model: {id}"),
                        url: Some(format!("https://huggingface.co/{id}")),
                        body: Some(format!("tags: {tags}")),
                    });
                }
            }
        }
    }
    out
}

/// Fetch Product Hunt Atom feed for new AI / free-tier tools.
pub fn fetch_producthunt() -> Vec<RawItem> {
    let url = "https://www.producthunt.com/feed";
    let resp = match ureq::get(url)
        .timeout(std::time::Duration::from_secs(8))
        .call()
    {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let text = match resp.into_string() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    let entry_re = match regex::Regex::new(r"<entry>(.*?)</entry>") {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let title_re = match regex::Regex::new(r"<title>(.*?)</title>") {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let link_re = match regex::Regex::new(r#"<link[^>]+rel="alternate"[^>]+href="([^"]+)""#) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    for cap in entry_re.captures_iter(&text) {
        let entry = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let title = title_re
            .captures(entry)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let link = link_re
            .captures(entry)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());
        if title.is_empty() || title == "Product Hunt" {
            continue;
        }
        out.push(RawItem {
            source: "producthunt",
            title,
            url: link,
            body: None,
        });
    }
    out
}

// =============================================================== detector ==

const FREE_TOKENS: &[&str] = &[
    "free", "free tier", "free api", "no cost", "$0", "zero cost", "alpha",
    "beta access", "now free", "unlimited free", "api key", "apikey",
    "open source", "oss", "community edition",
];

const MODEL_TOKENS: &[&str] = &[
    "qwen", "claude", "gpt", "gemini", "llama", "mistral", "deepseek",
    "grok", "opus", "sonnet", "haiku", "o1", "o3", "r1", "kimi", "glm",
    "phi", "yi", "dolphin", "neural", "falcon", "tulu", "zephyr",
];

const PROVIDER_TOKENS: &[(&str, &str)] = &[
    ("tokenrouter", "tokenrouter"),
    ("token router", "tokenrouter"),
    ("openrouter", "openrouter"),
    ("groq", "groq"),
    ("together", "together"),
    ("cerebras", "cerebras"),
    ("mistral", "mistral"),
    ("cohere", "cohere"),
    ("fireworks", "fireworks"),
    ("deepinfra", "deepinfra"),
    ("sambanova", "sambanova"),
    ("x.ai", "xai"),
    ("xai", "xai"),
    ("google ai studio", "google"),
    ("ai studio", "google"),
    ("vertex", "google"),
    ("bedrock", "aws"),
    ("azure", "azure"),
    ("huggingface", "huggingface"),
    ("hf.co", "huggingface"),
    ("together.ai", "together"),
    ("together ai", "together"),
    ("fireworks.ai", "fireworks"),
];

/// Deterministic keyword detector. Returns `None` when the item is not
/// alpha-ish; otherwise a `Candidate` with a confidence score.
pub fn detect(item: &RawItem) -> Option<Candidate> {
    let hay = format!(
        "{} {}",
        item.title.to_lowercase(),
        item.body.as_deref().unwrap_or("").to_lowercase()
    );

    let free_hits = FREE_TOKENS.iter().filter(|t| hay.contains(**t)).count();
    if free_hits == 0 {
        return None;
    }

    let model_hits = MODEL_TOKENS.iter().filter(|t| hay.contains(**t)).count();
    let provider_hits = PROVIDER_TOKENS.iter().filter(|(tok, _)| hay.contains(*tok)).count();

    if model_hits == 0 && provider_hits == 0 && item.source != "openrouter" && item.source != "huggingface" && item.source != "github" {
        return None;
    }

    let mut score: i64 = (free_hits as i64) * 2 + (model_hits as i64) * 2 + (provider_hits as i64);

    // Provider hint.
    let provider = PROVIDER_TOKENS
        .iter()
        .find(|(tok, _)| hay.contains(*tok))
        .map(|(_, p)| p.to_string());
    if provider.is_some() {
        score += 2;
    }

    // Model-id hint: look for org/model patterns like qwen/qwen3-235b.
    let model_id = extract_model_id(&hay).or_else(|| {
        if item.source == "openrouter" || item.source == "huggingface" {
            item.body.clone()
        } else {
            None
        }
    });
    if model_id.is_some() {
        score += 3;
    }

    // Base-url hint: first https URL in body that looks API-ish.
    let base_url = extract_base_url(item.body.as_deref().unwrap_or(""));
    if base_url.is_some() {
        score += 2;
    }

    // Boost for GitHub repos that mention "free API" in description.
    if item.source == "github" && hay.contains("free api") {
        score += 3;
    }

    Some(Candidate {
        source: item.source.to_string(),
        title: item.title.chars().take(200).collect(),
        url: item.url.clone(),
        provider,
        model_id,
        base_url,
        score,
    })
}

/// Extract an `org/model` style id from text.
fn extract_model_id(hay: &str) -> Option<String> {
    let re = regex::Regex::new(r"[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._:-]{2,60}").ok()?;
    let m = re.find(hay)?;
    let id = m.as_str().to_string();
    // Filter obvious non-model matches.
    if id.starts_with("http") || id.contains("://") || id.starts_with("r/") || id.starts_with("u/") {
        return None;
    }
    Some(id)
}

/// Extract an API-ish base URL from text.
fn extract_base_url(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"https://[a-zA-Z0-9.-]+(?:/[a-zA-Z0-9._/-]*)?").ok()?;
    for m in re.find_iter(text) {
        let u = m.as_str().trim_end_matches('/');
        if u.contains("api") || u.ends_with("/v1") || u.contains("huggingface") {
            // Normalize: strip trailing /chat/completions if present.
            let u = u.trim_end_matches("/chat/completions");
            return Some(u.to_string());
        }
    }
    None
}

// =============================================================== verifier ==

/// Result of a live verification call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub ok: bool,
    pub latency_ms: i64,
    pub detail: String,
}

/// Verify a candidate with a real OpenAI-compatible chat completion call.
/// Requires `api_key` from the user; never stored.
pub fn verify(base_url: &str, model_id: &str, api_key: &str) -> VerifyResult {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model_id,
        "messages": [{ "role": "user", "content": "ping" }],
        "max_tokens": 10,
    });
    let start = std::time::Instant::now();
    let result = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(20))
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .send_json(body);
    let latency_ms = start.elapsed().as_millis() as i64;

    match result {
        Ok(resp) => {
            let json: serde_json::Value = resp
                .into_json()
                .unwrap_or_else(|_| serde_json::json!({}));
            let content = json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("");
            if !content.is_empty() || json["choices"].is_array() {
                VerifyResult {
                    ok: true,
                    latency_ms,
                    detail: format!("200 OK in {latency_ms}ms — model responded"),
                }
            } else {
                VerifyResult {
                    ok: false,
                    latency_ms,
                    detail: format!("200 but empty response: {json}"),
                }
            }
        }
        Err(e) => VerifyResult {
            ok: false,
            latency_ms,
            detail: format!("Request failed: {e}"),
        },
    }
}

// ================================================== config snippet builder ==

/// Build a copy-paste config snippet for a verified find.
pub fn build_config_snippet(
    provider: Option<&str>,
    model_id: Option<&str>,
    base_url: Option<&str>,
) -> String {
    let provider = provider.unwrap_or("custom");
    let model_id = model_id.unwrap_or("unknown-model");
    let base_url = base_url.unwrap_or("https://YOUR_ENDPOINT/v1");
    format!(
        r#"// 🎯 Alpha Hunter — verified free model config
// Paste into your provider settings (Kilo / OpenAI-compatible clients)

Provider ID   : {provider}
Display name  : {provider} (free alpha)
API type      : OpenAI Compatible
Base URL      : {base_url}
API key       : <YOUR_API_KEY>
Model ID      : {model_id}
Reasoning     : ON
Image input   : OFF

// Test: curl {base_url}/models -H "Authorization: Bearer <YOUR_API_KEY>"
"#
    )
}

// ================================================================== tests ==

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_free_model_mention() {
        let item = RawItem {
            source: "reddit",
            title: "Qwen 3.8 Max is FREE on Token Router right now".into(),
            url: None,
            body: Some("base url https://api.tokenrouter.ai/v1 model qwen/qwen3.8-max-free".into()),
        };
        let c = detect(&item).expect("should detect");
        assert_eq!(c.provider.as_deref(), Some("tokenrouter"));
        assert_eq!(c.model_id.as_deref(), Some("qwen/qwen3.8-max-free"));
        assert_eq!(c.base_url.as_deref(), Some("https://api.tokenrouter.ai/v1"));
        assert!(c.score >= 8);
    }

    #[test]
    fn ignores_irrelevant() {
        let item = RawItem {
            source: "hackernews",
            title: "Rust 1.80 released".into(),
            url: None,
            body: None,
        };
        assert!(detect(&item).is_none());
    }

    #[test]
    fn openrouter_free_model_detected() {
        let item = RawItem {
            source: "openrouter",
            title: "FREE model on OpenRouter: Qwen 3 (qwen/qwen3:free)".into(),
            url: None,
            body: Some("qwen/qwen3:free".into()),
        };
        let c = detect(&item).expect("should detect");
        assert_eq!(c.model_id.as_deref(), Some("qwen/qwen3:free"));
    }

    #[test]
    fn github_free_api_repo_detected() {
        let item = RawItem {
            source: "github",
            title: "awesome-free-ai-apis/awesome-free-ai-apis".into(),
            url: Some("https://github.com/awesome-free-ai-apis/awesome-free-ai-apis".into()),
            body: Some("A curated list of free AI API providers and free tier LLM tools".into()),
        };
        let c = detect(&item).expect("should detect");
        assert_eq!(c.source, "github");
        assert!(c.score > 0);
    }

    #[test]
    fn config_snippet_contains_fields() {
        let s = build_config_snippet(Some("tokenrouter"), Some("qwen/qwen3.8-max-free"), Some("https://api.tokenrouter.ai/v1"));
        assert!(s.contains("tokenrouter"));
        assert!(s.contains("qwen/qwen3.8-max-free"));
        assert!(s.contains("https://api.tokenrouter.ai/v1"));
        assert!(s.contains("OpenAI Compatible"));
    }
}
