//! Debounced compiler integration and isolated live story playback panel.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, ClipboardItem, Context, EventEmitter, FocusHandle, ImageSource, KeyDownEvent,
    ObjectFit, RenderImage, Window, div, img, prelude::*, px, relative, rgb,
};
use weave_compiler::{CompileOptions, compile_with_modules};
use weave_core::ir::{AnchorIr, Fit, Slot, StoryIr};
use weave_core::{Diagnostic, Span};
use weave_domain::DomainCatalog;
use weave_patterns::DrawResult;
use weave_runtime::{ChoiceView, PresentationEvent, Story, StoryEvent, Value};

use crate::play_view::{
    CharacterPresentation, NARRATOR, PlayMode, PlayScene, PresentationDrawerRows, SLOT_ORDER,
    StagedCharacter, WorldPresentation, presentation_drawer_rows, resolve_presentation,
    scene_from_session, scene_tone,
};
use crate::playtest::{PlaytestOutcome, PlaytestRecord};
use crate::presentation_assets::PresentationAssets;
use crate::theme::DARK_THEME;

const MAX_CONTINUE_BOUNDARIES: usize = 10_000;

/// Extra fraction of the frame an anchored cover background extends past each
/// edge, so the declared anchor can bias the crop in any direction.
const SCENE_OVERSCAN: f32 = 0.16;

/// Transcript item retained for the current preview run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewTranscript {
    Line(String),
    Choice(String),
    Jump(String),
}

/// Current playback boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewStatus {
    Ready,
    Running,
    WaitingForChoice,
    Ended,
    Stopped,
    Error,
}

impl PreviewStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::WaitingForChoice => "Waiting for choice",
            Self::Ended => "Ended",
            Self::Stopped => "Stopped",
            Self::Error => "Error",
        }
    }
}

/// Actionable compiler/runtime failure shown without discarding the last valid preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewIssue {
    pub message: String,
    pub span: Option<Span>,
}

/// Quiet-period tracker used by source edits before recompiling preview state.
#[derive(Debug, Clone)]
pub struct PreviewCompileDebouncer {
    delay: Duration,
    pending: Option<(u64, Instant)>,
}

impl PreviewCompileDebouncer {
    #[must_use]
    pub const fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: None,
        }
    }

    pub fn schedule(&mut self, revision: u64, now: Instant) {
        self.pending = Some((revision, now));
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn take_ready(&mut self, now: Instant) -> Option<u64> {
        let (revision, scheduled) = self.pending?;
        if now.saturating_duration_since(scheduled) < self.delay {
            return None;
        }
        self.pending = None;
        Some(revision)
    }
}

#[derive(Debug, Clone)]
struct PendingCompile {
    source: String,
    source_name: Option<String>,
    revision: u64,
}

/// Testable live preview session isolated from the editor's pattern browser.
#[derive(Debug, Clone)]
pub struct PreviewSession {
    last_valid_story: Option<StoryIr>,
    runtime: Option<Story>,
    seed: u64,
    compiled_revision: Option<u64>,
    stale: bool,
    status: PreviewStatus,
    transcript: Vec<PreviewTranscript>,
    pattern_draws: Vec<DrawResult>,
    presentation: Vec<PresentationEvent>,
    issue: Option<PreviewIssue>,
    domain_catalog: DomainCatalog,
    /// Knot playback began at after a jump; the story entry when unset.
    start_knot: Option<String>,
    /// Selected choice indices in order, for author playtest capture.
    selected_choices: Vec<usize>,
}

impl PreviewSession {
    #[must_use]
    pub fn empty(seed: u64) -> Self {
        Self {
            last_valid_story: None,
            runtime: None,
            seed,
            compiled_revision: None,
            stale: false,
            status: PreviewStatus::Ready,
            transcript: Vec::new(),
            pattern_draws: Vec::new(),
            presentation: Vec::new(),
            issue: None,
            domain_catalog: DomainCatalog::new(),
            start_knot: None,
            selected_choices: Vec::new(),
        }
    }

    pub fn from_story(story: StoryIr, seed: u64, revision: u64) -> Self {
        let mut session = Self::empty(seed);
        session.install_story(story, revision);
        session
    }

