//! Atomic project persistence, recent files, recovery, and external-change handling.

use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use semver::Version;
use serde::{Deserialize, Serialize};
use weave_compiler::{CompileOptions, compile_with_modules, to_ron};
use weave_core::Diagnostic;
use weave_domain::{
    DOMAIN_PROJECT_FILE_NAME, DomainCatalog, DomainPackageError, LoadedDomainProject,
    load_adjacent_domain_project,
};

const RECENT_LIMIT: usize = 12;

/// Error from a project or watcher operation.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("project I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("file watcher failed: {0}")]
    Watch(#[from] notify::Error),
    #[error("project source is not valid UTF-8")]
    InvalidUtf8,
    #[error("save is blocked by an unresolved external-edit conflict")]
    UnresolvedConflict,
    #[error("project has no file path; use Save As")]
    MissingPath,
    #[error("recovery data is invalid: {0}")]
    InvalidRecovery(#[from] serde_json::Error),
    #[error("domain project failed: {0}")]
    DomainPackage(#[from] DomainPackageError),
}

/// Result of saving source and its optional compiled output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSave {
    pub source_path: PathBuf,
    pub compile_output: Option<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Pending external edit that must not be silently overwritten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalConflict {
    pub path: PathBuf,
    pub memory_source: String,
    pub disk_source: String,
}

/// Outcome after observing a watched disk change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalChange {
    NoChange,
    SelfAuthored,
    Reloaded,
    DomainReloaded,
    DomainRejected(String),
    Conflict(ExternalConflict),
}

/// Explicit conflict policy selected by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    KeepMemory,
    TakeDisk,
    SaveBoth,
}

/// Resolved conflict result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictResult {
    pub source: String,
    pub memory_copy: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    bytes: usize,
    hash: u64,
}

/// One open or untitled Weave project.
#[derive(Debug, Clone)]
pub struct ProjectSession {
    path: Option<PathBuf>,
    source: String,
    saved: Fingerprint,
    dirty: bool,
    revision: u64,
    recent: VecDeque<PathBuf>,
    compile_output: Option<PathBuf>,
    pending_self_write: Option<Fingerprint>,
    conflict: Option<ExternalConflict>,
    domain_project: LoadedDomainProject,
}

impl ProjectSession {
    /// Create an untitled in-memory project.
    #[must_use]
    pub fn untitled(source: impl Into<String>) -> Self {
        let source = source.into();
        Self {
            path: None,
            saved: fingerprint(&source),
            source,
            dirty: false,
            revision: 0,
            recent: VecDeque::new(),
            compile_output: None,
            pending_self_write: None,
            conflict: None,
            domain_project: LoadedDomainProject::empty_for_source("untitled.weave"),
        }
    }

