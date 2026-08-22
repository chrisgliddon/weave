use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use semver::Version;
use weave_domain::{
    DomainPack, ModuleManifest, domain_pack_schema, module_manifest_schema, validate_manifest,
    validate_pack,
};

#[derive(Debug, Parser)]
#[command(
    name = "weave-module",
    about = "Inspect the Weave domain-module contract"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Write one canonical JSON Schema.
    Schema {
        /// Contract artifact to describe.
        #[arg(value_enum)]
        kind: ArtifactKind,
        /// Destination file.
        #[arg(long)]
        output: PathBuf,
    },
    /// Parse, validate, and canonicalize a manifest or pack.
    Normalize {
        /// Contract artifact to normalize.
        #[arg(value_enum)]
        kind: ArtifactKind,
        /// JSON or RON source file.
        input: PathBuf,
        /// Canonical output encoding.
        #[arg(long, value_enum)]
        format: OutputFormat,
        /// Destination file.
        #[arg(long)]
        output: PathBuf,
    },
    /// Validate a manifest and optional packs against one Weave release.
    Validate {
        /// Module manifest in JSON or RON.
        manifest: PathBuf,
        /// Data packs owned by the manifest.
        #[arg(long)]
        pack: Vec<PathBuf>,
        /// Weave semantic version to negotiate.
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        weave_version: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ArtifactKind {
    Manifest,
    Pack,
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
                ArtifactKind::Manifest => module_manifest_schema()?,
                ArtifactKind::Pack => domain_pack_schema()?,
            };
            write_atomic(&output, schema.as_bytes())?;
        }
        Command::Normalize {
            kind,
            input,
            format,
            output,
        } => {
            let source = fs::read_to_string(&input)?;
            let normalized = match kind {
                ArtifactKind::Manifest => {
                    let value = parse_manifest(&input, &source)?;
                    match format {
                        OutputFormat::Json => value.to_json()?,
                        OutputFormat::Ron => value.to_ron()?,
                    }
                }
                ArtifactKind::Pack => {
                    let value = parse_pack(&input, &source)?;
                    match format {
                        OutputFormat::Json => value.to_json()?,
                        OutputFormat::Ron => value.to_ron()?,
                    }
                }
            };
            write_atomic(&output, normalized.as_bytes())?;
        }
        Command::Validate {
            manifest,
            pack,
            weave_version,
        } => {
            let current = Version::parse(&weave_version)?;
            let manifest_source = fs::read_to_string(&manifest)?;
            let manifest_value = parse_manifest(&manifest, &manifest_source)?;
            validate_manifest(&manifest_value, &current)?;
            for path in &pack {
                let source = fs::read_to_string(path)?;
                let value = parse_pack(path, &source)?;
                validate_pack(&value, &manifest_value, &current)?;
            }
            println!(
                "valid domain module {}@{} ({} pack(s))",
                manifest_value.id,
                manifest_value.version,
                pack.len()
            );
        }
    }
    Ok(())
}

fn parse_manifest(path: &Path, source: &str) -> Result<ModuleManifest, weave_domain::DomainError> {
    if is_ron(path) {
        ModuleManifest::from_ron(source)
    } else {
        ModuleManifest::from_json(source)
    }
}

fn parse_pack(path: &Path, source: &str) -> Result<DomainPack, weave_domain::DomainError> {
    if is_ron(path) {
        DomainPack::from_ron(source)
    } else {
        DomainPack::from_json(source)
    }
}

fn is_ron(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("ron")
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}
