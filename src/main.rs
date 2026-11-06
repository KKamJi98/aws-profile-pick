//! awspick: pick an AWS profile and set `AWS_PROFILE` in your shell.

mod cli;
mod config;
mod out;
mod py;
mod shell;
mod table;

fn main() {
    // A panic would exit 101 with a Rust backtrace hint. The Python release
    // turned every unexpected error into exit 1; keep that contract.
    std::panic::set_hook(Box::new(|info| {
        out::error(&format!("An unexpected error occurred: {info}"));
    }));
    let code = std::panic::catch_unwind(cli::run).unwrap_or(1);
    std::process::exit(code);
}
