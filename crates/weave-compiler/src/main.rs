use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;

use clap::{Parser, ValueEnum};
use notify::{Event, RecursiveMode, Watcher};
use weave_compiler::{CompileOptions, compile_with_extensions, json_schema, to_json, to_ron};
use weave_core::{Diagnostic, Severity};
use weave_domain::{DomainCatalog, DomainPack, ModuleManifest};
use weave_patterns::{PackageRegistry, PackageRequirement};

#[derive(Debug, Parser)]
#[command(
    name = "weavec",
    version,
    about = "Compile Weave narrative source to runtime IR"
)]
struct Cli {
    /// Input `.weave` source file.
    #[arg(required_unless_present = "schema", conflicts_with = "schema")]
    input: Option<PathBuf>,

    /// Explicit output path. Use `-` for standard output.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Serialized output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Ron)]
    format: OutputFormat,

    /// Recompile whenever the input file changes.
    #[arg(long)]
    watch: bool,

    /// Emit the current story IR JSON Schema instead of compiling a story.
    #[arg(
        long,
        conflicts_with = "watch",
        conflicts_with = "patterns",
        conflicts_with = "pattern_registry"
    )]
    schema: bool,

    /// Installed package selector, for example `ember_omens@^1.0`. Repeat as needed.
    #[arg(
        long = "pattern",
        value_name = "ID@VERSION_REQ",
        requires = "pattern_registry"
    )]
    patterns: Vec<String>,

    /// Explicit local registry containing packages selected with `--pattern`.
    #[arg(long, value_name = "DIRECTORY", requires = "patterns")]
    pattern_registry: Option<PathBuf>,

    /// Explicit domain-module manifest artifact. Repeat to make more releases available.
    #[arg(
        long = "module-manifest",
        value_name = "FILE",
        conflicts_with = "schema"
    )]
    module_manifests: Vec<PathBuf>,

    /// Explicit domain data-pack artifact. Repeat to make more releases available.
    #[arg(long = "module-pack", value_name = "FILE", conflicts_with = "schema")]
    module_packs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Ron,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.schema {
        return ExitCode::from(emit_schema(&cli));
    }
    let Some(input) = cli.input.as_deref() else {
        eprintln!("weavec: an input file is required unless --schema is used");
        return ExitCode::from(2);
    };
    if cli.watch {
        match watch(&cli, input) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("weavec: {message}");
                ExitCode::from(2)
            }
        }
    } else {
        ExitCode::from(compile_once(&cli, input))
    }
}

fn compile_once(cli: &Cli, input: &Path) -> u8 {
    let source = match fs::read_to_string(input) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("weavec: could not read {}: {error}", input.display());
            return 2;
        }
    };
    let options = CompileOptions {
        source_name: Some(input.display().to_string()),
    };
    let external_patterns = match external_patterns(cli) {
        Ok(patterns) => patterns,
        Err(error) => {
            eprintln!("weavec: {error}");
            return 2;
        }
    };
    let domain_catalog = match domain_catalog(cli) {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("weavec: {error}");
            return 2;
        }
    };
    let compiled =
        match compile_with_extensions(&source, &options, &external_patterns, &domain_catalog) {
            Ok(compiled) => compiled,
            Err(error) => {
                print_diagnostics(input, &source, &error.diagnostics);
                return 1;
            }
        };
    print_diagnostics(input, &source, &compiled.diagnostics);

    let output = match cli.format {
        OutputFormat::Ron => to_ron(&compiled.story).map_err(|error| error.to_string()),
        OutputFormat::Json => to_json(&compiled.story).map_err(|error| error.to_string()),
    };
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            eprintln!("weavec: {error}");
            return 2;
        }
    };

    let destination = cli
        .output
        .clone()
        .unwrap_or_else(|| default_output(input, cli.format));
    if destination == Path::new("-") {
        if let Err(error) = io::stdout().write_all(output.as_bytes()) {
            eprintln!("weavec: could not write standard output: {error}");
            return 2;
        }
    } else if let Err(error) = atomic_write(&destination, output.as_bytes()) {
        eprintln!("weavec: could not write {}: {error}", destination.display());
        return 2;
    } else {
        eprintln!("compiled {} -> {}", input.display(), destination.display());
    }
    0
}