    /// Replace the explicit catalog used by subsequent preview compiles.
    pub fn set_domain_catalog(&mut self, domain_catalog: DomainCatalog) {
        self.domain_catalog = domain_catalog;
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub const fn status(&self) -> PreviewStatus {
        self.status
    }

    #[must_use]
    pub const fn is_stale(&self) -> bool {
        self.stale
    }

    #[must_use]
    pub const fn compiled_revision(&self) -> Option<u64> {
        self.compiled_revision
    }

    /// Last valid compiled story, when the session has one.
    #[must_use]
    pub fn story(&self) -> Option<&StoryIr> {
        self.last_valid_story.as_ref()
    }

    /// Knot playback began at after a jump; `None` means the story entry.
    #[must_use]
    pub fn start_knot(&self) -> Option<&str> {
        self.start_knot.as_deref()
    }

    /// Selected choice indices in the order the author chose them.
    #[must_use]
    pub fn selected_choices(&self) -> &[usize] {
        &self.selected_choices
    }

    /// Current variable snapshot, read from the live runtime state.
    #[must_use]
    pub fn variable_snapshot(&self) -> BTreeMap<String, Value> {
        self.runtime.as_ref().map_or_else(BTreeMap::new, |runtime| {
            runtime
                .state()
                .variables()
                .iter()
                .map(|(name, state)| (name.clone(), state.value.clone()))
                .collect()
        })
    }

    #[must_use]
    pub fn transcript(&self) -> &[PreviewTranscript] {
        &self.transcript
    }

    #[must_use]
    pub fn pattern_draws(&self) -> &[DrawResult] {
        &self.pattern_draws
    }

    /// Presentation cues produced so far, in source order.
    ///
    /// The Play surface reads this to stage the scene. It is cleared whenever a
    /// new story is installed, so a restart begins from an unstaged frame.
    #[must_use]
    pub fn presentation(&self) -> &[PresentationEvent] {
        &self.presentation
    }

    #[must_use]
    pub fn issue(&self) -> Option<&PreviewIssue> {
        self.issue.as_ref()
    }

    #[must_use]
    pub fn choices(&self) -> Vec<ChoiceView> {
        self.runtime.as_ref().map_or_else(Vec::new, Story::choices)
    }

    /// Display name declared for a stage, if the story names one.
    #[must_use]
    pub fn stage_name(&self, id: &str) -> Option<String> {
        self.last_valid_story
            .as_ref()?
            .presentation
            .stages
            .get(id)?
            .name
            .clone()
    }

    /// Project-relative background image declared by a scene, if it names one.
    #[must_use]
    pub fn scene_background(&self, id: &str) -> Option<&str> {
        self.last_valid_story
            .as_ref()?
            .presentation
            .scenes
            .get(id)?
            .background
            .as_deref()
    }

    /// Fit declared by a scene, when the story declares that scene.
    #[must_use]
    pub fn scene_fit(&self, id: &str) -> Option<Fit> {
        Some(
            self.last_valid_story
                .as_ref()?
                .presentation
                .scenes
                .get(id)?
                .fit,
        )
    }

    /// Normalized anchor declared by a scene, when the story declares it.
    #[must_use]
    pub fn scene_anchor(&self, id: &str) -> Option<AnchorIr> {
        Some(
            self.last_valid_story
                .as_ref()?
                .presentation
                .scenes
                .get(id)?
                .anchor,
        )
    }

    /// Project-relative character image declared by a stage, if it names one.
    #[must_use]
    pub fn stage_image(&self, id: &str) -> Option<&str> {
        self.last_valid_story
            .as_ref()?
            .presentation
            .stages
            .get(id)?
            .image
            .as_deref()
    }

    /// Slot declared by a stage, when the story declares that stage.
    #[must_use]
    pub fn stage_slot(&self, id: &str) -> Option<Slot> {
        Some(
            self.last_valid_story
                .as_ref()?
                .presentation
                .stages
                .get(id)?
                .slot,
        )
    }

    /// Paint layer declared by a stage, when the story declares that stage.
    #[must_use]
    pub fn stage_layer(&self, id: &str) -> Option<u8> {
        Some(
            self.last_valid_story
                .as_ref()?
                .presentation
                .stages
                .get(id)?
                .layer,
        )
    }

    /// Project-relative image declared for one named stage variant.
    #[must_use]
    pub fn stage_variant_image(&self, stage: &str, variant: &str) -> Option<&str> {
        self.last_valid_story
            .as_ref()?
            .presentation
            .stages
            .get(stage)?
            .variants
            .get(variant)
            .map(String::as_str)
    }

    #[must_use]
    pub fn knots(&self) -> Vec<String> {
        self.last_valid_story
            .as_ref()
            .map_or_else(Vec::new, |story| story.knots.keys().cloned().collect())
    }

    #[must_use]
    pub fn variables(&self) -> Vec<(String, Value)> {
        self.runtime.as_ref().map_or_else(Vec::new, |runtime| {
            runtime
                .state()
                .variables()
                .iter()
                .map(|(name, state)| (name.clone(), state.value.clone()))
                .collect()
        })
    }

    /// Compile current source. Failures preserve the complete last valid runtime and transcript.
    pub fn compile_source(
        &mut self,
        source: &str,
        source_name: Option<String>,
        revision: u64,
    ) -> bool {
        match compile_with_modules(
            source,
            &CompileOptions { source_name },
            &self.domain_catalog,
        ) {
            Ok(compiled) => {
                self.install_story(compiled.story, revision);
                true
            }
            Err(error) => {
                self.stale = self.runtime.is_some();
                self.issue = Some(issue_from_diagnostics(&error.diagnostics));
                if self.runtime.is_none() {
                    self.status = PreviewStatus::Error;
                }
                false
            }
        }
    }

    pub fn step(&mut self) -> bool {
        if self.status == PreviewStatus::Stopped {
            return false;
        }
        let Some(mut runtime) = self.runtime.take() else {
            self.fail("No valid compiled story is available", None);
            return false;
        };
        let event = runtime.advance();
        self.pattern_draws.extend(runtime.take_pattern_draws());
        self.presentation.extend(runtime.take_presentation());
        self.runtime = Some(runtime);
        match event {
            Ok(StoryEvent::Line(line)) => {
                self.transcript.push(PreviewTranscript::Line(line));
                self.status = PreviewStatus::Ready;
                if !self.stale {
                    self.issue = None;
                }
                true
            }
            Ok(StoryEvent::Choices(_)) => {
                self.status = PreviewStatus::WaitingForChoice;
                true
            }
            Ok(StoryEvent::Ended) => {
                self.status = PreviewStatus::Ended;
                true
            }
            Err(error) => {
                self.fail(error.to_string(), error.span);
                false
            }
        }
    }

    /// Continue through narrative lines until a choice, end, error, or safety boundary.
    pub fn run(&mut self) -> bool {
        if self.status == PreviewStatus::Stopped {
            self.status = PreviewStatus::Ready;
        }
        self.status = PreviewStatus::Running;
        for _ in 0..MAX_CONTINUE_BOUNDARIES {
            if self
                .runtime
                .as_ref()
                .is_some_and(|runtime| runtime.has_choices())
            {
                self.status = PreviewStatus::WaitingForChoice;
                return true;
            }
            if !self.step() {
                return false;
            }
            if matches!(
                self.status,
                PreviewStatus::WaitingForChoice | PreviewStatus::Ended | PreviewStatus::Error
            ) {
                return true;
            }
            self.status = PreviewStatus::Running;
        }
        self.fail(
            format!("Preview stopped after {MAX_CONTINUE_BOUNDARIES} output boundaries"),
            None,
        );
        false
    }

    pub fn choose(&mut self, index: usize) -> bool {
        let choice = self.choices().get(index).cloned();
        let Some(runtime) = self.runtime.as_mut() else {
            self.fail("No valid compiled story is available", None);
            return false;
        };
        match runtime.choose(index) {
            Ok(()) => {
                if let Some(choice) = choice {
                    self.transcript.push(PreviewTranscript::Choice(choice.text));
                }
                self.selected_choices.push(index);
                self.status = PreviewStatus::Ready;
                self.run()
            }
            Err(error) => {
                let span = error.span;
                self.fail(error.to_string(), span);
                false
            }
        }
    }

    pub fn restart(&mut self) -> bool {
        let Some(story) = self.last_valid_story.clone() else {
            self.fail("No valid compiled story is available", None);
            return false;
        };
        let stale = self.stale;
        let issue = self.issue.clone();
        let installed = self.install_story(story, self.compiled_revision.unwrap_or_default());
        if installed && stale {
            self.stale = true;
            self.issue = issue;
        }
        installed
    }

    /// Restart global state, then begin playback at one named knot.
    pub fn jump(&mut self, knot: &str) -> bool {
        let Some(story) = self.last_valid_story.clone() else {
            self.fail("No valid compiled story is available", None);
            return false;
        };
        let stale = self.stale;
        let stale_issue = self.issue.clone();
        let mut runtime = match Story::with_seed(story, self.seed) {
            Ok(runtime) => runtime,
            Err(error) => {
                let span = error.span;
                self.fail(error.to_string(), span);
                return false;
            }
        };
        if let Err(error) = runtime.jump(knot) {
            let span = error.span;
            self.fail(error.to_string(), span);
            return false;
        }
        self.transcript.clear();
        self.transcript
            .push(PreviewTranscript::Jump(knot.to_owned()));
        self.start_knot = Some(knot.to_owned());
        self.selected_choices.clear();
        self.pattern_draws = runtime.take_pattern_draws();
        self.runtime = Some(runtime);
        self.status = PreviewStatus::Ready;
        self.stale = stale;
        self.issue = if stale { stale_issue } else { None };
        self.run()
    }

    pub fn stop(&mut self) {
        self.status = PreviewStatus::Stopped;
    }

    pub fn change_seed(&mut self, delta: i64) -> bool {
        self.seed = self.seed.saturating_add_signed(delta);
        self.restart()
    }

    fn install_story(&mut self, story: StoryIr, revision: u64) -> bool {
        match Story::with_seed(story.clone(), self.seed) {
            Ok(mut runtime) => {
                self.pattern_draws = runtime.take_pattern_draws();
                self.presentation = runtime.take_presentation();
                self.last_valid_story = Some(story);
                self.runtime = Some(runtime);
                self.compiled_revision = Some(revision);
                self.stale = false;
                self.status = PreviewStatus::Ready;
                self.transcript.clear();
                self.issue = None;
                self.start_knot = None;
                self.selected_choices.clear();
                true
            }
            Err(error) => {
                self.stale = self.runtime.is_some();
                let span = error.span;
                self.fail(error.to_string(), span);
                false
            }
        }
    }

    fn fail(&mut self, message: impl Into<String>, span: Option<Span>) {
        self.issue = Some(PreviewIssue {
            message: message.into(),
            span,
        });
        self.status = PreviewStatus::Error;
    }
}

/// Collapsible author playtest drawer state.
///
/// The drawer holds UI state only. Capturing and replaying build records from
/// the compiled story and the supplied seed, so opening, exporting, importing,
/// or replaying never advances the live session or changes its cue order.
#[derive(Debug, Clone, Default)]
pub struct PlaytestDrawer {
    expanded: bool,
    record: Option<PlaytestRecord>,
    json: String,
    ron: String,
    message: Option<String>,
    mismatch: Option<String>,
    replay_outcome: Option<PlaytestOutcome>,
}

impl PlaytestDrawer {
    /// Whether the drawer body is visible.
    #[must_use]
    pub const fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Show or hide the drawer body.
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Captured or imported record.
    #[must_use]
    pub fn record(&self) -> Option<&PlaytestRecord> {
        self.record.as_ref()
    }

    /// Last exported JSON document.
    #[must_use]
    pub fn json(&self) -> &str {
        &self.json
    }

    /// Last exported RON document.
    #[must_use]
    pub fn ron(&self) -> &str {
        &self.ron
    }

