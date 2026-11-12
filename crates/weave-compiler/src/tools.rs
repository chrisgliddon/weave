//! `weavec ids`, `weavec export`, and `weavec play`: writer tools that run on source.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use serde_json::{Value, json};
use weave_compiler::writer::{list_line_ids, stamp_line_ids, unreal_export};
use weave_compiler::{CompileOptions, compile};
use weave_web::{Player, PlayerFrame};

use crate::{atomic_write, print_diagnostics};

/// Most frames one `weavec play` run records before stopping.
const MAX_PLAY_STEPS: usize = 10_000;

/// Run a writer subcommand when the first argument names one.
pub fn dispatch() -> Option<ExitCode> {
    let mut arguments = std::env::args_os();
    arguments.next()?;
    let command = arguments.next()?;
    let command = command.to_str()?;
    // Name the subcommand as the program so usage and errors read `weavec play ...`.
    let rest =
        std::iter::once(std::ffi::OsString::from(format!("weavec {command}"))).chain(arguments);
    let code = match command {
        "ids" => ids(&IdsCli::parse_from(rest)),
        "export" => export(&ExportCli::parse_from(rest)),
        "play" => play(&PlayCli::parse_from(rest)),
        _ => return None,
    };
    Some(ExitCode::from(code))
}

/// List the stable id of every narrative line, or stamp missing ids into the source.
#[derive(Debug, Parser)]
#[command(name = "weavec ids")]
struct IdsCli {
    /// Write ` ^id:<default>` onto every line without an explicit id, in place.
    #[arg(long)]
    stamp: bool,
    /// Input `.weave` source file.
    input: PathBuf,
}

/// Export engine-ready files from a story.
#[derive(Debug, Parser)]
#[command(name = "weavec export")]
struct ExportCli {
    /// Write Unreal string tables, DataTable line rows, and voice scripts into this directory.
    #[arg(long, value_name = "OUTDIR", required = true)]
    unreal: PathBuf,
    /// Input `.weave` source file.
    input: PathBuf,
}

/// Play a story with a fixed choice plan and print the transcript.
#[derive(Debug, Parser)]
#[command(name = "weavec play")]
struct PlayCli {
    /// Input `.weave` source file.
    input: PathBuf,
    /// Comma-separated zero-based choice indices, used in order at each choice set.
    #[arg(long, value_delimiter = ',', value_name = "INDICES")]
    plan: Vec<usize>,
    /// Deterministic random seed.
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Print a JSON transcript instead of readable text.
    #[arg(long)]
    json: bool,
}

fn read(input: &Path) -> Result<String, u8> {
    fs::read_to_string(input).map_err(|error| {
        eprintln!("weavec: could not read {}: {error}", input.display());
        2
    })
}

fn ids(cli: &IdsCli) -> u8 {
    let source = match read(&cli.input) {
        Ok(source) => source,
        Err(code) => return code,
    };
    if cli.stamp {
        let read_only = fs::metadata(&cli.input)
            .map(|metadata| metadata.permissions().readonly())
            .unwrap_or(false);
        if read_only {
            eprintln!("weavec: {} is read-only; not stamping", cli.input.display());
            return 2;
        }
        return match stamp_line_ids(&source) {
            Ok(stamped) => {
                for (id, line) in &stamped.skipped {
                    eprintln!(
                        "warning: {}:{line}: not stamping `{id}`; a tag appended to this line would change its text (as after a trailing `\\`), so add `^id:` by hand",
                        cli.input.display()
                    );
                }
                if stamped.count > 0
                    && let Err(error) = atomic_write(&cli.input, stamped.source.as_bytes())
                {
                    eprintln!("weavec: could not write {}: {error}", cli.input.display());
                    return 2;
                }
                eprintln!(
                    "stamped {} line id(s) in {}",
                    stamped.count,
                    cli.input.display()
                );
                0
            }
            Err(diagnostics) => {
                print_diagnostics(&cli.input, &source, &diagnostics);
                1
            }
        };
    }
    match list_line_ids(&source) {
        Ok(lines) => {
            for line in lines {
                println!(
                    "{}\t{}\t{}:{}\t{}",
                    line.id,
                    line.knot,
                    line.span.line,
                    line.span.column,
                    if line.explicit { "explicit" } else { "default" }
                );
            }
            0
        }
        Err(diagnostics) => {
            print_diagnostics(&cli.input, &source, &diagnostics);
            1
        }
    }
}

