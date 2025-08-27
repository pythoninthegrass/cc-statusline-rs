# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust-based statusline generator for Claude Code that creates rich terminal status displays with git information, model context usage, session details, and cost tracking. The tool processes JSON input from stdin and outputs a formatted statusline with ANSI color codes.

## Key Commands

### Build and Run

```bash
cargo build --release
cargo run
```

### Installation

```bash
make install    # Build and install to ~/.claude/cc-statusline-rs
```

### Testing with sample data

```bash
cargo run < test.json
make test              # Same as above
```

**Important**: Never generate test JSON files. Always look for existing ones in `~/.claude` directory for testing.

### Development

```bash
cargo check    # Quick syntax/type checking
cargo clippy   # Linting
cargo fmt      # Code formatting
make check     # Runs all three checks above
make fmt       # Format code
```

## Architecture

### Core Components

**src/main.rs**: Entry point that handles command-line arguments (`--short`, `--skip-pr-status`) and calls the main statusline function.

**statusline() function**: The main orchestrator that:

1. Parses JSON input containing workspace, model, and session information
2. Determines display strategy based on directory type (non-git, git repo, worktree)
3. Assembles final output string with proper color coding and formatting

### Key Features

**Smart Path Display**: Shows abbreviated paths in `--short` mode, hiding standard project locations (`~/Projects/{repo_name}`) but always showing non-standard paths.

**Git Integration**:

- Detects git repositories and worktrees
- Shows branch names with special `↟` indicator for worktrees
- Displays git status with file change counts (+, ~, -, ?) and line deltas (Δ)

**Context Management**:

- Parses transcript files to calculate context usage percentage
- Color-codes context percentage (red ≥90%, orange ≥70%, yellow ≥50%, gray <50%)
- Handles both string and numeric timestamp formats
- Uses 160,000 tokens as the maximum context limit

**Cost and Line Change Tracking**:

- Displays total cost in USD from the input JSON
- Color-codes cost display (green <$5, yellow <$20, red ≥$20)
- Shows lines added/removed from `cost.total_lines_added` and `cost.total_lines_removed`
- Formats costs with appropriate decimal places based on amount

**Session Analysis**:

- Extracts session duration from transcript timestamps
- Calculates duration between first and last timestamp in transcript
- Displays duration in hours/minutes format (e.g., "2h30m", "45m", "<1m")

### Display Format

The output components are displayed in this order: `path [branch] • model • context% • duration • +lines -lines • $cost`

Example outputs:
- Git repo: `[main] • Opus • 45% • 15m • +156 -23 • $7.50`
- Git repo with path: `~/project [main] • Opus • 45% • 15m • +156 -23 • $7.50`
- Non-git directory: `~/Downloads • Opus • 12% • 5m • +10 -2 • $0.50`
- Short mode in standard location: `[main] • Opus • 45% • 15m` (path hidden when in ~/Projects/repo_name)

Note: The current implementation has removed PR status display features mentioned in some comments.

### Dependencies

- **serde_json**: JSON parsing for input data and transcript analysis
- **chrono**: Timestamp parsing and duration calculations
- **External tools**: Requires `git` and optionally `gh` CLI for full functionality

### Input Format

Expects JSON on stdin with fields:

- `workspace.current_dir`: Working directory path
- `model.display_name`: AI model name for display
- `transcript_path`: Path to conversation transcript file
- `session_id`: Unique session identifier
- `cost.total_cost_usd`: Total cost in USD
- `cost.total_lines_added`: Total lines added count
- `cost.total_lines_removed`: Total lines removed count

The `test.json` file shows the expected input structure for development and testing.

### Code Structure

- **src/lib.rs**: Contains all core functionality including:
  - `statusline()`: Main entry point that orchestrates all display logic
  - `get_context_pct()`: Parses transcript to calculate context usage
  - `get_session_duration()`: Calculates session duration from transcript
  - `format_cost()`: Formats cost display with appropriate decimals
  - Git detection and branch name utilities
- **src/main.rs**: Minimal CLI wrapper that handles arguments and calls lib

### Implementation Notes

- The `--skip-pr-status` flag is accepted but currently has no effect (PR features removed)
- All ANSI color codes are hardcoded inline rather than using a color library
- Context percentage calculation assumes a 160,000 token limit
- Timestamps in transcripts can be either RFC3339 strings or numeric milliseconds
