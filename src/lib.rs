use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct ModelPricing {
    pub input_cost_per_token: f64,
    pub output_cost_per_token: f64,
    pub cache_creation_input_token_cost: Option<f64>,
    pub cache_read_input_token_cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCache {
    pub pr_url: Option<CachedValue>,
    pub pr_status: Option<CachedValue>,
    pub git_info: Option<CachedValue>,  // Cached git branch, dir, status
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedValue {
    pub value: String,
    pub timestamp: u64,
}

lazy_static! {
    pub static ref MODEL_PRICING: HashMap<String, ModelPricing> = {
        let mut m = HashMap::new();

        // Claude 4 Opus pricing
        m.insert("claude-opus-4-1-20250805".to_string(), ModelPricing {
            input_cost_per_token: 15.0 / 1_000_000.0,
            output_cost_per_token: 75.0 / 1_000_000.0,
            cache_creation_input_token_cost: Some(18.75 / 1_000_000.0),
            cache_read_input_token_cost: Some(1.875 / 1_000_000.0),
        });

        // Claude 4 Sonnet pricing
        m.insert("claude-sonnet-4-20250514".to_string(), ModelPricing {
            input_cost_per_token: 3.0 / 1_000_000.0,
            output_cost_per_token: 15.0 / 1_000_000.0,
            cache_creation_input_token_cost: Some(3.75 / 1_000_000.0),
            cache_read_input_token_cost: Some(0.30 / 1_000_000.0),
        });

        // Claude 4.1 Sonnet pricing
        m.insert("claude-sonnet-4-1-20250905".to_string(), ModelPricing {
            input_cost_per_token: 3.0 / 1_000_000.0,
            output_cost_per_token: 15.0 / 1_000_000.0,
            cache_creation_input_token_cost: Some(3.75 / 1_000_000.0),
            cache_read_input_token_cost: Some(0.30 / 1_000_000.0),
        });

        // Claude 3.5 Haiku pricing
        m.insert("claude-haiku-3-5-20241022".to_string(), ModelPricing {
            input_cost_per_token: 1.0 / 1_000_000.0,
            output_cost_per_token: 5.0 / 1_000_000.0,
            cache_creation_input_token_cost: Some(1.25 / 1_000_000.0),
            cache_read_input_token_cost: Some(0.10 / 1_000_000.0),
        });

        m
    };
}

pub fn statusline(short_mode: bool, show_pr_status: bool) -> String {
    let input = read_input().unwrap_or_default();

    let current_dir = input
        .get("workspace")
        .and_then(|w| w.get("current_dir"))
        .and_then(|d| d.as_str());

    let model = input
        .get("model")
        .and_then(|m| m.get("display_name"))
        .and_then(|d| d.as_str());

    let model_id = input
        .get("model")
        .and_then(|m| m.get("id"))
        .and_then(|d| d.as_str());

    let transcript_path = input.get("transcript_path").and_then(|t| t.as_str());

    let session_id = input.get("session_id").and_then(|s| s.as_str());

    // Build model display
    let model_display = if let Some(model) = model {
        format!("\x1b[38;5;208m{}", model)
    } else {
        String::new()
    };

    // Build context percentage display
    let context_display = {
        let pct = get_context_pct(transcript_path);
        let pct_num: f32 = pct.parse().unwrap_or(0.0);
        let pct_color = if pct_num >= 90.0 {
            "\x1b[31m"
        } else if pct_num >= 70.0 {
            "\x1b[38;5;208m"
        } else if pct_num >= 50.0 {
            "\x1b[33m"
        } else {
            "\x1b[90m"
        };
        format!("{}{}%\x1b[0m", pct_color, pct)
    };

    // Handle non-directory cases
    let current_dir = match current_dir {
        Some(dir) => dir,
        None => return format!("\x1b[36m~\x1b[0m"),
    };

    // Initialize variables that depend on git
    let (branch, git_dir, git_status, pr_url, pr_status, display_dir) = if is_git_repo(current_dir) {
        // Check cache for git info (5 second TTL)
        let cache_key = format!("git_{}", current_dir);
        let mut cache = if let Some(session_id) = session_id {
            read_cache(session_id)
        } else {
            SessionCache { pr_url: None, pr_status: None, git_info: None }
        };
        
        let (branch, git_dir, _repo_url) = if is_cache_valid(&cache.git_info, 5) {
            // Use cached git info
            if let Some(cached) = &cache.git_info {
                if cached.value.starts_with(&cache_key) {
                    let parts: Vec<&str> = cached.value.split('|').collect();
                    if parts.len() >= 4 {
                        (parts[1].to_string(), parts[2].to_string(), parts[3].to_string())
                    } else {
                        // Cache corrupted, fetch fresh
                        fetch_git_info(current_dir, &cache_key, &mut cache, session_id)
                    }
                } else {
                    // Different directory, fetch fresh
                    fetch_git_info(current_dir, &cache_key, &mut cache, session_id)
                }
            } else {
                // No cache, fetch fresh
                fetch_git_info(current_dir, &cache_key, &mut cache, session_id)
            }
        } else {
            // Cache expired, fetch fresh
            fetch_git_info(current_dir, &cache_key, &mut cache, session_id)
        };
        
        let repo_url = exec_git("remote get-url origin", current_dir);
        let repo_name = repo_url
            .split('/')
            .next_back()
            .unwrap_or("")
            .strip_suffix(".git")
            .unwrap_or(&repo_url);

        // Smart path display logic
        let pr_url = if let Some(session_id) = session_id {
            get_pr(&branch, current_dir, session_id)
        } else {
            String::new()
        };
        let pr_status = if show_pr_status && !pr_url.is_empty() {
            if let Some(session_id) = session_id {
                get_pr_status(&branch, current_dir, session_id)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let home_projects = format!("{}/Projects/{}", home_dir(), repo_name);
        let display_dir = if short_mode {
            // In short mode, only hide path if it's the standard project location
            if current_dir == home_projects {
                String::new()
            } else {
                // Always show path if it doesn't match the expected pattern
                format!("{} ", current_dir.replace(&home_dir(), "~"))
            }
        } else {
            // Without short mode, always show the path
            format!("{} ", current_dir.replace(&home_dir(), "~"))
        };

        // Git status
        let git_status = get_git_status(current_dir);
        
        (branch, git_dir, git_status, pr_url, pr_status, display_dir)
    } else {
        // Non-git directory - just show path
        let display_dir = format!("{} ", current_dir.replace(&home_dir(), "~"));
        (String::new(), String::new(), String::new(), String::new(), String::new(), display_dir)
    };

    // Remove session summary generation - just use empty string
    let session_summary = String::new();


    // Duration display
    let duration_display = if let Some(duration) = get_session_duration(transcript_path) {
        format!("\x1b[38;5;245m{}\x1b[0m", duration)
    } else {
        String::new()
    };

    // Cost display
    let cost_display = if let Some(cost) = calculate_session_cost(transcript_path, model_id) {
        let formatted_cost = format_cost(cost);
        // Color based on cost ranges
        let cost_color = if cost < 0.10 {
            "\x1b[32m"
        }
        // Green for < $0.10
        else if cost < 1.0 {
            "\x1b[33m"
        }
        // Yellow for < $1.00
        else {
            "\x1b[31m"
        }; // Red for >= $1.00

        format!("{}{}\x1b[0m", cost_color, formatted_cost)
    } else {
        String::new()
    };

    // Format PR display with status
    let pr_display = if !pr_url.is_empty() || !pr_status.is_empty() {
        let url_part = if !pr_url.is_empty() {
            pr_url.as_str()
        } else {
            ""
        };
        let status_part = if !pr_status.is_empty() {
            pr_status.as_str()
        } else {
            ""
        };
        let separator = if !pr_url.is_empty() && !pr_status.is_empty() {
            " "
        } else {
            ""
        };
        format!("{}{}{}\x1b[0m", url_part, separator, status_part)
    } else {
        String::new()
    };

    // Build the components list
    let mut components = Vec::new();

    // Always add PR display if available
    if !pr_display.is_empty() {
        components.push(pr_display.clone());
    }

    // Always add model display
    if !model_display.is_empty() {
        components.push(model_display.clone());
    }

    // Always add context display
    if !context_display.is_empty() {
        components.push(context_display.clone());
    }

    // Add summary if available
    if !session_summary.is_empty() {
        components.push(session_summary.clone());
    }

    // Always add duration and cost if available
    if !duration_display.is_empty() {
        components.push(duration_display.clone());
    }

    if !cost_display.is_empty() {
        components.push(cost_display.clone());
    }

    // Join components with bullet separator
    let components_str = if components.is_empty() {
        String::new()
    } else {
        format!(
            " \x1b[90m• \x1b[0m{}",
            components.join(" \x1b[90m• \x1b[0m")
        )
    };

    // Format final output - ORDER: path [branch+status] • PR status • model • context size • summary • duration • cost
    if !branch.is_empty() {
        // Git repository case
        let is_worktree = git_dir.contains("/.git/worktrees/");
        if is_worktree {
            let worktree_name = display_dir.trim_end().split('/').next_back().unwrap_or("");
            let branch_display = if branch == worktree_name {
                "↟".to_string()
            } else {
                format!("{}↟", branch)
            };
            format!(
                "\x1b[36m{}\x1b[0m\x1b[35m[{}{}]\x1b[0m{}",
                display_dir, branch_display, git_status, components_str
            )
        } else if display_dir.is_empty() {
            format!(
                "\x1b[32m[{}{}]\x1b[0m{}",
                branch, git_status, components_str
            )
        } else {
            format!(
                "\x1b[36m{}\x1b[0m\x1b[32m[{}{}]\x1b[0m{}",
                display_dir, branch, git_status, components_str
            )
        }
    } else {
        // Non-git directory case - just show path with components
        format!(
            "\x1b[36m{}\x1b[0m{}",
            display_dir.trim_end(), components_str
        )
    }
}

pub fn read_input() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(serde_json::from_str(&buffer)?)
}

pub fn get_context_pct(transcript_path: Option<&str>) -> String {
    let transcript_path = match transcript_path {
        Some(path) => path,
        None => return "0".to_string(),
    };

    let data = match fs::read_to_string(transcript_path) {
        Ok(data) => data,
        Err(_) => return "0".to_string(),
    };

    let lines: Vec<&str> = data.lines().collect();
    let start = if lines.len() > 50 {
        lines.len() - 50
    } else {
        0
    };

    let mut latest_usage = None;
    let mut latest_ts = 0i64;

    for line in &lines[start..] {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if let (Some(ts), Some(usage), Some(role)) = (
                json.get("timestamp"),
                json.get("message").and_then(|m| m.get("usage")),
                json.get("message")
                    .and_then(|m| m.get("role"))
                    .and_then(|r| r.as_str()),
            ) {
                if role == "assistant" {
                    let timestamp = if let Some(ts_str) = ts.as_str() {
                        chrono::DateTime::parse_from_rfc3339(ts_str)
                            .map(|dt| dt.timestamp())
                            .unwrap_or(0)
                    } else {
                        ts.as_i64().unwrap_or(0)
                    };

                    if timestamp > latest_ts {
                        latest_ts = timestamp;
                        latest_usage = Some(usage.clone());
                    }
                }
            }
        }
    }

    if let Some(usage) = latest_usage {
        let input_tokens = usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let output_tokens = usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_read = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_creation = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let used = input_tokens + output_tokens + cache_read + cache_creation;
        let pct = ((used as f32 * 100.0) / 160000.0).min(100.0);

        if pct >= 90.0 {
            format!("{:.1}", pct)
        } else {
            format!("{}", pct.round() as u32)
        }
    } else {
        "0".to_string()
    }
}

pub fn get_pr(branch: &str, working_dir: &str, session_id: &str) -> String {
    let mut cache = read_cache(session_id);
    let cache_key = format!("pr_url_{}", branch);

    // Check if cache is valid (60s TTL)
    if is_cache_valid(&cache.pr_url, 60) {
        if let Some(cached) = &cache.pr_url {
            if cached.value.starts_with(&cache_key) {
                return cached.value.split('|').nth(1).unwrap_or("").to_string();
            }
        }
    }

    // Fetch new PR data
    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--head",
            branch,
            "--json",
            "url",
            "--jq",
            ".[0].url // \"\"",
        ])
        .current_dir(working_dir)
        .output();

    let url = match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    };

    // Cache the result
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs();
    cache.pr_url = Some(CachedValue {
        value: format!("{}|{}", cache_key, url),
        timestamp: now,
    });
    write_cache(session_id, &cache);

    url
}