    /// Open a UTF-8 `.weave` source file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let path = normalize_path(path.as_ref());
        let bytes = fs::read(&path)?;
        let source = String::from_utf8(bytes).map_err(|_| ProjectError::InvalidUtf8)?;
        let domain_project = load_adjacent_domain_project(&path, &current_weave())?;
        let saved = fingerprint(&source);
        let mut session = Self {
            path: Some(path.clone()),
            source,
            saved,
            dirty: false,
            revision: 0,
            recent: VecDeque::new(),
            compile_output: output_path(&path).filter(|output| output.exists()),
            pending_self_write: None,
            conflict: None,
            domain_project,
        };
        session.remember(path);
        Ok(session)
    }

    /// Create and atomically persist a new project.
    pub fn create(
        path: impl AsRef<Path>,
        source: impl Into<String>,
    ) -> Result<(Self, ProjectSave), ProjectError> {
        let mut session = Self::untitled(source);
        session.path = Some(normalize_path(path.as_ref()));
        session.reload_domain_project()?;
        let saved = session.save()?;
        Ok((session, saved))
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    #[must_use]
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .to_owned()
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn recent(&self) -> &VecDeque<PathBuf> {
        &self.recent
    }

    #[must_use]
    pub fn compile_output(&self) -> Option<&Path> {
        self.compile_output.as_deref()
    }

    #[must_use]
    pub fn conflict(&self) -> Option<&ExternalConflict> {
        self.conflict.as_ref()
    }

    #[must_use]
    pub fn domain_catalog(&self) -> &DomainCatalog {
        self.domain_project.catalog()
    }

    #[must_use]
    pub fn domain_files(&self) -> &[PathBuf] {
        self.domain_project.files()
    }

    /// Reload adjacent module configuration into a candidate and swap only after full validation.
    pub fn reload_domain_project(&mut self) -> Result<(), ProjectError> {
        let path = self.path.clone().ok_or(ProjectError::MissingPath)?;
        let candidate = load_adjacent_domain_project(path, &current_weave())?;
        self.domain_project = candidate;
        Ok(())
    }

    /// Replace in-memory source and update dirty state.
    pub fn set_source(&mut self, source: impl Into<String>) {
        self.source = source.into();
        self.dirty = fingerprint(&self.source) != self.saved;
        self.revision = self.revision.saturating_add(1);
    }

    /// Save current source and, when valid, an adjacent canonical `.ron` output.
    pub fn save(&mut self) -> Result<ProjectSave, ProjectError> {
        if self.conflict.is_some() {
            return Err(ProjectError::UnresolvedConflict);
        }
        let path = self.path.clone().ok_or(ProjectError::MissingPath)?;
        self.reload_domain_project()?;
        atomic_write(&path, self.source.as_bytes())?;
        let written = fingerprint(&self.source);
        self.saved = written;
        self.pending_self_write = Some(written);
        self.dirty = false;
        self.remember(path.clone());

        let (compile_output, diagnostics) = match compile_with_modules(
            &self.source,
            &CompileOptions {
                source_name: Some(path.to_string_lossy().into_owned()),
            },
            self.domain_project.catalog(),
        ) {
            Ok(compiled) => {
                let ron = to_ron(&compiled.story)
                    .map_err(|error| ProjectError::Io(io::Error::other(error.to_string())))?;
                let output = output_path(&path).expect("normalized source has a file name");
                atomic_write(&output, ron.as_bytes())?;
                if self.domain_project.is_configured() {
                    let lock = self.domain_project.lock_for(&compiled.domain_graph)?;
                    self.domain_project.write_lock(&lock)?;
                }
                self.compile_output = Some(output.clone());
                (Some(output), compiled.diagnostics)
            }
            Err(error) => (self.compile_output.clone(), error.diagnostics),
        };
        Ok(ProjectSave {
            source_path: path,
            compile_output,
            diagnostics,
        })
    }

    /// Save to a different path without altering either file until the atomic write succeeds.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<ProjectSave, ProjectError> {
        let previous = self.path.clone();
        let previous_domain_project = self.domain_project.clone();
        self.path = Some(normalize_path(path.as_ref()));
        if let Err(error) = self.reload_domain_project() {
            self.path = previous;
            self.domain_project = previous_domain_project;
            return Err(error);
        }
        match self.save() {
            Ok(saved) => Ok(saved),
            Err(error) => {
                self.path = previous;
                self.domain_project = previous_domain_project;
                Err(error)
            }
        }
    }

    /// Reopen one path from the in-memory recent list.
    pub fn reopen_recent(&self, index: usize) -> Result<Self, ProjectError> {
        let path = self.recent.get(index).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "recent project is unavailable")
        })?;
        let mut reopened = Self::open(path)?;
        reopened.recent = self.recent.clone();
        reopened.remember(path.clone());
        Ok(reopened)
    }

    /// Open a project while carrying this session's recent-project history forward.
    pub fn open_from(&self, path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let mut opened = Self::open(path)?;
        opened.recent = self.recent.clone();
        if let Some(path) = &self.path {
            opened.remember(path.clone());
        }
        let opened_path = opened.path.clone().expect("opened project has a path");
        opened.remember(opened_path);
        Ok(opened)
    }

    /// Carry recent-project history into a newly-created or untitled session.
    pub fn inherit_recent(&mut self, previous: &Self) {
        self.recent = previous.recent.clone();
        if let Some(path) = &previous.path {
            self.remember(path.clone());
        }
    }

    /// Adjacent path used for recoverable unsaved session state.
    #[must_use]
    pub fn recovery_path(&self) -> Option<PathBuf> {
        self.path
            .as_ref()
            .map(|path| path.with_extension("weave.recovery.json"))
    }

    /// Classify newly-read disk source after watcher debouncing.
    pub fn observe_disk_source(&mut self, disk_source: String) -> ExternalChange {
        let observed = fingerprint(&disk_source);
        if self.pending_self_write == Some(observed) {
            self.pending_self_write = None;
            self.saved = observed;
            return ExternalChange::SelfAuthored;
        }
        if observed == self.saved {
            return ExternalChange::NoChange;
        }
        if self.dirty {
            let conflict = ExternalConflict {
                path: self.path.clone().unwrap_or_default(),
                memory_source: self.source.clone(),
                disk_source,
            };
            self.conflict = Some(conflict.clone());
            return ExternalChange::Conflict(conflict);
        }
        self.source = disk_source;
        self.saved = observed;
        self.revision = self.revision.saturating_add(1);
        ExternalChange::Reloaded
    }

    /// Read and classify the active project file.
    pub fn refresh_from_disk(&mut self) -> Result<ExternalChange, ProjectError> {
        let path = self.path.clone().ok_or(ProjectError::MissingPath)?;
        let bytes = fs::read(path)?;
        let source = String::from_utf8(bytes).map_err(|_| ProjectError::InvalidUtf8)?;
        Ok(self.observe_disk_source(source))
    }

    /// Resolve a pending conflict without silently discarding either version.
    pub fn resolve_conflict(
        &mut self,
        resolution: ConflictResolution,
    ) -> Result<ConflictResult, ProjectError> {
        let conflict = self.conflict.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "there is no external-edit conflict",
            )
        })?;
        let mut memory_copy = None;
        match resolution {
            ConflictResolution::KeepMemory => {
                self.saved = fingerprint(&conflict.disk_source);
                self.dirty = true;
            }
            ConflictResolution::TakeDisk => {
                self.source = conflict.disk_source;
                self.saved = fingerprint(&self.source);
                self.dirty = false;
            }
            ConflictResolution::SaveBoth => {
                let copy = conflict_copy_path(&conflict.path);
                atomic_write(&copy, conflict.memory_source.as_bytes())?;
                memory_copy = Some(copy);
                self.source = conflict.disk_source;
                self.saved = fingerprint(&self.source);
                self.dirty = false;
            }
        }
        self.revision = self.revision.saturating_add(1);
        Ok(ConflictResult {
            source: self.source.clone(),
            memory_copy,
        })
    }

    /// Persist recoverable session state as atomic JSON.
    pub fn write_recovery(&self, path: impl AsRef<Path>) -> Result<(), ProjectError> {
        let recovery = RecoverySnapshot {
            version: 1,
            project_path: self.path.clone(),
            source: self.source.clone(),
            revision: self.revision,
        };
        let encoded = serde_json::to_vec_pretty(&recovery)?;
        atomic_write(path.as_ref(), &encoded)?;
        Ok(())
    }

    /// Restore recoverable state without writing over the original project.
    pub fn read_recovery(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let recovery: RecoverySnapshot = serde_json::from_slice(&fs::read(path)?)?;
        let mut session = Self::untitled(recovery.source);
        session.path = recovery.project_path;
        if session.path.is_some() {
            session.reload_domain_project()?;
        }
        session.revision = recovery.revision;
        session.dirty = true;
        Ok(session)
    }

    fn remember(&mut self, path: PathBuf) {
        self.recent.retain(|recent| recent != &path);
        self.recent.push_front(path);
        self.recent.truncate(RECENT_LIMIT);
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RecoverySnapshot {
    version: u32,
    project_path: Option<PathBuf>,
    source: String,
    revision: u64,
}

/// Debounced native watcher for one project file.
pub struct ProjectWatcher {
    _watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<Event>>,
    target: PathBuf,
    debounce: Duration,
    pending_since: Option<Instant>,
    source_pending: bool,
    domain_pending: bool,
}

impl ProjectWatcher {
    /// Watch one source path with the supplied quiet period.
    pub fn new(path: impl AsRef<Path>, debounce: Duration) -> Result<Self, ProjectError> {
        let target = normalize_path(path.as_ref());
        let (sender, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })?;
        watcher.watch(
            target.parent().unwrap_or_else(|| Path::new(".")),
            RecursiveMode::Recursive,
        )?;
        Ok(Self {
            _watcher: watcher,
            receiver,
            target,
            debounce,
            pending_since: None,
            source_pending: false,
            domain_pending: false,
        })
    }

    /// Poll events and return one coalesced project change after the quiet period.
    pub fn poll(
        &mut self,
        project: &mut ProjectSession,
        now: Instant,
    ) -> Result<Option<ExternalChange>, ProjectError> {
        for event in self.receiver.try_iter() {
            let event = event?;
            if matches!(event.kind, EventKind::Access(_)) {
                continue;
            }
            let source_changed = event
                .paths
                .iter()
                .any(|path| paths_refer_to_same_file(path, &self.target));
            let domain_changed = event
                .paths
                .iter()
                .any(|path| relevant_domain_event(path, project.domain_files()));
            if source_changed || domain_changed {
                self.source_pending |= source_changed;
                self.domain_pending |= domain_changed;
                self.pending_since = Some(now);
            }
        }
        let Some(pending) = self.pending_since else {
            return Ok(None);
        };
        if now.saturating_duration_since(pending) < self.debounce {
            return Ok(None);
        }
        self.pending_since = None;
        let source_pending = std::mem::take(&mut self.source_pending);
        let domain_pending = std::mem::take(&mut self.domain_pending);

        let domain_change = if domain_pending {
            Some(match project.reload_domain_project() {
                Ok(()) => ExternalChange::DomainReloaded,
                Err(error) => ExternalChange::DomainRejected(error.to_string()),
            })
        } else {
            None
        };
        if source_pending {
            let source_change = project.refresh_from_disk()?;
            if !matches!(source_change, ExternalChange::NoChange) {
                return Ok(Some(source_change));
            }
        }
        Ok(domain_change)
    }
}

