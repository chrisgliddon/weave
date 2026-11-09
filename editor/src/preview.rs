//! Debounced compiler integration and isolated live story playback panel.

// The loader lives in its own file beside this module. It is declared here with
// an explicit path so the new file stays adjacent to `preview.rs` in the source
// tree. A crate root declaration in `lib.rs` would make the path
// `crate::presentation_projection` instead.
#[path = "presentation_projection.rs"]
pub mod presentation_projection;

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

use self::presentation_projection::{
    PresentationInputIssue, PresentationInputIssueKind, load_adjacent_presentation_inputs,
};
use crate::play_view::{
    AutoAdvancePolicy, AutoAdvanceState, CharacterPresentation, DEFAULT_TEXT_SCALE,
    DIALOGUE_BASE_PX, DIALOGUE_MAX_HEIGHT_PX, MotionPreference, NARRATOR, PLAY_KEYBOARD_HINT,
    PlayAction, PlayDiagnostic, PlayDiagnosticSeverity, PlayFocus, PlayFocusContext,
    PlayFocusTarget, PlayMode, PresentationDrawerRows, REVEAL_SPEED_STEP, SLOT_ORDER,
    StagedCharacter, TextReveal, WorldPresentation, focus_indicator_colour,
    focus_indicator_colour_on_fill, has_critical_diagnostics, on_accent_text_colour,
    play_diagnostics, play_focus_order_for, play_key_for, presentation_drawer_rows,
    resolve_presentation, scaled_dialogue_height, scaled_text_px, scene_from_session, scene_tone,
    step_text_scale,
};
use crate::playtest::{PlaytestOutcome, PlaytestRecord};
use crate::presentation_assets::PresentationAssets;
use crate::theme::DARK_THEME;

const MAX_CONTINUE_BOUNDARIES: usize = 10_000;

/// Extra fraction of the frame an anchored cover background extends past each
/// edge, so the declared anchor can bias the crop in any direction.
const SCENE_OVERSCAN: f32 = 0.16;

