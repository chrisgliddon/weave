use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use tempfile::NamedTempFile;
use weave_character::{
    CharacterOverlay, CharacterProfile, CharacterSynthesisResult, CharacterTemplate,
    character_diagnostic_schema, character_overlay_schema, character_profile_schema,
    character_synthesis_schema, character_template_schema, synthesize_character,
};

#[derive(Debug, Parser)]
#[command(
    name = "weave-character",
    about = "Validate and synthesize portable Weave Character profiles"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Write one canonical Character JSON Schema.
    Schema {
        #[arg(value_enum)]
        kind: DocumentKind,
        #[arg(long)]
        output: PathBuf,
    },
    /// Strictly parse and validate one Character document.
    Validate {
        #[arg(value_enum)]
        kind: DocumentKind,
        input: PathBuf,
    },
    /// Apply one sparse overlay over an optional immutable template.
    Synthesize {
        overlay: PathBuf,
        #[arg(long)]
        template: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DocumentKind {
    Profile,
    Template,
    Overlay,
    Synthesis,
    Diagnostic,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Ron,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Schema { kind, output } => {
            let schema = match kind {
                DocumentKind::Profile => character_profile_schema()?,
                DocumentKind::Template => character_template_schema()?,
                DocumentKind::Overlay => character_overlay_schema()?,
                DocumentKind::Synthesis => character_synthesis_schema()?,
                DocumentKind::Diagnostic => character_diagnostic_schema()?,
            };
            atomic_write(&output, schema.as_bytes())?;
        }
        Command::Validate { kind, input } => {
            let source = fs::read_to_string(&input)?;
            match kind {
                DocumentKind::Profile => {
                    parse_profile(&input, &source)?;
                }
                DocumentKind::Template => {
                    parse_template(&input, &source)?;
                }
                DocumentKind::Overlay => {
                    parse_overlay(&input, &source)?;
                }
                DocumentKind::Synthesis => {
                    parse_synthesis(&input, &source)?;
                }
                DocumentKind::Diagnostic => {
                    return Err(
                        "diagnostic documents have a schema but no standalone validator".into(),
                    );
                }
            }
            println!("validated {}", input.display());
        }
        Command::Synthesize {
            overlay,
            template,
            format,
            output,
        } => {
            let overlay_source = fs::read_to_string(&overlay)?;
            let overlay = parse_overlay(&overlay, &overlay_source)?;
            let template = template
                .as_ref()
                .map(|path| {
                    fs::read_to_string(path)
                        .map_err(Box::<dyn std::error::Error>::from)
                        .and_then(|source| {
                            parse_template(path, &source)
                                .map_err(Box::<dyn std::error::Error>::from)
                        })
                })
                .transpose()?;
            let result = synthesize_character(template.as_ref(), &overlay)?;
            let serialized = match format {
                OutputFormat::Json => result.to_json()?,
                OutputFormat::Ron => result.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("synthesized {}", output.display());
        }
    }
    Ok(())
}

fn parse_profile(
    path: &Path,
    source: &str,
) -> Result<CharacterProfile, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterProfile::from_ron(source)
    } else {
        CharacterProfile::from_json(source)
    }
}

fn parse_template(
    path: &Path,
    source: &str,
) -> Result<CharacterTemplate, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterTemplate::from_ron(source)
    } else {
        CharacterTemplate::from_json(source)
    }
}

fn parse_overlay(
    path: &Path,
    source: &str,
) -> Result<CharacterOverlay, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterOverlay::from_ron(source)
    } else {
        CharacterOverlay::from_json(source)
    }
}

fn parse_synthesis(
    path: &Path,
    source: &str,
) -> Result<CharacterSynthesisResult, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterSynthesisResult::from_ron(source)
    } else {
        CharacterSynthesisResult::from_json(source)
    }
}

fn is_ron(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "ron")
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}
