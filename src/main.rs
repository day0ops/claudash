mod credentials;
mod daily_cost;
mod display;
mod profile;
mod status;
mod types;
mod usage;

#[cfg(test)]
mod tests;

use display::*;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use types::StdinData;

// ── Debug logging ────────────────────────────────────────────────────

static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn debug_log(msg: impl AsRef<str>) {
    if !DEBUG_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    use std::io::Write;
    let path = debug_log_path();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{}", msg.as_ref());
    }
}

fn debug_log_path() -> String {
    format!(
        "/tmp/claudash-debug{}.log",
        credentials::config_dir_hash_suffix()
    )
}

// ── CLI config ───────────────────────────────────────────────────────

struct Config {
    debug: bool,
    show_version: bool,
    show_git_branch: bool,
    git_branch_max_len: usize,
    show_cwd: bool,
    cwd_max_len: usize,
    light: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            debug: false,
            show_version: false,
            show_git_branch: false,
            git_branch_max_len: 30,
            show_cwd: false,
            cwd_max_len: 30,
            light: false,
        }
    }
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();
    let mut cfg = Config::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--debug" => cfg.debug = true,
            "--version" => cfg.show_version = true,
            "--git-branch" => cfg.show_git_branch = true,
            "--git-branch-max-len" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    cfg.git_branch_max_len = v.parse().unwrap_or(30);
                }
            }
            "--cwd" => cfg.show_cwd = true,
            "--cwd-max-len" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    cfg.cwd_max_len = v.parse().unwrap_or(30);
                }
            }
            "--light" => cfg.light = true,
            _ => {}
        }
        i += 1;
    }
    cfg
}

// ── Git branch ───────────────────────────────────────────────────────

/// Read the current git branch from .git/HEAD without spawning a subprocess.
fn git_branch(cwd: &str) -> Option<String> {
    let head = std::fs::read_to_string(format!("{}/.git/HEAD", cwd)).ok()?;
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(|b| b.to_string())
}

// ── Main ─────────────────────────────────────────────────────────────

