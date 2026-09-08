// Cargo subcommand entry point.
//
// When invoked as `cargo gts <subcommand>`, Cargo runs the `cargo-gts` binary
// with argv = ["cargo-gts", "gts", "<subcommand>", ...]. This wrapper strips
// the extra "gts" token so that clap sees the real subcommand, then delegates
// to the same CLI logic used by the standalone `gts` binary.
//
// These Clippy lints are disabled because this is a CLI binary, not a library:
// - print_stdout/print_stderr: CLI tools are expected to print to stdout/stderr for user output.
// - exit: Calling `std::process::exit()` is standard for CLI apps to signal failure to the shell.
// - unwrap_used/expect_used: In a CLI binary, panicking on unrecoverable errors is acceptable.
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::exit)]

use clap::Parser;
use gts_cli::{Cli, run_with_cli};

#[tokio::main]
async fn main() {
    // Strip the "gts" token that Cargo injects as argv[1].
    // Direct invocation (`cargo-gts validate-json`) still works because the
    // filter only fires when argv[1] is exactly "gts".
    let args: Vec<std::ffi::OsString> = std::env::args_os()
        .enumerate()
        .filter_map(|(i, arg)| {
            if i == 1 && arg == "gts" {
                None
            } else {
                Some(arg)
            }
        })
        .collect();

    let cli = Cli::parse_from(args);

    if let Err(e) = run_with_cli(cli).await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
