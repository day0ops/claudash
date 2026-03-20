use crate::credentials::config_dir_hash_suffix;
use crate::display::now_secs;
use crate::types::{CacheEntry, UsageResponse};
use std::time::Duration;

const CACHE_TTL_OK: i64 = 60;
const CACHE_TTL_FAIL: i64 = 15;
const CACHE_TTL_RATE_LIMIT_DEFAULT: i64 = 5 * 60;
const CACHE_TTL_RATE_LIMIT_MAX: i64 = 30 * 60;
const IO_TIMEOUT_SECS: u64 = 5;
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// Fetch usage data, using a file-based cache.
pub fn fetch_usage(access_token: &str) -> Option<UsageResponse> {
    let cache_path = cache_file_path();

    if let Some(cached) = read_cache(&cache_path) {
        let now = now_secs();
        let age = now - cached.timestamp;

        if cached.rate_limited {
            // retry_after is a Unix timestamp; 0 means use default TTL (old cache format)
            let still_limited = if cached.retry_after > 0 {
                cached.retry_after > now
            } else {
                age < CACHE_TTL_RATE_LIMIT_DEFAULT
            };
            if still_limited {
                return None;
            }
            // Cooldown passed — fall through to re-fetch
        } else if cached.ok && age < CACHE_TTL_OK {
            return cached.data.and_then(|d| serde_json::from_value(d).ok());
        } else if !cached.ok && age < CACHE_TTL_FAIL {
            return None;
        }
    }

    fetch_and_cache(access_token, &cache_path)
}

fn cache_file_path() -> String {
    let suffix = config_dir_hash_suffix();
    format!("/tmp/claudash-usage{}.json", suffix)
}

fn read_cache(path: &str) -> Option<CacheEntry> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_cache(path: &str, usage: Option<&UsageResponse>, ok: bool, retry_after_ts: i64) {
    let data = usage.and_then(|u| serde_json::to_value(u).ok());
    let entry = CacheEntry {
        data,
        timestamp: now_secs(),
        ok,
        rate_limited: retry_after_ts > 0,
        retry_after: retry_after_ts,
    };
    if let Ok(json) = serde_json::to_string(&entry) {
        let _ = std::fs::write(path, json);
    }
}

enum RequestError {
    RateLimited(String), // raw Retry-After header value
    Other,
}

fn build_agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(IO_TIMEOUT_SECS)))
        .http_status_as_error(false)
        .build();
    config.into()
}

fn do_single_request(agent: &ureq::Agent, token: &str) -> Result<UsageResponse, RequestError> {
    let mut response = agent
        .get(USAGE_URL)
        .header("Authorization", &format!("Bearer {}", token))
        .header("Anthropic-Beta", "oauth-2025-04-20")
        .call()
        .map_err(|_| RequestError::Other)?;

    if response.status().as_u16() == 429 {
        let raw = response
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        return Err(RequestError::RateLimited(raw));
    }

    if !response.status().is_success() {
        return Err(RequestError::Other);
    }

    response
        .body_mut()
        .read_json::<UsageResponse>()
        .map_err(|_| RequestError::Other)
}

fn fetch_and_cache(token: &str, cache_path: &str) -> Option<UsageResponse> {
    let agent = build_agent();

    match do_single_request(&agent, token) {
        Ok(usage) => {
            write_cache(cache_path, Some(&usage), true, 0);
            Some(usage)
        }
        Err(RequestError::RateLimited(raw)) => {
            if raw == "0" {
                // Retry-After: 0 means "retry now" per spec — try once more to verify
                crate::debug_log("usage: rate limited with Retry-After: 0, retrying once");
                match do_single_request(&agent, token) {
                    Ok(usage) => {
                        write_cache(cache_path, Some(&usage), true, 0);
                        return Some(usage);
                    }
                    Err(_) => {
                        // Still failing — treat "0" as a bad signal, use default TTL
                        crate::debug_log(
                            "usage: second attempt also rate limited, using default TTL",
                        );
                        let ts = now_secs() + CACHE_TTL_RATE_LIMIT_DEFAULT;
                        write_cache(cache_path, None, false, ts);
                        return None;
                    }
                }
            }
            let secs = parse_retry_after(&raw);
            let ts = now_secs() + secs;
            crate::debug_log(format!("usage: rate limited, retry after {}s", secs));
            write_cache(cache_path, None, false, ts);
            None
        }
        Err(RequestError::Other) => {
            crate::debug_log("usage: fetch failed");
            write_cache(cache_path, None, false, 0);
            None
        }
    }
}

/// Parse Retry-After header as seconds (integer) or HTTP-date.
/// Returns 0 only when the raw value is literally "0" (handled by caller).
fn parse_retry_after(value: &str) -> i64 {
    if value.is_empty() {
        return CACHE_TTL_RATE_LIMIT_DEFAULT;
    }
    if let Ok(secs) = value.parse::<i64>() {
        if secs > 0 {
            return secs.min(CACHE_TTL_RATE_LIMIT_MAX);
        }
        return 0; // literal "0" — caller handles the single-retry logic
    }
    CACHE_TTL_RATE_LIMIT_DEFAULT
}
