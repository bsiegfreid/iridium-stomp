use clap::Parser;
use std::process::ExitCode;

mod cli;

use cli::args::Cli;
use cli::exit_codes;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = if cli.tui {
        cli::tui::run(&cli).await
    } else {
        cli::plain::run(&cli).await
    };

    match result {
        Ok(()) => ExitCode::from(exit_codes::SUCCESS),
        Err((message, code)) => {
            eprintln!("{}", message);
            ExitCode::from(code)
        }
    }
}
