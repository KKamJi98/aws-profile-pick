//! Command-line flow. Mirrors `awspick/cli.py` from 2.x step for step,
//! including the order of its log lines.

use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{ArgAction, CommandFactory, Parser, ValueEnum};

use crate::config::{self, GroupOptions};
use crate::out;
use crate::py;
use crate::shell::{self, Update, UpdateError};
use crate::table;

/// Options match the argparse parser of 2.x: repeating an option keeps the
/// last value, long options may be abbreviated, `-f` and `-x` collect every
/// value given.
#[derive(Parser, Debug)]
#[command(
    name = "awspick",
    about = "AWS profile picker",
    version,
    args_override_self = true,
    infer_long_args = true
)]
pub struct Cli {
    /// Switch to this profile without showing the table (exact name, else a
    /// case-insensitive match). Filters and groups do not apply
    #[arg(value_name = "PROFILE", value_hint = clap::ValueHint::Other)]
    profile: Option<String>,

    /// Deprecated, accepted and ignored for backward compatibility.
    #[arg(long = "export-command", hide = true)]
    export_command: bool,

    /// Only show profiles that match any of these substrings (can repeat or comma-separate)
    #[arg(short = 'f', long = "filter", value_name = "FILTER", action = ArgAction::Append, allow_negative_numbers = true)]
    filter: Vec<String>,

    /// Exclude profiles that match any of these substrings (can repeat or comma-separate)
    #[arg(short = 'x', long = "exclude", value_name = "EXCLUDE", action = ArgAction::Append, allow_negative_numbers = true)]
    exclude: Vec<String>,

    /// Comma-separated group names to display (e.g., prod,dev)
    #[arg(
        short = 'g',
        long = "groups",
        value_name = "GROUPS",
        allow_negative_numbers = true
    )]
    groups: Option<String>,

    /// Custom group rules, e.g. 'preprod=preprod;prod=prod,production;dev=dev' (order matters)
    #[arg(
        long = "group-rules",
        value_name = "GROUP_RULES",
        allow_negative_numbers = true
    )]
    group_rules: Option<String>,

    /// Treat filter/exclude as regular expressions
    #[arg(long)]
    regex: bool,

    /// Make filter/exclude matching case-sensitive
    #[arg(long = "case-sensitive")]
    case_sensitive: bool,

    /// Print INFO log lines on stderr
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Print a completion script for SHELL and exit
    #[arg(long, value_name = "SHELL", value_enum)]
    completions: Option<CompletionShell>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

fn print_completions(shell: CompletionShell) {
    let shell = match shell {
        CompletionShell::Bash => clap_complete::Shell::Bash,
        CompletionShell::Zsh => clap_complete::Shell::Zsh,
        CompletionShell::Fish => clap_complete::Shell::Fish,
    };
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut Cli::command(), "awspick", &mut script);
    out::print_bytes(&script);
}

static IN_PROMPT: AtomicBool = AtomicBool::new(false);

fn install_interrupt_handler() {
    let _ = ctrlc::set_handler(|| {
        if IN_PROMPT.load(Ordering::SeqCst) {
            // 2.x caught KeyboardInterrupt only around input().
            out::err_raw("\nOperation cancelled.\n");
            out::info("Profile selection cancelled via keyboard interrupt");
            out::info("No profile selected, exiting");
            std::process::exit(1);
        }
        std::process::exit(130);
    });
}