    /// Last action result or failure shown by the drawer.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// First replay mismatch, when the record did not reproduce.
    #[must_use]
    pub fn mismatch(&self) -> Option<&str> {
        self.mismatch.as_deref()
    }

    /// Outcome reproduced by the last successful replay.
    #[must_use]
    pub fn replay_outcome(&self) -> Option<&PlaytestOutcome> {
        self.replay_outcome.as_ref()
    }

    /// Record a note shown by the drawer.
    pub fn note(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    /// Capture the current session playthrough.
    pub fn capture(&mut self, session: &PreviewSession) -> bool {
        let Some(story) = session.story() else {
            self.message = Some("No compiled story is available to capture".to_owned());
            return false;
        };
        let start_knot = session.start_knot().unwrap_or(story.entry.as_str());
        match PlaytestRecord::capture_from(
            story,
            session.seed(),
            start_knot,
            session.selected_choices(),
        ) {
            Ok(record) => {
                self.message = Some(format!(
                    "Captured {} events and {} checkpoints",
                    record.events.len(),
                    record.checkpoints.len()
                ));
                self.record = Some(record);
                self.mismatch = None;
                self.replay_outcome = None;
                true
            }
            Err(error) => {
                self.message = Some(error.to_string());
                false
            }
        }
    }

    /// Export the current record as JSON.
    pub fn export_json(&mut self) -> bool {
        let Some(record) = self.record.as_ref() else {
            self.message = Some("Capture or import a playtest before exporting".to_owned());
            return false;
        };
        match record.to_json() {
            Ok(encoded) => {
                self.json = encoded;
                self.message = Some("Exported JSON to the clipboard".to_owned());
                true
            }
            Err(error) => {
                self.message = Some(error.to_string());
                false
            }
        }
    }

    /// Export the current record as RON.
    pub fn export_ron(&mut self) -> bool {
        let Some(record) = self.record.as_ref() else {
            self.message = Some("Capture or import a playtest before exporting".to_owned());
            return false;
        };
        match record.to_ron() {
            Ok(encoded) => {
                self.ron = encoded;
                self.message = Some("Exported RON to the clipboard".to_owned());
                true
            }
            Err(error) => {
                self.message = Some(error.to_string());
                false
            }
        }
    }

    /// Import a JSON record document.
    pub fn import_json(&mut self, source: &str) -> bool {
        match PlaytestRecord::from_json(source) {
            Ok(record) => {
                self.record = Some(record);
                self.json = source.to_owned();
                self.mismatch = None;
                self.replay_outcome = None;
                self.message = Some("Imported a JSON playtest".to_owned());
                true
            }
            Err(error) => {
                self.message = Some(error.to_string());
                false
            }
        }
    }

    /// Import a RON record document.
    pub fn import_ron(&mut self, source: &str) -> bool {
        match PlaytestRecord::from_ron(source) {
            Ok(record) => {
                self.record = Some(record);
                self.ron = source.to_owned();
                self.mismatch = None;
                self.replay_outcome = None;
                self.message = Some("Imported a RON playtest".to_owned());
                true
            }
            Err(error) => {
                self.message = Some(error.to_string());
                false
            }
        }
    }

    /// Replay the current record against the session's compiled story.
    pub fn replay(&mut self, session: &PreviewSession) -> bool {
        let Some(story) = session.story() else {
            self.message = Some("No compiled story is available to replay".to_owned());
            return false;
        };
        self.replay_against(story)
    }

    /// Replay the current record against an explicit compiled story.
    pub fn replay_against(&mut self, story: &StoryIr) -> bool {
        let Some(record) = self.record.as_ref() else {
            self.message = Some("Capture or import a playtest before replaying".to_owned());
            return false;
        };
        match record.replay(story) {
            Ok(report) => {
                self.replay_outcome = Some(report.outcome.clone());
                self.mismatch = None;
                self.message = Some("Replayed the record against this story".to_owned());
                true
            }
            Err(error) => {
                self.replay_outcome = None;
                self.mismatch = Some(error.to_string());
                false
            }
        }
    }
}

/// Collapsible resolved-presentation drawer state.
///
/// The optional World and Character inputs are host configuration. Resolution
/// reads only the compiled story and the live variable snapshot, so the drawer
/// never changes story state or cue order.
#[derive(Debug, Clone, Default)]
pub struct PresentationDrawer {
    expanded: bool,
    world: Option<WorldPresentation>,
    character: Option<CharacterPresentation>,
}

impl PresentationDrawer {
    /// Whether the drawer body is visible.
    #[must_use]
    pub const fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Show or hide the drawer body.
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// World-driven presentation configuration.
    #[must_use]
    pub fn world(&self) -> Option<&WorldPresentation> {
        self.world.as_ref()
    }

    /// Character-driven presentation configuration.
    #[must_use]
    pub fn character(&self) -> Option<&CharacterPresentation> {
        self.character.as_ref()
    }

    /// Replace the world-driven presentation configuration.
    pub fn set_world(&mut self, world: Option<WorldPresentation>) {
        self.world = world;
    }

    /// Replace the character-driven presentation configuration.
    pub fn set_character(&mut self, character: Option<CharacterPresentation>) {
        self.character = character;
    }

    /// Resolve this drawer's inputs against one preview session.
    #[must_use]
    pub fn view(&self, session: &PreviewSession) -> PresentationDrawerRows {
        let Some(story) = session.story() else {
            return PresentationDrawerRows::default();
        };
        let snapshot = session.variable_snapshot();
        self.view_with(story, Some(&snapshot))
    }

    /// Resolve this drawer's inputs against a story and optional snapshot.
    #[must_use]
    pub fn view_with(
        &self,
        story: &StoryIr,
        variables: Option<&BTreeMap<String, Value>>,
    ) -> PresentationDrawerRows {
        let plan = resolve_presentation(
            story,
            self.world.as_ref(),
            self.character.as_ref(),
            variables,
        );
        presentation_drawer_rows(&plan, story, variables)
    }
}

/// Cross-panel event emitted by actionable preview diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewEvent {
    RevealSource(Span),
    /// The panel switched between the transcript and the staged scene.
    ModeChanged(PlayMode),
}

/// Native live-play surface with a debounced pending source compiler.
pub struct PreviewSurface {
    session: PreviewSession,
    debouncer: PreviewCompileDebouncer,
    pending: Option<PendingCompile>,
    assets: PresentationAssets,
    /// Last background frame that decoded, kept so a file that disappears
    /// mid-play does not blank the stage.
    last_background: Option<(String, Arc<RenderImage>)>,
    /// Last decoded figure for each stage, kept for the same reason.
    last_stages: HashMap<String, Arc<RenderImage>>,
    mode: PlayMode,
    playtest_drawer: PlaytestDrawer,
    presentation_drawer: PresentationDrawer,
    focus: FocusHandle,
}

impl PreviewSurface {
    pub fn new(story: Option<StoryIr>, seed: u64, revision: u64, cx: &mut Context<Self>) -> Self {
        Self {
            session: story.map_or_else(
                || PreviewSession::empty(seed),
                |story| PreviewSession::from_story(story, seed, revision),
            ),
            debouncer: PreviewCompileDebouncer::new(Duration::from_millis(250)),
            pending: None,
            assets: PresentationAssets::unbound(),
            last_background: None,
            last_stages: HashMap::new(),
            mode: PlayMode::Debug,
            playtest_drawer: PlaytestDrawer::default(),
            presentation_drawer: PresentationDrawer::default(),
            focus: cx.focus_handle(),
        }
    }

    /// Bind presentation images to the open project directory.
    ///
    /// Rebinding clears the image cache and the last decoded frame of every
    /// element, so opening or saving a different project cannot stage a frame,
    /// or its fallback, decoded from the previous one.
    pub fn set_project_directory(&mut self, directory: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.assets.rebind(directory) {
            self.last_background = None;
            self.last_stages.clear();
            cx.notify();
        }
    }

    /// Re-stat staged images and repaint when one changed on disk.
    ///
    /// The editor polls this beside the source compile poll, which is enough to
    /// pick up a replaced image within a tick without a second file watcher.
    pub fn refresh_assets(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.assets.refresh();
        if changed {
            cx.notify();
        }
        changed
    }