/// Maximum events, cues, or checkpoints the playtest drawer renders at once.
pub const PLAYTEST_DRAWER_WINDOW: usize = 200;

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

    /// Number of pending choices without cloning their text.
    #[must_use]
    pub fn choice_count(&self) -> usize {
        self.runtime
            .as_ref()
            .map_or(0, |runtime| runtime.choices().len())
    }

    /// Number of knots in the last valid story.
    #[must_use]
    pub fn knot_count(&self) -> usize {
        self.last_valid_story
            .as_ref()
            .map_or(0, |story| story.knots.len())
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
        if index >= self.choices().len() {
            return false;
        }
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
    /// Outcome summary computed once when the record is captured or imported.
    outcome_summary: Option<String>,
    /// Replay outcome summary computed once when replay succeeds.
    replay_summary: Option<String>,
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

    /// Force the drawer body open or closed.
    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
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

    /// Record outcome summary, computed when the record was captured or imported.
    #[must_use]
    pub fn outcome_summary(&self) -> Option<&str> {
        self.outcome_summary.as_deref()
    }

    /// Replay outcome summary, computed when the last replay succeeded.
    #[must_use]
    pub fn replay_summary(&self) -> Option<&str> {
        self.replay_summary.as_deref()
    }

    /// First bounded window of recorded events and the count omitted.
    #[must_use]
    pub fn event_window(&self) -> (Vec<String>, usize) {
        self.record.as_ref().map_or((Vec::new(), 0), |record| {
            let shown = record.events.len().min(PLAYTEST_DRAWER_WINDOW);
            let rows = record.events[..shown]
                .iter()
                .map(|event| format!("{} · {}", event.kind, event.knot))
                .collect();
            (rows, record.events.len() - shown)
        })
    }

    /// First bounded window of recorded cues and the count omitted.
    #[must_use]
    pub fn cue_window(&self) -> (Vec<String>, usize) {
        self.record.as_ref().map_or((Vec::new(), 0), |record| {
            let shown = record.cues.len().min(PLAYTEST_DRAWER_WINDOW);
            let rows = record.cues[..shown]
                .iter()
                .map(|cue| format!("{:?} · {}", cue.cue, cue.knot))
                .collect();
            (rows, record.cues.len() - shown)
        })
    }

    /// First bounded window of checkpoints and the count omitted.
    #[must_use]
    pub fn checkpoint_window(&self) -> (Vec<String>, usize) {
        self.record.as_ref().map_or((Vec::new(), 0), |record| {
            let shown = record.checkpoints.len().min(PLAYTEST_DRAWER_WINDOW);
            let rows = record.checkpoints[..shown]
                .iter()
                .enumerate()
                .map(|(index, checkpoint)| {
                    format!(
                        "#{index} · event {} · choice {} · `{}`",
                        checkpoint.event, checkpoint.choice, checkpoint.knot
                    )
                })
                .collect();
            (rows, record.checkpoints.len() - shown)
        })
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
                self.outcome_summary = Some(record.outcome.summary());
                self.record = Some(record);
                self.mismatch = None;
                self.replay_outcome = None;
                self.replay_summary = None;
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
                self.outcome_summary = Some(record.outcome.summary());
                self.record = Some(record);
                self.json = source.to_owned();
                self.mismatch = None;
                self.replay_outcome = None;
                self.replay_summary = None;
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
                self.outcome_summary = Some(record.outcome.summary());
                self.record = Some(record);
                self.ron = source.to_owned();
                self.mismatch = None;
                self.replay_outcome = None;
                self.replay_summary = None;
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
                self.replay_summary = Some(report.outcome.summary());
                self.replay_outcome = Some(report.outcome.clone());
                self.mismatch = None;
                self.message = Some("Replayed the record against this story".to_owned());
                true
            }
            Err(error) => {
                self.replay_outcome = None;
                self.replay_summary = None;
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
    input_issues: Vec<PresentationInputIssue>,
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

    /// Force the drawer body open or closed.
    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
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

    /// Problems found while loading the optional presentation inputs.
    #[must_use]
    pub fn input_issues(&self) -> &[PresentationInputIssue] {
        &self.input_issues
    }

    fn set_input_issues(&mut self, issues: Vec<PresentationInputIssue>) {
        self.input_issues = issues;
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
    /// Logical focus order shared by the scene and the author controls.
    play_focus: PlayFocus,
    /// Whether the diagnostics drawer body is visible.
    diagnostics_expanded: bool,
    /// Requested motion preference.
    motion: MotionPreference,
    /// Dialogue text scale, clamped into the documented steps.
    text_scale: f32,
    /// Dialogue reveal timing.
    text_reveal: TextReveal,
    /// Bounded auto-advance progress.
    auto_advance: AutoAdvanceState,
    /// Line the reveal clock belongs to.
    reveal_line: String,
    /// Characters visible at the last tick.
    revealed_chars: usize,
    /// When the current line began revealing.
    line_started: Instant,
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
            play_focus: PlayFocus::default(),
            diagnostics_expanded: false,
            motion: MotionPreference::Full,
            text_scale: DEFAULT_TEXT_SCALE,
            text_reveal: TextReveal::default(),
            auto_advance: AutoAdvanceState::default(),
            reveal_line: String::new(),
            revealed_chars: 0,
            line_started: Instant::now(),
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
        self.refresh_presentation_inputs(cx);
    }

    /// Reload optional World and Character presentation inputs beside the project.
    ///
    /// The adjacent `weave.presentation.json` file is optional. Missing or
    /// invalid input is recorded on the presentation drawer and shown in the
    /// diagnostics panel; playback keeps using the compiled story and the
    /// documented no-reference fallbacks. The loader reads the file on every
    /// call, so a source edit that changes a selector value, or an edited input
    /// file, produces the same plan for the same bytes.
    ///
    /// Returns whether the inputs changed and the surface repainted.
    pub fn refresh_presentation_inputs(&mut self, cx: &mut Context<Self>) -> bool {
        let inputs = load_adjacent_presentation_inputs(self.assets.root());
        let changed = {
            let drawer = &self.presentation_drawer;
            drawer.world() != inputs.world.as_ref()
                || drawer.character() != inputs.character.as_ref()
                || drawer.input_issues() != inputs.issues.as_slice()
        };
        if !changed {
            return false;
        }
        self.set_world_presentation(inputs.world, cx);
        self.set_character_presentation(inputs.character, cx);
        self.presentation_drawer.set_input_issues(inputs.issues);
        cx.notify();
        true
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
        let refreshed = self.refresh_presentation_inputs(cx);
        changed || refreshed
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

    /// Currently focused Play control.
    #[must_use]
    pub const fn focused_control(&self) -> PlayFocusTarget {
        self.play_focus.current()
    }

    /// Whether the diagnostics drawer body is visible.
    #[must_use]
    pub const fn diagnostics_expanded(&self) -> bool {
        self.diagnostics_expanded
    }

    /// Current motion preference.
    #[must_use]
    pub const fn motion(&self) -> MotionPreference {
        self.motion
    }

    /// Apply a reduced-motion preference.
    pub fn set_reduced_motion(&mut self, reduced: bool, cx: &mut Context<Self>) {
        self.motion = if reduced {
            MotionPreference::Reduced
        } else {
            MotionPreference::Full
        };
        cx.notify();
    }

    /// Current dialogue text scale.
    #[must_use]
    pub const fn text_scale(&self) -> f32 {
        self.text_scale
    }

    /// Apply a dialogue text scale, clamped into the documented steps.
    pub fn set_text_scale(&mut self, scale: f32, cx: &mut Context<Self>) {
        self.text_scale = crate::play_view::clamp_text_scale(scale);
        cx.notify();
    }

    /// Current reveal timing.
    #[must_use]
    pub const fn text_reveal(&self) -> TextReveal {
        self.text_reveal
    }

    /// Replace reveal timing and restart the current line.
    pub fn set_text_reveal(&mut self, reveal: TextReveal, cx: &mut Context<Self>) {
        self.text_reveal = reveal;
        self.revealed_chars = 0;
        self.line_started = Instant::now();
        cx.notify();
    }

    /// Current auto-advance policy.
    #[must_use]
    pub const fn auto_advance(&self) -> AutoAdvancePolicy {
        self.auto_advance.policy()
    }

    /// Replace the bounded auto-advance policy.
    pub fn set_auto_advance(&mut self, policy: AutoAdvancePolicy, cx: &mut Context<Self>) {
        self.auto_advance.set_policy(policy);
        cx.notify();
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
        self.tick_playback(now, cx);
        let Some(revision) = self.debouncer.take_ready(now) else {
            return false;
        };
        let Some(pending) = self.pending.take() else {
            return false;
        };
        if pending.revision != revision {
            return false;
        }
        self.refresh_presentation_inputs(cx);
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
        self.refresh_presentation_inputs(cx);
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

    fn mode_bar(&self, focused: PlayFocusTarget, cx: &mut Context<Self>) -> AnyElement {
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
            let target = PlayFocusTarget::Mode(mode);
            bar = bar.child(
                div()
                    .id(match mode {
                        PlayMode::Debug => "preview-mode-debug",
                        PlayMode::Play => "preview-mode-play",
                    })
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(if active {
                        focus_indicator_colour_on_fill(focused == target)
                    } else {
                        focus_indicator_colour(focused == target)
                    }))
                    .role(gpui::Role::Tab)
                    .aria_label(mode.label())
                    .aria_selected(active)
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(if active {
                        on_accent_text_colour()
                    } else {
                        DARK_THEME.muted_text
                    }))
                    .when(active, |element| element.bg(rgb(DARK_THEME.accent)))
                    .child(mode.label())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.set_mode(mode, cx);
                        this.play_focus.focus(target);
                        this.reconcile_play_focus();
                    })),
            );
        }
        bar.child(
            div()
                .ml_auto()
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child(PLAY_KEYBOARD_HINT),
        )
        .into_any_element()
    }

    fn play_panel(&mut self, focused: PlayFocusTarget, cx: &mut Context<Self>) -> AnyElement {
        let scene = scene_from_session(&self.session);
        let now = Instant::now();
        if self.reveal_line != scene.line {
            self.reveal_line = scene.line.clone();
            self.line_started = now;
            self.revealed_chars = 0;
        }
        let elapsed = now.saturating_duration_since(self.line_started);
        let reveal = self.motion.reveal(self.text_reveal);
        let revealed_line = if scene.line.is_empty() {
            "Press Space to begin".to_owned()
        } else if self.reveal_line == scene.line {
            reveal.reveal(&scene.line, elapsed)
        } else {
            scene.line.clone()
        };

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
            let opacity = self.motion.background_opacity(&scene);
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
                    self.motion,
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
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(rgb(DARK_THEME.panel))
                    .text_xs()
                    .text_color(rgb(DARK_THEME.text))
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
            .max_h(px(scaled_dialogue_height(
                DIALOGUE_MAX_HEIGHT_PX,
                self.text_scale,
            )))
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .border_1()
            .border_color(rgb(focus_indicator_colour(
                focused == PlayFocusTarget::Advance,
            )))
            .role(gpui::Role::Group)
            .aria_label("Dialogue")
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.activate_play_target(PlayFocusTarget::Advance, cx);
            }))
            .child(
                div()
                    .text_size(px(scaled_text_px(DIALOGUE_BASE_PX, self.text_scale)))
                    .text_color(rgb(DARK_THEME.text))
                    .child(revealed_line),
            );

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
                    .child(preview_button(
                        "Restart",
                        "play-restart",
                        focused == PlayFocusTarget::Restart,
                        cx,
                        |this, cx| {
                            this.session.restart();
                            this.session.run();
                            cx.notify();
                        },
                    )),
            );
        } else if scene.choices.is_empty() {
            dialogue = dialogue.child(preview_button(
                "Continue",
                "play-continue",
                focused == PlayFocusTarget::Advance,
                cx,
                |this, cx| {
                    this.activate_play_target(PlayFocusTarget::Advance, cx);
                },
            ));
        } else if !scene.choices.is_empty() {
            for (index, choice) in scene.choices.iter().enumerate() {
                dialogue = dialogue.child(
                    div()
                        .id(("play-choice", index))
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(focus_indicator_colour(
                            focused == PlayFocusTarget::Choice(index),
                        )))
                        .bg(rgb(DARK_THEME.chrome))
                        .role(gpui::Role::Button)
                        .aria_label(choice_label(index, &choice.text))
                        .cursor_pointer()
                        .text_size(px(scaled_text_px(13.0, self.text_scale)))
                        .text_color(rgb(DARK_THEME.text))
                        .child(format!(
                            "{}. {}",
                            index + 1,
                            choice_label(index, &choice.text)
                        ))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.focus.focus(window, cx);
                            this.session.choose(index);
                            this.play_focus.focus(PlayFocusTarget::Advance);
                            this.reconcile_play_focus();
                            cx.notify();
                        })),
                );
            }
        }

        frame
            .child(stage_area)
            .child(dialogue)
            .child(self.playtest_drawer_panel(focused, cx))
            .child(self.presentation_drawer_panel(focused, cx))
            .child(self.diagnostics_panel(focused, cx))
            .into_any_element()
    }

    fn playtest_drawer_panel(
        &self,
        focused: PlayFocusTarget,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                    .border_1()
                    .border_color(rgb(focus_indicator_colour(
                        focused == PlayFocusTarget::PlaytestToggle,
                    )))
                    .role(gpui::Role::Button)
                    .aria_label("Playtest drawer")
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
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.playtest_drawer.toggle();
                        this.play_focus.focus(PlayFocusTarget::PlaytestToggle);
                        this.reconcile_play_focus();
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
                focused == PlayFocusTarget::PlaytestCapture,
                cx,
                |this, _cx| {
                    let session = &this.session;
                    this.playtest_drawer.capture(session);
                },
            ))
            .child(preview_button(
                "Export JSON",
                "playtest-export-json",
                focused == PlayFocusTarget::PlaytestExportJson,
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
                focused == PlayFocusTarget::PlaytestExportRon,
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
                focused == PlayFocusTarget::PlaytestImportJson,
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
                focused == PlayFocusTarget::PlaytestImportRon,
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
                focused == PlayFocusTarget::PlaytestReplay,
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
                self.playtest_drawer
                    .outcome_summary()
                    .unwrap_or("unknown outcome"),
                record.seed,
                record.start_knot
            )));
            if let Some(outcome) = self.playtest_drawer.replay_summary() {
                body = body.child(drawer_text(format!("Replay reproduced: {outcome}")));
            }

            body = body.child(drawer_section("CHECKPOINTS"));
            let (checkpoints, omitted) = self.playtest_drawer.checkpoint_window();
            for checkpoint in checkpoints {
                body = body.child(drawer_text(checkpoint));
            }
            if omitted > 0 {
                body = body.child(drawer_note(format!("{omitted} more checkpoints not shown")));
            }

            body = body.child(drawer_section("HISTORY"));
            let (events, omitted) = self.playtest_drawer.event_window();
            for event in events {
                body = body.child(drawer_text(event));
            }
            if omitted > 0 {
                body = body.child(drawer_note(format!("{omitted} more events not shown")));
            }

            body = body.child(drawer_section("CUES"));
            let (cues, omitted) = self.playtest_drawer.cue_window();
            for cue in cues {
                body = body.child(drawer_text(cue));
            }
            if omitted > 0 {
                body = body.child(drawer_note(format!("{omitted} more cues not shown")));
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

    fn presentation_drawer_panel(
        &self,
        focused: PlayFocusTarget,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                    .border_1()
                    .border_color(rgb(focus_indicator_colour(
                        focused == PlayFocusTarget::PresentationToggle,
                    )))
                    .role(gpui::Role::Button)
                    .aria_label("Resolved presentation drawer")
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
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.presentation_drawer.toggle();
                        this.play_focus.focus(PlayFocusTarget::PresentationToggle);
                        this.reconcile_play_focus();
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

        if !self.presentation_drawer.input_issues().is_empty() {
            body = body.child(drawer_section("INPUTS"));
            for issue in self.presentation_drawer.input_issues() {
                body = body.child(drawer_note(format!(
                    "{} · {} — {}",
                    issue.kind.label(),
                    issue.source,
                    issue.detail
                )));
            }
        }

        if !rows.diagnostics.is_empty() {
            body = body.child(drawer_section("DIAGNOSTICS"));
            for diagnostic in &rows.diagnostics {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(rgb(DARK_THEME.text))
                        .child(format!(
                            "{} · {} — {}",
                            diagnostic.kind.label(),
                            diagnostic.source,
                            diagnostic.detail
                        )),
                );
            }
        }

        panel.child(body).into_any_element()
    }

    /// Diagnostics drawer below the scene and dialogue.
    ///
    /// The body sits in the author-control column, so a critical diagnostic
    /// stays reachable without covering the playable frame.
    fn diagnostics_panel(&self, focused: PlayFocusTarget, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.diagnostics_expanded;
        let scene = scene_from_session(&self.session);
        let rows = self.presentation_drawer.view(&self.session);
        let mut diagnostics = play_diagnostics(&scene, Some(&rows));
        diagnostics.extend(self.presentation_drawer.input_issues().iter().map(|issue| {
            PlayDiagnostic {
                severity: match issue.kind {
                    PresentationInputIssueKind::Missing => PlayDiagnosticSeverity::Note,
                    PresentationInputIssueKind::Invalid => PlayDiagnosticSeverity::Warning,
                },
                summary: format!(
                    "{} · {} — {}",
                    issue.kind.label(),
                    issue.source,
                    issue.detail
                ),
            }
        }));
        let critical = has_critical_diagnostics(&diagnostics);
        let summary = if diagnostics.is_empty() {
            String::new()
        } else if critical {
            format!(" · {} critical", diagnostics.len())
        } else {
            format!(" · {}", diagnostics.len())
        };
        let panel = div()
            .id("play-diagnostics")
            .flex_none()
            .border_t_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .child(
                div()
                    .id("play-diagnostics-toggle")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .border_1()
                    .border_color(rgb(focus_indicator_colour(
                        focused == PlayFocusTarget::DiagnosticsToggle,
                    )))
                    .role(gpui::Role::Button)
                    .aria_label("Diagnostics drawer")
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(if expanded {
                        DARK_THEME.text
                    } else {
                        DARK_THEME.muted_text
                    }))
                    .child(format!(
                        "{} Diagnostics · I{summary}",
                        if expanded { "▾" } else { "▸" }
                    ))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.diagnostics_expanded = !this.diagnostics_expanded;
                        this.play_focus.focus(PlayFocusTarget::DiagnosticsToggle);
                        this.reconcile_play_focus();
                        cx.notify();
                    })),
            );
        if !expanded {
            return panel.into_any_element();
        }

        let mut body = div()
            .id("play-diagnostics-body")
            .px_3()
            .pb_2()
            .max_h(px(160.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        if diagnostics.is_empty() {
            body = body.child(drawer_note("No diagnostics".to_owned()));
        }
        for diagnostic in &diagnostics {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.text))
                    .child(format!(
                        "{} · {}",
                        diagnostic.severity.label(),
                        diagnostic.summary
                    )),
            );
        }
        panel.child(body).into_any_element()
    }

    fn preview_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let Some(key) = play_key_for(
            &keystroke.key,
            keystroke.key_char.as_deref(),
            keystroke.modifiers.shift,
            keystroke.modifiers.control,
            keystroke.modifiers.alt,
            keystroke.modifiers.platform,
        ) else {
            return;
        };
        let order = self.focus_order_now();
        let action = self.play_focus.handle_key(key, &order);
        self.apply_play_action(action, cx);
        self.reconcile_play_focus();
        cx.notify();
    }

    fn focus_order_now(&self) -> Vec<PlayFocusTarget> {
        let choice_count = self.session.choice_count();
        play_focus_order_for(
            self.session.status() == PreviewStatus::Ended,
            choice_count,
            PlayFocusContext {
                mode: self.mode,
                playtest_expanded: self.playtest_drawer.is_expanded(),
                debug_choices: choice_count,
                debug_knots: self.session.knot_count(),
                has_issue: self.session.issue().is_some(),
            },
        )
    }

    fn reconcile_play_focus(&mut self) {
        let order = self.focus_order_now();
        self.play_focus.reconcile(&order);
    }

    fn reset_reveal(&mut self, now: Instant) {
        self.reveal_line.clear();
        self.line_started = now;
        self.revealed_chars = 0;
    }

    fn after_advance(&mut self) {
        self.reset_reveal(Instant::now());
        self.auto_advance.reset();
    }

    fn tick_playback(&mut self, now: Instant, cx: &mut Context<Self>) {
        let scene = scene_from_session(&self.session);
        if self.reveal_line != scene.line {
            self.reveal_line = scene.line.clone();
            self.line_started = now;
            self.revealed_chars = 0;
        }

        let reveal = self.motion.reveal(self.text_reveal);
        let elapsed = now.saturating_duration_since(self.line_started);
        let visible = reveal.visible_chars(&scene.line, elapsed);
        if visible != self.revealed_chars {
            self.revealed_chars = visible;
            cx.notify();
        }

        if self.mode == PlayMode::Play && self.auto_advance.can_advance(&scene, elapsed) {
            self.auto_advance.record_advance();
            self.session.step();
            // Keep the auto-advance count across the steps it owns. The
            // bound is reset only when the author acts.
            self.reset_reveal(now);
            cx.notify();
        }
    }

    fn apply_play_action(&mut self, action: PlayAction, cx: &mut Context<Self>) {
        match action {
            PlayAction::FocusChanged(_) => {}
            PlayAction::Activate(target) => self.activate_play_target(target, cx),
            PlayAction::Choose(index) => {
                if index >= self.session.choices().len() {
                    return;
                }
                self.session.choose(index);
                self.after_advance();
                if self.mode == PlayMode::Play {
                    self.play_focus.focus(PlayFocusTarget::Advance);
                }
            }
            PlayAction::Restart => {
                self.session.restart();
                self.session.run();
                self.after_advance();
                self.play_focus.focus(PlayFocusTarget::Advance);
            }
            PlayAction::TogglePlaytest => {
                let opening = !self.playtest_drawer.is_expanded();
                self.playtest_drawer.toggle();
                if opening {
                    self.presentation_drawer.set_expanded(false);
                    self.diagnostics_expanded = false;
                }
                self.play_focus.focus(PlayFocusTarget::PlaytestToggle);
            }
            PlayAction::TogglePresentation => {
                let opening = !self.presentation_drawer.is_expanded();
                self.presentation_drawer.toggle();
                if opening {
                    self.playtest_drawer.set_expanded(false);
                    self.diagnostics_expanded = false;
                }
                self.play_focus.focus(PlayFocusTarget::PresentationToggle);
            }
            PlayAction::ToggleDiagnostics => {
                let opening = !self.diagnostics_expanded;
                self.diagnostics_expanded = opening;
                if opening {
                    self.playtest_drawer.set_expanded(false);
                    self.presentation_drawer.set_expanded(false);
                }
                self.play_focus.focus(PlayFocusTarget::DiagnosticsToggle);
            }
            PlayAction::StepTextScale(direction) => {
                self.text_scale = step_text_scale(self.text_scale, direction);
            }
            PlayAction::ToggleMotion => {
                self.motion = self.motion.toggled();
            }
            PlayAction::SkipReveal => {
                self.text_reveal = self.text_reveal.with_instant(!self.text_reveal.instant);
                self.revealed_chars = self
                    .text_reveal
                    .visible_chars(&self.reveal_line, Duration::ZERO);
            }
            PlayAction::StepRevealSpeed(direction) => {
                let speed = if direction > 0 {
                    self.text_reveal
                        .chars_per_second
                        .saturating_add(REVEAL_SPEED_STEP)
                } else {
                    self.text_reveal
                        .chars_per_second
                        .saturating_sub(REVEAL_SPEED_STEP)
                };
                self.text_reveal = self.text_reveal.with_speed(speed);
            }
            PlayAction::ToggleAutoAdvance => {
                self.auto_advance.toggle();
            }
        }
    }

    fn activate_play_target(&mut self, target: PlayFocusTarget, cx: &mut Context<Self>) {
        match target {
            PlayFocusTarget::Mode(mode) => {
                self.set_mode(mode, cx);
                self.play_focus.focus(match mode {
                    PlayMode::Play => PlayFocusTarget::Advance,
                    PlayMode::Debug => PlayFocusTarget::Mode(PlayMode::Debug),
                });
            }
            PlayFocusTarget::Advance => {
                self.session.run();
                self.after_advance();
            }
            PlayFocusTarget::Restart => {
                self.session.restart();
                self.session.run();
                self.after_advance();
                self.play_focus.focus(PlayFocusTarget::Advance);
            }
            PlayFocusTarget::Choice(index) | PlayFocusTarget::DebugChoice(index) => {
                self.session.choose(index);
                self.after_advance();
                if self.mode == PlayMode::Play {
                    self.play_focus.focus(PlayFocusTarget::Advance);
                }
            }
            PlayFocusTarget::DebugStep => {
                self.session.step();
                self.after_advance();
            }
            PlayFocusTarget::DebugContinue => {
                self.session.run();
                self.after_advance();
            }
            PlayFocusTarget::DebugRestart => {
                self.session.restart();
                self.after_advance();
            }
            PlayFocusTarget::DebugStop => {
                self.session.stop();
            }
            PlayFocusTarget::DebugSeedDown => {
                self.session.change_seed(-1);
                self.after_advance();
            }
            PlayFocusTarget::DebugSeedUp => {
                self.session.change_seed(1);
                self.after_advance();
            }
            PlayFocusTarget::DebugKnot(index) => {
                let knot = self.session.knots().get(index).cloned();
                if let Some(knot) = knot {
                    self.session.jump(&knot);
                }
                self.after_advance();
            }
            PlayFocusTarget::DebugIssue => {
                let span = self.session.issue().and_then(|issue| issue.span);
                if let Some(span) = span {
                    cx.emit(PreviewEvent::RevealSource(span));
                }
            }
            PlayFocusTarget::PlaytestToggle => {
                self.playtest_drawer.toggle();
            }
            PlayFocusTarget::PlaytestCapture => {
                let session = &self.session;
                self.playtest_drawer.capture(session);
            }
            PlayFocusTarget::PlaytestExportJson => {
                if self.playtest_drawer.export_json() {
                    let encoded = self.playtest_drawer.json().to_owned();
                    cx.write_to_clipboard(ClipboardItem::new_string(encoded));
                }
            }
            PlayFocusTarget::PlaytestExportRon => {
                if self.playtest_drawer.export_ron() {
                    let encoded = self.playtest_drawer.ron().to_owned();
                    cx.write_to_clipboard(ClipboardItem::new_string(encoded));
                }
            }
            PlayFocusTarget::PlaytestImportJson => {
                match cx.read_from_clipboard().and_then(|item| item.text()) {
                    Some(source) => {
                        self.playtest_drawer.import_json(&source);
                    }
                    None => self
                        .playtest_drawer
                        .note("The clipboard has no text to import"),
                }
            }
            PlayFocusTarget::PlaytestImportRon => {
                match cx.read_from_clipboard().and_then(|item| item.text()) {
                    Some(source) => {
                        self.playtest_drawer.import_ron(&source);
                    }
                    None => self
                        .playtest_drawer
                        .note("The clipboard has no text to import"),
                }
            }
            PlayFocusTarget::PlaytestReplay => {
                let session = &self.session;
                self.playtest_drawer.replay(session);
            }
            PlayFocusTarget::PresentationToggle => {
                self.presentation_drawer.toggle();
            }
            PlayFocusTarget::DiagnosticsToggle => {
                self.diagnostics_expanded = !self.diagnostics_expanded;
            }
        }
        self.reconcile_play_focus();
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
                PreviewTranscript::Jump(knot) => ("↪ ", knot.as_str(), DARK_THEME.text),
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

    fn controls_panel(&self, focused: PlayFocusTarget, cx: &mut Context<Self>) -> AnyElement {
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
                    .child(preview_button(
                        "Step",
                        "preview-step",
                        focused == PlayFocusTarget::DebugStep,
                        cx,
                        |this, cx| {
                            this.step(cx);
                        },
                    ))
                    .child(preview_button(
                        "Continue",
                        "preview-continue",
                        focused == PlayFocusTarget::DebugContinue,
                        cx,
                        |this, cx| {
                            this.run(cx);
                        },
                    ))
                    .child(preview_button(
                        "Restart",
                        "preview-restart",
                        focused == PlayFocusTarget::DebugRestart,
                        cx,
                        |this, cx| {
                            this.restart(cx);
                        },
                    ))
                    .child(preview_button(
                        "Stop",
                        "preview-stop",
                        focused == PlayFocusTarget::DebugStop,
                        cx,
                        |this, cx| {
                            this.stop(cx);
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(preview_button(
                        "Seed −",
                        "preview-seed-down",
                        focused == PlayFocusTarget::DebugSeedDown,
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
                        focused == PlayFocusTarget::DebugSeedUp,
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
                    .border_1()
                    .border_color(rgb(focus_indicator_colour_on_fill(
                        focused == PlayFocusTarget::DebugChoice(index),
                    )))
                    .bg(rgb(DARK_THEME.accent))
                    .text_color(rgb(on_accent_text_colour()))
                    .role(gpui::Role::Button)
                    .aria_label(choice_label(index, &choice.text))
                    .cursor_pointer()
                    .text_xs()
                    .child(choice_label(index, &choice.text))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.choose(index, cx);
                        this.reconcile_play_focus();
                    })),
            );
        }

        if let Some(issue) = self.session.issue() {
            let span = issue.span;
            panel = panel.child(
                div()
                    .id("preview-issue")
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(focus_indicator_colour(
                        focused == PlayFocusTarget::DebugIssue,
                    )))
                    .bg(rgb(DARK_THEME.error))
                    .role(gpui::Role::Button)
                    .aria_label(issue.message.clone())
                    .cursor_pointer()
                    .text_xs()
                    .child(issue.message.clone())
                    .when_some(span, |issue, span| {
                        issue.on_click(cx.listener(move |this, _, window, cx| {
                            this.focus.focus(window, cx);
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
                    .border_1()
                    .border_color(rgb(focus_indicator_colour(
                        focused == PlayFocusTarget::DebugKnot(index),
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(format!("Replay {knot}"))
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.accent))
                    .child(format!("Replay {knot}"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.jump(&target, cx);
                    })),
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
        let order = self.focus_order_now();
        let focused = self.play_focus.reconcile(&order);
        let body = match self.mode {
            PlayMode::Debug => div()
                .flex_1()
                .min_h_0()
                .p_3()
                .flex()
                .gap_3()
                .child(self.transcript_panel())
                .child(self.controls_panel(focused, cx))
                .child(self.state_panel())
                .into_any_element(),
            PlayMode::Play => self.play_panel(focused, cx),
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
            .child(self.mode_bar(focused, cx))
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

fn choice_label(index: usize, text: &str) -> String {
    if text.trim().is_empty() {
        format!("Choice {}", index + 1)
    } else {
        text.to_owned()
    }
}

fn preview_button(
    label: &str,
    id: &'static str,
    focused: bool,
    cx: &mut Context<PreviewSurface>,
    action: impl Fn(&mut PreviewSurface, &mut Context<PreviewSurface>) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label.to_owned())
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(focus_indicator_colour(focused)))
        .bg(rgb(DARK_THEME.chrome))
        .cursor_pointer()
        .text_xs()
        .child(label.to_owned())
        .on_click(cx.listener(move |this, _, window, cx| {
            this.focus.focus(window, cx);
            action(this, cx);
        }))
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
fn stage_figure(
    stage: &StagedCharacter,
    image: Option<Arc<RenderImage>>,
    motion: MotionPreference,
) -> AnyElement {
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
        .opacity(motion.stage_opacity(stage))
        .child(div().flex_1().w_full().child(figure))
        .child(
            div()
                .px_2()
                .rounded_sm()
                .bg(rgb(DARK_THEME.panel))
                .text_xs()
                .text_color(rgb(DARK_THEME.text))
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
    use crate::playtest::{PlaytestCue, PlaytestEvent, PlaytestEventKind};
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
    fn empty_choice_labels_receive_a_stable_fallback() {
        assert_eq!(choice_label(0, ""), "Choice 1");
        assert_eq!(choice_label(2, "   "), "Choice 3");
        assert_eq!(choice_label(1, "Leave"), "Leave");
    }

    #[test]
    fn an_out_of_range_choice_is_ignored_without_failing_the_session() {
        let mut preview = session(7);
        assert!(preview.run());
        assert_eq!(preview.status(), PreviewStatus::WaitingForChoice);
        assert!(!preview.choose(9));
        assert_eq!(preview.status(), PreviewStatus::WaitingForChoice);
        assert!(preview.choose(0));
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
        assert!(
            drawer
                .outcome_summary()
                .is_some_and(|summary| summary.contains("waiting for choice"))
        );

        assert!(drawer.export_json());
        let json = drawer.json().to_owned();
        assert!(json.contains("\"format_version\": 1"));
        assert!(drawer.export_ron());
        let ron = drawer.ron().to_owned();
        assert!(ron.contains("format_version: 1"));

        let mut imported = PlaytestDrawer::default();
        assert!(imported.import_json(&json));
        assert_eq!(imported.record(), Some(&record));
        assert!(imported.outcome_summary().is_some());
        assert!(imported.import_ron(&ron));
        assert_eq!(imported.record(), Some(&record));
        assert!(imported.outcome_summary().is_some());

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

    #[test]
    fn playtest_drawer_renders_a_bounded_window_for_a_large_record() {
        let preview = drawer_session(DRAWER_STORY);
        let mut drawer = PlaytestDrawer::default();
        assert!(drawer.capture(&preview));

        let mut record = drawer.record().cloned().expect("captured record");
        while record.events.len() < PLAYTEST_DRAWER_WINDOW + 5 {
            record.events.push(PlaytestEvent {
                kind: PlaytestEventKind::Line {
                    text: "line".to_owned(),
                },
                knot: "start".to_owned(),
                span: None,
            });
        }
        while record.cues.len() < PLAYTEST_DRAWER_WINDOW + 3 {
            record.cues.push(PlaytestCue {
                cue: PresentationEvent::Scene("harbour".to_owned()),
                knot: "start".to_owned(),
                span: None,
            });
        }
        drawer.record = Some(record);

        let (events, omitted_events) = drawer.event_window();
        assert_eq!(events.len(), PLAYTEST_DRAWER_WINDOW);
        assert_eq!(omitted_events, 5);

        let (cues, omitted_cues) = drawer.cue_window();
        assert_eq!(cues.len(), PLAYTEST_DRAWER_WINDOW);
        assert_eq!(omitted_cues, 3);
    }
}
