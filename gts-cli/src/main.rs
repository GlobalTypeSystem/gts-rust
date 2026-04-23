// These Clippy lints are disabled because this is a CLI binary, not a library:
// - print_stdout/print_stderr: CLI tools are expected to print to stdout/stderr for user output.
// - exit: Calling `std::process::exit()` is standard for CLI apps to signal failure to the shell.
// - unwrap_used/expect_used: In a CLI binary, panicking on unrecoverable errors is acceptable.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::exit,
    clippy::unwrap_used,
    clippy::expect_used
)]

mod cli;
mod gen_common;
mod gen_instances;
mod gen_schemas;
mod logging;
mod server;

#[tokio::main]
async fn main() {
    if let Err(e) = cli::run().await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
