use crate::credentials::config_dir_hash_suffix;
use crate::display::now_secs;
use crate::types::{ProfileCacheEntry, ProfileResponse};
use std::time::Duration;

const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
const CACHE_TTL_OK: i64 = 60 * 60; // 1 hour (profile data rarely changes)
const CACHE_TTL_FAIL: i64 = 5 * 60; // 5 minutes on failure
const IO_TIMEOUT_SECS: u64 = 5;

fn cache_file_path() -> String {
    let suffix = config_dir_hash_suffix();
    format!("/tmp/claudash-profile{}.json", suffix)
}

/// Fetch profile data (email, org name). Cached for 1 hour.
pub fn fetch_profile(access_token: &str) -> Option<ProfileResponse> {
    let cache_path = cache_file_path();

    if let Some(cached) = read_cache(&cache_path) {
        let age = now_secs() - cached.timestamp;
        if cached.ok && age < CACHE_TTL_OK {
            return cached.data.and_then(|d| serde_json::from_value(d).ok());
        }
        if !cached.ok && age < CACHE_TTL_FAIL {
            return None;
        }
    }

    match do_request(access_token) {
        Ok(profile) => {
            write_cache(&cache_path, Some(&profile), true);
            Some(profile)
        }
        Err(e) => {
            crate::debug_log(format!("profile: fetch failed: {}", e));
            write_cache(&cache_path, None, false);
            None
        }
    }
}

fn do_request(token: &str) -> Result<ProfileResponse, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(IO_TIMEOUT_SECS)))
        .build();
    let agent: ureq::Agent = config.into();

    agent
        .get(PROFILE_URL)
        .header("Authorization", &format!("Bearer {}", token))
        .header("Anthropic-Beta", "oauth-2025-04-20")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_json::<ProfileResponse>()
        .map_err(|e| e.to_string())
}

fn read_cache(path: &str) -> Option<ProfileCacheEntry> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_cache(path: &str, profile: Option<&ProfileResponse>, ok: bool) {
    let data = profile.and_then(|p| serde_json::to_value(p).ok());
    let entry = ProfileCacheEntry {
        data,
        timestamp: now_secs(),
        ok,
    };
    if let Ok(json) = serde_json::to_string(&entry) {
        let _ = std::fs::write(path, json);
    }
}
