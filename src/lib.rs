use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

pub fn statusline(short_mode: bool, _show_pr_status: bool) -> String {
    let input = read_input().unwrap_or_default();

    let current_dir = input
        .get("workspace")
        .and_then(|w| w.get("current_dir"))
        .and_then(|d| d.as_str());

    let model = input
        .get("model")
        .and_then(|m| m.get("display_name"))
        .and_then(|d| d.as_str());

    let transcript_path = input.get("transcript_path").and_then(|t| t.as_str());

    let output_style = input
        .get("output_style")
        .and_then(|o| o.get("name"))
        .and_then(|n| n.as_str());

    // Build model display with output style (nerd font icon: )
    let model_display = if let Some(model) = model {
        let style_suffix = match output_style {
            Some(style) => format!(" \x1b[90m({})\x1b[0m", style),
            None => String::new(),
        };
        format!("\x1b[38;5;14m\u{e26d} \x1b[38;5;208m{}{}", model, style_suffix)
    } else {
        String::new()
    };

    // Build context percentage display (nerd font icon: )
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
        format!("\x1b[38;5;13m\u{f49b} {}{}%\x1b[0m", pct_color, pct)
    };

    // Handle non-directory cases
    let current_dir = match current_dir {
        Some(dir) => dir,
        None => return format!("\x1b[36m~\x1b[0m"),
    };

    // Get branch name if in git repo
    let branch = if is_git_repo(current_dir) {
        get_git_branch(current_dir)
    } else {
        String::new()
    };

    // Smart path display logic
    let display_dir = if short_mode {
        // Fish-style path shortening
        format!("{} ", fish_shorten_path(current_dir))
    } else {
        // Full path with ~ for home
        format!("{} ", current_dir.replace(&home_dir(), "~"))
    };


    // Lines changed for branch display
    let lines_changed = if let Some(cost_obj) = input.get("cost") {
        let lines_added = cost_obj.get("total_lines_added").and_then(|l| l.as_u64()).unwrap_or(0);
        let lines_removed = cost_obj.get("total_lines_removed").and_then(|l| l.as_u64()).unwrap_or(0);

        if lines_added > 0 || lines_removed > 0 {
            format!("(\x1b[32m+{}\x1b[0m \x1b[31m-{}\x1b[0m)", lines_added, lines_removed)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Cost display from input JSON (nerd font icon: )
    let cost_display = if let Some(cost_obj) = input.get("cost") {
        if let Some(total_cost) = cost_obj.get("total_cost_usd").and_then(|c| c.as_f64()) {
            let formatted_cost = format_cost(total_cost);
            // Color based on cost ranges
            let cost_color = if total_cost < 5.0 {
                "\x1b[32m"  // Green for < $5.00
            } else if total_cost < 20.0 {
                "\x1b[33m"  // Yellow for < $20.00
            } else {
                "\x1b[31m"  // Red for >= $20.00
            };
            format!("\x1b[38;5;3m\u{f155} {}{}\x1b[0m", cost_color, formatted_cost)
        } else {
            String::new()
        }
    } else {
        String::new()
    };


    // Build the components list
    let mut components = Vec::new();

    // Always add model display
    if !model_display.is_empty() {
        components.push(model_display.clone());
    }

    // Always add context display
    if !context_display.is_empty() {
        components.push(context_display.clone());
    }

    // Always add cost if available
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

    // Format final output (nerd font icon: 󰊢 for git)
    if !branch.is_empty() {
        // Git repository case - show branch with lines changed
        if display_dir.is_empty() {
            format!(
                "\x1b[38;5;12m\u{f02a2} \x1b[32m{}{}\x1b[0m{}",
                branch, lines_changed, components_str
            )
        } else {
            format!(
                "\x1b[36m{}\x1b[0m \x1b[38;5;12m\u{f02a2} \x1b[32m{}{}\x1b[0m{}",
                display_dir.trim_end(), branch, lines_changed, components_str
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

pub fn get_git_branch(working_dir: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
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

pub fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("{:.3}", cost)
    } else {
        format!("{:.2}", cost)
    }
}

/// Fish-style path shortening: abbreviate each component to first char except last
/// e.g. ~/Developer/code/project -> ~/D/c/project
pub fn fish_shorten_path(path: &str) -> String {
    let home = home_dir();
    let path = path.replace(&home, "~");

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 1 {
        return path;
    }

    let shortened: Vec<String> = parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            if i == parts.len() - 1 || part.is_empty() || *part == "~" {
                part.to_string()
            } else {
                part.chars().next().map(|c| c.to_string()).unwrap_or_default()
            }
        })
        .collect();

    shortened.join("/")
}