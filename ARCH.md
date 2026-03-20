# Architecture

This document describes the internal architecture of claudash — a compiled Rust binary that produces a rich, ANSI-colored status line for [Claude Code](https://docs.anthropic.com/en/docs/claude-code).

## Overview

claudash is a single-process, synchronous CLI tool. Claude Code invokes it on every prompt render, piping session state as JSON on stdin. claudash parses this data, enriches it with cached API responses, formats everything into a single ANSI-escaped line, and prints it to stdout.

```
Claude Code                  claudash                       APIs
┌──────────┐  stdin JSON  ┌────────────┐  HTTP (cached)  ┌──────────────┐
│  session  │────────────▶│  parse +   │────────────────▶│  Anthropic   │
│  state    │             │  enrich +  │◀────────────────│  OAuth API   │
└──────────┘  stdout line │  format    │                 ├──────────────┤
       ◀──────────────────│            │────────────────▶│  Claude      │
     ANSI status line     └────────────┘                 │  Status API  │
                               │                         └──────────────┘
                               ▼
                          /tmp cache files
```

Design goals:

- **Fast**: cached network calls, no startup overhead, pure functions for formatting
- **Resilient**: silent failure on network errors, graceful degradation on missing data
- **Minimal**: three runtime dependencies (serde, serde_json, ureq), no async runtime
- **Small**: LTO + symbol stripping in release builds, pure-Rust SHA-256

## Module map

```
src/
├── main.rs            Orchestration: CLI parsing, stdin, segment assembly, stdout
├── types.rs           Data models (no dependencies — pure structs)
├── display.rs         Formatting: ANSI colors, progress bars, cost/duration formatting
├── credentials.rs     Auth: macOS Keychain + file fallback, SHA-256, plan mapping
├── profile.rs         API: /api/oauth/profile (email, org) with 1h cache
├── usage.rs           API: /api/oauth/usage (quotas, extra usage) with 60s cache
├── status.rs          API: status.claude.com (service health) with 2m cache
├── daily_cost.rs      Local: per-day cost accumulation across sessions
└── tests/
    └── mod.rs         Integration-level tests (JSON parsing, output format)
```

### Dependency graph

```
main.rs
├── types.rs
├── display.rs ──── types.rs
├── credentials.rs ── types.rs
├── profile.rs ──── credentials.rs, display.rs, types.rs
├── usage.rs ────── credentials.rs, display.rs, types.rs
├── status.rs ───── credentials.rs, display.rs, types.rs
└── daily_cost.rs ── display.rs, types.rs
```

`types.rs` is at the bottom of the dependency graph — it defines all data models with no internal imports. `display.rs` depends only on types. API modules depend on credentials (for the OAuth token) and display (for `now_secs()`). `main.rs` coordinates everything.

## Data flow

### 1. Input (stdin)

Claude Code pipes a JSON object on every status line render:

```json
{
  "session_id": "abc123",
  "cwd": "/home/user/project",
  "model": { "id": "claude-opus-4-6", "display_name": "Opus" },
  "cost": { "total_cost_usd": 0.45, "total_duration_ms": 330000 },
  "context_window": { "used_percentage": 72.0, "context_window_size": 200000 }
}
```

All fields are optional — claudash gracefully skips segments when data is missing.

### 2. Credential resolution

```
┌─────────────────────────┐
│ macOS Keychain           │◀── Primary (same store Claude Code uses)
│ service: "Claude Code-   │    Command: /usr/bin/security find-generic-password
│   credentials[-{hash}]" │
└───────────┬─────────────┘
            │ not found
            ▼
┌─────────────────────────┐
│ ~/.claude/               │◀── Fallback
│   .credentials.json      │    (or $CLAUDE_CONFIG_DIR/.credentials.json)
└──────────────────────────┘
```

The credential JSON contains:

```json
{
  "claudeAiOauth": {
    "accessToken": "sk-...",
    "subscriptionType": "pro"
  }
}
```

When `CLAUDE_CONFIG_DIR` is set, the Keychain service name gets a SHA-256 hash suffix (first 4 bytes, 8 hex chars) to support multiple Claude instances without collision. The SHA-256 is implemented in pure Rust (~90 lines in `credentials.rs`) to avoid a crypto crate dependency.

### 3. API enrichment (cached)

Three API endpoints are called, each with its own cache file and TTL strategy:

| Module | Endpoint | Auth | Cache TTL (ok) | Cache TTL (fail) |
|--------|----------|------|---------------|-----------------|
| `profile.rs` | `api.anthropic.com/api/oauth/profile` | Bearer + Beta header | 1 hour | 5 min |
| `usage.rs` | `api.anthropic.com/api/oauth/usage` | Bearer + Beta header | 60 sec | 15 sec |
| `status.rs` | `status.claude.com/api/v2/status.json` | None | 2 min | 30 sec |

All requests use a 5-second global timeout via ureq.

> **Note:** The usage API (quota bars) requires a paid Claude Code subscription (Pro, Max, or Team). Free tier, Enterprise, and API key users will not see quota data. Quota bars may also disappear silently when the usage API is temporarily unavailable or rate-limited — run with `--debug` and check `/tmp/claudash-debug.log` to diagnose.

#### Rate-limit handling (usage API)

The usage API can return HTTP 429 with a `Retry-After` header. The handler:

1. Parses `Retry-After` as integer seconds (clamped to 5min–30min range)
2. Stores the retry-after timestamp in the cache file
3. On cache read, compares against current time — won't retry until cooldown passes
4. Special case: `Retry-After: 0` triggers one immediate retry before applying default TTL

The usage module uses `http_status_as_error(false)` in ureq to inspect response headers on non-2xx status codes.

### 4. Segment assembly

`main.rs` builds a `Vec<String>` of segments, joined by a dim pipe separator (`│`). Each segment is independently formatted with ANSI escape codes.

Segment order:

| # | Segment | Source | Condition |
|---|---------|--------|-----------|
| 1 | Identity `[Model \| Plan \| email]` | stdin + credentials + profile API | Always shown |
| 1a | Working directory | stdin `cwd` | `--cwd` flag |
| 1b | Git branch | `.git/HEAD` file read | `--git-branch` flag |
| 2 | Session duration | stdin `cost.total_duration_ms` | Duration available |
| 3 | Context window bar | stdin `context_window.used_percentage` | Percentage available |
| 4 | Session cost | stdin `cost.total_cost_usd` | Cost available |
| 5 | Daily cost | daily_cost cache | Always shown |
| 6 | Quota `[5h \| 7d]` | usage API | Quota data available |
| 7 | Per-model sub-bars | usage API | 7-day quota available |
| 8 | Overage | usage API `extra_usage` | Enabled and >= 60% of limit |
| 9 | Service status | status API | Indicator != "none" |

### 5. Output (stdout)

A single line of ANSI-formatted text. All spaces are replaced with non-breaking spaces (U+00A0) to prevent terminal whitespace collapse. Claude Code reads this and renders it in the status bar area.

## Display formatting

### ANSI color palette

Colors are theme-aware — the `--light` flag switches to a 256-color palette where all accent colors are chosen at a similar dark intensity for uniform readability on white backgrounds. Each color is a function (`dim()`, `green()`, etc.) that checks a global `LIGHT_MODE` flag.

| Color | Dark mode | Light mode | Usage |
|-------|-----------|------------|-------|
| `dim` | `\x1b[2m` | `\x1b[38;5;244m` (gray) | Brackets, separators, labels |
| `green` | `\x1b[32m` | `\x1b[38;5;28m` (dark green) | Context 0–40% |
| `yellow` | `\x1b[33m` | `\x1b[38;5;130m` (dark amber) | Context 41–60%, warning icon |
| `orange` | `\x1b[38;5;208m` | `\x1b[38;5;166m` (dark orange) | Context 61–warn%, status siren |
| `red` | `\x1b[31m` | `\x1b[38;5;124m` (dark red) | Context >= warn%, quota >= 90% |
| `cyan` | `\x1b[36m` | `\x1b[38;5;30m` (dark teal) | Model name |
| `blue` | `\x1b[94m` | `\x1b[38;5;25m` (dark blue) | Quota 0–74%, overage < 80% |
| `magenta` | `\x1b[95m` | `\x1b[38;5;90m` (dark purple) | Git branch, quota 75–89% |

`RESET` (`\x1b[0m`) and `NBSP` (U+00A0) are constants that don't change with theme.

### Progress bars

Rendered with Unicode block characters:

- Filled: `█` (U+2588)
- Empty: `░` (U+2591)

The `bar(pct, width, color)` function clamps to [0, 100], calculates filled count as `round((pct / 100) * width)`, and wraps in ANSI color codes.

Two standard widths:
- Context window: **15 chars** (wider for better resolution)
- Quota bars: **5 chars** (compact for grouped display)

### Context window color zones

The context bar uses a 4-tier gradient to indicate quality degradation:

```
[▓▓▓▓▓▓░░░░░░░░░ 40%]  Green   — full capability
[▓▓▓▓▓▓▓▓▓░░░░░░ 55%]  Yellow  — quality starts to degrade
[▓▓▓▓▓▓▓▓▓▓▓░░░░ 70%]  Orange  — significant quality loss
[▓▓▓▓▓▓▓▓▓▓▓▓▓░░ 85%]  Red ⚠   — near auto-compaction
```

The warning threshold defaults to 80% (= `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` (85) minus 5). When context usage crosses this threshold, a yellow `⚠` icon appears.

### Cost formatting

- Values < $10: two decimals (`$0.45`, `$3.21`)
- Values >= $10: one decimal (`$12.3`)

### Name truncation

`compact_name(name, max_len)` truncates with a Unicode ellipsis in the middle, preserving both the start and end of the string for readability:

```
"my-very-long-branch-name"  →  "my-ve…-name"  (max_len=11)
```

## Caching strategy

All caches are JSON files in `/tmp`, keyed by a SHA-256 hash suffix derived from `CLAUDE_CONFIG_DIR` (or empty string for the default config).

```
/tmp/claudash-profile{-hash}.json    ProfileCacheEntry
/tmp/claudash-usage{-hash}.json      CacheEntry (with rate_limited + retry_after fields)
/tmp/claudash-status{-hash}.json     StatusCacheEntry
/tmp/claudash-daily-YYYY-MM-DD.json  DailyCostCache (per-calendar-day, no hash)
```

Each cache entry includes:
- `data`: the serialised API response (or null on failure)
- `timestamp`: Unix seconds when cached
- `ok`: whether the API call succeeded

The usage cache additionally tracks:
- `rate_limited`: whether the last response was HTTP 429
- `retry_after`: Unix timestamp after which retry is allowed

### Daily cost tracking

The daily cost module tracks per-session costs in a date-keyed file. On each invocation, it:

1. Loads or creates `/tmp/claudash-daily-YYYY-MM-DD.json`
2. Updates the current session's entry in a `HashMap<session_id, cost>`
3. Recalculates the total as `sessions.values().sum()`
4. Writes back

Date rollover is automatic — a new file is created when the date changes, and old files are naturally cleaned up by the OS.

## Error handling

claudash follows a "never crash, never clutter" philosophy:

- **Network failures**: cached with `ok: false`, retried after short TTL
- **Missing credentials**: authenticated API segments silently skipped
- **Parse errors**: logged to debug file (if `--debug`), segment skipped
- **Cache I/O errors**: silently ignored (re-fetch on next invocation)
- **Rate limits**: precise cooldown using `Retry-After` header timestamp

No error is ever printed to stdout — that would break the status line. When `--debug` is enabled, diagnostic messages go to `/tmp/claudash-debug{-hash}.log`.

## Build configuration

```toml
[profile.release]
strip = true    # Remove symbol tables
lto = true      # Link-time optimization for smaller binary
```

The release binary has no debug symbols, no unused code, and benefits from cross-crate inlining via LTO.

## External dependencies

| Crate | Purpose |
|-------|---------|
| `serde` + `serde_json` | JSON serialisation/deserialisation for stdin, API responses, and cache files |
| `ureq` | Synchronous HTTP client with timeout support and header inspection |

Notable non-dependencies:
- No async runtime (tokio, async-std) — synchronous I/O is simpler and sufficient
- No crypto crate — SHA-256 implemented inline (~90 lines) for a single use case
- No datetime crate — Howard Hinnant's civil date algorithm computes YYYY-MM-DD from Unix timestamp
- No libc — all system interaction through std or macOS `security` command