fn home_dir() -> PathBuf {
    std::env::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

fn split_csv_many(values: &[String]) -> Vec<String> {
    let mut parts = Vec::new();
    for v in values {
        if v.is_empty() {
            continue;
        }
        for p in v.split(',') {
            let p = py::strip(p);
            if !p.is_empty() {
                parts.push(p.to_owned());
            }
        }
    }
    parts
}

/// Environment variables split the 2.x way: stripped, empty parts kept.
fn split_env(name: &str) -> Option<Vec<String>> {
    let v = std::env::var(name).ok().filter(|v| !v.is_empty())?;
    Some(v.split(',').map(|p| py::strip(p).to_owned()).collect())
}

fn env_flag(name: &str) -> bool {
    let v = std::env::var(name).unwrap_or_else(|_| "0".to_owned());
    matches!(py::lower(&v).as_str(), "1" | "true" | "yes")
}

enum Input {
    Line(String),
    Eof,
    Error(String),
}

fn read_line() -> Input {
    let mut buf = Vec::new();
    let r = io::stdin().lock().read_until(b'\n', &mut buf);
    IN_PROMPT.store(false, Ordering::SeqCst);
    match r {
        Err(e) => Input::Error(e.to_string()),
        Ok(0) => Input::Eof,
        Ok(_) => {
            if buf.last() == Some(&b'\n') {
                buf.pop();
            }
            match py::decode_utf8(&buf) {
                Ok(s) => Input::Line(s),
                Err(m) => Input::Error(m),
            }
        }
    }
}

enum Selection {
    Profile(String),
    Cancelled,
    Unexpected(String),
}

fn get_profile_selection(profiles: &[String]) -> Selection {
    loop {
        // 2.x caught KeyboardInterrupt around both the prompt and input().
        IN_PROMPT.store(true, Ordering::SeqCst);
        out::err_raw("Enter profile number or name: ");
        let line = match read_line() {
            Input::Line(l) => l,
            // End of input is a cancel, like an empty answer. 2.x let
            // EOFError escape with a traceback.
            Input::Eof => {
                out::eprintln("\nNo selection made.");
                out::info("Empty selection");
                return Selection::Cancelled;
            }
            Input::Error(m) => return Selection::Unexpected(m),
        };
        let selection = py::strip(&line);
        if selection.is_empty() {
            out::eprintln("No selection made.");
            out::info("Empty selection");
            return Selection::Cancelled;
        }
        if matches!(py::lower(selection).as_str(), "q" | "quit" | "exit") {
            out::info("User cancelled profile selection");
            return Selection::Cancelled;
        }
        if let Some(p) = config::validate_profile_selection(selection, profiles) {
            return Selection::Profile(p);
        }
        out::eprintln("Invalid selection. Please enter a valid profile number or name.");
    }
}

fn unexpected(msg: &str) -> i32 {
    out::error(&format!("An unexpected error occurred: {msg}"));
    1
}

pub fn run() -> i32 {
    install_interrupt_handler();
    let home = home_dir();
    run_with_home(&home)
}

fn run_with_home(home: &Path) -> i32 {
    // Arguments first, so --help and usage errors work without a config.
    // 2.x read the config first.
    let args = match Cli::try_parse() {
        Ok(a) => a,
        Err(e) => {
            let code = e.exit_code();
            let _ = e.print();
            return code;
        }
    };
    out::set_verbose(args.verbose);
    if let Some(shell) = args.completions {
        print_completions(shell);
        return 0;
    }

    let profiles = config::read_aws_profiles(home);
    if profiles.is_empty() {
        out::error("No AWS profiles found. Please check your AWS configuration.");
        return 1;
    }

    if let Some(name) = &args.profile {
        return match config::find_profile(name, &profiles) {
            Ok(p) => switch_to(home, &p),
            Err(msg) => {
                out::error(&msg);
                1
            }
        };
    }

    let include = {
        let v = split_csv_many(&args.filter);
        if v.is_empty() {
            split_env("AWSPICK_FILTER").unwrap_or_default()
        } else {
            v
        }
    };
    let exclude = {
        let v = split_csv_many(&args.exclude);
        if v.is_empty() {
            split_env("AWSPICK_EXCLUDE").unwrap_or_default()
        } else {
            v
        }
    };
    let groups_to_show: Option<Vec<String>> = match args.groups.as_deref() {
        Some(g) if !g.is_empty() => Some(
            g.split(',')
                .map(py::strip)
                .filter(|p| !p.is_empty())
                .map(str::to_owned)
                .collect(),
        ),
        _ => split_env("AWSPICK_GROUPS_SHOW"),
    };
    let env_rules = std::env::var("AWSPICK_GROUP_RULES").ok();
    let group_rules = match args.group_rules.as_deref() {
        Some(r) if !r.is_empty() => Some(r.to_owned()),
        _ => env_rules,
    };
    let use_regex = args.regex || env_flag("AWSPICK_REGEX");
    let case_sensitive = args.case_sensitive || env_flag("AWSPICK_CASE_SENSITIVE");

    let opts = GroupOptions {
        group_rules: group_rules.as_deref(),
        show_groups: groups_to_show.as_deref(),
        include: Some(&include),
        exclude: Some(&exclude),
        regex: use_regex,
        case_sensitive,
    };
    let grouped = match config::get_grouped_profiles(&profiles, &opts) {
        Ok(g) => g,
        Err(msg) => {
            // 2.x let re.error escape with a traceback.
            out::error(&msg);
            return 1;
        }
    };
    let current = shell::get_current_profile(home);
    table::display(&grouped, current.as_deref());

    let selection_profiles: Vec<String> = grouped.iter().map(|(p, _)| p.clone()).collect();
    let profile = match get_profile_selection(&selection_profiles) {
        Selection::Profile(p) => p,
        Selection::Cancelled => {
            out::info("No profile selected, exiting");
            return 1;
        }
        Selection::Unexpected(m) => return unexpected(&m),
    };
    switch_to(home, &profile)
}

/// Write the selection to the rc file and the shared profile file, and print
/// the export line.
fn switch_to(home: &Path, profile: &str) -> i32 {
    out::eprintln(&format!("Selected profile: {profile}"));

    let shell_name = shell::detect_shell();
    let rc = shell::get_rc_path(home, Some(&shell_name));

    let backup = match shell::update_aws_profile(home, profile, &shell_name) {
        Ok(Update::Unchanged) => None,
        Ok(Update::Written(b)) => Some(b),
        Err(UpdateError::Failed) => {
            out::error("Failed to update AWS profile.");
            return 1;
        }
        Err(UpdateError::Unexpected) => return 1,
    };
    if let Some(b) = backup {
        out::eprintln(&format!("Backup created at {}", b.display()));
    }
    out::eprintln(&format!(
        "Updated {} with AWS_PROFILE={profile}",
        rc.rc_path.display()
    ));

    match shell::write_shared_profile(home, profile) {
        Ok(p) => out::eprintln(&format!("Updated shared profile at {}", p.display())),
        Err(_) => out::eprintln(
            "Warning: failed to write shared profile file; other shells may not sync.",
        ),
    }

    let export_cmd = shell::generate_export_command(home, profile, &shell_name);
    out::println(&export_cmd);
    out::eprintln("Run 'eval \"$(awspick)\"' to apply in the current shell");
    0
}