pub fn get_git_status(working_dir: &str) -> String {
    let mut result = String::new();

    // Batch operation 1: Get status and branch tracking info in one call
    let status_output = exec_git("status --porcelain --branch", working_dir);
    let lines: Vec<&str> = status_output.lines().collect();
    
    // Parse branch tracking info from first line (## branch...origin/branch [ahead N, behind M])
    if let Some(first_line) = lines.first() {
        if first_line.starts_with("##") {
            // Extract ahead/behind counts from branch line
            if first_line.contains("[ahead ") {
                if let Some(ahead_str) = first_line.split("[ahead ").nth(1) {
                    if let Ok(ahead_count) = ahead_str.split(|c| c == ']' || c == ',').next().unwrap_or("").parse::<i32>() {
                        if ahead_count > 0 {
                            result.push_str(&format!(" ⇡{}", ahead_count));
                        }
                    }
                }
            }
            if first_line.contains("behind ") {
                if let Some(behind_str) = first_line.split("behind ").nth(1) {
                    if let Ok(behind_count) = behind_str.split(']').next().unwrap_or("").parse::<i32>() {
                        if behind_count > 0 {
                            result.push_str(&format!(" ⇣{}", behind_count));
                        }
                    }
                }
            }
        }
    }
    
    // Check if repository is dirty (skip first line which is branch info)
    if lines.len() > 1 {
        result.push_str(" *");
    }

    // Check git stash status (still needs separate call)
    let stash_output = exec_git("stash list", working_dir);
    if !stash_output.is_empty() {
        result.push_str(" ≡");
    }

    result
}

