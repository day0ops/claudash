use serde::Deserialize;
use std::collections::HashMap;

// ── Claude Code stdin JSON ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StdinData {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<Model>,
    pub cost: Option<Cost>,
    pub context_window: Option<ContextWindow>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Model {
    pub id: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Cost {
    pub total_cost_usd: Option<f64>,
    pub total_duration_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ContextWindow {
    pub used_percentage: Option<f64>,
    pub context_window_size: Option<u64>,
    pub remaining_percentage: Option<f64>,
}

// ── Credentials (.credentials.json / Keychain) ─────────────────────

#[derive(Debug, Deserialize)]
pub struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    pub claude_ai_oauth: Option<OAuthCreds>,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCreds {
    #[serde(rename = "accessToken")]
    pub access_token: Option<String>,
    #[serde(rename = "subscriptionType")]
    pub subscription_type: Option<String>,
}

// ── Profile API response ────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ProfileResponse {
    pub account: ProfileAccount,
    pub organization: ProfileOrganization,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ProfileAccount {
    pub email: String,
    pub display_name: Option<String>,
    pub full_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ProfileOrganization {
    pub name: Option<String>,
}

// ── Profile cache ──────────────────────────────────────────────────

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct ProfileCacheEntry {
    pub data: Option<serde_json::Value>,
    pub timestamp: i64,
    pub ok: bool,
}

// ── Usage API response ──────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct UsageResponse {
    pub five_hour: Option<QuotaLimit>,
    pub seven_day: Option<QuotaLimit>,
    pub seven_day_sonnet: Option<QuotaLimit>,
    pub seven_day_opus: Option<QuotaLimit>,
    pub seven_day_cowork: Option<QuotaLimit>,
    pub seven_day_oauth_apps: Option<QuotaLimit>,
    pub extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct QuotaLimit {
    pub utilization: Option<f64>,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ExtraUsage {
    pub is_enabled: bool,
    pub monthly_limit: Option<f64>,
    pub used_credits: Option<f64>,
}

// ── Usage cache ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct CacheEntry {
    pub data: Option<serde_json::Value>,
    pub timestamp: i64,
    pub ok: bool,
    #[serde(default)]
    pub rate_limited: bool,
    /// Unix timestamp after which a retry is allowed (0 = not rate limited).
    #[serde(default)]
    pub retry_after: i64,
}

// ── Daily cost cache ────────────────────────────────────────────────

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct DailyCostCache {
    pub date: String,
    pub sessions: HashMap<String, f64>,
    pub total: f64,
}

// ── Claude service status ───────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct StatusResponse {
    pub status: StatusIndicator,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct StatusIndicator {
    pub indicator: String,
    pub description: String,
}

// ── Status cache ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct StatusCacheEntry {
    pub data: Option<serde_json::Value>,
    pub timestamp: i64,
    pub ok: bool,
}