    /// Image loader currently bound to the preview.
    #[must_use]
    pub fn assets(&self) -> &PresentationAssets {
        &self.assets
    }

    /// Current presentation.
    #[must_use]
    pub const fn mode(&self) -> PlayMode {
        self.mode
    }

    /// Switch between the transcript and the scene.
    ///
    /// Entering Play starts the session when it has not run yet, so the scene
    /// opens on the first line rather than on an empty frame.
    pub fn set_mode(&mut self, mode: PlayMode, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        if mode == PlayMode::Play && self.session.transcript().is_empty() {
            self.session.run();
        }
        cx.emit(PreviewEvent::ModeChanged(mode));
        cx.notify();
    }

    #[must_use]
    pub const fn session(&self) -> &PreviewSession {
        &self.session
    }

    /// Author playtest drawer state.
    #[must_use]
    pub const fn playtest_drawer(&self) -> &PlaytestDrawer {
        &self.playtest_drawer
    }

    /// Resolved-presentation drawer state.
    #[must_use]
    pub const fn presentation_drawer(&self) -> &PresentationDrawer {
        &self.presentation_drawer
    }

    /// Replace the world-driven presentation configuration shown by the drawer.
    pub fn set_world_presentation(
        &mut self,
        world: Option<WorldPresentation>,
        cx: &mut Context<Self>,
    ) {
        self.presentation_drawer.set_world(world);
        cx.notify();
    }

    /// Replace the character-driven presentation configuration shown by the drawer.
    pub fn set_character_presentation(
        &mut self,
        character: Option<CharacterPresentation>,
        cx: &mut Context<Self>,
    ) {
        self.presentation_drawer.set_character(character);
        cx.notify();
    }

    pub fn set_domain_catalog(&mut self, domain_catalog: DomainCatalog, cx: &mut Context<Self>) {
        self.session.set_domain_catalog(domain_catalog);
        cx.notify();
    }

    pub fn schedule_compile(
        &mut self,
        source: String,
        source_name: Option<String>,
        revision: u64,
        now: Instant,
        cx: &mut Context<Self>,
    ) {
        self.pending = Some(PendingCompile {
            source,
            source_name,
            revision,
        });
        self.debouncer.schedule(revision, now);
        cx.notify();
    }

    pub fn poll_compile(&mut self, now: Instant, cx: &mut Context<Self>) -> bool {
        let Some(revision) = self.debouncer.take_ready(now) else {
            return false;
        };
        let Some(pending) = self.pending.take() else {
            return false;
        };
        if pending.revision != revision {
            return false;
        }
        let compiled =
            self.session
                .compile_source(&pending.source, pending.source_name, pending.revision);
        cx.notify();
        compiled
    }

    pub fn compile_now(
        &mut self,
        source: &str,
        source_name: Option<String>,
        revision: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        self.pending = None;
        self.debouncer.cancel();
        let compiled = self.session.compile_source(source, source_name, revision);
        cx.notify();
        compiled
    }

    pub fn run(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.session.run();
        cx.notify();
        changed
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.session.stop();
        cx.notify();
    }

    fn step(&mut self, cx: &mut Context<Self>) {
        self.session.step();
        cx.notify();
    }

    fn restart(&mut self, cx: &mut Context<Self>) {
        self.session.restart();
        cx.notify();
    }

    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        self.session.choose(index);
        cx.notify();
    }

    fn jump(&mut self, knot: &str, cx: &mut Context<Self>) {
        self.session.jump(knot);
        cx.notify();
    }

    fn change_seed(&mut self, delta: i64, cx: &mut Context<Self>) {
        self.session.change_seed(delta);
        cx.notify();
    }

