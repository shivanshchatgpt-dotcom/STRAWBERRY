//! 📰 Optional tech-news fetcher for the daily briefing.
//! Uses the free HackerNews Algolia API — no key, generous limits.
//! Called only when `app_meta.news_enabled = '1'`.

/// Fetch top tech headlines as short lines. Never panics; errors → empty.
pub fn fetch_top_headlines(limit: usize) -> Result<Vec<String>, String> {
    let url = format!(
        "https://hn.algolia.com/api/v1/search?tags=front_page&hitsPerPage={limit}",
        limit = limit * 2
    );
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    if let Some(hits) = json["hits"].as_array() {
        for hit in hits {
            if out.len() >= limit {
                break;
            }
            let title = hit["title"]
                .as_str()
                .or_else(|| hit["story_title"].as_str())
                .unwrap_or("");
            if title.is_empty() {
                continue;
            }
            let short: String = title.chars().take(90).collect();
            out.push(short);
        }
    }
    Ok(out)
}