pub fn exec_git(args: &str, working_dir: &str) -> String {
    let output = Command::new("git")
        .args(args.split_whitespace())
        .current_dir(working_dir)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    }
}

pub fn is_git_repo(dir: &str) -> bool {
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(dir)
        .output();

    matches!(output, Ok(output) if output.status.success() &&
             String::from_utf8_lossy(&output.stdout).trim() == "true")
}

pub fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
}

// Cache helper functions
pub fn get_cache_path(session_id: &str) -> String {
    format!("/tmp/{}.json", session_id)
}

pub fn read_cache(session_id: &str) -> SessionCache {
    let cache_path = get_cache_path(session_id);
    if let Ok(content) = fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<SessionCache>(&content) {
            return cache;
        }
    }
    SessionCache {
        pr_url: None,
        pr_status: None,
        git_info: None,
    }
}

pub fn write_cache(session_id: &str, cache: &SessionCache) {
    let cache_path = get_cache_path(session_id);
    if let Ok(json) = serde_json::to_string_pretty(cache) {
        let _ = fs::write(&cache_path, json);
    }
}

pub fn is_cache_valid(cached_value: &Option<CachedValue>, ttl_seconds: u64) -> bool {
    if let Some(cached) = cached_value {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        return now - cached.timestamp < ttl_seconds;
    }
    false
}

