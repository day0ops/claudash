use crate::types::{ExtraUsage, QuotaLimit, StatusResponse};
use std::sync::atomic::{AtomicBool, Ordering};

static LIGHT_MODE: AtomicBool = AtomicBool::new(false);

/// Enable light-background color palette.
pub fn set_light_mode(enabled: bool) {
    LIGHT_MODE.store(enabled, Ordering::Relaxed);
}

fn light() -> bool {
    LIGHT_MODE.load(Ordering::Relaxed)
}

// ANSI codes that don't change with theme
pub const RESET: &str = "\x1b[0m";

const BAR_FILLED: char = '\u{2588}'; // █
const BAR_EMPTY: char = '\u{2591}'; // ░

/// Non-breaking space to prevent terminal whitespace collapse.
pub const NBSP: char = '\u{00A0}';

// ── Theme-aware color accessors ─────────────────────────────────────
//
// Dark mode: standard ANSI / bright colors (good contrast on dark bg)
// Light mode: 256-color palette — all colors chosen at a similar dark
//             intensity for uniform readability on white backgrounds.

pub fn dim() -> &'static str {
    if light() {
        "\x1b[38;5;244m" // gray — labels, brackets, separators
    } else {
        "\x1b[2m"
    }
}
pub fn green() -> &'static str {
    if light() {
        "\x1b[38;5;28m" // dark green
    } else {
        "\x1b[32m"
    }
}
pub fn yellow() -> &'static str {
    if light() {
        "\x1b[38;5;130m" // dark amber
    } else {
        "\x1b[33m"
    }
}
pub fn red() -> &'static str {
    if light() {
        "\x1b[38;5;124m" // dark red
    } else {
        "\x1b[31m"
    }
}
pub fn orange() -> &'static str {
    if light() {
        "\x1b[38;5;166m" // dark orange
    } else {
        "\x1b[38;5;208m"
    }
}
pub fn cyan() -> &'static str {
    if light() {
        "\x1b[38;5;30m" // dark teal
    } else {
        "\x1b[36m"
    }
}
pub fn blue() -> &'static str {
    if light() {
        "\x1b[38;5;25m" // dark blue
    } else {
        "\x1b[94m"
    }
}
pub fn magenta() -> &'static str {
    if light() {
        "\x1b[38;5;90m" // dark purple
    } else {
        "\x1b[95m"
    }
}

/// Render a progress bar of `width` chars using █ and ░ with the given ANSI color.
pub fn bar(pct: f64, width: usize, color: &str) -> String {
    let dim = dim();
    let clamped = pct.clamp(0.0, 100.0);
    let filled = ((clamped / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!(
        "{color}{}{RESET}{dim}{}{RESET}",
        BAR_FILLED.to_string().repeat(filled),
        BAR_EMPTY.to_string().repeat(empty),
    )
}

/// Return the context window warning threshold (5% below auto-compaction point).
/// Respects CLAUDE_AUTOCOMPACT_PCT_OVERRIDE env var.
pub fn context_warn_pct() -> u8 {
    let compact_pct: u8 = std::env::var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&v: &u8| v > 0)
        .unwrap_or(85);
    compact_pct.saturating_sub(5)
}

/// Choose ANSI color for context window usage:
/// - green:  0–40%  (full capability)
/// - yellow: 41–60% (quality starts to degrade)
/// - orange: 61%–warn_pct (significant quality loss)
/// - red:    ≥warn_pct (near auto-compaction)
pub fn context_color(pct: f64) -> &'static str {
    let warn = context_warn_pct() as f64;
    if pct >= warn {
        red()
    } else if pct > 60.0 {
        orange()
    } else if pct > 40.0 {
        yellow()
    } else {
        green()
    }
}

/// Choose ANSI color for quota usage.
pub fn quota_color(pct: f64) -> &'static str {
    match pct as u8 {
        0..=74 => blue(),
        75..=89 => magenta(),
        _ => red(),
    }
}

/// Format a duration in milliseconds as `1h 23m` or `5m 30s`.
pub fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{}h{NBSP}{}m", hours, mins)
    } else {
        format!("{}m{NBSP}{}s", mins, secs)
    }
}

/// Format a USD cost value, e.g. `$0.12`.
pub fn format_cost(usd: f64) -> String {
    if usd < 10.0 {
        format!("${:.2}", usd)
    } else {
        format!("${:.1}", usd)
    }
}