fn domain_catalog(cli: &Cli) -> Result<DomainCatalog, String> {
    let mut catalog = DomainCatalog::new();
    for path in &cli.module_manifests {
        let source = fs::read_to_string(path).map_err(|error| {
            format!("could not read domain manifest {}: {error}", path.display())
        })?;
        let manifest = if path.extension().is_some_and(|extension| extension == "ron") {
            ModuleManifest::from_ron(&source)
        } else {
            ModuleManifest::from_json(&source)
        }
        .map_err(|error| format!("could not load domain manifest {}: {error}", path.display()))?;
        catalog
            .insert_manifest(manifest)
            .map_err(|error| format!("could not catalog domain manifest: {error}"))?;
    }
    for path in &cli.module_packs {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("could not read domain pack {}: {error}", path.display()))?;
        let pack = if path.extension().is_some_and(|extension| extension == "ron") {
            DomainPack::from_ron(&source)
        } else {
            DomainPack::from_json(&source)
        }
        .map_err(|error| format!("could not load domain pack {}: {error}", path.display()))?;
        catalog
            .insert_pack(pack)
            .map_err(|error| format!("could not catalog domain pack: {error}"))?;
    }
    Ok(catalog)
}

fn external_patterns(
    cli: &Cli,
) -> Result<BTreeMap<String, weave_core::ir::PatternSystemIr>, String> {
    if cli.patterns.is_empty() {
        return Ok(BTreeMap::new());
    }
    let registry_path = cli
        .pattern_registry
        .as_ref()
        .ok_or_else(|| "--pattern requires --pattern-registry".to_owned())?;
    let registry = PackageRegistry::new(registry_path);
    let mut patterns = BTreeMap::new();
    for selector in &cli.patterns {
        let requirement = selector
            .parse::<PackageRequirement>()
            .map_err(|error| error.to_string())?;
        let package = registry
            .resolve(&requirement)
            .map_err(|error| error.to_string())?;
        let id = package.metadata.id.clone();
        let pattern = package.pattern_ir().map_err(|error| error.to_string())?;
        if patterns.insert(id.clone(), pattern).is_some() {
            return Err(format!("pattern `{id}` was selected more than once"));
        }
    }
    Ok(patterns)
}

fn emit_schema(cli: &Cli) -> u8 {
    let output = match json_schema() {
        Ok(output) => output,
        Err(error) => {
            eprintln!("weavec: {error}");
            return 2;
        }
    };
    let destination = cli.output.as_deref().unwrap_or_else(|| Path::new("-"));
    if destination == Path::new("-") {
        if let Err(error) = io::stdout().write_all(output.as_bytes()) {
            eprintln!("weavec: could not write standard output: {error}");
            return 2;
        }
    } else if let Err(error) = atomic_write(destination, output.as_bytes()) {
        eprintln!("weavec: could not write {}: {error}", destination.display());
        return 2;
    }
    0
}

fn watch(cli: &Cli, input_path: &Path) -> Result<(), String> {
    let input = absolute(input_path).map_err(|error| error.to_string())?;
    let parent = input
        .parent()
        .ok_or_else(|| "input path has no parent directory".to_owned())?;
    let (sender, receiver) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(sender).map_err(|error| error.to_string())?;
    watcher
        .watch(parent, RecursiveMode::NonRecursive)
        .map_err(|error| error.to_string())?;

    let _ = compile_once(cli, input_path);
    eprintln!("watching {}", input_path.display());
    loop {
        match receiver.recv() {
            Ok(Ok(event)) if event_targets(&event, &input) => {
                let _ = compile_once(cli, input_path);
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => eprintln!("weavec: watch error: {error}"),
            Err(error) => return Err(format!("watch channel closed: {error}")),
        }
    }
}

fn event_targets(event: &Event, input: &Path) -> bool {
    event.paths.iter().any(|path| {
        absolute(path)
            .map(|candidate| candidate == input)
            .unwrap_or_else(|_| path.file_name() == input.file_name())
    })
}

fn absolute(path: &Path) -> io::Result<PathBuf> {
    if path.exists() {
        fs::canonicalize(path)
    } else if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn default_output(input: &Path, format: OutputFormat) -> PathBuf {
    input.with_extension(match format {
        OutputFormat::Ron => "ron",
        OutputFormat::Json => "json",
    })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .map(|_| ())
}

fn print_diagnostics(path: &Path, source: &str, diagnostics: &[Diagnostic]) {
    let lines = source.lines().collect::<Vec<_>>();
    for diagnostic in diagnostics {
        let severity = match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        if let Some(span) = diagnostic.span {
            eprintln!(
                "{severity}[{}]: {}\n  --> {}:{}:{}",
                diagnostic.code,
                diagnostic.message,
                path.display(),
                span.line,
                span.column
            );
            if let Some(line) = lines.get(span.line.saturating_sub(1)) {
                eprintln!("   |\n{:>3} | {line}", span.line);
                let caret_width = span.end.saturating_sub(span.start).max(1);
                eprintln!(
                    "   | {}{}",
                    " ".repeat(span.column.saturating_sub(1)),
                    "^".repeat(caret_width.min(line.len().saturating_add(1)))
                );
            }
        } else {
            eprintln!("{severity}[{}]: {}", diagnostic.code, diagnostic.message);
        }
        if let Some(help) = &diagnostic.help {
            eprintln!("   = help: {help}");
        }
    }
}