    fn mode_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut bar = div()
            .id("preview-mode-bar")
            .flex()
            .items_center()
            .gap_1()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(rgb(DARK_THEME.border));
        for mode in [PlayMode::Debug, PlayMode::Play] {
            let active = self.mode == mode;
            bar = bar.child(
                div()
                    .id(match mode {
                        PlayMode::Debug => "preview-mode-debug",
                        PlayMode::Play => "preview-mode-play",
                    })
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(if active {
                        DARK_THEME.text
                    } else {
                        DARK_THEME.muted_text
                    }))
                    .when(active, |element| element.bg(rgb(DARK_THEME.accent)))
                    .child(mode.label())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.set_mode(mode, cx);
                    })),
            );
        }
        bar.child(
            div()
                .ml_auto()
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child(
                    "Space to advance · 1-9 to choose · R to restart · D playtest · V references",
                ),
        )
        .into_any_element()
    }

    fn play_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let scene = scene_from_session(&self.session);

        // Resolve every declared path before touching the image cache, so the
        // session reads and the loader writes stay independent borrows.
        let background_asset = scene.background.as_deref().and_then(|id| {
            self.session
                .scene_background(id)
                .map(|path| (id.to_owned(), path.to_owned()))
        });
        let stage_assets = scene
            .stages
            .iter()
            .map(|stage| (stage.id.clone(), stage.variant.clone(), stage.image.clone()))
            .collect::<Vec<_>>();

        let background_image = background_asset.as_ref().and_then(|(id, path)| {
            let decoded = self.assets.background(id, path);
            if let Some(image) = &decoded {
                self.last_background = Some((id.clone(), Arc::clone(image)));
            }
            decoded.or_else(|| {
                self.last_background
                    .as_ref()
                    .filter(|(last, _)| last == id)
                    .map(|(_, image)| Arc::clone(image))
            })
        });

        let mut stage_images: HashMap<String, Option<Arc<RenderImage>>> = HashMap::new();
        for (id, variant, path) in &stage_assets {
            let decoded = path.as_deref().and_then(|path| match variant.as_deref() {
                Some(variant) => self.assets.variant(id, variant, path),
                None => self.assets.stage(id, path),
            });
            if let Some(image) = &decoded {
                self.last_stages.insert(id.clone(), Arc::clone(image));
            }
            stage_images.insert(
                id.clone(),
                decoded.or_else(|| self.last_stages.get(id).cloned()),
            );
        }

        let mut notes = scene.notes.clone();
        if let Some((_, path)) = background_asset.as_ref() {
            push_note(&mut notes, self.assets.failure(path));
        }
        for (_, _, path) in &stage_assets {
            if let Some(path) = path {
                push_note(&mut notes, self.assets.failure(path));
            }
        }

        let mut frame = div()
            .id("play-scene")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden();

        if let Some(issue) = &scene.issue {
            frame = frame.child(
                div()
                    .px_3()
                    .py_1()
                    .bg(rgb(DARK_THEME.warning))
                    .text_xs()
                    .text_color(rgb(DARK_THEME.text))
                    .child(format!("Last valid frame kept: {issue}")),
            );
        }

        let mut stage_area = div()
            .id("play-stage")
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .bg(rgb(scene_tone(scene.background.as_deref())));

        // Background: cover fills and crops, contain letterboxes, and the
        // declared anchor biases the crop. The frame is overscanned, so an
        // anchor anywhere in `0..=1` stays inside the image instead of
        // revealing the tone behind it.
        let mut backdrop = div()
            .absolute()
            .left_0()
            .top_0()
            .right_0()
            .bottom_0()
            .overflow_hidden();
        if let Some(image) = background_image {
            let opacity = scene.background_opacity();
            backdrop = match scene.fit {
                Fit::Cover => {
                    let (left, right) = anchor_inset(scene.anchor.x);
                    let (top, bottom) = anchor_inset(scene.anchor.y);
                    backdrop.child(
                        div()
                            .absolute()
                            .left(relative(left))
                            .right(relative(right))
                            .top(relative(top))
                            .bottom(relative(bottom))
                            .child(
                                img(ImageSource::Render(image))
                                    .size_full()
                                    .object_fit(ObjectFit::Cover)
                                    .opacity(opacity),
                            ),
                    )
                }
                Fit::Contain => backdrop.child(
                    img(ImageSource::Render(image))
                        .size_full()
                        .object_fit(ObjectFit::Contain)
                        .opacity(opacity),
                ),
            };
        }
        stage_area = stage_area.child(backdrop);

        // Character grid: five equal columns for far_left through far_right,
        // each holding its stages ordered by layer then insertion order.
        let mut grid = div()
            .id("play-stage-grid")
            .absolute()
            .left_0()
            .right_0()
            .top_0()
            .bottom(px(38.0))
            .flex()
            .items_end()
            .justify_center()
            .gap_2();
        for slot in SLOT_ORDER {
            let mut column = div()
                .flex_1()
                .h_full()
                .flex()
                .items_end()
                .justify_center()
                .gap_1();
            for character in scene.stages_in_slot(slot) {
                column = column.child(stage_figure(
                    character,
                    stage_images.get(&character.id).cloned().flatten(),
                ));
            }
            grid = grid.child(column);
        }
        stage_area = stage_area.child(grid);

        stage_area = stage_area.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(38.0))
                .h(px(1.0))
                .bg(rgb(DARK_THEME.border)),
        );
        if let Some(background) = &scene.background {
            stage_area = stage_area.child(
                div()
                    .absolute()
                    .top_3()
                    .right_3()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(background.clone()),
            );
        }
        if scene.speaker != NARRATOR {
            stage_area = stage_area.child(
                div()
                    .id("play-nameplate")
                    .absolute()
                    .top_3()
                    .left_3()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.chrome))
                    .text_xs()
                    .text_color(rgb(DARK_THEME.accent))
                    .child(scene.speaker.clone()),
            );
        }
        if !notes.is_empty() {
            stage_area = stage_area.child(
                div()
                    .id("play-notes")
                    .absolute()
                    .bottom_3()
                    .left_3()
                    .right_3()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.chrome))
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(notes.join(" · ")),
            );
        }

        let mut dialogue = div()
            .id("play-dialogue")
            .flex_none()
            .max_h(px(220.0))
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .border_t_1()
            .border_color(rgb(DARK_THEME.border))
            .child(div().text_lg().text_color(rgb(DARK_THEME.text)).child(
                if scene.line.is_empty() {
                    "Press Space to begin".to_owned()
                } else {
                    scene.line.clone()
                },
            ));

        if scene.ended {
            dialogue = dialogue.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child("The end"),
                    )
                    .child(preview_button("Restart", "play-restart", cx, |this, cx| {
                        this.session.restart();
                        this.session.run();
                        cx.notify();
                    })),
            );
        } else if !scene.choices.is_empty() {
            for (index, choice) in scene.choices.iter().enumerate() {
                dialogue = dialogue.child(
                    div()
                        .id(("play-choice", index))
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(DARK_THEME.border))
                        .bg(rgb(DARK_THEME.chrome))
                        .cursor_pointer()
                        .text_sm()
                        .text_color(rgb(DARK_THEME.text))
                        .child(format!("{}. {}", index + 1, choice.text))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.session.choose(index);
                            cx.notify();
                        })),
                );
            }
        }

        frame
            .child(stage_area)
            .child(dialogue)
            .child(self.playtest_drawer_panel(cx))
            .child(self.presentation_drawer_panel(cx))
            .into_any_element()
    }

    fn playtest_drawer_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.playtest_drawer.is_expanded();
        let panel = div()
            .id("playtest-drawer")
            .flex_none()
            .border_t_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .child(
                div()
                    .id("playtest-drawer-toggle")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(if expanded {
                        DARK_THEME.text
                    } else {
                        DARK_THEME.muted_text
                    }))
                    .child(format!(
                        "{} Playtest drawer · D",
                        if expanded { "▾" } else { "▸" }
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.playtest_drawer.toggle();
                        cx.notify();
                    })),
            );
        if !expanded {
            return panel.into_any_element();
        }

        let controls = div()
            .flex()
            .flex_wrap()
            .gap_1()
            .px_3()
            .pb_1()
            .child(preview_button(
                "Capture",
                "playtest-capture",
                cx,
                |this, _cx| {
                    let session = &this.session;
                    this.playtest_drawer.capture(session);
                },
            ))
            .child(preview_button(
                "Export JSON",
                "playtest-export-json",
                cx,
                |this, cx| {
                    if this.playtest_drawer.export_json() {
                        let encoded = this.playtest_drawer.json().to_owned();
                        cx.write_to_clipboard(ClipboardItem::new_string(encoded));
                    }
                },
            ))
            .child(preview_button(
                "Export RON",
                "playtest-export-ron",
                cx,
                |this, cx| {
                    if this.playtest_drawer.export_ron() {
                        let encoded = this.playtest_drawer.ron().to_owned();
                        cx.write_to_clipboard(ClipboardItem::new_string(encoded));
                    }
                },
            ))
            .child(preview_button(
                "Import JSON",
                "playtest-import-json",
                cx,
                |this, cx| match cx.read_from_clipboard().and_then(|item| item.text()) {
                    Some(source) => {
                        this.playtest_drawer.import_json(&source);
                    }
                    None => this
                        .playtest_drawer
                        .note("The clipboard has no text to import"),
                },
            ))
            .child(preview_button(
                "Import RON",
                "playtest-import-ron",
                cx,
                |this, cx| match cx.read_from_clipboard().and_then(|item| item.text()) {
                    Some(source) => {
                        this.playtest_drawer.import_ron(&source);
                    }
                    None => this
                        .playtest_drawer
                        .note("The clipboard has no text to import"),
                },
            ))
            .child(preview_button(
                "Replay",
                "playtest-replay",
                cx,
                |this, _cx| {
                    let session = &this.session;
                    this.playtest_drawer.replay(session);
                },
            ));

        let mut body = div()
            .id("playtest-drawer-body")
            .px_3()
            .pb_2()
            .max_h(px(260.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();

        if let Some(mismatch) = self.playtest_drawer.mismatch() {
            body = body.child(
                div()
                    .id("playtest-mismatch")
                    .p_2()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.error))
                    .text_xs()
                    .text_color(rgb(DARK_THEME.text))
                    .child(format!("Replay mismatch: {mismatch}")),
            );
        }
        if let Some(message) = self.playtest_drawer.message() {
            body = body.child(drawer_note(message.to_owned()));
        }

        body = body.child(drawer_section("CURRENT SESSION"));
        body = body.child(drawer_text(format!(
            "{} · seed {}",
            self.session.status().label(),
            self.session.seed()
        )));
        body = body.child(drawer_section("CURRENT HISTORY"));
        if self.session.transcript().is_empty() {
            body = body.child(drawer_note("No transcript yet".to_owned()));
        }
        for entry in self.session.transcript() {
            let text = match entry {
                PreviewTranscript::Line(line) => line.clone(),
                PreviewTranscript::Choice(choice) => format!("› {choice}"),
                PreviewTranscript::Jump(knot) => format!("↪ {knot}"),
            };
            body = body.child(drawer_text(text));
        }

        if let Some(record) = self.playtest_drawer.record() {
            body = body.child(drawer_section("OUTCOME"));
            body = body.child(drawer_text(format!(
                "{} · seed {} · start `{}`",
                record.outcome.summary(),
                record.seed,
                record.start_knot
            )));
            if let Some(outcome) = self.playtest_drawer.replay_outcome() {
                body = body.child(drawer_text(format!(
                    "Replay reproduced: {}",
                    outcome.summary()
                )));
            }

            body = body.child(drawer_section("CHECKPOINTS"));
            for (index, checkpoint) in record.checkpoints.iter().enumerate() {
                body = body.child(drawer_text(format!(
                    "#{index} · event {} · choice {} · `{}`",
                    checkpoint.event, checkpoint.choice, checkpoint.knot
                )));
            }

            body = body.child(drawer_section("HISTORY"));
            for event in &record.events {
                body = body.child(drawer_text(format!("{} · {}", event.kind, event.knot)));
            }

            body = body.child(drawer_section("CUES"));
            for cue in &record.cues {
                body = body.child(drawer_text(format!("{:?} · {}", cue.cue, cue.knot)));
            }
        } else {
            body = body.child(drawer_note(
                "Capture the current playthrough, or import JSON or RON from the clipboard."
                    .to_owned(),
            ));
        }

        body = body.child(drawer_section("VARIABLES"));
        let variables = self.session.variables();
        if variables.is_empty() {
            body = body.child(drawer_note("No variables".to_owned()));
        }
        for (name, value) in variables {
            body = body.child(drawer_text(format!("{name} = {value}")));
        }

        panel.child(controls).child(body).into_any_element()
    }

    fn presentation_drawer_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.presentation_drawer.is_expanded();
        let panel = div()
            .id("presentation-drawer")
            .flex_none()
            .border_t_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .child(
                div()
                    .id("presentation-drawer-toggle")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(if expanded {
                        DARK_THEME.text
                    } else {
                        DARK_THEME.muted_text
                    }))
                    .child(format!(
                        "{} Resolved presentation · V",
                        if expanded { "▾" } else { "▸" }
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.presentation_drawer.toggle();
                        cx.notify();
                    })),
            );
        if !expanded {
            return panel.into_any_element();
        }

        let rows = self.presentation_drawer.view(&self.session);
        let configured = self.presentation_drawer.world().is_some()
            || self.presentation_drawer.character().is_some();
        let mut body = div()
            .id("presentation-drawer-body")
            .px_3()
            .pb_2()
            .max_h(px(220.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();

        body = body.child(drawer_section("REFERENCES"));
        if rows.references.is_empty() {
            body = body.child(drawer_note(if configured {
                "No scene or stage resolved from the configured references.".to_owned()
            } else {
                "No World or Character presentation is configured; Play uses explicit or fallback presentation.".to_owned()
            }));
        }
        for reference in &rows.references {
            let mut text = format!(
                "{} → {} `{}`",
                reference.reference,
                reference.target.label(),
                reference.id
            );
            if let Some(value) = &reference.value {
                text.push_str(" = ");
                text.push_str(value);
            }
            if reference.fallback {
                text.push_str(" (fallback)");
            }
            body = body.child(drawer_text(text));
        }

        if !rows.diagnostics.is_empty() {
            body = body.child(drawer_section("DIAGNOSTICS"));
            for diagnostic in &rows.diagnostics {
                body = body.child(div().text_xs().text_color(rgb(DARK_THEME.warning)).child(
                    format!(
                        "{} · {} — {}",
                        diagnostic.kind.label(),
                        diagnostic.source,
                        diagnostic.detail
                    ),
                ));
            }
        }

        panel.child(body).into_any_element()
    }

    fn preview_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.mode != PlayMode::Play
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
        {
            return;
        }
        match event.keystroke.key.as_str() {
            "space" | "enter" => {
                self.session.run();
            }
            "r" => {
                self.session.restart();
                self.session.run();
            }
            "d" => {
                self.playtest_drawer.toggle();
            }
            "v" => {
                self.presentation_drawer.toggle();
            }
            _ => {
                if let Some(character) = &event.keystroke.key_char
                    && let Some(digit) = character.chars().next().and_then(|c| c.to_digit(10))
                    && digit >= 1
                {
                    self.session.choose(digit as usize - 1);
                }
            }
        }
        cx.notify();
    }

    fn transcript_panel(&self) -> AnyElement {
        let mut transcript = div()
            .id("preview-transcript-scroll")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .pr_3();
        if self.session.transcript().is_empty() {
            transcript = transcript.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("Press Continue to play the current compiled story."),
            );
        }
        for (index, entry) in self.session.transcript().iter().enumerate() {
            let (prefix, text, color) = match entry {
                PreviewTranscript::Line(line) => ("", line.as_str(), DARK_THEME.text),
                PreviewTranscript::Choice(choice) => ("› ", choice.as_str(), DARK_THEME.accent),
                PreviewTranscript::Jump(knot) => ("↪ ", knot.as_str(), DARK_THEME.warning),
            };
            transcript = transcript.child(
                div()
                    .id(("preview-transcript", index))
                    .text_sm()
                    .text_color(rgb(color))
                    .child(format!("{prefix}{text}")),
            );
        }
        transcript.into_any_element()
    }

    fn controls_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let stale = if self.session.is_stale() {
            " · showing last valid build"
        } else {
            ""
        };
        let mut panel = div()
            .w(px(310.0))
            .h_full()
            .flex_none()
            .px_3()
            .border_l_1()
            .border_color(rgb(DARK_THEME.border))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(format!("{}{}", self.session.status().label(), stale)),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(preview_button("Step", "preview-step", cx, |this, cx| {
                        this.step(cx);
                    }))
                    .child(preview_button(
                        "Continue",
                        "preview-continue",
                        cx,
                        |this, cx| {
                            this.run(cx);
                        },
                    ))
                    .child(preview_button(
                        "Restart",
                        "preview-restart",
                        cx,
                        |this, cx| {
                            this.restart(cx);
                        },
                    ))
                    .child(preview_button("Stop", "preview-stop", cx, |this, cx| {
                        this.stop(cx);
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(preview_button(
                        "Seed −",
                        "preview-seed-down",
                        cx,
                        |this, cx| {
                            this.change_seed(-1, cx);
                        },
                    ))
                    .child(
                        div()
                            .px_2()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(self.session.seed().to_string()),
                    )
                    .child(preview_button(
                        "Seed +",
                        "preview-seed-up",
                        cx,
                        |this, cx| {
                            this.change_seed(1, cx);
                        },
                    )),
            );

        for (index, choice) in self.session.choices().into_iter().enumerate() {
            panel = panel.child(
                div()
                    .id(("preview-choice", index))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.accent))
                    .cursor_pointer()
                    .text_xs()
                    .child(choice.text)
                    .on_click(cx.listener(move |this, _, _, cx| this.choose(index, cx))),
            );
        }

        if let Some(issue) = self.session.issue() {
            let span = issue.span;
            panel = panel.child(
                div()
                    .id("preview-issue")
                    .p_2()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.error))
                    .cursor_pointer()
                    .text_xs()
                    .child(issue.message.clone())
                    .when_some(span, |issue, span| {
                        issue.on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(PreviewEvent::RevealSource(span));
                        }))
                    }),
            );
        }

        let mut knots = div()
            .id("preview-knots-scroll")
            .max_h(px(64.0))
            .overflow_y_scroll();
        for (index, knot) in self.session.knots().into_iter().enumerate() {
            let target = knot.clone();
            knots = knots.child(
                div()
                    .id(("preview-knot", index))
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.accent))
                    .child(format!("Replay {knot}"))
                    .on_click(cx.listener(move |this, _, _, cx| this.jump(&target, cx))),
            );
        }
        panel.child(knots).into_any_element()
    }

    fn state_panel(&self) -> AnyElement {
        let mut state = div()
            .id("preview-state-scroll")
            .w(px(310.0))
            .h_full()
            .flex_none()
            .pl_3()
            .border_l_1()
            .border_color(rgb(DARK_THEME.border))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().child("State"));
        let variables = self.session.variables();
        if variables.is_empty() {
            state = state.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("No variables"),
            );
        }
        for (name, value) in variables {
            state = state.child(div().text_xs().child(format!("{name} = {value}")));
        }
        if !self.session.pattern_draws().is_empty() {
            state = state.child(
                div()
                    .mt_2()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("PATTERN DRAWS"),
            );
        }
        for draw in self.session.pattern_draws().iter().rev().take(12) {
            state = state.child(pattern_draw_summary(draw));
        }
        state.into_any_element()
    }
}