/// Format a quota sub-indicator with bar and label (e.g. for per-model 7-day quotas).
/// Returns None when the quota is absent or has no utilization.
pub fn format_sub_bar(q: Option<&QuotaLimit>, label: &str) -> Option<String> {
    let dim = dim();
    let q = q?;
    let util = q.utilization?;
    let color = quota_color(util);
    let q_bar = bar(util, 5, color);
    Some(format!(
        "{q_bar}{NBSP}{color}{util:.0}%{RESET}{NBSP}{dim}{label}{RESET}"
    ))
}

/// Format pay-as-you-go extra usage as `◑ $used/$limit overage`.
/// Returns None when disabled, missing, or used credits are zero.
pub fn format_extra_usage(extra: &ExtraUsage) -> Option<String> {
    let dim = dim();
    if !extra.is_enabled {
        return None;
    }
    let limit = extra.monthly_limit?;
    let used = extra.used_credits?;
    if used == 0.0 {
        return None;
    }
    let used_dollars = (used / 100.0) as i64;
    let limit_dollars = (limit / 100.0) as i64;
    let pct = if limit_dollars > 0 {
        ((used_dollars * 100) / limit_dollars) as f64
    } else {
        0.0
    };
    // Hide overage until it reaches 60% of the limit
    if pct < 60.0 {
        return None;
    }
    // Red when ≥80% of limit is consumed
    let color = if pct >= 80.0 { red() } else { blue() };
    let usage_bar = bar(pct, 5, color);
    Some(format!(
        "{usage_bar}{NBSP}{color}${used_dollars}/${limit_dollars}{RESET}{NBSP}{dim}overage{RESET}"
    ))
}

/// Format Claude service status as a siren icon with severity bars.
/// Returns None when the service is operational.
pub fn format_status_indicator(status: &StatusResponse) -> Option<String> {
    let orange = orange();
    match status.status.indicator.as_str() {
        "minor" => Some(format!("{orange}🚨▂{RESET}")),
        "major" => Some(format!("{orange}🚨▄▂{RESET}")),
        "critical" => Some(format!("{orange}🚨▆▄▂{RESET}")),
        _ => None,
    }
}

/// Truncate a name to `max_len` chars using a Unicode ellipsis in the middle.
pub fn compact_name(name: &str, max_len: usize) -> String {
    let runes: Vec<char> = name.chars().collect();
    if runes.len() <= max_len {
        return name.to_string();
    }
    let half = (max_len - 1) / 2;
    let left: String = runes[..half].iter().collect();
    let right: String = runes[runes.len() - (max_len - 1 - half)..].iter().collect();
    format!("{left}…{right}")
}

/// Extract the last path segment from a directory path for display.
/// Returns None for root paths or bare drive letters.
pub fn cwd_name(cwd: &str, max_len: usize) -> Option<String> {
    let name = cwd.replace('\\', "/");
    let name = name.trim_end_matches('/');
    let name = name.rsplit('/').next().unwrap_or("");
    if name.is_empty() || name == "." || (name.len() == 2 && name.ends_with(':')) {
        return None;
    }
    Some(compact_name(name, max_len))
}

/// Dim separator for joining segments.
pub fn sep() -> String {
    let dim = dim();
    format!("{NBSP}{dim}│{RESET}{NBSP}")
}

/// Replace regular spaces with non-breaking spaces.
pub fn nbsp(s: &str) -> String {
    s.replace(' ', &NBSP.to_string())
}

/// Get today's date as "YYYY-MM-DD" (UTC).
pub fn today_local() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    // Civil date from days since Unix epoch (Howard Hinnant's algorithm)
    let z = secs / 86400 + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Get current unix timestamp in seconds.
