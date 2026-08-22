use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use weave_patterns::{PackageRegistry, community_package_schema, load_package, publish_package};

#[derive(Debug, Parser)]
#[command(
    name = "weave-pattern",
    version,
    about = "Validate, publish, install, and discover data-only Weave pattern packages"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate one package without writing anything.
    Validate {
        /// Community package JSON file.
        package: PathBuf,
    },
    /// Stage a canonical versioned artifact and SHA-256 sidecar.
    Publish {
        /// Community package JSON file.
        package: PathBuf,
        /// Publication staging directory.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Install a package into a deterministic local registry.
    Install {
        /// Community package JSON file.
        package: PathBuf,
        /// Registry root.
        #[arg(long)]
        registry: PathBuf,
    },
    /// List validated packages installed in a local registry.
    List {
        /// Registry root.
        #[arg(long)]
        registry: PathBuf,
    },
    /// Generate a portable JSON discovery index for a local registry.
    Index {
        /// Registry root.
        #[arg(long)]
        registry: PathBuf,
        /// Index destination, or `-` for standard output.
        #[arg(short, long, default_value = "-")]
        output: PathBuf,
    },
    /// Emit the current community package JSON Schema.
    Schema {
        /// Schema destination, or `-` for standard output.
        #[arg(short, long, default_value = "-")]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("weave-pattern: {error}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Validate { package } => {
            let package = load_package(package)?;
            println!(
                "valid {}@{}: {} elements, {} spreads",
                package.metadata.id,
                package.metadata.version,
                package.pattern.elements.len(),
                package.pattern.spreads.len()
            );
        }
        Command::Publish { package, output } => {
            let published = publish_package(package, output)?;
            println!(
                "published {} (sha256 {})",
                published.artifact.display(),
                published.sha256
            );
        }
        Command::Install { package, registry } => {
            let installed = PackageRegistry::new(registry).install(package)?;
            println!(
                "installed {}@{} (sha256 {})",
                installed.id, installed.version, installed.sha256
            );
        }
        Command::List { registry } => {
            for package in PackageRegistry::new(registry).discover()? {
                println!(
                    "{}@{}\t{}\t{}\t{}",
                    package.id, package.version, package.license, package.sha256, package.title
                );
            }
        }
        Command::Index { registry, output } => {
            let index = PackageRegistry::new(registry).index()?.to_json()?;
            write_output(&output, index.as_bytes())?;
        }
        Command::Schema { output } => {
            let schema = community_package_schema()?;
            write_output(&output, schema.as_bytes())?;
        }
    }
    Ok(())
}

fn write_output(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if path == Path::new("-") {
        return io::stdout().write_all(bytes);
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_command_defaults_to_stdout() {
        let cli = Cli::try_parse_from(["weave-pattern", "schema"]).expect("parse command");
        assert!(matches!(
            cli.command,
            Command::Schema { output } if output == Path::new("-")
        ));
    }
}
