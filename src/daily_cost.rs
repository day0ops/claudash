use crate::display::today_local;
use crate::types::DailyCostCache;
use std::collections::HashMap;

/// Update daily cost tracking and return the accumulated total for today.
pub fn update_daily_cost(session_id: &str, session_cost: f64) -> f64 {
    let today = today_local();
    let cache_path = format!("/tmp/claudash-daily-{}.json", today);

    let mut cache = read_cache(&cache_path, &today);

    // Update this session's cost
    cache.sessions.insert(session_id.to_string(), session_cost);

    // Recalculate total
    cache.total = cache.sessions.values().sum();

    // Write back
    write_cache(&cache_path, &cache);

    cache.total
}

fn read_cache(path: &str, today: &str) -> DailyCostCache {
    if let Ok(contents) = std::fs::read_to_string(path) {
        if let Ok(cache) = serde_json::from_str::<DailyCostCache>(&contents) {
            if cache.date == today {
                return cache;
            }
        }
    }

    // Fresh cache for today
    DailyCostCache {
        date: today.to_string(),
        sessions: HashMap::new(),
        total: 0.0,
    }
}

fn write_cache(path: &str, cache: &DailyCostCache) {
    if let Ok(json) = serde_json::to_string(cache) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_accumulation() {
        let today = today_local();
        let test_path = format!("/tmp/claudash-test-daily-{}.json", today);
        // Clean up
        let _ = fs::remove_file(&test_path);

        let mut cache = DailyCostCache {
            date: today.clone(),
            sessions: HashMap::new(),
            total: 0.0,
        };

        // Session A costs $1.00
        cache.sessions.insert("sess_a".to_string(), 1.0);
        cache.total = cache.sessions.values().sum();
        assert!((cache.total - 1.0).abs() < f64::EPSILON);

        // Session B costs $2.50
        cache.sessions.insert("sess_b".to_string(), 2.5);
        cache.total = cache.sessions.values().sum();
        assert!((cache.total - 3.5).abs() < f64::EPSILON);

        // Session A updates to $1.50
        cache.sessions.insert("sess_a".to_string(), 1.5);
        cache.total = cache.sessions.values().sum();
        assert!((cache.total - 4.0).abs() < f64::EPSILON);

        let _ = fs::remove_file(&test_path);
    }

    #[test]
    fn test_date_rollover() {
        let cache = read_cache("/tmp/nonexistent-claudash-daily.json", "2099-01-01");
        assert_eq!(cache.date, "2099-01-01");
        assert!(cache.sessions.is_empty());
        assert!((cache.total - 0.0).abs() < f64::EPSILON);
    }
}