pub fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_zero() {
        let b = bar(0.0, 5, green());
        assert!(b.contains(&BAR_EMPTY.to_string().repeat(5)));
        assert!(!b.contains(BAR_FILLED));
    }

    #[test]
    fn test_bar_full() {
        let b = bar(100.0, 5, red());
        assert!(b.contains(&BAR_FILLED.to_string().repeat(5)));
    }

    #[test]
    fn test_bar_half() {
        let b = bar(50.0, 5, yellow());
        let filled_count = b.matches(BAR_FILLED).count();
        let empty_count = b.matches(BAR_EMPTY).count();
        assert_eq!(filled_count, 3);
        assert_eq!(empty_count, 2);
    }

    #[test]
    fn test_context_color_thresholds() {
        // Default warn_pct = 85 - 5 = 80
        assert_eq!(context_color(0.0), green());
        assert_eq!(context_color(40.0), green());
        assert_eq!(context_color(41.0), yellow());
        assert_eq!(context_color(60.0), yellow());
        assert_eq!(context_color(61.0), orange());
        assert_eq!(context_color(79.0), orange());
        assert_eq!(context_color(80.0), red());
        assert_eq!(context_color(100.0), red());
    }

    #[test]
    fn test_quota_color_thresholds() {
        assert_eq!(quota_color(0.0), blue());
        assert_eq!(quota_color(74.0), blue());
        assert_eq!(quota_color(75.0), magenta());
        assert_eq!(quota_color(89.0), magenta());
        assert_eq!(quota_color(90.0), red());
        assert_eq!(quota_color(100.0), red());
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.0), "$0.00");
        assert_eq!(format_cost(0.12345), "$0.12");
        assert_eq!(format_cost(3.456), "$3.46");
        assert_eq!(format_cost(12.3), "$12.3");
    }

    #[test]
    fn test_compact_name_short() {
        assert_eq!(compact_name("hello", 10), "hello");
    }

    #[test]
    fn test_compact_name_long() {
        let s = compact_name("my-very-long-branch-name", 10);
        assert_eq!(s.chars().count(), 10);
        assert!(s.contains('…'));
    }

    #[test]
    fn test_cwd_name() {
        assert_eq!(
            cwd_name("/home/user/projects", 30),
            Some("projects".to_string())
        );
        assert_eq!(cwd_name("/", 30), None);
        assert_eq!(cwd_name(".", 30), None);
    }

    #[test]
    fn test_format_extra_usage_disabled() {
        let extra = ExtraUsage {
            is_enabled: false,
            monthly_limit: Some(2000.0),
            used_credits: Some(500.0),
        };
        assert_eq!(format_extra_usage(&extra), None);
    }

    #[test]
    fn test_format_extra_usage_zero() {
        let extra = ExtraUsage {
            is_enabled: true,
            monthly_limit: Some(2000.0),
            used_credits: Some(0.0),
        };
        assert_eq!(format_extra_usage(&extra), None);
    }

    #[test]
    fn test_format_extra_usage_above_threshold() {
        let extra = ExtraUsage {
            is_enabled: true,
            monthly_limit: Some(2000.0),
            used_credits: Some(1400.0), // 70% of limit → above 60% threshold
        };
        let s = format_extra_usage(&extra).unwrap();
        assert!(s.contains("$14/$20"));
        assert!(s.contains("overage"));
    }

    #[test]
    fn test_format_extra_usage_high() {
        let extra = ExtraUsage {
            is_enabled: true,
            monthly_limit: Some(2000.0),
            used_credits: Some(1800.0), // 90% of limit → red
        };
        let s = format_extra_usage(&extra).unwrap();
        assert!(s.contains("$18/$20"));
        assert!(s.contains("overage"));
        assert!(s.contains(red()));
    }

    #[test]
    fn test_format_status_indicator() {
        use crate::types::{StatusIndicator, StatusResponse};

        let make_status = |indicator: &str| StatusResponse {
            status: StatusIndicator {
                indicator: indicator.to_string(),
                description: String::new(),
            },
        };

        assert!(format_status_indicator(&make_status("minor"))
            .unwrap()
            .contains("🚨"));
        assert!(format_status_indicator(&make_status("none")).is_none());
        assert!(format_status_indicator(&make_status("")).is_none());
    }

    #[test]
    fn test_nbsp() {
        assert_eq!(nbsp("a b"), "a\u{00A0}b");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(45000), format!("0m{NBSP}45s"));
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(330000), format!("5m{NBSP}30s"));
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(5430000), format!("1h{NBSP}30m"));
    }

    #[test]
    fn test_format_duration_zero() {
        assert_eq!(format_duration(0), format!("0m{NBSP}0s"));
    }

    #[test]
    fn test_format_sub_bar() {
        let q = QuotaLimit {
            utilization: Some(50.0),
            resets_at: None,
        };
        let result = format_sub_bar(Some(&q), "sonnet").unwrap();
        assert!(result.contains("50%"));
        assert!(result.contains("sonnet"));
    }

    #[test]
    fn test_format_sub_bar_none() {
        assert!(format_sub_bar(None, "opus").is_none());
        let q = QuotaLimit {
            utilization: None,
            resets_at: None,
        };
        assert!(format_sub_bar(Some(&q), "opus").is_none());
    }

    #[test]
    fn test_format_extra_usage_below_threshold() {
        let extra = ExtraUsage {
            is_enabled: true,
            monthly_limit: Some(2000.0),
            used_credits: Some(800.0), // 40% of limit → below 60% threshold
        };
        assert_eq!(format_extra_usage(&extra), None);
    }

    #[test]
    fn test_context_warn_pct_default() {
        // Default: 85 - 5 = 80
        assert_eq!(context_warn_pct(), 80);
    }
}
