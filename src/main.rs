use cc_statusline_rs::statusline;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let show_pr_status = !args.contains(&"--skip-pr-status".to_string());

    print!("{}", statusline(show_pr_status));
}