impl EventEmitter<PreviewEvent> for PreviewSurface {}

impl gpui::Render for PreviewSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.mode {
            PlayMode::Debug => div()
                .flex_1()
                .min_h_0()
                .p_3()
                .flex()
                .gap_3()
                .child(self.transcript_panel())
                .child(self.controls_panel(cx))
                .child(self.state_panel())
                .into_any_element(),
            PlayMode::Play => self.play_panel(cx),
        };
        div()
            .id("play-preview")
            .key_context("WeavePreview")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(DARK_THEME.panel))
            .border_t_1()
            .border_color(rgb(DARK_THEME.border))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.preview_key(event, cx);
            }))
            .child(self.mode_bar(cx))
            .child(body)
    }
}

fn drawer_section(label: &str) -> AnyElement {
    div()
        .mt_1()
        .text_xs()
        .text_color(rgb(DARK_THEME.muted_text))
        .child(label.to_owned())
        .into_any_element()
}

fn drawer_text(text: String) -> AnyElement {
    div()
        .text_xs()
        .text_color(rgb(DARK_THEME.text))
        .child(text)
        .into_any_element()
}

fn drawer_note(text: String) -> AnyElement {
    div()
        .text_xs()
        .text_color(rgb(DARK_THEME.muted_text))
        .child(text)
        .into_any_element()
}