fn main() {
    let cfg = parse_args();

    if cfg.show_version {
        println!("claudash {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if cfg.debug {
        DEBUG_ENABLED.store(true, Ordering::Relaxed);
    }

    if cfg.light {
        set_light_mode(true);
    }

    // Read all stdin
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() || input.trim().is_empty() {
        return;
    }

    // Parse stdin JSON
    let data: StdinData = match serde_json::from_str(&input) {
        Ok(d) => d,
        Err(e) => {
            debug_log(format!("failed to parse stdin: {}", e));
            return;
        }
    };

    // Bind theme-aware colors once for format strings
    let dim = dim();
    let cyan = cyan();
    let yellow_c = yellow();
    let magenta_c = magenta();

    let mut segments: Vec<String> = Vec::new();

    // ── Identity: [Model | Plan | email] ────────────────────────────
    let model_name = data
        .model
        .as_ref()
        .and_then(|m| m.display_name.as_deref())
        .unwrap_or("Claude");

    let cred_info = credentials::read_credentials();
    let plan_name = cred_info
        .as_ref()
        .map(|c| c.plan_name.as_str())
        .unwrap_or("?");

    // Fetch email from profile API (cached for 1 hour)
    let email = cred_info
        .as_ref()
        .and_then(|c| profile::fetch_profile(&c.access_token))
        .map(|p| p.account.email);

    let mut identity = if let Some(ref email) = email {
        format!("{dim}[{RESET}{cyan}{model_name}{RESET}{dim}{NBSP}|{NBSP}{plan_name}{NBSP}|{NBSP}{email}]{RESET}")
    } else {
        format!("{dim}[{RESET}{cyan}{model_name}{RESET}{dim}{NBSP}|{NBSP}{plan_name}]{RESET}")
    };

    // ── Optional cwd ─────────────────────────────────────────────────
    if cfg.show_cwd {
        let cwd = data.cwd.as_deref().unwrap_or(".");
        if let Some(name) = cwd_name(cwd, cfg.cwd_max_len) {
            identity += &format!("{}{yellow_c}{name}{RESET}", sep());
        }
    }

    // ── Optional git branch ──────────────────────────────────────────
    if cfg.show_git_branch {
        let cwd = data.cwd.as_deref().unwrap_or(".");
        if let Some(branch) = git_branch(cwd) {
            let branch = compact_name(&branch, cfg.git_branch_max_len);
            identity += &format!("{}{magenta_c}{branch}{RESET}", sep());
        }
    }

    segments.push(identity);

    // ── Session duration ─────────────────────────────────────────────
    if let Some(ref cost) = data.cost {
        if let Some(ms) = cost.total_duration_ms {
            segments.push(format!("{}{NBSP}{dim}elapsed{RESET}", format_duration(ms)));
        }
    }

    // ── Context window ───────────────────────────────────────────────
    if let Some(ref ctx) = data.context_window {
        if let Some(pct) = ctx.used_percentage {
            let warn_pct = context_warn_pct() as f64;
            let color = context_color(pct);
            let ctx_bar = bar(pct, 15, color);
            let warn = if pct >= warn_pct {
                format!("{NBSP}{yellow_c}⚠{RESET}")
            } else {
                String::new()
            };
            segments.push(format!(
                "{ctx_bar}{NBSP}{color}{pct:.0}%{RESET}{warn}{NBSP}{dim}ctx{RESET}"
            ));
        }
    }

    // ── Session cost ─────────────────────────────────────────────────
    if let Some(ref cost) = data.cost {
        if let Some(usd) = cost.total_cost_usd {
            segments.push(format!("{}{NBSP}{dim}sess{RESET}", format_cost(usd)));
        }
    }

    // ── Daily cost ───────────────────────────────────────────────────
    let session_id = data.session_id.as_deref().unwrap_or("unknown");
    let session_cost = data
        .cost
        .as_ref()
        .and_then(|c| c.total_cost_usd)
        .unwrap_or(0.0);
    let daily_total = daily_cost::update_daily_cost(session_id, session_cost);
    segments.push(format!(
        "{}{NBSP}{dim}today{RESET}",
        format_cost(daily_total)
    ));

    // ── Quota usage: [5h ███░░ 55% | 7d █░░░░ 13%] quota ──────────
    if let Some(ref cred) = cred_info {
        if let Some(usage_data) = usage::fetch_usage(&cred.access_token) {
            let mut quota_parts: Vec<String> = Vec::new();

            // 5-hour quota
            if let Some(ref five) = usage_data.five_hour {
                if let Some(util) = five.utilization {
                    let color = quota_color(util);
                    let q_bar = bar(util, 5, color);
                    quota_parts.push(format!(
                        "{dim}5h{RESET}{NBSP}{q_bar}{NBSP}{color}{util:.0}%{RESET}"
                    ));
                }
            }

            // 7-day quota
            if let Some(ref seven) = usage_data.seven_day {
                if let Some(util) = seven.utilization {
                    let color = quota_color(util);
                    let q_bar = bar(util, 5, color);
                    quota_parts.push(format!(
                        "{dim}7d{RESET}{NBSP}{q_bar}{NBSP}{color}{util:.0}%{RESET}"
                    ));
                }
            }

            if !quota_parts.is_empty() {
                let inner = quota_parts.join(&format!("{NBSP}{dim}|{RESET}{NBSP}"));
                segments.push(format!(
                    "{dim}[{RESET}{inner}{dim}]{RESET}{NBSP}{dim}quota{RESET}"
                ));
            }

            // Per-model sub-bars (after the grouped quota bracket)
            if let Some(ref seven) = usage_data.seven_day {
                if seven.utilization.is_some() {
                    let sub_sep = format!("{NBSP}{dim}·{RESET}{NBSP}");
                    let mut sub_parts: Vec<String> = Vec::new();
                    for (q, label) in [
                        (usage_data.seven_day_sonnet.as_ref(), "sonnet"),
                        (usage_data.seven_day_opus.as_ref(), "opus"),
                        (usage_data.seven_day_cowork.as_ref(), "cowork"),
                        (usage_data.seven_day_oauth_apps.as_ref(), "oauth"),
                    ] {
                        if let Some(sub) = format_sub_bar(q, label) {
                            sub_parts.push(sub);
                        }
                    }
                    if !sub_parts.is_empty() {
                        segments.push(sub_parts.join(&sub_sep));
                    }
                }
            }

            // Extra usage (pay-as-you-go overage)
            if let Some(ref extra) = usage_data.extra_usage {
                if let Some(s) = format_extra_usage(extra) {
                    segments.push(s);
                }
            }
        }
    }

    // ── Claude service status ────────────────────────────────────────
    if let Some(ref st) = status::fetch_status() {
        if let Some(s) = format_status_indicator(st) {
            segments.push(s);
        }
    }

    // ── Assemble and print ───────────────────────────────────────────
    let line = segments.join(&sep());
    println!("{RESET}{}", nbsp(&line));
}
