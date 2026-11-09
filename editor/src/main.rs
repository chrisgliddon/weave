use std::process::ExitCode;

use std::path::PathBuf;

use weave_editor::{LaunchMode, launch_with_project};

fn main() -> ExitCode {
    let mut mode = LaunchMode::Desktop;
    let mut project: Option<PathBuf> = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--smoke-test" => mode = LaunchMode::HeadlessSmoke,
            "--project" => match arguments.next() {
                Some(path) => project = Some(PathBuf::from(path)),
                None => {
                    eprintln!("`--project` needs a path.");
                    return ExitCode::from(2);
                }
            },
            "-h" | "--help" => {
                println!("Usage: weave_editor [--smoke-test] [--project <path>]");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("Unknown option `{argument}`. Use --help for usage.");
                return ExitCode::from(2);
            }
        }
    }
    match launch_with_project(mode, project) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Weave Editor failed to start: {error}");
            ExitCode::FAILURE
        }
    }
}
