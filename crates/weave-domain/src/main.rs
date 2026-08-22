use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use semver::Version;
use weave_domain::{
    DomainPack, DomainRegistry, ModuleManifest, domain_lock_schema, domain_pack_schema,
    domain_project_schema, domain_registry_index_schema, load_domain_project,
    module_manifest_schema, validate_manifest, validate_pack,
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
        kind: NormalizableArtifactKind,
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
    /// Validate and stage one immutable release in portable registry layout.
    Publish {
        /// Module manifest in JSON or RON.
        manifest: PathBuf,
        /// Data packs owned by the manifest.
        #[arg(long)]
        pack: Vec<PathBuf>,
        /// Empty or existing publication directory.
        #[arg(long, value_name = "DIRECTORY")]
        output: PathBuf,
        /// Weave semantic version to negotiate.
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        weave_version: String,
    },
    /// Install one immutable release in a local registry.
    Install {
        /// Module manifest in JSON or RON.
        manifest: PathBuf,
        /// Data packs owned by the manifest.
        #[arg(long)]
        pack: Vec<PathBuf>,
        /// Local registry directory.
        #[arg(long, value_name = "DIRECTORY")]
        registry: PathBuf,
        /// Weave semantic version to negotiate.
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        weave_version: String,
    },
    /// Print the canonical JSON index for an installed registry.
    List {
        /// Local registry directory.
        #[arg(long, value_name = "DIRECTORY")]
        registry: PathBuf,
        /// Weave semantic version to negotiate.
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        weave_version: String,
    },
    /// Write the canonical portable index for an installed registry.
    Index {
        /// Local registry directory.
        #[arg(long, value_name = "DIRECTORY")]
        registry: PathBuf,
        /// Destination JSON file.
        #[arg(long)]
        output: PathBuf,
        /// Weave semantic version to negotiate.
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        weave_version: String,
    },
    /// Validate a project configuration and every artifact it discovers.
    ValidateProject {
        /// `weave.modules.json` project configuration.
        project: PathBuf,
        /// Weave semantic version to negotiate.
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        weave_version: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ArtifactKind {
    Manifest,
    Pack,
    Project,
    Lock,
    Registry,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum NormalizableArtifactKind {
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
                ArtifactKind::Project => domain_project_schema()?,
                ArtifactKind::Lock => domain_lock_schema()?,
                ArtifactKind::Registry => domain_registry_index_schema()?,
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
                NormalizableArtifactKind::Manifest => {
                    let value = parse_manifest(&input, &source)?;
                    match format {
                        OutputFormat::Json => value.to_json()?,
                        OutputFormat::Ron => value.to_ron()?,
                    }
                }
                NormalizableArtifactKind::Pack => {
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
        Command::Publish {
            manifest,
            pack,
            output,
            weave_version,
        } => {
            install_release("published", &output, &manifest, &pack, &weave_version)?;
        }
        Command::Install {
            manifest,
            pack,
            registry,
            weave_version,
        } => {
            install_release("installed", &registry, &manifest, &pack, &weave_version)?;
        }
        Command::List {
            registry,
            weave_version,
        } => {
            let current = Version::parse(&weave_version)?;
            let snapshot = DomainRegistry::new(registry).discover(&current)?;
            print!("{}", snapshot.index.to_json()?);
        }
        Command::Index {
            registry,
            output,
            weave_version,
        } => {
            let current = Version::parse(&weave_version)?;
            let snapshot = DomainRegistry::new(registry).discover(&current)?;
            write_atomic(&output, snapshot.index.to_json()?.as_bytes())?;
        }
        Command::ValidateProject {
            project,
            weave_version,
        } => {
            let current = Version::parse(&weave_version)?;
            let project = load_domain_project(project, &current)?;
            println!(
                "valid domain project ({} module release(s), {} pack release(s))",
                project.catalog().manifests().count(),
                project.catalog().packs().count()
            );
        }
    }
    Ok(())
}

fn install_release(
    verb: &str,
    registry: &Path,
    manifest: &Path,
    packs: &[PathBuf],
    weave_version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let current = Version::parse(weave_version)?;
    let installed = DomainRegistry::new(registry).install(manifest, packs, &current)?;
    println!(
        "{verb} domain module {}@{} sha256={} ({} pack(s))",
        installed.module_id,
        installed.module_version,
        installed.manifest_sha256,
        installed.packs.len()
    );
    for pack in installed.packs {
        println!(
            "{verb} domain pack {}@{} sha256={}",
            pack.id, pack.version, pack.sha256
        );
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
