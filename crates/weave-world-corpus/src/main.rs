use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use weave_world_corpus::{corpus_index_schema, corpus_preset_schema, process_corpus};

#[derive(Debug, Parser)]
#[command(
    name = "weave-world-corpus",
    about = "Build provenance-aware Weave World packs without network access"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Write one canonical corpus JSON Schema.
    Schema {
        /// Corpus artifact to describe.
        #[arg(value_enum)]
        kind: SchemaKind,
        /// Destination file.
        #[arg(long)]
        output: PathBuf,
    },
    /// Validate local records and write every canonical domain pack.
    Build {
        /// Versioned corpus index.
        index: PathBuf,
        /// Owning Weave World module manifest.
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Validate records and require every checked pack to be byte-exact.
    Check {
        /// Versioned corpus index.
        index: PathBuf,
        /// Owning Weave World module manifest.
        #[arg(long)]
        manifest: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaKind {
    Index,
    Preset,
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
                SchemaKind::Index => corpus_index_schema()?,
                SchemaKind::Preset => corpus_preset_schema()?,
            };
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(output, schema)?;
        }
        Command::Build { index, manifest } => {
            let count = process_corpus(&index, &manifest, false)?;
            println!("built {count} offline Weave World preset(s)");
        }
        Command::Check { index, manifest } => {
            let count = process_corpus(&index, &manifest, true)?;
            println!("checked {count} offline Weave World preset(s)");
        }
    }
    Ok(())
}
