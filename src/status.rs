use crate::credentials::config_dir_hash_suffix;
use crate::display::now_secs;
use crate::types::{StatusCacheEntry, StatusResponse};
use std::time::Duration;

const STATUS_URL: &str = "https://status.claude.com/api/v2/status.json";
const CACHE_TTL_OK: i64 = 2 * 60; // 2 minutes
const CACHE_TTL_FAIL: i64 = 30; // 30 seconds
const IO_TIMEOUT_SECS: u64 = 5;

fn cache_file_path() -> String {
    let suffix = config_dir_hash_suffix();
    format!("/tmp/claudash-status{}.json", suffix)
}

/// Fetch Claude service status. Returns None when operational or unavailable.
pub fn fetch_status() -> Option<StatusResponse> {
    let cache_path = cache_file_path();

    if let Some(cached) = read_cache(&cache_path) {
        let age = now_secs() - cached.timestamp;
        if cached.ok && age < CACHE_TTL_OK {
            let status: StatusResponse =
                cached.data.and_then(|d| serde_json::from_value(d).ok())?;
            return if status.status.indicator == "none" {
                None
            } else {
                Some(status)
            };
        }
        if !cached.ok && age < CACHE_TTL_FAIL {
            return None;
        }
    }

    match do_status_request() {
        Ok(status) => {
            crate::debug_log(format!(
                "status: {} - {}",
                status.status.indicator, status.status.description
            ));
            write_cache(&cache_path, Some(&status), true);
            if status.status.indicator == "none" {
                None
            } else {
                Some(status)
            }
        }
        Err(e) => {
            crate::debug_log(format!("status: fetch failed: {}", e));
            write_cache(&cache_path, None, false);
            None
        }
    }
}

fn do_status_request() -> Result<StatusResponse, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(IO_TIMEOUT_SECS)))
        .build();
    let agent: ureq::Agent = config.into();

    agent
        .get(STATUS_URL)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_json::<StatusResponse>()
        .map_err(|e| e.to_string())
}

fn read_cache(path: &str) -> Option<StatusCacheEntry> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_cache(path: &str, status: Option<&StatusResponse>, ok: bool) {
    let data = status.and_then(|s| serde_json::to_value(s).ok());
    let entry = StatusCacheEntry {
        data,
        timestamp: now_secs(),
        ok,
    };
    if let Ok(json) = serde_json::to_string(&entry) {
        let _ = std::fs::write(path, json);
    }
}
