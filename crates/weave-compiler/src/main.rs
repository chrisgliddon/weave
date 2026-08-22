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
use weave_domain::{
    DOMAIN_PROJECT_FILE_NAME, DomainCatalog, DomainPack, DomainRegistry, LoadedDomainProject,
    ModuleManifest, load_adjacent_domain_project, load_domain_project,
};
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

    /// Explicit domain-module project configuration. By default, `weave.modules.json` beside the
    /// input is discovered when no other module source is supplied.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "schema",
        conflicts_with_all = ["module_manifests", "module_packs", "module_registries"]
    )]
    module_project: Option<PathBuf>,

    /// Explicit installed domain-module registry. Repeat to combine registries.
    #[arg(
        long = "module-registry",
        value_name = "DIRECTORY",
        conflicts_with = "schema"
    )]
    module_registries: Vec<PathBuf>,

    /// Require the existing project `weave.lock` to match exactly instead of updating it.
    #[arg(long, conflicts_with = "schema")]
    locked: bool,
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
    let domain_inputs = match domain_inputs(cli, input) {
        Ok(inputs) => inputs,
        Err(error) => {
            eprintln!("weavec: {error}");
            return 2;
        }
    };
    let compiled = match compile_with_extensions(
        &source,
        &options,
        &external_patterns,
        &domain_inputs.catalog,
    ) {
        Ok(compiled) => compiled,
        Err(error) => {
            print_diagnostics(input, &source, &error.diagnostics);
            return 1;
        }
    };
    print_diagnostics(input, &source, &compiled.diagnostics);

    let lock = if let Some(project) = &domain_inputs.project {
        let lock = match project.lock_for(&compiled.domain_graph) {
            Ok(lock) => lock,
            Err(error) => {
                eprintln!("weavec: could not build domain lock: {error}");
                return 2;
            }
        };
        if cli.locked
            && let Err(error) = project.verify_lock(&lock)
        {
            eprintln!("weavec: locked domain build failed: {error}");
            return 2;
        }
        Some(lock)
    } else if cli.locked {
        eprintln!("weavec: --locked requires a domain project configuration");
        return 2;
    } else {
        None
    };

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

    if !cli.locked
        && let (Some(project), Some(lock)) = (&domain_inputs.project, &lock)
        && let Err(error) = project.write_lock(lock)
    {
        eprintln!("weavec: could not write domain lock: {error}");
        return 2;
    }
    0
}

struct DomainInputs {
    catalog: DomainCatalog,
    project: Option<LoadedDomainProject>,
}

fn domain_inputs(cli: &Cli, input: &Path) -> Result<DomainInputs, String> {
    let current_weave = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| "compiler compatibility version is invalid".to_owned())?;
    if let Some(project_path) = &cli.module_project {
        let project = load_domain_project(project_path, &current_weave)
            .map_err(|error| format!("could not load domain project: {error}"))?;
        return Ok(DomainInputs {
            catalog: project.catalog().clone(),
            project: Some(project),
        });
    }

    let has_explicit_sources = !cli.module_manifests.is_empty()
        || !cli.module_packs.is_empty()
        || !cli.module_registries.is_empty();
    if !has_explicit_sources {
        let project = load_adjacent_domain_project(input, &current_weave)
            .map_err(|error| format!("could not load adjacent domain project: {error}"))?;
        if project.config_path().exists() {
            return Ok(DomainInputs {
                catalog: project.catalog().clone(),
                project: Some(project),
            });
        }
    }

    let mut catalog = DomainCatalog::new();
    for path in &cli.module_registries {
        let snapshot = DomainRegistry::new(path)
            .discover(&current_weave)
            .map_err(|error| format!("could not discover domain registry: {error}"))?;
        catalog
            .extend(snapshot.catalog)
            .map_err(|error| format!("could not combine domain registries: {error}"))?;
    }
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
    Ok(DomainInputs {
        catalog,
        project: None,
    })
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
        .watch(parent, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;
    for root in additional_watch_roots(cli) {
        let root = absolute(&root).map_err(|error| error.to_string())?;
        if root != parent {
            watcher
                .watch(&root, RecursiveMode::Recursive)
                .map_err(|error| error.to_string())?;
        }
    }

    let mut explicit_targets = vec![input.clone()];
    explicit_targets.extend(
        cli.module_manifests
            .iter()
            .chain(&cli.module_packs)
            .filter_map(|path| absolute(path).ok()),
    );
    if let Some(project) = &cli.module_project {
        if let Ok(project) = absolute(project) {
            explicit_targets.push(project);
        }
    } else {
        explicit_targets.push(parent.join(DOMAIN_PROJECT_FILE_NAME));
    }
    extend_project_watch_targets(cli, input_path, &mut explicit_targets);
    explicit_targets.sort();
    explicit_targets.dedup();

    let _ = compile_once(cli, input_path);
    eprintln!("watching {}", input_path.display());
    loop {
        match receiver.recv() {
            Ok(Ok(event)) if event_targets(&event, &explicit_targets) => {
                let _ = compile_once(cli, input_path);
                extend_project_watch_targets(cli, input_path, &mut explicit_targets);
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => eprintln!("weavec: watch error: {error}"),
            Err(error) => return Err(format!("watch channel closed: {error}")),
        }
    }
}

fn extend_project_watch_targets(cli: &Cli, input: &Path, targets: &mut Vec<PathBuf>) {
    if let Ok(inputs) = domain_inputs(cli, input)
        && let Some(project) = inputs.project
    {
        targets.extend(
            project
                .files()
                .iter()
                .filter_map(|path| absolute(path).ok()),
        );
        targets.sort();
        targets.dedup();
    }
}

fn additional_watch_roots(cli: &Cli) -> Vec<PathBuf> {
    let mut roots = cli.module_registries.clone();
    roots.extend(
        cli.module_manifests
            .iter()
            .chain(&cli.module_packs)
            .filter_map(|path| path.parent().map(Path::to_path_buf)),
    );
    if let Some(parent) = cli.module_project.as_deref().and_then(Path::parent) {
        roots.push(parent.to_path_buf());
    }
    roots.sort();
    roots.dedup();
    roots
}

fn event_targets(event: &Event, targets: &[PathBuf]) -> bool {
    event.paths.iter().any(|path| {
        let candidate = absolute(path).unwrap_or_else(|_| path.to_path_buf());
        targets.contains(&candidate) || is_domain_artifact_event(path)
    })
}

fn is_domain_artifact_event(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    name == DOMAIN_PROJECT_FILE_NAME
        || name == "module.weave-module.json"
        || name == "module.weave-module.ron"
        || name == "pack.weave-domain.json"
        || name == "pack.weave-domain.ron"
        || name.ends_with(".sha256")
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