fn export(cli: &ExportCli) -> u8 {
    let source = match read(&cli.input) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let export = match unreal_export(&source) {
        Ok(export) => export,
        Err(diagnostics) => {
            print_diagnostics(&cli.input, &source, &diagnostics);
            return 1;
        }
    };
    for warning in &export.warnings {
        eprintln!("warning: {warning}");
    }
    for file in &export.files {
        let destination = file
            .path
            .split('/')
            .fold(cli.unreal.clone(), |path, segment| path.join(segment));
        if let Some(parent) = destination.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            eprintln!("weavec: could not create {}: {error}", parent.display());
            return 2;
        }
        if let Err(error) = atomic_write(&destination, file.contents.as_bytes()) {
            eprintln!("weavec: could not write {}: {error}", destination.display());
            return 2;
        }
    }
    eprintln!(
        "exported {} file(s) from {} -> {}",
        export.files.len(),
        cli.input.display(),
        cli.unreal.display()
    );
    0
}

fn play(cli: &PlayCli) -> u8 {
    let source = match read(&cli.input) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let options = CompileOptions {
        source_name: Some(cli.input.display().to_string()),
    };
    let compiled = match compile(&source, &options) {
        Ok(compiled) => compiled,
        Err(error) => {
            print_diagnostics(&cli.input, &source, &error.diagnostics);
            return 1;
        }
    };
    let mut player = match Player::new(compiled.story, cli.seed) {
        Ok(player) => player,
        Err(error) => {
            eprintln!("weavec: {error}");
            return 1;
        }
    };

    let mut frames = Vec::new();
    let mut plan = cli.plan.iter().copied();
    let mut outcome = "step_limit";
    let mut error = None;
    for _ in 0..MAX_PLAY_STEPS {
        let frame = match player.advance() {
            Ok(frame) => frame,
            Err(failure) => {
                frames.push(json!({
                    "kind": "error",
                    "error": failure.error.to_string(),
                    "presentation": failure.presentation,
                }));
                outcome = "error";
                error = Some(failure.error.to_string());
                break;
            }
        };
        let mut value = serde_json::to_value(&frame).unwrap_or(Value::Null);
        match &frame {
            PlayerFrame::Line { .. } => frames.push(value),
            PlayerFrame::Ended { .. } => {
                frames.push(value);
                outcome = "ended";
                break;
            }
            PlayerFrame::Choices { choices, .. } => {
                let Some(index) = plan.next() else {
                    frames.push(value);
                    outcome = "plan_exhausted";
                    break;
                };
                if index >= choices.len() {
                    frames.push(value);
                    outcome = "error";
                    error = Some(format!(
                        "plan index {index} is out of range for {} choice(s)",
                        choices.len()
                    ));
                    break;
                }
                value["chosen"] = json!(index);
                frames.push(value);
                if let Err(failure) = player.choose(index) {
                    outcome = "error";
                    error = Some(failure.to_string());
                    break;
                }
            }
        }
    }

    if cli.json {
        let transcript = json!({
            "source": cli.input.display().to_string(),
            "seed": cli.seed,
            "plan": cli.plan,
            "outcome": outcome,
            "error": error,
            "frames": frames,
        });
        match serde_json::to_string_pretty(&transcript) {
            Ok(text) => println!("{text}"),
            Err(failure) => {
                eprintln!("weavec: {failure}");
                return 2;
            }
        }
    } else {
        for frame in &frames {
            print_frame(frame);
        }
        println!("-- {outcome}");
        if let Some(error) = &error {
            eprintln!("weavec: {error}");
        }
    }
    u8::from(outcome == "error")
}

fn print_frame(frame: &Value) {
    let text = |key: &str| frame[key].as_str().unwrap_or_default().to_owned();
    match frame["kind"].as_str() {
        Some("line") => {
            let speaker = frame["speaker"]
                .as_str()
                .map(|speaker| format!("{speaker}: "))
                .unwrap_or_default();
            println!(
                "[{}] ({}) {speaker}{}",
                text("id"),
                text("channel"),
                text("text")
            );
        }
        Some("choices") => {
            for (index, choice) in frame["choices"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                let marker = if frame["chosen"].as_u64() == Some(index as u64) {
                    '>'
                } else {
                    ' '
                };
                println!(
                    " {marker} {index}: {}",
                    choice["text"].as_str().unwrap_or_default()
                );
            }
        }
        Some("error") => println!("!! {}", text("error")),
        _ => {}
    }
}