fn relevant_domain_event(path: &Path, domain_files: &[PathBuf]) -> bool {
    if domain_files
        .iter()
        .any(|target| paths_refer_to_same_file(path, target))
    {
        return true;
    }
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

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    let left = normalize_path(left);
    let right = normalize_path(right);
    left == right
        || match (left.canonicalize(), right.canonicalize()) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        }
}

fn normalize_path(path: &Path) -> PathBuf {
    if path.extension().is_none() {
        path.with_extension("weave")
    } else {
        path.to_path_buf()
    }
}

fn current_weave() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("workspace package version is valid")
}

fn output_path(path: &Path) -> Option<PathBuf> {
    path.file_name().map(|_| path.with_extension("ron"))
}

fn conflict_copy_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("story");
    path.with_file_name(format!("{stem}.memory-conflict.weave"))
}

fn fingerprint(source: &str) -> Fingerprint {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in source.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Fingerprint {
        bytes: source.len(),
        hash,
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::thread;

    use tempfile::tempdir;

    use super::*;

    const VALID: &str = "=== start ===\nHello.\n-> END\n";
    const DOMAIN_SOURCE: &str = include_str!("../../examples/domain-modules/contract/tracer.weave");
    const DOMAIN_MANIFEST: &str =
        include_str!("../../examples/domain-modules/contract/module.weave-module.json");
    const DOMAIN_PACK: &str =
        include_str!("../../examples/domain-modules/contract/pack.weave-domain.json");

    fn await_watcher_change(
        watcher: &mut ProjectWatcher,
        project: &mut ProjectSession,
    ) -> ExternalChange {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let now = Instant::now();
            if let Some(change) = watcher.poll(project, now).expect("watch poll") {
                return change;
            }
            assert!(now < deadline, "watch event timed out");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn create_save_open_and_reopen_track_source_and_compiled_output() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("story");
        let (mut project, saved) = ProjectSession::create(&path, VALID).expect("create project");
        assert_eq!(
            saved
                .source_path
                .extension()
                .and_then(|value| value.to_str()),
            Some("weave")
        );
        assert!(
            saved
                .compile_output
                .as_ref()
                .is_some_and(|path| path.exists())
        );
        project.set_source("=== start ===\nChanged.\n-> END\n");
        assert!(project.is_dirty());
        project.save().expect("save project");
        let reopened = ProjectSession::open(saved.source_path).expect("open project");
        assert!(reopened.source().contains("Changed."));
        assert!(!reopened.is_dirty());
    }

    #[test]
    fn external_edits_reload_or_raise_an_explicit_conflict() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("story.weave");
        let (mut project, _) = ProjectSession::create(&path, VALID).expect("create project");
        assert_eq!(
            project.observe_disk_source(VALID.to_owned()),
            ExternalChange::SelfAuthored
        );
        let disk = "=== start ===\nExternal.\n-> END\n";
        assert_eq!(
            project.observe_disk_source(disk.to_owned()),
            ExternalChange::Reloaded
        );
        assert_eq!(project.source(), disk);

        project.set_source("=== start ===\nMemory.\n-> END\n");
        let newer_disk = "=== start ===\nNew disk.\n-> END\n";
        assert!(matches!(
            project.observe_disk_source(newer_disk.to_owned()),
            ExternalChange::Conflict(_)
        ));
        assert!(matches!(
            project.save(),
            Err(ProjectError::UnresolvedConflict)
        ));
        let resolved = project
            .resolve_conflict(ConflictResolution::SaveBoth)
            .expect("resolve conflict");
        assert_eq!(resolved.source, newer_disk);
        assert!(
            resolved
                .memory_copy
                .as_ref()
                .is_some_and(|path| path.exists())
        );
    }

    #[test]
    fn recovery_preserves_unsaved_source_without_touching_the_project() {
        let directory = tempdir().expect("temp dir");
        let project_path = directory.path().join("story.weave");
        let recovery_path = directory.path().join("session-recovery.json");
        let (mut project, _) = ProjectSession::create(&project_path, VALID).expect("create");
        project.set_source("=== start ===\nUnsaved.\n-> END\n");
        project
            .write_recovery(&recovery_path)
            .expect("write recovery");
        let recovered = ProjectSession::read_recovery(&recovery_path).expect("read recovery");
        assert!(recovered.is_dirty());
        assert!(recovered.source().contains("Unsaved."));
        assert_eq!(fs::read_to_string(project_path).expect("original"), VALID);
    }

    #[test]
    fn failed_atomic_save_does_not_damage_an_existing_project() {
        let directory = tempdir().expect("temp dir");
        let original = directory.path().join("original.weave");
        atomic_write(&original, VALID.as_bytes()).expect("initial write");
        let blocking_file = directory.path().join("not-a-directory");
        fs::write(&blocking_file, "block").expect("blocking file");
        let invalid_target = blocking_file.join("story.weave");
        assert!(atomic_write(&invalid_target, b"replacement").is_err());
        assert_eq!(fs::read_to_string(original).expect("original"), VALID);
    }

    #[test]
    fn native_watcher_survives_atomic_saves_and_debounces_external_writes() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("watched.weave");
        let (mut project, _) = ProjectSession::create(&path, VALID).expect("create");
        let mut watcher = ProjectWatcher::new(&path, Duration::from_millis(20)).expect("watcher");
        thread::sleep(Duration::from_millis(30));

        project.set_source("=== start ===\nSelf-authored.\n-> END\n");
        project.save().expect("atomic editor save");
        assert_eq!(
            await_watcher_change(&mut watcher, &mut project),
            ExternalChange::SelfAuthored
        );

        let external = "=== start ===\nWatched change.\n-> END\n";
        fs::write(&path, external).expect("external write");
        let change = await_watcher_change(&mut watcher, &mut project);
        assert_eq!(change, ExternalChange::Reloaded);
        assert_eq!(project.source(), external);
    }

    #[test]
    fn domain_watcher_swaps_valid_catalogs_and_rejects_invalid_updates() {
        let directory = tempdir().expect("temp dir");
        let source_path = directory.path().join("tracer.weave");
        let manifest_path = directory.path().join("module.weave-module.json");
        let pack_path = directory.path().join("pack.weave-domain.json");
        fs::write(&source_path, DOMAIN_SOURCE).expect("write source");
        fs::write(manifest_path, DOMAIN_MANIFEST).expect("write manifest");
        fs::write(&pack_path, DOMAIN_PACK).expect("write pack");
        fs::write(
            directory.path().join(DOMAIN_PROJECT_FILE_NAME),
            concat!(
                "{\n",
                "  \"schema_version\": 1,\n",
                "  \"registries\": [],\n",
                "  \"manifests\": [\"module.weave-module.json\"],\n",
                "  \"packs\": [\"pack.weave-domain.json\"]\n",
                "}\n"
            ),
        )
        .expect("write module project");
        let mut project = ProjectSession::open(&source_path).expect("open project");
        let mut watcher =
            ProjectWatcher::new(&source_path, Duration::from_millis(20)).expect("watcher");

        let initial = compile_with_modules(
            project.source(),
            &CompileOptions::default(),
            project.domain_catalog(),
        )
        .expect("initial module compile");
        assert!(
            to_ron(&initial.story)
                .expect("initial RON")
                .contains("Glasswing constellation")
        );

        fs::write(
            &pack_path,
            DOMAIN_PACK.replace("Glasswing constellation", "Reloaded constellation"),
        )
        .expect("write valid update");
        assert_eq!(
            await_watcher_change(&mut watcher, &mut project),
            ExternalChange::DomainReloaded
        );
        let updated = compile_with_modules(
            project.source(),
            &CompileOptions::default(),
            project.domain_catalog(),
        )
        .expect("updated module compile");
        assert!(
            to_ron(&updated.story)
                .expect("updated RON")
                .contains("Reloaded constellation")
        );

        fs::write(&pack_path, "{}").expect("write invalid update");
        assert!(matches!(
            await_watcher_change(&mut watcher, &mut project),
            ExternalChange::DomainRejected(_)
        ));
        let preserved = compile_with_modules(
            project.source(),
            &CompileOptions::default(),
            project.domain_catalog(),
        )
        .expect("preserved module compile");
        assert!(
            to_ron(&preserved.story)
                .expect("preserved RON")
                .contains("Reloaded constellation")
        );
    }

    #[test]
    fn editor_project_discovers_and_selects_the_adjacent_world_preset() {
        let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/domain-modules/weave-world/reference-place.weave");
        let project = ProjectSession::open(source_path).expect("open world fixture");
        let compiled = compile_with_modules(
            project.source(),
            &CompileOptions::default(),
            project.domain_catalog(),
        )
        .expect("compile editor world project");

        assert_eq!(
            compiled.domain_modules["world"].manifest.id,
            "org.weave.world"
        );
        assert_eq!(
            compiled.domain_modules["world"].pack.id,
            "aotearoa_new_zealand"
        );
        assert!(
            project
                .domain_files()
                .iter()
                .any(|path| path.ends_with("pack.weave-domain.json"))
        );
    }
}