fn preview_button(
    label: &str,
    id: &'static str,
    cx: &mut Context<PreviewSurface>,
    action: impl Fn(&mut PreviewSurface, &mut Context<PreviewSurface>) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgb(DARK_THEME.chrome))
        .cursor_pointer()
        .text_xs()
        .child(label.to_owned())
        .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
        .into_any_element()
}

/// Negative leading and trailing inset for one axis of an anchored cover frame.
///
/// A cover background is drawn slightly larger than the frame, then shifted so
/// the declared anchor lands near the centre. Both values are bounded by
/// [`SCENE_OVERSCAN`], so the backdrop always covers the frame.
fn anchor_inset(anchor: f64) -> (f32, f32) {
    let anchor = if anchor.is_finite() { anchor } else { 0.5 };
    let anchor = (anchor as f32).clamp(0.0, 1.0);
    (
        -2.0 * SCENE_OVERSCAN * (1.0 - anchor),
        -2.0 * SCENE_OVERSCAN * anchor,
    )
}

/// One character figure, sized as a fraction of its slot column.
///
/// The figure keeps its aspect ratio inside the column, so the scene stays
/// readable when the panel is narrow and does not balloon when it is wide.
fn stage_figure(stage: &StagedCharacter, image: Option<Arc<RenderImage>>) -> AnyElement {
    let figure = match image {
        Some(image) => img(ImageSource::Render(image))
            .w_full()
            .h_full()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        None => div()
            .w_full()
            .h_full()
            .rounded_t_full()
            .bg(rgb(0x1a2030))
            .border_1()
            .border_color(rgb(DARK_THEME.border))
            .into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_end()
        .gap_1()
        .h_full()
        .max_h(px(300.0))
        .w(relative(0.9))
        .min_w(px(24.0))
        .max_w(px(170.0))
        .opacity(PlayScene::stage_opacity(stage))
        .child(div().flex_1().w_full().child(figure))
        .child(
            div()
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child(stage.name.clone()),
        )
        .into_any_element()
}

/// Add one fallback note, ignoring a repeat of a note already shown.
fn push_note(notes: &mut Vec<String>, note: Option<&str>) {
    let Some(note) = note else {
        return;
    };
    if !notes.iter().any(|existing| existing == note) {
        notes.push(note.to_owned());
    }
}

fn pattern_draw_summary(draw: &DrawResult) -> AnyElement {
    let entries = draw
        .entries
        .iter()
        .map(|entry| {
            let name = entry.name().unwrap_or(&entry.id);
            entry
                .position
                .as_ref()
                .map_or_else(|| name.to_owned(), |position| format!("{position}: {name}"))
        })
        .collect::<Vec<_>>()
        .join(" · ");
    div()
        .p_2()
        .rounded_md()
        .bg(rgb(DARK_THEME.chrome))
        .child(
            div()
                .text_xs()
                .text_color(rgb(DARK_THEME.accent))
                .child(draw.system.clone()),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child(entries),
        )
        .into_any_element()
}

fn issue_from_diagnostics(diagnostics: &[Diagnostic]) -> PreviewIssue {
    let first = diagnostics.first();
    PreviewIssue {
        message: first.map_or_else(
            || "Compilation failed without a diagnostic".to_owned(),
            |diagnostic| {
                format!(
                    "{}: {}{}",
                    diagnostic.code,
                    diagnostic.message,
                    if diagnostics.len() > 1 {
                        format!(" (+{} more)", diagnostics.len() - 1)
                    } else {
                        String::new()
                    }
                )
            },
        ),
        span: first.and_then(|diagnostic| diagnostic.span),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play_view::{
        CharacterProjection, PresentationDiagnosticKind, ResolvedTarget, ValueRef,
    };
    use weave_core::ir::{Instruction, InstructionKind};

    const STORY: &str = r#"grammar names {
    value: ["Mira", "Aldric"]
}
pattern runes {
    builtin: elder_futhark
    draw: uniform
    reversals: false
}
VAR score = 0
VAR omen = runes.draw()
=== start ===
Hello, #names.value#.
* [Continue]
    SET score = 1
    -> finish
=== finish ===
Done.
-> END
"#;

    fn session(seed: u64) -> PreviewSession {
        let compiled =
            weave_compiler::compile(STORY, &CompileOptions::default()).expect("story compiles");
        PreviewSession::from_story(compiled.story, seed, 1)
    }

    #[test]
    fn playback_steps_chooses_restarts_and_exposes_state() {
        let mut preview = session(7);
        assert!(preview.run());
        assert_eq!(preview.status(), PreviewStatus::WaitingForChoice);
        assert_eq!(preview.choices().len(), 1);
        assert!(preview.choose(0));
        assert_eq!(preview.status(), PreviewStatus::Ended);
        assert!(
            preview
                .variables()
                .iter()
                .any(|(name, value)| name == "score" && *value == Value::Number(1.0))
        );
        assert!(!preview.pattern_draws().is_empty());
        assert!(preview.restart());
        assert!(preview.transcript().is_empty());
        assert!(preview.jump("finish"));
        assert!(
            preview
                .transcript()
                .iter()
                .any(|entry| matches!(entry, PreviewTranscript::Jump(knot) if knot == "finish"))
        );
    }

    #[test]
    fn seed_replays_are_deterministic() {
        let mut first = session(42);
        first.run();
        let first_transcript = first.transcript().to_vec();
        let first_draws = first.pattern_draws().to_vec();
        first.restart();
        first.run();
        assert_eq!(first.transcript(), first_transcript);
        assert_eq!(first.pattern_draws(), first_draws);
    }

    #[test]
    fn compiler_failure_preserves_last_valid_preview() {
        let mut preview = session(5);
        preview.run();
        let transcript = preview.transcript().to_vec();
        let revision = preview.compiled_revision();
        assert!(!preview.compile_source("=== broken", None, 2));
        assert!(preview.is_stale());
        assert_eq!(preview.compiled_revision(), revision);
        assert_eq!(preview.transcript(), transcript);
        assert!(preview.issue().is_some());
    }

    #[test]
    fn runtime_failure_is_actionable_without_crashing_the_session() {
        let mut preview = session(5);
        let invalid_runtime =
            "VAR score = 0\nVAR divisor = 0\n=== start ===\nSET score = 1 / divisor\n-> END\n";
        assert!(preview.compile_source(invalid_runtime, None, 2));
        assert!(!preview.run());
        assert_eq!(preview.status(), PreviewStatus::Error);
        assert!(
            preview
                .issue()
                .is_some_and(|issue| issue.message.contains("division"))
        );
    }

    #[test]
    fn compile_debounce_uses_the_latest_revision_and_quiet_period() {
        let start = Instant::now();
        let mut debounce = PreviewCompileDebouncer::new(Duration::from_millis(250));
        debounce.schedule(1, start);
        assert_eq!(
            debounce.take_ready(start + Duration::from_millis(249)),
            None
        );
        debounce.schedule(2, start + Duration::from_millis(100));
        assert_eq!(
            debounce.take_ready(start + Duration::from_millis(349)),
            None
        );
        assert_eq!(
            debounce.take_ready(start + Duration::from_millis(350)),
            Some(2)
        );
    }

    #[test]
    fn anchored_cover_insets_are_centred_and_bounded() {
        assert_eq!(anchor_inset(0.5), (-SCENE_OVERSCAN, -SCENE_OVERSCAN));

        let (left, right) = anchor_inset(0.0);
        assert_eq!(left, -2.0 * SCENE_OVERSCAN);
        assert_eq!(right, 0.0);

        let (left, right) = anchor_inset(1.0);
        assert_eq!(left, 0.0);
        assert_eq!(right, -2.0 * SCENE_OVERSCAN);

        assert_eq!(anchor_inset(f64::NAN), anchor_inset(0.5));
        assert_eq!(anchor_inset(-5.0), anchor_inset(0.0));
        assert_eq!(anchor_inset(5.0), anchor_inset(1.0));
    }

    #[test]
    fn a_fallback_note_is_shown_once() {
        let mut notes = Vec::new();
        push_note(&mut notes, Some("stage \"mara\" — the file was not found"));
        push_note(&mut notes, Some("stage \"mara\" — the file was not found"));
        push_note(&mut notes, None);
        assert_eq!(notes.len(), 1);
    }

    const DRAWER_STORY: &str = r#"
scene harbour {
    background: "art/harbour.png"
}

stage mara {
    name: "Mara"
}

VAR place = "harbour"
VAR speaker = "mara"

=== start ===
Mara: The ferry leaves at dusk.
* [Board the ferry]
    SET place = "cabin"
    Mara: Then we cross together.
    -> END
* [Wait for morning]
    -> start
"#;

    const SECRET_STORY: &str = r#"
scene harbour {
    background: "art/harbour.png"
}

VAR place = "never-print-this-token"

=== start ===
The harbour is quiet.
-> END
"#;

    fn drawer_session(source: &str) -> PreviewSession {
        let compiled =
            weave_compiler::compile(source, &CompileOptions::default()).expect("drawer story");
        let mut session = PreviewSession::from_story(compiled.story, 7, 1);
        session.run();
        session
    }

    #[test]
    fn playtest_drawer_captures_and_round_trips_json_and_ron() {
        let preview = drawer_session(DRAWER_STORY);
        let mut drawer = PlaytestDrawer::default();
        assert!(!drawer.is_expanded());
        drawer.toggle();
        assert!(drawer.is_expanded());

        let transcript = preview.transcript().to_vec();
        let cues = preview.presentation().to_vec();
        let choices = preview.choices();
        let variables = preview.variables();

        assert!(drawer.capture(&preview));
        let record = drawer.record().cloned().expect("captured record");
        assert_eq!(record.seed, 7);
        assert_eq!(record.start_knot, "start");
        assert!(record.outcome.summary().contains("waiting for choice"));

        assert!(drawer.export_json());
        let json = drawer.json().to_owned();
        assert!(json.contains("\"format_version\": 1"));
        assert!(drawer.export_ron());
        let ron = drawer.ron().to_owned();
        assert!(ron.contains("format_version: 1"));

        let mut imported = PlaytestDrawer::default();
        assert!(imported.import_json(&json));
        assert_eq!(imported.record(), Some(&record));
        assert!(imported.import_ron(&ron));
        assert_eq!(imported.record(), Some(&record));

        assert_eq!(preview.transcript(), transcript);
        assert_eq!(preview.presentation(), cues);
        assert_eq!(preview.choices(), choices);
        assert_eq!(preview.variables(), variables);
    }

    #[test]
    fn playtest_drawer_surfaces_the_first_replay_mismatch() {
        let preview = drawer_session(DRAWER_STORY);
        let mut drawer = PlaytestDrawer::default();
        assert!(drawer.capture(&preview));

        let mut changed = preview.story().cloned().expect("compiled story");
        changed
            .knots
            .get_mut("start")
            .expect("start knot")
            .content
            .push(Instruction {
                span: Span::new(0, 0, 1, 1),
                kind: InstructionKind::End,
            });

        assert!(!drawer.replay_against(&changed));
        assert!(
            drawer
                .mismatch()
                .is_some_and(|mismatch| mismatch.contains("story content changed"))
        );
        assert!(drawer.replay_outcome().is_none());

        assert!(drawer.replay(&preview));
        assert!(drawer.mismatch().is_none());
        assert!(drawer.replay_outcome().is_some());
    }

    #[test]
    fn presentation_drawer_shows_world_and_character_references() {
        let preview = drawer_session(DRAWER_STORY);
        let mut drawer = PresentationDrawer::default();
        assert!(!drawer.is_expanded());
        drawer.toggle();
        assert!(drawer.is_expanded());

        drawer.set_world(Some(WorldPresentation {
            selector: ValueRef::Variable {
                name: "place".to_owned(),
            },
            scenes: BTreeMap::from([("harbour".to_owned(), "harbour".to_owned())]),
            fallback: Some("harbour".to_owned()),
        }));
        drawer.set_character(Some(CharacterPresentation {
            selector: ValueRef::Variable {
                name: "speaker".to_owned(),
            },
            projections: BTreeMap::from([(
                "mara".to_owned(),
                CharacterProjection {
                    display_name: Some("Mara Vale".to_owned()),
                    stage: Some("mara".to_owned()),
                    expression: None,
                    portrait: None,
                },
            )]),
            stage_ids: BTreeMap::from([("mara".to_owned(), "mara".to_owned())]),
            expressions: BTreeMap::new(),
            fallback_stage: Some("mara".to_owned()),
        }));

        let rows = drawer.view(&preview);
        assert!(rows.diagnostics.is_empty());
        assert_eq!(rows.references.len(), 2);

        let scene = rows
            .references
            .iter()
            .find(|row| row.target == ResolvedTarget::Scene)
            .expect("world-driven scene row");
        assert_eq!(scene.reference, "variable `place`");
        assert_eq!(scene.id, "harbour");
        assert_eq!(scene.value.as_deref(), Some("harbour"));
        assert!(!scene.fallback);

        let stage = rows
            .references
            .iter()
            .find(|row| row.target == ResolvedTarget::Stage)
            .expect("character-driven stage row");
        assert_eq!(stage.reference, "variable `speaker`");
        assert_eq!(stage.id, "mara");
        assert_eq!(stage.value.as_deref(), Some("mara"));
        assert!(!stage.fallback);
    }

    #[test]
    fn presentation_drawer_shows_a_missing_reference_fallback_with_its_diagnostic() {
        let preview = drawer_session(DRAWER_STORY);
        let mut drawer = PresentationDrawer::default();
        drawer.set_world(Some(WorldPresentation {
            selector: ValueRef::Variable {
                name: "absent".to_owned(),
            },
            scenes: BTreeMap::new(),
            fallback: Some("harbour".to_owned()),
        }));

        let rows = drawer.view(&preview);
        let reference = &rows.references[0];
        assert_eq!(reference.id, "harbour");
        assert!(reference.fallback);
        assert_eq!(reference.value, None);
        assert_eq!(
            rows.diagnostics
                .iter()
                .map(|diagnostic| diagnostic.kind)
                .collect::<Vec<_>>(),
            [
                PresentationDiagnosticKind::UnavailableValue,
                PresentationDiagnosticKind::Fallback
            ]
        );
        assert_eq!(rows.diagnostics[0].kind.label(), "unavailable_value");
        assert_eq!(rows.diagnostics[0].source, "variable `absent`");
        assert_eq!(rows.diagnostics[1].source, "variable `absent`");
    }

    #[test]
    fn presentation_drawer_never_echoes_a_rejected_value() {
        let preview = drawer_session(SECRET_STORY);
        let mut drawer = PresentationDrawer::default();
        drawer.set_world(Some(WorldPresentation {
            selector: ValueRef::Variable {
                name: "place".to_owned(),
            },
            scenes: BTreeMap::new(),
            fallback: Some("harbour".to_owned()),
        }));

        let rows = drawer.view(&preview);
        let reference = &rows.references[0];
        assert_eq!(reference.id, "harbour");
        assert!(reference.fallback);
        assert_eq!(reference.value, None);
        assert_eq!(
            rows.diagnostics
                .iter()
                .map(|diagnostic| diagnostic.kind)
                .collect::<Vec<_>>(),
            [
                PresentationDiagnosticKind::UnmappedValue,
                PresentationDiagnosticKind::Fallback
            ]
        );
        assert!(!format!("{rows:?}").contains("never-print-this-token"));
        for diagnostic in &rows.diagnostics {
            assert!(!diagnostic.source.contains("never-print-this-token"));
            assert!(!diagnostic.detail.contains("never-print-this-token"));
        }
    }
}
