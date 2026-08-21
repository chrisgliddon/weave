use std::process::ExitCode;

use weave_editor::{LaunchMode, launch};

fn main() -> ExitCode {
    let mut mode = LaunchMode::Desktop;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--smoke-test" => mode = LaunchMode::HeadlessSmoke,
            "-h" | "--help" => {
                println!("Usage: weave_editor [--smoke-test]");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("Unknown option `{argument}`. Use --help for usage.");
                return ExitCode::from(2);
            }
        }
    }
    match launch(mode) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Weave Editor failed to start: {error}");
            ExitCode::FAILURE
        }
    }
}
