//! 👻 Ghost Attention — tracks when and what the user pays attention to.
//!
//! Builds a 7×24 heatmap (day-of-week × hour) and computes streaks / peak times.

use rusqlite::Connection;
use std::collections::HashMap;
use crate::ghost::AttentionCell;

/// Compute the attention heatmap (7 days × 24 hours).
pub fn heatmap(conn: &Connection) -> Result<Vec<AttentionCell>, String> {
    let mut buckets: HashMap<(u8, u8), (i64, i64)> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT strftime('%w', created_at) AS dow,
                strftime('%H', created_at) AS hour,
                COUNT(*),
                COALESCE(SUM(duration_ms), 0)
         FROM ghost_events
         WHERE created_at >= datetime('now', '-90 days')
         GROUP BY dow, hour"
    ).map_err(|e| format!("heatmap prep: {e}"))?;

    let rows = stmt.query_map([], |r| {
        let dow_str: String = r.get(0)?;
        let hour_str: String = r.get(1)?;
        let count: i64 = r.get(2)?;
        let dur: i64 = r.get(3)?;
        Ok((dow_str, hour_str, count, dur))
    }).map_err(|e| format!("heatmap rows: {e}"))?;

    for row in rows {
        let (dow_str, hour_str, count, dur) = row.map_err(|e| format!("heatmap row: {e}"))?;
        // SQLite %w: 0=Sunday. Convert to 0=Monday for our grid.
        let dow_raw: i64 = dow_str.parse().unwrap_or(0);
        let dow = ((dow_raw + 6) % 7) as u8;
        let hour: u8 = hour_str.parse().unwrap_or(0);
        buckets.insert((dow, hour), (count, dur));
    }

    let mut out = Vec::new();
    for day in 0..7u8 {
        for hour in 0..24u8 {
            let (count, dur) = buckets.get(&(day, hour)).copied().unwrap_or((0, 0));
            out.push(AttentionCell {
                day, hour, count, duration_ms: dur,
            });
        }
    }
    Ok(out)
}

/// Calculate current consecutive-day streak.
pub fn streak_days(conn: &Connection) -> Result<i64, String> {
    // Get distinct days with events, ordered desc.
    let mut stmt = conn.prepare(
        "SELECT DISTINCT date(created_at) AS d FROM ghost_events
         ORDER BY d DESC LIMIT 365"
    ).map_err(|e| format!("streak prep: {e}"))?;
    let days: Vec<String> = stmt.query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("streak rows: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    let mut streak = 0i64;
    let today = chrono::Utc::now().date_naive();
    let mut expected = today;
    for d in &days {
        // d is "YYYY-MM-DD" UTC.
        if let Ok(parsed) = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
            if parsed == expected {
                streak += 1;
                expected = expected.pred_opt().unwrap_or(expected);
            } else if parsed == expected.pred_opt().unwrap_or(expected) && streak == 0 {
                // Allow streak starting from yesterday.
                streak += 1;
                expected = parsed.pred_opt().unwrap_or(parsed);
            } else {
                break;
            }
        }
    }
    Ok(streak)
}

/// Find the most-visited chats in the last 30 days.
pub fn top_chats(conn: &Connection, limit: usize) -> Result<Vec<(String, String, i64)>, String> {
    let mut stmt = conn.prepare(
        "SELECT g.source_id, COALESCE(ch.title, g.source_id), COUNT(*) AS c
         FROM ghost_events g
         LEFT JOIN chats ch ON ch.id = g.source_id
         WHERE g.event_type = 'open_chat' AND g.source_id IS NOT NULL
           AND g.created_at >= datetime('now', '-30 days')
         GROUP BY g.source_id
         ORDER BY c DESC
         LIMIT ?1"
    ).map_err(|e| format!("top chats prep: {e}"))?;

    let rows = stmt.query_map([limit as i64], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
    }).map_err(|e| format!("top chats rows: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("top chats row: {e}"))?);
    }
    Ok(out)
}

/// Top tags used.
pub fn top_tags(conn: &Connection, limit: usize) -> Result<Vec<(String, i64)>, String> {
    let mut tag_count: HashMap<String, i64> = HashMap::new();
    let mut stmt = conn.prepare("SELECT tags FROM chats WHERE tags IS NOT NULL AND tags != ''")
        .map_err(|e| format!("top tags prep: {e}"))?;
    let rows = stmt.query_map([], |r| r.get::<_, Option<String>>(0))
        .map_err(|e| format!("top tags rows: {e}"))?;
    for row in rows {
        if let Some(Some(tags)) = row.ok() {
            for tag in tags.split(',') {
                let t = tag.trim().to_string();
                if !t.is_empty() {
                    *tag_count.entry(t).or_insert(0) += 1;
                }
            }
        }
    }
    let mut vec: Vec<(String, i64)> = tag_count.into_iter().collect();
    vec.sort_by(|a, b| b.1.cmp(&a.1));
    vec.truncate(limit);
    Ok(vec)
}

/// Peak hour and day from the heatmap.
pub fn peak(heatmap: &[AttentionCell]) -> (Option<u8>, Option<u8>) {
    let mut max = 0i64;
    let mut peak_hour = None;
    let mut peak_day = None;
    for cell in heatmap {
        if cell.count > max {
            max = cell.count;
            peak_hour = Some(cell.hour);
        }
    }
    // For peak day, sum by day.
    let mut day_totals: HashMap<u8, i64> = HashMap::new();
    for cell in heatmap {
        *day_totals.entry(cell.day).or_insert(0) += cell.count;
    }
    if let Some((&d, &c)) = day_totals.iter().max_by_key(|(_, c)| *c) {
        if c > 0 { peak_day = Some(d); }
    }
    (peak_hour, peak_day)
}
