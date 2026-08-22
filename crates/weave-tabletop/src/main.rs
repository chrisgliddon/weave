use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use tempfile::NamedTempFile;
use weave_tabletop::{
    AdapterManifest, AdapterSelection, HostAudience, ResolutionReceipt, ResolutionRequest,
    TabletopCharacterProjection, TabletopError, TabletopState, adapter_manifest_schema,
    adapter_selection_schema, canonical_fingerprint, character_projection_schema,
    project_receipt_for_audience, resolution_receipt_schema, resolution_request_schema,
    tabletop_state_schema, validate_adapter_manifest, validate_adapter_selection,
    validate_character_projection, validate_resolution_receipt_for_request,
    validate_resolution_request, validate_tabletop_state, verify_adapter_source,
};

#[derive(Debug, Parser)]
#[command(
    name = "weave-tabletop",
    about = "Validate portable tabletop adapter contracts and source policy"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate one contract artifact without mutating it.
    Validate {
        #[arg(value_enum)]
        kind: ArtifactKind,
        input: PathBuf,
        /// Exact adapter manifest; repeat only for selection/projection validation.
        #[arg(long = "manifest")]
        manifests: Vec<PathBuf>,
        /// Exact before-state required when validating a resolver request.
        #[arg(long)]
        state: Option<PathBuf>,
        /// Exact request required when validating a resolver receipt.
        #[arg(long)]
        request: Option<PathBuf>,
    },
    /// Verify the source allowlist plus exact source and license-text hashes.
    LicenseGate {
        manifest: PathBuf,
        #[arg(long)]
        source_artifact: PathBuf,
        #[arg(long)]
        license_text: PathBuf,
    },
    /// Print the stable SHA-256 embedded by project selections.
    Fingerprint { manifest: PathBuf },
    /// Emit one canonical JSON Schema atomically.
    Schema {
        #[arg(value_enum)]
        kind: SchemaKind,
        #[arg(long)]
        output: PathBuf,
    },
    /// Redact event payloads for one host audience while retaining audit envelopes.
    ProjectEvents {
        receipt: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long, value_enum)]
        audience: Audience,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ArtifactKind {
    Manifest,
    Selection,
    Projection,
    State,
    Request,
    Receipt,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaKind {
    Manifest,
    Selection,
    Projection,
    State,
    Request,
    Receipt,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Audience {
    Runtime,
    Authoring,
    AuthorityHost,
}

impl From<Audience> for HostAudience {
    fn from(value: Audience) -> Self {
        match value {
            Audience::Runtime => Self::Runtime,
            Audience::Authoring => Self::Authoring,
            Audience::AuthorityHost => Self::AuthorityHost,
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("artifact path must end in `.json` or `.ron`")]
    UnsupportedFormat,
    #[error("required companion artifact was not supplied")]
    MissingCompanion,
    #[error("could not read or write a tabletop artifact")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Contract(#[from] TabletopError),
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Validate {
            kind,
            input,
            manifests,
            state,
            request,
        } => {
            let manifests = manifests
                .iter()
                .map(|path| read_manifest(path))
                .collect::<Result<Vec<_>, _>>()?;
            match kind {
                ArtifactKind::Manifest => validate_adapter_manifest(&read_manifest(&input)?)?,
                ArtifactKind::Selection => {
                    validate_adapter_selection(&read_selection(&input)?, &manifests)?
                }
                ArtifactKind::Projection => {
                    validate_character_projection(&read_projection(&input)?, &manifests)?
                }
                ArtifactKind::State => {
                    validate_tabletop_state(&read_state(&input)?, exactly_one(&manifests)?)?
                }
                ArtifactKind::Request => {
                    let before = read_state(state.as_deref().ok_or(CliError::MissingCompanion)?)?;
                    validate_resolution_request(
                        &read_request(&input)?,
                        &before,
                        exactly_one(&manifests)?,
                    )?
                }
                ArtifactKind::Receipt => validate_resolution_receipt_for_request(
                    &read_receipt(&input)?,
                    exactly_one(&manifests)?,
                    &read_request(request.as_deref().ok_or(CliError::MissingCompanion)?)?,
                )?,
            }
            println!("valid tabletop {}", kind_name(kind));
        }
        Command::LicenseGate {
            manifest,
            source_artifact,
            license_text,
        } => {
            let manifest = read_manifest(&manifest)?;
            let source = fs::read(source_artifact)?;
            let license = fs::read(license_text)?;
            verify_adapter_source(&manifest, &source, &license)?;
            println!("adapter source policy passed");
        }
        Command::Fingerprint { manifest } => {
            let manifest = read_manifest(&manifest)?;
            validate_adapter_manifest(&manifest)?;
            println!("{}", canonical_fingerprint(&manifest)?);
        }
        Command::Schema { kind, output } => {
            let schema = match kind {
                SchemaKind::Manifest => adapter_manifest_schema(),
                SchemaKind::Selection => adapter_selection_schema(),
                SchemaKind::Projection => character_projection_schema(),
                SchemaKind::State => tabletop_state_schema(),
                SchemaKind::Request => resolution_request_schema(),
                SchemaKind::Receipt => resolution_receipt_schema(),
            }?;
            write_atomic(&output, schema)?;
        }
        Command::ProjectEvents {
            receipt,
            manifest,
            audience,
            output,
        } => {
            let manifest = read_manifest(&manifest)?;
            let receipt = read_receipt(&receipt)?;
            let projected = project_receipt_for_audience(&receipt, &manifest, audience.into())?;
            write_atomic(&output, serialize_receipt(&projected, &output)?)?;
        }
    }
    Ok(())
}

fn exactly_one(manifests: &[AdapterManifest]) -> Result<&AdapterManifest, CliError> {
    match manifests {
        [manifest] => Ok(manifest),
        _ => Err(CliError::MissingCompanion),
    }
}

fn kind_name(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Manifest => "manifest",
        ArtifactKind::Selection => "selection",
        ArtifactKind::Projection => "projection",
        ArtifactKind::State => "state",
        ArtifactKind::Request => "request",
        ArtifactKind::Receipt => "receipt",
    }
}

fn read_manifest(path: &Path) -> Result<AdapterManifest, CliError> {
    parse(path, AdapterManifest::from_json, AdapterManifest::from_ron)
}

fn read_selection(path: &Path) -> Result<AdapterSelection, CliError> {
    parse(
        path,
        AdapterSelection::from_json,
        AdapterSelection::from_ron,
    )
}

fn read_projection(path: &Path) -> Result<TabletopCharacterProjection, CliError> {
    parse(
        path,
        TabletopCharacterProjection::from_json,
        TabletopCharacterProjection::from_ron,
    )
}

fn read_state(path: &Path) -> Result<TabletopState, CliError> {
    parse(path, TabletopState::from_json, TabletopState::from_ron)
}

fn read_request(path: &Path) -> Result<ResolutionRequest, CliError> {
    parse(
        path,
        ResolutionRequest::from_json,
        ResolutionRequest::from_ron,
    )
}

fn read_receipt(path: &Path) -> Result<ResolutionReceipt, CliError> {
    parse(
        path,
        ResolutionReceipt::from_json,
        ResolutionReceipt::from_ron,
    )
}

fn parse<T>(
    path: &Path,
    json: fn(&str) -> Result<T, TabletopError>,
    ron: fn(&str) -> Result<T, TabletopError>,
) -> Result<T, CliError> {
    let source = fs::read_to_string(path)?;
    match artifact_format(path)? {
        ArtifactFormat::Json => Ok(json(&source)?),
        ArtifactFormat::Ron => Ok(ron(&source)?),
    }
}

fn serialize_receipt(receipt: &ResolutionReceipt, path: &Path) -> Result<String, CliError> {
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