// Get session duration from transcript
pub fn get_session_duration(transcript_path: Option<&str>) -> Option<String> {
    let transcript_path = transcript_path?;
    if !Path::new(transcript_path).exists() {
        return None;
    }

    let data = fs::read_to_string(transcript_path).ok()?;
    let lines: Vec<&str> = data.lines().filter(|l| !l.trim().is_empty()).collect();

    if lines.len() < 2 {
        return None;
    }

    let mut first_ts = None;
    let mut last_ts = None;

    // Get first timestamp
    for line in &lines {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(timestamp) = json.get("timestamp") {
                first_ts = Some(parse_timestamp(timestamp)?);
                break;
            }
        }
    }

    // Get last timestamp
    for line in lines.iter().rev() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(timestamp) = json.get("timestamp") {
                last_ts = Some(parse_timestamp(timestamp)?);
                break;
            }
        }
    }

    if let (Some(first), Some(last)) = (first_ts, last_ts) {
        let duration_ms = last - first;
        let hours = duration_ms / (1000 * 60 * 60);
        let minutes = (duration_ms % (1000 * 60 * 60)) / (1000 * 60);

        if hours > 0 {
            Some(format!("{}h{}m", hours, minutes))
        } else if minutes > 0 {
            Some(format!("{}m", minutes))
        } else {
            Some("<1m".to_string())
        }
    } else {
        None
    }
}

pub fn parse_timestamp(timestamp: &serde_json::Value) -> Option<i64> {
    if let Some(ts_str) = timestamp.as_str() {
        chrono::DateTime::parse_from_rfc3339(ts_str)
            .map(|dt| dt.timestamp_millis())
            .ok()
    } else {
        timestamp.as_i64()
    }
}


