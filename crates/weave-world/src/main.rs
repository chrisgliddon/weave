use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use semver::Version;
use tempfile::NamedTempFile;
use weave_domain::{DomainPack, ModuleManifest};
use weave_world::{
    WorldCompositionPlan, compact_world_schema, compose_world_pack, full_world_schema,
    world_composition_schema,
};

#[derive(Debug, Parser)]
#[command(
    name = "weave-world",
    about = "Compose and describe portable Weave World artifacts"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compose reviewed broad, regional, and ecosystem layers into one ordinary domain pack.
    Compose {
        /// Versioned World composition plan in JSON or RON.
        plan: PathBuf,
        /// Owning Weave World module manifest in JSON or RON.
        #[arg(long)]
        manifest: PathBuf,
        /// Available candidate pack in JSON or RON; repeat for every plan candidate.
        #[arg(long = "pack", required = true)]
        packs: Vec<PathBuf>,
        /// Generated domain pack destination in JSON or RON.
        #[arg(long)]
        output: PathBuf,
        /// Optional composition receipt destination in JSON or RON.
        #[arg(long)]
        receipt: Option<PathBuf>,
        /// Weave semantic version used for compatibility validation.
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        weave_version: String,
    },
    /// Write one canonical portable World JSON Schema.
    Schema {
        /// Artifact contract to describe.
        #[arg(value_enum)]
        kind: SchemaKind,
        /// Destination file.
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaKind {
    Composition,
    Full,
    Compact,
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("artifact path must end in `.json` or `.ron`")]
    UnsupportedFormat,
    #[error("invalid Weave semantic version")]
    InvalidWeaveVersion,
    #[error("could not read or write a World artifact")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Domain(#[from] weave_domain::DomainError),
    #[error(transparent)]
    Composition(#[from] weave_world::WorldCompositionError),
    #[error(transparent)]
    Export(#[from] weave_world::WorldExportError),
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Compose {
            plan,
            manifest,
            packs,
            output,
            receipt,
            weave_version,
        } => {
            let plan = read_plan(&plan)?;
            let manifest = read_manifest(&manifest)?;
            let packs = packs
                .iter()
                .map(|path| read_pack(path))
                .collect::<Result<Vec<_>, _>>()?;
            let weave_version =
                Version::parse(&weave_version).map_err(|_| CliError::InvalidWeaveVersion)?;
            let composed = compose_world_pack(&plan, &manifest, packs, &weave_version)?;
            write_atomic(&output, serialize_pack(&composed.pack, &output)?)?;
            if let Some(receipt) = receipt {
                write_atomic(&receipt, serialize_receipt(&composed.receipt, &receipt)?)?;
            }
            println!(
                "composed {}@={} from {} selected layer(s)",
                composed.pack.id,
                composed.pack.version,
                composed.receipt.layers.len()
            );
        }
        Command::Schema { kind, output } => {
            let schema = match kind {
                SchemaKind::Composition => world_composition_schema(),
                SchemaKind::Full => full_world_schema(),
                SchemaKind::Compact => compact_world_schema(),
            }?;
            write_atomic(&output, schema)?;
        }
    }
    Ok(())
}

fn read_plan(path: &Path) -> Result<WorldCompositionPlan, CliError> {
    let source = fs::read_to_string(path)?;
    match artifact_format(path)? {
        ArtifactFormat::Json => Ok(WorldCompositionPlan::from_json(&source)?),
        ArtifactFormat::Ron => Ok(WorldCompositionPlan::from_ron(&source)?),
    }
}

fn read_manifest(path: &Path) -> Result<ModuleManifest, CliError> {
    let source = fs::read_to_string(path)?;
    match artifact_format(path)? {
        ArtifactFormat::Json => Ok(ModuleManifest::from_json(&source)?),
        ArtifactFormat::Ron => Ok(ModuleManifest::from_ron(&source)?),
    }
}

fn read_pack(path: &Path) -> Result<DomainPack, CliError> {
    let source = fs::read_to_string(path)?;
    match artifact_format(path)? {
        ArtifactFormat::Json => Ok(DomainPack::from_json(&source)?),
        ArtifactFormat::Ron => Ok(DomainPack::from_ron(&source)?),
    }
}

fn serialize_pack(pack: &DomainPack, path: &Path) -> Result<String, CliError> {
    match artifact_format(path)? {
        ArtifactFormat::Json => Ok(pack.to_json()?),
        ArtifactFormat::Ron => Ok(pack.to_ron()?),
    }
}

fn serialize_receipt(
    receipt: &weave_world::WorldCompositionReceipt,
    path: &Path,
) -> Result<String, CliError> {
    match artifact_format(path)? {
        ArtifactFormat::Json => Ok(receipt.to_json()?),
        ArtifactFormat::Ron => Ok(receipt.to_ron()?),
    }
}

#[derive(Debug, Clone, Copy)]
enum ArtifactFormat {
    Json,
    Ron,
}

fn artifact_format(path: &Path) -> Result<ArtifactFormat, CliError> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => Ok(ArtifactFormat::Json),
        Some("ron") => Ok(ArtifactFormat::Ron),
        _ => Err(CliError::UnsupportedFormat),
    }
}

fn write_atomic(path: &Path, contents: String) -> Result<(), CliError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(contents.as_bytes())?;
    temporary.as_file_mut().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| CliError::Io(error.error))?;
    Ok(())
}