// Cached PR status lookup
pub fn calculate_session_cost(transcript_path: Option<&str>, model_id: Option<&str>) -> Option<f64> {
    let transcript_path = transcript_path?;
    let model_id = model_id?;

    // Get pricing for the model
    let pricing = MODEL_PRICING.get(model_id)?;

    // Read and parse the transcript
    let data = fs::read_to_string(transcript_path).ok()?;

    let mut total_cost = 0.0;

    // Process each line in the transcript
    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            // Handle two formats:
            // 1. Full format with message.role and message.usage
            // 2. Simplified format with type and message.usage
            let is_assistant = json
                .get("message")
                .and_then(|m| m.get("role"))
                .and_then(|r| r.as_str())
                .map(|r| r == "assistant")
                .unwrap_or_else(|| {
                    json.get("type")
                        .and_then(|t| t.as_str())
                        .map(|t| t == "assistant")
                        .unwrap_or(false)
                });

            if is_assistant {
                if let Some(usage) = json.get("message").and_then(|m| m.get("usage")) {
                    let input_tokens = usage
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as f64;
                    let output_tokens = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as f64;
                    let cache_creation = usage
                        .get("cache_creation_input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as f64;
                    let cache_read = usage
                        .get("cache_read_input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as f64;

                    // Calculate cost for this message
                    let mut message_cost = 0.0;
                    message_cost += input_tokens * pricing.input_cost_per_token;
                    message_cost += output_tokens * pricing.output_cost_per_token;

                    if let Some(cache_creation_cost) = pricing.cache_creation_input_token_cost {
                        message_cost += cache_creation * cache_creation_cost;
                    }

                    if let Some(cache_read_cost) = pricing.cache_read_input_token_cost {
                        message_cost += cache_read * cache_read_cost;
                    }

                    total_cost += message_cost;
                }
            }
        }
    }

    if total_cost > 0.0 {
        Some(total_cost)
    } else {
        None
    }
}

pub fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

// Helper function to fetch git info and cache it
fn fetch_git_info(current_dir: &str, cache_key: &str, cache: &mut SessionCache, session_id: Option<&str>) -> (String, String, String) {
    // Batch git operations: Get multiple values in one call
    let git_info = exec_git("rev-parse --show-toplevel --git-dir --abbrev-ref HEAD", current_dir);
    let git_lines: Vec<&str> = git_info.lines().collect();
    
    let git_dir = git_lines.get(1).unwrap_or(&"").to_string();
    let branch = git_lines.get(2).unwrap_or(&"").to_string();
    let repo_url = exec_git("remote get-url origin", current_dir);
    
    // Cache the result
    if let Some(session_id) = session_id {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        cache.git_info = Some(CachedValue {
            value: format!("{}|{}|{}|{}", cache_key, branch, git_dir, repo_url),
            timestamp: now,
        });
        write_cache(session_id, cache);
    }
    
    (branch, git_dir, repo_url)
}

pub fn get_pr_status(branch: &str, working_dir: &str, session_id: &str) -> String {
    let mut cache = read_cache(session_id);
    let cache_key = format!("pr_status_{}", branch);

    // Check if cache is valid (30s TTL for CI status)
    if is_cache_valid(&cache.pr_status, 30) {
        if let Some(cached) = &cache.pr_status {
            if cached.value.starts_with(&cache_key) {
                return cached.value.split('|').nth(1).unwrap_or("").to_string();
            }
        }
    }

    // Fetch and cache new PR status data
    let checks_output = Command::new("gh")
        .args(["pr", "checks", "--json", "bucket,name", "--jq", "."])
        .current_dir(working_dir)
        .output();

    let mut status = String::new();
    if let Ok(output) = checks_output {
        if output.status.success() {
            let checks_json = String::from_utf8_lossy(&output.stdout);
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&checks_json) {
                if let Some(array) = parsed.as_array() {
                    let mut groups: std::collections::HashMap<String, Vec<String>> =
                        std::collections::HashMap::new();

                    // Group checks by bucket
                    for check in array {
                        let bucket = check
                            .get("bucket")
                            .and_then(|b| b.as_str())
                            .unwrap_or("pending")
                            .to_string();
                        let name = check
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();

                        groups.entry(bucket).or_default().push(name);
                    }

                    // Format output with colors
                    if let Some(fail) = groups.get("fail") {
                        let names = fail.iter().take(3).cloned().collect::<Vec<_>>().join(",");
                        let more = if fail.len() > 3 { "..." } else { "" };
                        let count = if fail.len() > 1 {
                            fail.len().to_string()
                        } else {
                            String::new()
                        };
                        status.push_str(&format!("\x1b[31m✗{}:{}{}\x1b[0m ", count, names, more));
                    }

                    if let Some(pending) = groups.get("pending") {
                        let names = pending
                            .iter()
                            .take(3)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(",");
                        let more = if pending.len() > 3 { "..." } else { "" };
                        let count = if pending.len() > 1 {
                            pending.len().to_string()
                        } else {
                            String::new()
                        };
                        status.push_str(&format!("\x1b[33m○{}:{}{}\x1b[0m ", count, names, more));
                    }

                    if let Some(pass) = groups.get("pass") {
                        status.push_str(&format!("\x1b[32m✓{}\x1b[0m", pass.len()));
                    }
                }
            }
        }
    }

    // Cache the result
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs();
    cache.pr_status = Some(CachedValue {
        value: format!("{}|{}", cache_key, status.trim()),
        timestamp: now,
    });
    write_cache(session_id, &cache);

    status.trim().to_string()
}