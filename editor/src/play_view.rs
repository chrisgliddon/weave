//! Play mode: the preview session presented as a scene rather than a transcript.
//!
//! The scene is derived from the same session the debug transcript reads, so
//! the two views cannot disagree about the story state. This module holds the
//! derived data and the naming rules; the surface only draws it.
//!
//! Presentation cues are folded in source order into [`PlayScene`]. The fold is
//! pure: it reads declared presentation data through [`PresentationLookup`] and
//! never touches the runtime, the clock, or the frame rate. A transition is
//! recorded for the next staged operation and can only affect how that
//! operation is drawn, never which stages are visible or what they show.
//!
//! Cue order is deterministic. The runtime emits one cue list per story step in
//! source order, and [`fold_cues`] applies that list in order. Stages are stored
//! in first-show order and drawn by [`ordered_stages`], which sorts by paint
//! layer and then by that stable insertion order.
//!
//! A transition that is interrupted before it can settle degrades to a
//! documented fallback:
//!
//! - a later transition replaces an earlier one that has not been applied yet;
//! - a removal of a visible stage consumes the pending transition and settles
//!   immediately, because a stage that has left the frame has nothing left to
//!   animate;
//! - a cue that changes no frame — a move, swap, or hide that targets a hidden
//!   stage — leaves the transition pending for the next operation that does;
//! - a transition left at the end of a cue list changes no staging and is
//!   exposed on the scene, where a renderer may ignore it or draw it.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use weave_core::ir::{AnchorIr, DomainValueIr, Fit, Slot, StoryIr, TransitionKind};
use weave_runtime::{ChoiceView, PresentationEvent, Value};

use crate::preview::{PreviewSession, PreviewStatus, PreviewTranscript};
use crate::theme::DARK_THEME;

/// Which presentation the preview panel uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlayMode {
    /// The diagnostic transcript.
    #[default]
    Debug,
    /// The staged scene.
    Play,
}

impl PlayMode {
    /// Toggle label for the other mode.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Debug => Self::Play,
            Self::Play => Self::Debug,
        }
    }

    /// Mode name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Debug => "Debug",
            Self::Play => "Play",
        }
    }
}

/// Longest transition the presentation contract allows.
pub const MAX_TRANSITION_MS: u16 = 2_000;

/// Bounded transition recorded for the next staged operation.
///
/// The duration is presentation data. It never delays a story event, changes
/// story state, or depends on the frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayTransition {
    /// Requested transition style.
    pub kind: TransitionKind,
    /// Nominal duration in milliseconds, already clamped to the contract.
    pub ms: u16,
}

impl PlayTransition {
    /// Record one transition, clamping the duration to the contract bound.
    #[must_use]
    pub const fn new(kind: TransitionKind, ms: u16) -> Self {
        Self {
            kind,
            ms: if ms > MAX_TRANSITION_MS {
                MAX_TRANSITION_MS
            } else {
                ms
            },
        }
    }

    /// Kind name used by renderers and notes.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self.kind {
            TransitionKind::Cut => "cut",
            TransitionKind::Fade => "fade",
            TransitionKind::Slide => "slide",
            TransitionKind::Wipe => "wipe",
        }
    }

    /// Opacity a clock-free renderer applies to the element that just changed.
    ///
    /// A cut is immediate. This surface has no transition clock, so every kind settles at the
    /// final staging opacity instead of leaving a figure permanently dimmed. A renderer with a
    /// clock may interpolate to `1.0` over [`PlayTransition::ms`] and reach the same end state.
    #[must_use]
    pub const fn opacity(self) -> f32 {
        match self.kind {
            TransitionKind::Cut
            | TransitionKind::Slide
            | TransitionKind::Fade
            | TransitionKind::Wipe => 1.0,
        }
    }
}

/// One visible character stage in the current frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedCharacter {
    /// Declared stage identifier.
    pub id: String,
    /// Author-facing display name, defaulting to the identifier.
    pub name: String,
    /// Horizontal slot the stage currently occupies.
    pub slot: Slot,
    /// Paint order declared by the stage, from `0` to `9`.
    pub layer: u8,
    /// Current image path, base or variant, when one is declared.
    pub image: Option<String>,
    /// Variant that supplied the current image, when one did.
    pub variant: Option<String>,
    /// Stable insertion order, used to break layer ties.
    pub order: usize,
    /// Transition that introduced this stage's current visual.
    pub transition: Option<PlayTransition>,
}

/// Presentation state folded from one ordered cue list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PresentationState {
    /// Staged background scene id, when a cue set one.
    pub background: Option<String>,
    /// Fit declared by the staged background, or the contract default.
    pub fit: Fit,
    /// Anchor declared by the staged background, or the contract default.
    pub anchor: AnchorIr,
    /// Visible stages in cue insertion order.
    pub stages: Vec<StagedCharacter>,
    /// Transition recorded for the next staged operation.
    pub transition: Option<PlayTransition>,
    /// Transition that introduced the current background.
    pub background_transition: Option<PlayTransition>,
    /// Speaker declared by the last speaker cue; outer `None` means no cue.
    pub speaker: Option<Option<String>>,
    /// Fallback notes raised while folding, in cue order.
    pub notes: Vec<String>,
}

/// Declared presentation data a cue fold needs.
///
/// Every method is total: a host that cannot resolve a declaration still
/// produces a complete frame from the contract defaults.
pub trait PresentationLookup {
    /// Fit declared by one scene.
    fn scene_fit(&self, id: &str) -> Option<Fit>;
    /// Anchor declared by one scene.
    fn scene_anchor(&self, id: &str) -> Option<AnchorIr>;
    /// Display name declared by one stage.
    fn stage_name(&self, id: &str) -> Option<String>;
    /// Slot declared by one stage.
    fn stage_slot(&self, id: &str) -> Option<Slot>;
    /// Layer declared by one stage.
    fn stage_layer(&self, id: &str) -> Option<u8>;
    /// Base image declared by one stage.
    fn stage_image(&self, id: &str) -> Option<&str>;
    /// Image declared for one named stage variant.
    fn stage_variant_image(&self, stage: &str, variant: &str) -> Option<&str>;
}

/// Everything the Play surface draws, derived from one preview session.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayScene {
    /// Speaker of the current line.
    pub speaker: String,
    /// Current line, without its speaker prefix.
    pub line: String,
    /// Choices waiting for the author.
    pub choices: Vec<ChoiceView>,
    /// Staged background scene id, when a cue set one.
    pub background: Option<String>,
    /// Fit declared by the staged background.
    pub fit: Fit,
    /// Anchor declared by the staged background.
    pub anchor: AnchorIr,
    /// Every visible stage, in cue insertion order.
    pub stages: Vec<StagedCharacter>,
    /// Visible stage most recently added to the frame.
    pub stage: Option<String>,
    /// Transition recorded for the next staged operation.
    pub transition: Option<PlayTransition>,
    /// Transition that introduced the current background.
    pub background_transition: Option<PlayTransition>,
    /// Session status.
    pub status: PreviewStatus,
    /// Whether the story has ended.
    pub ended: bool,
    /// Diagnostic shown without discarding the last valid frame.
    pub issue: Option<String>,
    /// Fallback notes for the current frame, in cue order.
    pub notes: Vec<String>,
}

impl PlayScene {
    /// Visible stages ordered by layer, then by stable insertion order.
    #[must_use]
    pub fn ordered_stages(&self) -> Vec<&StagedCharacter> {
        ordered_stages(&self.stages)
    }

    /// Visible stages in one slot, ordered by layer then insertion order.
    #[must_use]
    pub fn stages_in_slot(&self, slot: Slot) -> Vec<&StagedCharacter> {
        stages_in_slot(&self.stages, slot)
    }

    /// Opacity the surface applies to one stage figure.
    #[must_use]
    pub fn stage_opacity(stage: &StagedCharacter) -> f32 {
        stage.transition.map_or(1.0, PlayTransition::opacity)
    }

    /// Opacity the surface applies to the background image.
    #[must_use]
    pub fn background_opacity(&self) -> f32 {
        self.background_transition
            .map_or(1.0, PlayTransition::opacity)
    }
}

/// Visible stages ordered by paint layer, then by stable insertion order.
#[must_use]
pub fn ordered_stages(stages: &[StagedCharacter]) -> Vec<&StagedCharacter> {
    let mut ordered: Vec<&StagedCharacter> = stages.iter().collect();
    ordered.sort_by_key(|stage| (stage.layer, stage.order));
    ordered
}

/// Visible stages in one slot ordered by paint layer, then insertion order.
#[must_use]
pub fn stages_in_slot(stages: &[StagedCharacter], slot: Slot) -> Vec<&StagedCharacter> {
    let mut ordered: Vec<&StagedCharacter> =
        stages.iter().filter(|stage| stage.slot == slot).collect();
    ordered.sort_by_key(|stage| (stage.layer, stage.order));
    ordered
}

/// The name used when a line has no explicit speaker.
pub const NARRATOR: &str = "Narrator";

/// The normalized horizontal grid, left to right.
pub const SLOT_ORDER: [Slot; 5] = [
    Slot::FarLeft,
    Slot::Left,
    Slot::Center,
    Slot::Right,
    Slot::FarRight,
];

/// Left-to-right position of each slot on the normalized grid.
pub const SLOT_CENTRES: [f32; 5] = [0.1, 0.3, 0.5, 0.7, 0.9];

/// Column index of one slot on the normalized grid.
#[must_use]
pub const fn slot_index(slot: Slot) -> usize {
    match slot {
        Slot::FarLeft => 0,
        Slot::Left => 1,
        Slot::Center => 2,
        Slot::Right => 3,
        Slot::FarRight => 4,
    }
}

/// Normalized horizontal centre of one slot.
#[must_use]
pub const fn slot_fraction(slot: Slot) -> f32 {
    match slot {
        Slot::FarLeft => SLOT_CENTRES[0],
        Slot::Left => SLOT_CENTRES[1],
        Slot::Center => SLOT_CENTRES[2],
        Slot::Right => SLOT_CENTRES[3],
        Slot::FarRight => SLOT_CENTRES[4],
    }
}

/// Flat background tones the Play surface uses while a declared scene has no
/// decoded image.
///
/// The cue carries an asset reference. When the loader has no frame yet, the
/// surface renders a distinct synthetic tone per scene id, so a cue change is
/// visible without pretending an image was loaded.
pub const SCENE_TONES: [u32; 4] = [0x101722, 0x171420, 0x141d1c, 0x1d1814];

/// Pick the synthetic tone for a background cue.
#[must_use]
pub fn scene_tone(scene: Option<&str>) -> u32 {
    let Some(scene) = scene else {
        return 0x0e1118;
    };
    let hash = scene.bytes().fold(0_u32, |accumulator, byte| {
        accumulator.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    SCENE_TONES[hash as usize % SCENE_TONES.len()]
}

/// Keep a focal point inside the image.
///
/// The compiler rejects anchors outside `0..=1`, but a compiled story may come
/// from another producer. Clamping keeps a decoded anchor on the image so the
/// background still fills the frame. A non-finite component has no meaningful
/// direction, so it falls back to the centred default.
#[must_use]
fn sanitize_anchor(anchor: AnchorIr) -> AnchorIr {
    fn component(value: f64) -> f64 {
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            AnchorIr::default().x
        }
    }
    AnchorIr {
        x: component(anchor.x),
        y: component(anchor.y),
    }
}

/// Fold one ordered cue list into presentation state.
///
/// Cues apply in source order, which is the only ordering input. A swap
/// replaces only the named stage's current image; when the stage does not
/// declare the variant, the previous image is kept and a note names the stage
/// and the variant. A transition is recorded for the next staged operation and
/// never changes staging itself. A removal of a visible stage consumes the
/// pending transition and settles immediately; a move, swap, or hide that
/// changes no frame leaves the transition pending.
#[must_use]
pub fn fold_cues(cues: &[PresentationEvent], lookup: &dyn PresentationLookup) -> PresentationState {
    let mut state = PresentationState::default();
    let mut insertion = 0_usize;

    for cue in cues {
        match cue {
            PresentationEvent::Scene(id) => {
                state.background = Some(id.clone());
                state.fit = lookup.scene_fit(id).unwrap_or_default();
                state.anchor = sanitize_anchor(lookup.scene_anchor(id).unwrap_or_default());
                state.background_transition = state.transition.take();
            }
            PresentationEvent::Show(id) => {
                let slot = lookup.stage_slot(id).unwrap_or_default();
                let layer = lookup.stage_layer(id).unwrap_or_default();
                let name = lookup.stage_name(id).unwrap_or_else(|| id.clone());
                let image = lookup.stage_image(id).map(str::to_owned);
                let pending = state.transition.take();
                if let Some(stage) = state.stages.iter_mut().find(|stage| stage.id == *id) {
                    stage.slot = slot;
                    stage.layer = layer;
                    stage.name = name;
                    stage.image = image;
                    stage.variant = None;
                    stage.transition = pending;
                } else {
                    state.stages.push(StagedCharacter {
                        id: id.clone(),
                        name,
                        slot,
                        layer,
                        image,
                        variant: None,
                        order: insertion,
                        transition: pending,
                    });
                    insertion += 1;
                }
            }
            PresentationEvent::Hide(id) => {
                let before = state.stages.len();
                state.stages.retain(|stage| stage.id != *id);
                if state.stages.len() != before {
                    // A visible removal is a staged operation. A stage that has
                    // left the frame has no visual left to record, so the
                    // pending transition is consumed rather than leaking onto
                    // a later cue. Hiding a stage that is not visible changes
                    // no frame, so it leaves the transition pending like a
                    // move or swap on a hidden stage.
                    state.transition = None;
                }
            }
            PresentationEvent::Speaker(name) => {
                state.speaker = Some(name.clone());
            }
            PresentationEvent::Move { stage, to } => {
                let pending = state.transition.take();
                if let Some(target) = state.stages.iter_mut().find(|target| target.id == *stage) {
                    target.slot = *to;
                    target.transition = pending;
                } else {
                    // A hidden stage has no frame to move, so the transition
                    // stays pending for the next operation that does change
                    // the frame.
                    state.transition = pending;
                }
            }
            PresentationEvent::Swap { stage, variant } => {
                let pending = state.transition.take();
                let position = state.stages.iter().position(|target| target.id == *stage);
                if let Some(position) = position {
                    match lookup.stage_variant_image(stage, variant) {
                        Some(path) => {
                            state.stages[position].image = Some(path.to_owned());
                            state.stages[position].variant = Some(variant.clone());
                        }
                        None => state.notes.push(format!(
                            "stage \"{stage}\" · variant \"{variant}\" — the stage does not declare this variant; keeping the current image"
                        )),
                    }
                    state.stages[position].transition = pending;
                } else {
                    state.transition = pending;
                }
            }
            PresentationEvent::Transition { kind, ms } => {
                state.transition = Some(PlayTransition::new(*kind, *ms));
            }
        }
    }

    state
}

/// Build the scene for the current session state.
///
/// The scene reads the same runtime and transcript the debug view reads, so
/// advancing, choosing, restarting, and jumping produce the same state in both
/// presentations.
#[must_use]
pub fn scene_from_session(session: &PreviewSession) -> PlayScene {
    let raw = session
        .transcript()
        .iter()
        .rev()
        .find_map(|entry| match entry {
            PreviewTranscript::Line(line) => Some(line.clone()),
            PreviewTranscript::Choice(_) | PreviewTranscript::Jump(_) => None,
        })
        .unwrap_or_default();
    let (prose_speaker, line) = split_speaker(&raw);

    let presentation = fold_cues(session.presentation(), session);
    let speaker = match &presentation.speaker {
        Some(Some(id)) => session.stage_name(id).unwrap_or_else(|| id.clone()),
        Some(None) => NARRATOR.to_owned(),
        None => prose_speaker,
    };
    let stage = presentation
        .stages
        .iter()
        .max_by_key(|stage| stage.order)
        .map(|stage| stage.id.clone());

    PlayScene {
        speaker,
        line,
        choices: session.choices(),
        background: presentation.background,
        fit: presentation.fit,
        anchor: presentation.anchor,
        stages: presentation.stages,
        stage,
        transition: presentation.transition,
        background_transition: presentation.background_transition,
        status: session.status(),
        ended: session.status() == PreviewStatus::Ended,
        issue: session.issue().map(|issue| issue.message.clone()),
        notes: presentation.notes,
    }
}

impl PresentationLookup for PreviewSession {
    fn scene_fit(&self, id: &str) -> Option<Fit> {
        PreviewSession::scene_fit(self, id)
    }

    fn scene_anchor(&self, id: &str) -> Option<AnchorIr> {
        PreviewSession::scene_anchor(self, id)
    }

    fn stage_name(&self, id: &str) -> Option<String> {
        PreviewSession::stage_name(self, id)
    }

    fn stage_slot(&self, id: &str) -> Option<Slot> {
        PreviewSession::stage_slot(self, id)
    }

    fn stage_layer(&self, id: &str) -> Option<u8> {
        PreviewSession::stage_layer(self, id)
    }

    fn stage_image(&self, id: &str) -> Option<&str> {
        PreviewSession::stage_image(self, id)
    }

    fn stage_variant_image(&self, stage: &str, variant: &str) -> Option<&str> {
        PreviewSession::stage_variant_image(self, stage, variant)
    }
}

/// Split a leading `Speaker: ` prefix from a line.
///
/// Weave lines carry no speaker field, so Play names a line the way prose does:
/// a short prefix ending in a colon before the text. Anything else is
/// narration, which keeps a diagnostic or a long sentence from becoming a
/// character name.
#[must_use]
pub fn split_speaker(line: &str) -> (String, String) {
    let trimmed = line.trim_start();
    if let Some(index) = trimmed.find(": ")
        && index > 0
        && index <= 40
    {
        let candidate = &trimmed[..index];
        let words: Vec<&str> = candidate.split_whitespace().collect();
        let capitalised = words.iter().all(|word| {
            word.chars()
                .next()
                .is_some_and(|first| first.is_uppercase())
        });
        let clean = !candidate.contains(['.', '!', '?', ',', ';'])
            && !words.is_empty()
            && words.len() <= 3
            && capitalised
            && candidate.chars().all(|character| {
                character.is_alphanumeric() || character.is_whitespace() || character == '-'
            });
        if clean {
            return (
                candidate.to_owned(),
                trimmed[index + 2..].trim_start().to_owned(),
            );
        }
    }
    (NARRATOR.to_owned(), line.trim_start().to_owned())
}

/// Current version of the resolved presentation plan.
pub const RESOLVED_PRESENTATION_FORMAT_VERSION: u32 = 1;

/// One typed value a presentation selector reads.
///
/// A story variable is read from the optional runtime snapshot. A module value
/// is read from the compiled story's active exports. The host names the value
/// once; Play core never assumes a module schema field name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ValueRef {
    /// Declared story variable.
    Variable {
        /// Variable name.
        name: String,
    },
    /// Namespaced domain-module export path.
    Module {
        /// Story-local module alias.
        alias: String,
        /// Export name followed by nested field names.
        path: Vec<String>,
    },
}

impl ValueRef {
    /// Redaction-safe label naming the reference, never its value.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Variable { name } => format!("variable `{name}`"),
            Self::Module { alias, path } if path.is_empty() => format!("module `{alias}`"),
            Self::Module { alias, path } => {
                format!("module `{alias}` export `{}`", path.join("."))
            }
        }
    }
}

/// World-driven scene selection supplied by the host.
///
/// The host chooses which world value selects the background and how each
/// value maps to a declared scene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldPresentation {
    /// World place or environment value that selects the background.
    pub selector: ValueRef,
    /// Selected world value to declared scene id.
    pub scenes: BTreeMap<String, String>,
    /// Declared scene used when the selected value is absent or unknown.
    pub fallback: Option<String>,
}

/// Typed presentation values for one character.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterProjection {
    /// Author-facing display name.
    pub display_name: Option<String>,
    /// Stage identity declared by the projection.
    pub stage: Option<String>,
    /// Expression identity declared by the projection.
    pub expression: Option<String>,
    /// Portrait asset path declared by the projection.
    pub portrait: Option<String>,
}

/// Character-driven stage selection supplied by the host.
///
/// Projections, stage identities, and expression identities come from typed
/// character data. The host supplies the explicit identity-to-declaration
/// maps, so Play core stays independent of any character schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterPresentation {
    /// Value that names the current character's stable id.
    pub selector: ValueRef,
    /// Typed projections keyed by stable character id.
    pub projections: BTreeMap<String, CharacterProjection>,
    /// Projection stage identity to declared stage id.
    pub stage_ids: BTreeMap<String, String>,
    /// Projection expression identity to declared stage variant.
    pub expressions: BTreeMap<String, String>,
    /// Declared stage used when no projection resolves.
    pub fallback_stage: Option<String>,
}

/// Category of one resolution diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationDiagnosticKind {
    /// The module alias is not active in the compiled story.
    UnknownModule,
    /// The module does not export the named value.
    UnknownExport,
    /// The module value has no such nested field.
    UnknownPath,
    /// No current value is available for the reference.
    UnavailableValue,
    /// The value is not text, or an intermediate value is not an object.
    IncompatibleType,
    /// The selected world value has no host scene mapping.
    UnmappedValue,
    /// The story does not declare the selected or fallback scene.
    UnknownScene,
    /// No character projection is supplied for the selected character.
    UnavailableProjection,
    /// The projected stage identity has no host stage mapping.
    UnmappedStage,
    /// The story does not declare the selected or fallback stage.
    UnknownStage,
    /// The projected expression has no host variant mapping.
    UnmappedExpression,
    /// The resolved stage does not declare the mapped variant.
    UnknownVariant,
    /// A documented fallback supplied the current selection.
    Fallback,
}

impl PresentationDiagnosticKind {
    /// Short author-facing name for the diagnostic category.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UnknownModule => "unknown_module",
            Self::UnknownExport => "unknown_export",
            Self::UnknownPath => "unknown_path",
            Self::UnavailableValue => "unavailable_value",
            Self::IncompatibleType => "incompatible_type",
            Self::UnmappedValue => "unmapped_value",
            Self::UnknownScene => "unknown_scene",
            Self::UnavailableProjection => "unavailable_projection",
            Self::UnmappedStage => "unmapped_stage",
            Self::UnknownStage => "unknown_stage",
            Self::UnmappedExpression => "unmapped_expression",
            Self::UnknownVariant => "unknown_variant",
            Self::Fallback => "fallback",
        }
    }
}

/// One precise, redaction-safe resolution diagnostic.
///
/// `source` names the reference and `detail` explains the failure. Neither
/// field echoes a selected value, so a variable or module value that contains
/// sensitive text never reaches a diagnostic or the serialized plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationDiagnostic {
    /// Diagnostic category.
    pub kind: PresentationDiagnosticKind,
    /// Redaction-safe reference label.
    pub source: String,
    /// Redaction-safe explanation.
    pub detail: String,
}

/// Selected background scene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedScene {
    /// Declared scene id.
    pub id: String,
    /// Reference that selected it.
    pub source: ValueRef,
    /// Whether a documented fallback supplied it.
    pub fallback: bool,
}

/// Selected character stage after projection resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedStage {
    /// Declared stage id.
    pub id: String,
    /// Author-facing display name.
    pub name: String,
    /// Image after variant or portrait resolution.
    pub image: Option<String>,
    /// Variant that supplied the image.
    pub variant: Option<String>,
    /// Reference that selected the character.
    pub source: ValueRef,
    /// Whether a documented fallback supplied the stage.
    pub fallback: bool,
}

/// Complete resolved presentation plan for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedPresentation {
    /// Serialized plan version.
    pub format_version: u32,
    /// Selected background scene.
    pub scene: Option<ResolvedScene>,
    /// Selected character stage.
    pub stage: Option<ResolvedStage>,
    /// Diagnostics in deterministic resolution order.
    pub diagnostics: Vec<PresentationDiagnostic>,
}

impl ResolvedPresentation {
    /// Empty plan at the current format version.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            format_version: RESOLVED_PRESENTATION_FORMAT_VERSION,
            scene: None,
            stage: None,
            diagnostics: Vec::new(),
        }
    }

    /// Encode as stable, pretty JSON.
    pub fn to_json(&self) -> Result<String, PresentationFormatError> {
        let mut output = serde_json::to_string_pretty(self)
            .map_err(|error| PresentationFormatError::JsonEncode(error.to_string()))?;
        output.push('\n');
        Ok(output)
    }

    /// Decode stable JSON and reject an unsupported format version.
    pub fn from_json(source: &str) -> Result<Self, PresentationFormatError> {
        let plan = serde_json::from_str::<Self>(source)
            .map_err(|error| PresentationFormatError::JsonDecode(error.to_string()))?;
        plan.validate_version()?;
        Ok(plan)
    }

    /// Encode as stable, pretty RON.
    pub fn to_ron(&self) -> Result<String, PresentationFormatError> {
        let config = ron::ser::PrettyConfig::new()
            .new_line("\n")
            .indentor("    ")
            .struct_names(false)
            .enumerate_arrays(false)
            .compact_arrays(false)
            .compact_maps(false);
        let mut output = ron::ser::to_string_pretty(self, config)
            .map_err(|error| PresentationFormatError::RonEncode(error.to_string()))?;
        output.push('\n');
        Ok(output)
    }

    /// Decode stable RON and reject an unsupported format version.
    pub fn from_ron(source: &str) -> Result<Self, PresentationFormatError> {
        let plan = ron::from_str::<Self>(source)
            .map_err(|error| PresentationFormatError::RonDecode(error.to_string()))?;
        plan.validate_version()?;
        Ok(plan)
    }

    fn validate_version(&self) -> Result<(), PresentationFormatError> {
        if self.format_version == RESOLVED_PRESENTATION_FORMAT_VERSION {
            Ok(())
        } else {
            Err(PresentationFormatError::UnsupportedVersion {
                found: self.format_version,
                expected: RESOLVED_PRESENTATION_FORMAT_VERSION,
            })
        }
    }
}

/// Failure while encoding or decoding a resolved presentation plan.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PresentationFormatError {
    /// JSON encoding failed.
    #[error("presentation JSON could not be encoded: {0}")]
    JsonEncode(String),
    /// JSON decoding failed.
    #[error("presentation JSON could not be decoded: {0}")]
    JsonDecode(String),
    /// RON encoding failed.
    #[error("presentation RON could not be encoded: {0}")]
    RonEncode(String),
    /// RON decoding failed.
    #[error("presentation RON could not be decoded: {0}")]
    RonDecode(String),
    /// The plan format version is not supported.
    #[error("unsupported presentation format version {found}; expected {expected}")]
    UnsupportedVersion {
        /// Version found in the encoded plan.
        found: u32,
        /// Version this build supports.
        expected: u32,
    },
}

/// Resolve presentation from a compiled story and optional host inputs.
///
/// Resolution is pure and deterministic. World selection resolves before
/// character selection, and every collection is ordered. Optional inputs that
/// are absent produce no diagnostics and leave that part of the plan empty, so
/// a project with no World or Character module still runs Play mode with
/// explicit or fallback presentation.
#[must_use]
pub fn resolve_presentation(
    story: &StoryIr,
    world: Option<&WorldPresentation>,
    character: Option<&CharacterPresentation>,
    variables: Option<&BTreeMap<String, Value>>,
) -> ResolvedPresentation {
    let mut plan = ResolvedPresentation::empty();
    if let Some(world) = world {
        resolve_world(story, world, variables, &mut plan);
    }
    if let Some(character) = character {
        resolve_character(story, character, variables, &mut plan);
    }
    plan
}

fn resolve_world(
    story: &StoryIr,
    world: &WorldPresentation,
    variables: Option<&BTreeMap<String, Value>>,
    plan: &mut ResolvedPresentation,
) {
    let selected = resolve_text(&world.selector, story, variables, &mut plan.diagnostics);
    let scene = selected.and_then(|value| world.scenes.get(value).cloned());
    match scene {
        Some(id) if story.presentation.scenes.contains_key(&id) => {
            plan.scene = Some(ResolvedScene {
                id,
                source: world.selector.clone(),
                fallback: false,
            });
        }
        Some(id) => {
            plan.diagnostics.push(diagnostic(
                PresentationDiagnosticKind::UnknownScene,
                &world.selector,
                format!("scene `{id}` is not declared by the story"),
            ));
            apply_scene_fallback(story, world, plan);
        }
        None => {
            if selected.is_some() {
                plan.diagnostics.push(diagnostic(
                    PresentationDiagnosticKind::UnmappedValue,
                    &world.selector,
                    "the selected value has no host scene mapping",
                ));
            }
            apply_scene_fallback(story, world, plan);
        }
    }
}

fn apply_scene_fallback(
    story: &StoryIr,
    world: &WorldPresentation,
    plan: &mut ResolvedPresentation,
) {
    let Some(id) = world.fallback.as_deref() else {
        return;
    };
    if !story.presentation.scenes.contains_key(id) {
        plan.diagnostics.push(diagnostic(
            PresentationDiagnosticKind::UnknownScene,
            &world.selector,
            format!("configured fallback scene `{id}` is not declared by the story"),
        ));
        return;
    }
    plan.scene = Some(ResolvedScene {
        id: id.to_owned(),
        source: world.selector.clone(),
        fallback: true,
    });
    plan.diagnostics.push(diagnostic(
        PresentationDiagnosticKind::Fallback,
        &world.selector,
        format!("using fallback scene `{id}`"),
    ));
}

fn resolve_character(
    story: &StoryIr,
    character: &CharacterPresentation,
    variables: Option<&BTreeMap<String, Value>>,
    plan: &mut ResolvedPresentation,
) {
    let Some(character_id) =
        resolve_text(&character.selector, story, variables, &mut plan.diagnostics)
    else {
        apply_stage_fallback(story, character, None, plan);
        return;
    };
    let Some(projection) = character.projections.get(character_id) else {
        plan.diagnostics.push(diagnostic(
            PresentationDiagnosticKind::UnavailableProjection,
            &character.selector,
            "no character projection is supplied for the selected character",
        ));
        apply_stage_fallback(story, character, None, plan);
        return;
    };

    let mapped = projection
        .stage
        .as_deref()
        .and_then(|identity| character.stage_ids.get(identity).cloned());
    if projection.stage.is_some() && mapped.is_none() {
        let identity = projection.stage.as_deref().unwrap_or_default();
        plan.diagnostics.push(diagnostic(
            PresentationDiagnosticKind::UnmappedStage,
            &character.selector,
            format!("projected stage identity `{identity}` has no host stage mapping"),
        ));
    }
    let stage_id = match mapped {
        Some(id) if story.presentation.stages.contains_key(&id) => id,
        Some(id) => {
            plan.diagnostics.push(diagnostic(
                PresentationDiagnosticKind::UnknownStage,
                &character.selector,
                format!("stage `{id}` is not declared by the story"),
            ));
            apply_stage_fallback(story, character, Some(projection), plan);
            return;
        }
        None => {
            apply_stage_fallback(story, character, Some(projection), plan);
            return;
        }
    };

    let stage = &story.presentation.stages[&stage_id];
    let name = projection
        .display_name
        .clone()
        .or_else(|| stage.name.clone())
        .unwrap_or_else(|| stage_id.clone());
    let (image, variant) = resolve_image(story, character, projection, &stage_id, plan);
    plan.stage = Some(ResolvedStage {
        id: stage_id,
        name,
        image,
        variant,
        source: character.selector.clone(),
        fallback: false,
    });
}

fn apply_stage_fallback(
    story: &StoryIr,
    character: &CharacterPresentation,
    projection: Option<&CharacterProjection>,
    plan: &mut ResolvedPresentation,
) {
    let Some(id) = character.fallback_stage.as_deref() else {
        return;
    };
    let Some(stage) = story.presentation.stages.get(id) else {
        plan.diagnostics.push(diagnostic(
            PresentationDiagnosticKind::UnknownStage,
            &character.selector,
            format!("configured fallback stage `{id}` is not declared by the story"),
        ));
        return;
    };
    let name = projection
        .and_then(|projection| projection.display_name.clone())
        .or_else(|| stage.name.clone())
        .unwrap_or_else(|| id.to_owned());
    let (image, variant) = projection.map_or_else(
        || (stage.image.clone(), None),
        |projection| resolve_image(story, character, projection, id, plan),
    );
    plan.stage = Some(ResolvedStage {
        id: id.to_owned(),
        name,
        image,
        variant,
        source: character.selector.clone(),
        fallback: true,
    });
    plan.diagnostics.push(diagnostic(
        PresentationDiagnosticKind::Fallback,
        &character.selector,
        format!("using fallback stage `{id}`"),
    ));
}

fn resolve_image(
    story: &StoryIr,
    character: &CharacterPresentation,
    projection: &CharacterProjection,
    stage_id: &str,
    plan: &mut ResolvedPresentation,
) -> (Option<String>, Option<String>) {
    let Some(stage) = story.presentation.stages.get(stage_id) else {
        return (projection.portrait.clone(), None);
    };
    if let Some(expression) = projection.expression.as_deref() {
        match character.expressions.get(expression) {
            Some(variant) => {
                if let Some(path) = stage.variants.get(variant) {
                    return (Some(path.clone()), Some(variant.clone()));
                }
                plan.diagnostics.push(diagnostic(
                    PresentationDiagnosticKind::UnknownVariant,
                    &character.selector,
                    format!(
                        "stage `{stage_id}` does not declare variant `{variant}` mapped from expression `{expression}`"
                    ),
                ));
            }
            None => plan.diagnostics.push(diagnostic(
                PresentationDiagnosticKind::UnmappedExpression,
                &character.selector,
                format!("expression `{expression}` has no host variant mapping"),
            )),
        }
    }
    if let Some(portrait) = &projection.portrait {
        return (Some(portrait.clone()), None);
    }
    (stage.image.clone(), None)
}

fn resolve_text<'a>(
    reference: &ValueRef,
    story: &'a StoryIr,
    variables: Option<&'a BTreeMap<String, Value>>,
    diagnostics: &mut Vec<PresentationDiagnostic>,
) -> Option<&'a str> {
    match reference {
        ValueRef::Variable { name } => {
            let Some(values) = variables else {
                diagnostics.push(diagnostic(
                    PresentationDiagnosticKind::UnavailableValue,
                    reference,
                    "no variable snapshot was supplied",
                ));
                return None;
            };
            match values.get(name) {
                Some(Value::String(text) | Value::Symbol(text)) => Some(text),
                Some(_) => {
                    diagnostics.push(diagnostic(
                        PresentationDiagnosticKind::IncompatibleType,
                        reference,
                        "the variable is not text; expected a string or symbol",
                    ));
                    None
                }
                None => {
                    diagnostics.push(diagnostic(
                        PresentationDiagnosticKind::UnavailableValue,
                        reference,
                        "the variable is not present in the current snapshot",
                    ));
                    None
                }
            }
        }
        ValueRef::Module { alias, path } => {
            let Some(module) = story.modules.get(alias) else {
                diagnostics.push(diagnostic(
                    PresentationDiagnosticKind::UnknownModule,
                    reference,
                    "the module alias is not active in the compiled story",
                ));
                return None;
            };
            let Some((export, fields)) = path.split_first() else {
                diagnostics.push(diagnostic(
                    PresentationDiagnosticKind::UnknownExport,
                    reference,
                    "the module path names no export",
                ));
                return None;
            };
            let Some(entry) = module.exports.get(export) else {
                diagnostics.push(diagnostic(
                    PresentationDiagnosticKind::UnknownExport,
                    reference,
                    format!("the module does not export `{export}`"),
                ));
                return None;
            };
            let mut current = &entry.value;
            for field in fields {
                let DomainValueIr::Object(object) = current else {
                    diagnostics.push(diagnostic(
                        PresentationDiagnosticKind::IncompatibleType,
                        reference,
                        format!("the module value is not an object before field `{field}`"),
                    ));
                    return None;
                };
                let Some(next) = object.get(field) else {
                    diagnostics.push(diagnostic(
                        PresentationDiagnosticKind::UnknownPath,
                        reference,
                        format!("the module value has no field `{field}`"),
                    ));
                    return None;
                };
                current = next;
            }
            match current {
                DomainValueIr::String(text) | DomainValueIr::Symbol(text) => Some(text),
                _ => {
                    diagnostics.push(diagnostic(
                        PresentationDiagnosticKind::IncompatibleType,
                        reference,
                        "the module value is not text; expected a string or symbol",
                    ));
                    None
                }
            }
        }
    }
}

fn diagnostic(
    kind: PresentationDiagnosticKind,
    reference: &ValueRef,
    detail: impl Into<String>,
) -> PresentationDiagnostic {
    PresentationDiagnostic {
        kind,
        source: reference.label(),
        detail: detail.into(),
    }
}

/// What one resolved selector produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedTarget {
    /// A background scene.
    Scene,
    /// A character stage.
    Stage,
}

impl ResolvedTarget {
    /// Author-facing target name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Stage => "stage",
        }
    }
}

/// One resolved selector row for the author-facing Play drawer.
///
/// The reference is a redaction-safe label. `value` is present only when a
/// successful selection exposes the text that selected it; a fallback never
/// echoes the value it rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedReferenceRow {
    /// Redaction-safe label naming the selector.
    pub reference: String,
    /// What the selector resolved.
    pub target: ResolvedTarget,
    /// Declared scene or stage id.
    pub id: String,
    /// Current text value, when a successful selection exposes one.
    pub value: Option<String>,
    /// Whether a documented fallback supplied the selection.
    pub fallback: bool,
}

impl ResolvedReferenceRow {
    /// One-line author-facing summary of the row.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut summary = format!("{} → {} `{}`", self.reference, self.target.label(), self.id);
        if let Some(value) = &self.value {
            summary.push_str(" = ");
            summary.push_str(value);
        }
        if self.fallback {
            summary.push_str(" (fallback)");
        }
        summary
    }
}

/// Resolved references and diagnostics shown by the Play drawer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresentationDrawerRows {
    /// One row per resolved scene or stage, in resolution order.
    pub references: Vec<ResolvedReferenceRow>,
    /// Redaction-safe diagnostics in deterministic resolution order.
    pub diagnostics: Vec<PresentationDiagnostic>,
}

/// Build the drawer rows for one resolved plan.
///
/// Only a successful selection exposes its current value. A fallback reports
/// the declared fallback id and its diagnostic, but never repeats the value
/// that failed to map.
#[must_use]
pub fn presentation_drawer_rows(
    plan: &ResolvedPresentation,
    story: &StoryIr,
    variables: Option<&BTreeMap<String, Value>>,
) -> PresentationDrawerRows {
    let mut rows = PresentationDrawerRows {
        references: Vec::new(),
        diagnostics: plan.diagnostics.clone(),
    };
    if let Some(scene) = &plan.scene {
        rows.references.push(ResolvedReferenceRow {
            reference: scene.source.label(),
            target: ResolvedTarget::Scene,
            id: scene.id.clone(),
            value: if scene.fallback {
                None
            } else {
                current_text(&scene.source, story, variables)
            },
            fallback: scene.fallback,
        });
    }
    if let Some(stage) = &plan.stage {
        rows.references.push(ResolvedReferenceRow {
            reference: stage.source.label(),
            target: ResolvedTarget::Stage,
            id: stage.id.clone(),
            value: if stage.fallback {
                None
            } else {
                current_text(&stage.source, story, variables)
            },
            fallback: stage.fallback,
        });
    }
    rows
}

/// Current text a resolved reference reads, without raising diagnostics.
fn current_text(
    reference: &ValueRef,
    story: &StoryIr,
    variables: Option<&BTreeMap<String, Value>>,
) -> Option<String> {
    match reference {
        ValueRef::Variable { name } => match variables?.get(name)? {
            Value::String(text) | Value::Symbol(text) => Some(text.clone()),
            _ => None,
        },
        ValueRef::Module { alias, path } => {
            let module = story.modules.get(alias)?;
            let (export, fields) = path.split_first()?;
            let mut current = &module.exports.get(export)?.value;
            for field in fields {
                let DomainValueIr::Object(object) = current else {
                    return None;
                };
                current = object.get(field)?;
            }
            match current {
                DomainValueIr::String(text) | DomainValueIr::Symbol(text) => Some(text.clone()),
                _ => None,
            }
        }
    }
}

// --- Accessibility and interaction -------------------------------------------------
//
// The Play surface exposes one logical focus order. A window toolkit maps its
// own keys onto [`PlayKey`]; the model below owns every decision about order,
// focus restoration, activation, motion, scaling, reveal timing, and
// auto-advance, so the behaviour is testable without a window.
//
// Documented focus order:
//
// 1. the Debug and Play mode tabs, left to right;
// 2. in Debug mode, the Step, Continue, Restart, Stop, seed, choice, knot, and
//    diagnostic controls, in the order they are drawn;
// 3. in Play mode, the scene action (Advance, or Restart after the end), then
//    the numbered choices;
// 4. in Play mode, the author controls from top to bottom: Playtest drawer and
//    its buttons when open, Resolved presentation, then Diagnostics.
//
// Tab and Shift+Tab move one step and wrap. Home and End jump to the first and
// last reachable control. Enter or Space activates the focused control. A
// control that disappears because a drawer closed restores focus to that
// drawer's toggle or to the nearest reachable control.

/// Minimum contrast ratio for body text against its background (WCAG 2.2 AA).
pub const MIN_TEXT_CONTRAST: f32 = 4.5;

/// Minimum contrast ratio for the focus indicator (WCAG 2.2 non-text).
pub const MIN_FOCUS_CONTRAST: f32 = 3.0;

/// Available dialogue text scales, smallest first.
pub const TEXT_SCALE_STEPS: [f32; 5] = [0.85, 1.0, 1.15, 1.3, 1.5];

/// Default dialogue text scale.
pub const DEFAULT_TEXT_SCALE: f32 = 1.0;

/// Base dialogue size in logical pixels at [`DEFAULT_TEXT_SCALE`].
pub const DIALOGUE_BASE_PX: f32 = 18.0;

/// Base maximum dialogue height in logical pixels at [`DEFAULT_TEXT_SCALE`].
pub const DIALOGUE_MAX_HEIGHT_PX: f32 = 220.0;

/// Default reveal speed in characters per second.
pub const DEFAULT_REVEAL_CPS: u32 = 45;

/// Lowest configurable reveal speed.
pub const MIN_REVEAL_CPS: u32 = 1;

/// Highest configurable reveal speed.
pub const MAX_REVEAL_CPS: u32 = 240;

/// Characters per second added or removed by one reveal-speed key.
pub const REVEAL_SPEED_STEP: u32 = 15;

/// Largest number of steps one auto-advance run may take.
pub const MAX_AUTO_ADVANCE_STEPS: u32 = 1_000;

/// Documented shortcuts shared by the Play surface, its hint text, and tests.
pub const PLAY_KEYBOARD_HINT: &str = "Tab/Shift+Tab move · Enter/Space activate · 1-9 choose · R restart · D playtest · V references · I diagnostics · +/− text size · [ ] reveal speed · M reduced motion · S skip text · A auto-advance";

/// WCAG relative luminance of one `0xRRGGBB` colour.
#[must_use]
pub fn relative_luminance(colour: u32) -> f32 {
    fn channel(value: u32) -> f32 {
        let value = (value & 0xff) as f32 / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    let red = channel(colour >> 16);
    let green = channel(colour >> 8);
    let blue = channel(colour);
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

/// WCAG contrast ratio between two `0xRRGGBB` colours.
#[must_use]
pub fn contrast_ratio(first: u32, second: u32) -> f32 {
    let first = relative_luminance(first);
    let second = relative_luminance(second);
    let (lighter, darker) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// Border colour for a control, derived from the theme accent token.
#[must_use]
pub fn focus_indicator_colour(focused: bool) -> u32 {
    if focused {
        DARK_THEME.accent
    } else {
        DARK_THEME.border
    }
}

/// Focus ring for a control that already uses the accent as its fill.
///
/// Accent on accent has no contrast, so the ring switches to the workspace tone, which reaches
/// about 7:1 against the accent.
#[must_use]
pub const fn focus_indicator_colour_on_fill(focused: bool) -> u32 {
    if focused {
        DARK_THEME.workspace
    } else {
        DARK_THEME.border
    }
}

/// Text colour for content drawn on the accent fill.
#[must_use]
pub const fn on_accent_text_colour() -> u32 {
    DARK_THEME.workspace
}

/// Clamp a requested text scale into the documented steps.
#[must_use]
pub fn clamp_text_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(
            TEXT_SCALE_STEPS[0],
            TEXT_SCALE_STEPS[TEXT_SCALE_STEPS.len() - 1],
        )
    } else {
        DEFAULT_TEXT_SCALE
    }
}

/// Scale one logical pixel size. Dialogue reflows because larger type wraps.
#[must_use]
pub fn scaled_text_px(base: f32, scale: f32) -> f32 {
    base * clamp_text_scale(scale)
}

/// Scale the maximum dialogue height so enlarged text is not clipped.
#[must_use]
pub fn scaled_dialogue_height(base: f32, scale: f32) -> f32 {
    // Cap the growth so a large text scale cannot collapse the stage area inside the fixed
    // Play panel. The dialogue scrolls instead.
    (base * clamp_text_scale(scale)).min(240.0)
}

/// Next or previous text scale step. Direction `0` keeps the current scale.
#[must_use]
pub fn step_text_scale(scale: f32, direction: i32) -> f32 {
    let current = clamp_text_scale(scale);
    if direction > 0 {
        TEXT_SCALE_STEPS
            .iter()
            .copied()
            .find(|step| *step > current)
            .unwrap_or(TEXT_SCALE_STEPS[TEXT_SCALE_STEPS.len() - 1])
    } else if direction < 0 {
        TEXT_SCALE_STEPS
            .iter()
            .rev()
            .copied()
            .find(|step| *step < current)
            .unwrap_or(TEXT_SCALE_STEPS[0])
    } else {
        current
    }
}

/// Whether the surface animates transitions and text reveal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MotionPreference {
    /// Draw bounded transitions and animated text reveal.
    #[default]
    Full,
    /// Settle every frame immediately while keeping cue order unchanged.
    Reduced,
}

impl MotionPreference {
    /// The other preference.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Full => Self::Reduced,
            Self::Reduced => Self::Full,
        }
    }

    /// Author-facing preference name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Full => "Full motion",
            Self::Reduced => "Reduced motion",
        }
    }

    /// Transition to apply, suppressed entirely under reduced motion.
    #[must_use]
    pub fn transition(self, transition: Option<PlayTransition>) -> Option<PlayTransition> {
        match self {
            Self::Full => transition,
            Self::Reduced => None,
        }
    }

    /// Opacity of one staged figure under this preference.
    #[must_use]
    pub fn stage_opacity(self, stage: &StagedCharacter) -> f32 {
        match self {
            Self::Full => PlayScene::stage_opacity(stage),
            Self::Reduced => 1.0,
        }
    }

    /// Opacity of the background under this preference.
    #[must_use]
    pub fn background_opacity(self, scene: &PlayScene) -> f32 {
        match self {
            Self::Full => scene.background_opacity(),
            Self::Reduced => 1.0,
        }
    }

    /// Reveal setting to use. Reduced motion reveals text immediately.
    #[must_use]
    pub fn reveal(self, reveal: TextReveal) -> TextReveal {
        match self {
            Self::Full => reveal,
            Self::Reduced => TextReveal::instant(),
        }
    }
}

/// Configurable dialogue text reveal.
///
/// Counting characters, not bytes, keeps multi-byte text intact. The speed is
/// clamped so timing stays bounded on both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextReveal {
    /// Characters revealed per second.
    pub chars_per_second: u32,
    /// Whether the whole line appears at once.
    pub instant: bool,
}

impl Default for TextReveal {
    fn default() -> Self {
        Self::new(DEFAULT_REVEAL_CPS)
    }
}

impl TextReveal {
    /// Clamp a requested speed into the documented bounds.
    #[must_use]
    pub const fn clamp_speed(chars_per_second: u32) -> u32 {
        if chars_per_second < MIN_REVEAL_CPS {
            MIN_REVEAL_CPS
        } else if chars_per_second > MAX_REVEAL_CPS {
            MAX_REVEAL_CPS
        } else {
            chars_per_second
        }
    }

    /// Animated reveal at the requested speed.
    #[must_use]
    pub const fn new(chars_per_second: u32) -> Self {
        Self {
            chars_per_second: Self::clamp_speed(chars_per_second),
            instant: false,
        }
    }

    /// Reveal the whole line immediately.
    #[must_use]
    pub const fn instant() -> Self {
        Self {
            chars_per_second: MAX_REVEAL_CPS,
            instant: true,
        }
    }

    /// Same mode at a different speed.
    #[must_use]
    pub const fn with_speed(self, chars_per_second: u32) -> Self {
        Self {
            chars_per_second: Self::clamp_speed(chars_per_second),
            instant: self.instant,
        }
    }

    /// Same speed with instant reveal on or off.
    #[must_use]
    pub const fn with_instant(self, instant: bool) -> Self {
        Self {
            chars_per_second: self.chars_per_second,
            instant,
        }
    }

    /// Number of characters visible after `elapsed`.
    #[must_use]
    pub fn visible_chars(self, text: &str, elapsed: Duration) -> usize {
        let total = text.chars().count();
        if self.instant {
            return total;
        }
        let revealed = (elapsed.as_secs_f64() * f64::from(self.chars_per_second)).floor() as usize;
        revealed.min(total)
    }

    /// Whether the whole line is visible after `elapsed`.
    #[must_use]
    pub fn is_complete(self, text: &str, elapsed: Duration) -> bool {
        self.visible_chars(text, elapsed) >= text.chars().count()
    }

    /// The visible prefix after `elapsed`.
    #[must_use]
    pub fn reveal(self, text: &str, elapsed: Duration) -> String {
        text.chars()
            .take(self.visible_chars(text, elapsed))
            .collect()
    }
}

/// Bounded auto-advance configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoAdvancePolicy {
    /// Whether auto-advance is requested.
    pub enabled: bool,
    /// Quiet time after a line before advancing.
    pub delay: Duration,
    /// Maximum number of steps one run may take.
    pub max_steps: u32,
}

impl Default for AutoAdvancePolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            delay: Duration::from_millis(1_500),
            max_steps: 200,
        }
    }
}

impl AutoAdvancePolicy {
    /// Whether the surface may take another step.
    fn bounded_max_steps(max_steps: u32) -> u32 {
        if max_steps > MAX_AUTO_ADVANCE_STEPS {
            MAX_AUTO_ADVANCE_STEPS
        } else {
            max_steps
        }
    }
}

/// Auto-advance progress, bounded by its policy.
#[derive(Debug, Clone)]
pub struct AutoAdvanceState {
    policy: AutoAdvancePolicy,
    advanced: u32,
}

impl Default for AutoAdvanceState {
    fn default() -> Self {
        Self::new(AutoAdvancePolicy::default())
    }
}

impl AutoAdvanceState {
    /// New state with a bounded copy of `policy`.
    #[must_use]
    pub fn new(policy: AutoAdvancePolicy) -> Self {
        let policy = AutoAdvancePolicy {
            max_steps: AutoAdvancePolicy::bounded_max_steps(policy.max_steps),
            ..policy
        };
        Self {
            policy,
            advanced: 0,
        }
    }

    /// Current policy.
    #[must_use]
    pub const fn policy(&self) -> AutoAdvancePolicy {
        self.policy
    }

    /// Replace the policy, resetting the step count.
    pub fn set_policy(&mut self, policy: AutoAdvancePolicy) {
        *self = Self::new(policy);
    }

    /// Enable or disable auto-advance and reset the step count.
    pub fn toggle(&mut self) {
        self.policy.enabled = !self.policy.enabled;
        self.advanced = 0;
    }

    /// Steps taken in this run.
    #[must_use]
    pub const fn advanced(&self) -> u32 {
        self.advanced
    }

    /// Reset the step count.
    pub fn reset(&mut self) {
        self.advanced = 0;
    }

    /// Whether one more step is allowed now.
    ///
    /// Auto-advance stops at a choice, at the end, on a diagnostic, and when
    /// the bounded step count is exhausted. It never reorders cues; it only
    /// asks the session to take its next step.
    #[must_use]
    pub fn can_advance(&self, scene: &PlayScene, elapsed_since_line: Duration) -> bool {
        self.policy.enabled
            && self.advanced < self.policy.max_steps
            && scene.status == PreviewStatus::Ready
            && !scene.ended
            && scene.choices.is_empty()
            && scene.issue.is_none()
            && elapsed_since_line >= self.policy.delay
    }

    /// Record one step. Returns `false` when the bound is already reached.
    pub fn record_advance(&mut self) -> bool {
        if self.advanced >= self.policy.max_steps {
            return false;
        }
        self.advanced += 1;
        true
    }
}

/// Every focusable control in the Play or Debug surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayFocusTarget {
    /// A presentation mode tab.
    Mode(PlayMode),
    /// Advance the scene in Play mode.
    Advance,
    /// Restart the story after the end.
    Restart,
    /// One numbered Play choice.
    Choice(usize),
    /// Debug transcript: step one line.
    DebugStep,
    /// Debug transcript: continue to the next boundary.
    DebugContinue,
    /// Debug transcript: restart the story.
    DebugRestart,
    /// Debug transcript: stop playback.
    DebugStop,
    /// Debug transcript: previous seed.
    DebugSeedDown,
    /// Debug transcript: next seed.
    DebugSeedUp,
    /// Debug transcript: choose one option.
    DebugChoice(usize),
    /// Debug transcript: replay one knot.
    DebugKnot(usize),
    /// Debug transcript: reveal the source of the current diagnostic.
    DebugIssue,
    /// Playtest drawer toggle.
    PlaytestToggle,
    /// Capture the current playthrough.
    PlaytestCapture,
    /// Export the record as JSON.
    PlaytestExportJson,
    /// Export the record as RON.
    PlaytestExportRon,
    /// Import a record as JSON.
    PlaytestImportJson,
    /// Import a record as RON.
    PlaytestImportRon,
    /// Replay the record.
    PlaytestReplay,
    /// Resolved-presentation drawer toggle.
    PresentationToggle,
    /// Diagnostics drawer toggle.
    DiagnosticsToggle,
}

impl PlayFocusTarget {
    /// Author-facing label.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Mode(mode) => format!("{} mode", mode.label()),
            Self::Advance => "Advance scene".to_owned(),
            Self::Restart => "Restart story".to_owned(),
            Self::Choice(index) => format!("Choice {}", index + 1),
            Self::DebugStep => "Step".to_owned(),
            Self::DebugContinue => "Continue".to_owned(),
            Self::DebugRestart => "Restart".to_owned(),
            Self::DebugStop => "Stop".to_owned(),
            Self::DebugSeedDown => "Previous seed".to_owned(),
            Self::DebugSeedUp => "Next seed".to_owned(),
            Self::DebugChoice(index) => format!("Debug choice {}", index + 1),
            Self::DebugKnot(index) => format!("Replay knot {}", index + 1),
            Self::DebugIssue => "Reveal diagnostic source".to_owned(),
            Self::PlaytestToggle => "Playtest drawer".to_owned(),
            Self::PlaytestCapture => "Capture playtest".to_owned(),
            Self::PlaytestExportJson => "Export playtest JSON".to_owned(),
            Self::PlaytestExportRon => "Export playtest RON".to_owned(),
            Self::PlaytestImportJson => "Import playtest JSON".to_owned(),
            Self::PlaytestImportRon => "Import playtest RON".to_owned(),
            Self::PlaytestReplay => "Replay playtest".to_owned(),
            Self::PresentationToggle => "Resolved presentation drawer".to_owned(),
            Self::DiagnosticsToggle => "Diagnostics drawer".to_owned(),
        }
    }

    /// Accessibility role.
    #[must_use]
    pub const fn role(self) -> &'static str {
        match self {
            Self::Mode(_) => "tab",
            Self::Choice(_) | Self::DebugChoice(_) => "option",
            Self::DiagnosticsToggle => "disclosure",
            _ => "button",
        }
    }

    /// The toggle to focus when this control disappears, if any.
    #[must_use]
    pub const fn restore_target(self) -> Option<Self> {
        match self {
            Self::Restart | Self::Choice(_) => Some(Self::Advance),
            Self::Advance => Some(Self::Restart),
            Self::PlaytestCapture
            | Self::PlaytestExportJson
            | Self::PlaytestExportRon
            | Self::PlaytestImportJson
            | Self::PlaytestImportRon
            | Self::PlaytestReplay => Some(Self::PlaytestToggle),
            _ => None,
        }
    }
}

/// Inputs that determine which controls are reachable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayFocusContext {
    /// Current presentation mode.
    pub mode: PlayMode,
    /// Whether the Playtest drawer body is open.
    pub playtest_expanded: bool,
    /// Number of choices in the Debug panel.
    pub debug_choices: usize,
    /// Number of knots in the Debug panel.
    pub debug_knots: usize,
    /// Whether a compile or runtime issue is shown.
    pub has_issue: bool,
}

/// Build the documented focus order for the current surface state.
#[must_use]
pub fn play_focus_order(scene: &PlayScene, context: PlayFocusContext) -> Vec<PlayFocusTarget> {
    play_focus_order_for(scene.ended, scene.choices.len(), context)
}

/// Build the focus order from the few session facts it depends on.
///
/// The surface uses this form so a render does not fold every presentation cue
/// just to decide which controls are reachable.
#[must_use]
pub fn play_focus_order_for(
    ended: bool,
    choice_count: usize,
    context: PlayFocusContext,
) -> Vec<PlayFocusTarget> {
    let mut order = vec![
        PlayFocusTarget::Mode(PlayMode::Debug),
        PlayFocusTarget::Mode(PlayMode::Play),
    ];
    match context.mode {
        PlayMode::Debug => {
            order.extend([
                PlayFocusTarget::DebugStep,
                PlayFocusTarget::DebugContinue,
                PlayFocusTarget::DebugRestart,
                PlayFocusTarget::DebugStop,
                PlayFocusTarget::DebugSeedDown,
                PlayFocusTarget::DebugSeedUp,
            ]);
            order.extend((0..context.debug_choices).map(PlayFocusTarget::DebugChoice));
            if context.has_issue {
                order.push(PlayFocusTarget::DebugIssue);
            }
            order.extend((0..context.debug_knots).map(PlayFocusTarget::DebugKnot));
        }
        PlayMode::Play => {
            if ended {
                order.push(PlayFocusTarget::Restart);
            } else {
                order.push(PlayFocusTarget::Advance);
            }
            order.extend((0..choice_count).map(PlayFocusTarget::Choice));
            order.push(PlayFocusTarget::PlaytestToggle);
            if context.playtest_expanded {
                order.extend([
                    PlayFocusTarget::PlaytestCapture,
                    PlayFocusTarget::PlaytestExportJson,
                    PlayFocusTarget::PlaytestExportRon,
                    PlayFocusTarget::PlaytestImportJson,
                    PlayFocusTarget::PlaytestImportRon,
                    PlayFocusTarget::PlaytestReplay,
                ]);
            }
            // The resolved-presentation body is read-only, so its toggle is the
            // single focus stop for that drawer.
            order.push(PlayFocusTarget::PresentationToggle);
            order.push(PlayFocusTarget::DiagnosticsToggle);
        }
    }
    order
}

/// Logical keys accepted by the Play surface, independent of a window toolkit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayKey {
    /// Move to the next control.
    Next,
    /// Move to the previous control.
    Previous,
    /// Jump to the first control.
    Home,
    /// Jump to the last control.
    End,
    /// Activate the focused control.
    Activate,
    /// Select one choice by zero-based index.
    Choose(usize),
    /// Restart the story.
    Restart,
    /// Toggle the Playtest drawer.
    TogglePlaytest,
    /// Toggle the resolved-presentation drawer.
    TogglePresentation,
    /// Toggle the diagnostics drawer.
    ToggleDiagnostics,
    /// Enlarge dialogue text by one step.
    ScaleUp,
    /// Reduce dialogue text by one step.
    ScaleDown,
    /// Toggle full and reduced motion.
    ToggleMotion,
    /// Toggle instant text reveal.
    SkipReveal,
    /// Make text reveal slower by one step.
    RevealSlower,
    /// Make text reveal faster by one step.
    RevealFaster,
    /// Toggle bounded auto-advance.
    ToggleAutoAdvance,
}

/// Result of one logical key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayAction {
    /// Focus moved to this control.
    FocusChanged(PlayFocusTarget),
    /// Activate this control.
    Activate(PlayFocusTarget),
    /// Select one choice.
    Choose(usize),
    /// Restart the story.
    Restart,
    /// Toggle the Playtest drawer.
    TogglePlaytest,
    /// Toggle the resolved-presentation drawer.
    TogglePresentation,
    /// Toggle the diagnostics drawer.
    ToggleDiagnostics,
    /// Change the text scale by one step.
    StepTextScale(i32),
    /// Toggle full and reduced motion.
    ToggleMotion,
    /// Toggle instant text reveal.
    SkipReveal,
    /// Change reveal speed by one step.
    StepRevealSpeed(i32),
    /// Toggle bounded auto-advance.
    ToggleAutoAdvance,
}

/// Map one window key onto a logical Play key.
///
/// Platform, control, and alt chords are left to the host. Shift is used only
/// by Tab and does not change a letter shortcut.
#[must_use]
pub fn play_key_for(
    key: &str,
    key_char: Option<&str>,
    shift: bool,
    control: bool,
    alt: bool,
    platform: bool,
) -> Option<PlayKey> {
    if control || alt || platform {
        return None;
    }
    match key {
        "tab" => Some(if shift {
            PlayKey::Previous
        } else {
            PlayKey::Next
        }),
        "up" => Some(PlayKey::Previous),
        "down" => Some(PlayKey::Next),
        "home" => Some(PlayKey::Home),
        "end" => Some(PlayKey::End),
        "enter" | "space" => Some(PlayKey::Activate),
        "r" => Some(PlayKey::Restart),
        "d" => Some(PlayKey::TogglePlaytest),
        "v" => Some(PlayKey::TogglePresentation),
        "i" => Some(PlayKey::ToggleDiagnostics),
        "m" => Some(PlayKey::ToggleMotion),
        "s" => Some(PlayKey::SkipReveal),
        "[" => Some(PlayKey::RevealSlower),
        "]" => Some(PlayKey::RevealFaster),
        "a" => Some(PlayKey::ToggleAutoAdvance),
        "=" | "+" => Some(PlayKey::ScaleUp),
        "-" | "_" => Some(PlayKey::ScaleDown),
        _ => {
            let character = key_char
                .and_then(|text| text.chars().next())
                .or_else(|| key.chars().next())?;
            let digit = character.to_digit(10)?;
            if (1..=9).contains(&digit) {
                Some(PlayKey::Choose(digit as usize - 1))
            } else {
                None
            }
        }
    }
}

/// Logical focus with predictable restoration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayFocus {
    focused: PlayFocusTarget,
}

impl Default for PlayFocus {
    fn default() -> Self {
        Self {
            focused: PlayFocusTarget::Advance,
        }
    }
}

impl PlayFocus {
    /// Currently focused control.
    #[must_use]
    pub const fn current(&self) -> PlayFocusTarget {
        self.focused
    }

    /// Focus one control explicitly.
    pub fn focus(&mut self, target: PlayFocusTarget) {
        self.focused = target;
    }

    /// Reconcile focus with the reachable controls.
    ///
    /// A control that vanished restores focus to its drawer toggle or to the
    /// first reachable control, so focus never becomes unreachable.
    pub fn reconcile(&mut self, order: &[PlayFocusTarget]) -> PlayFocusTarget {
        if order.contains(&self.focused) {
            return self.focused;
        }
        let restored = self
            .focused
            .restore_target()
            .filter(|target| order.contains(target));
        if let Some(target) = restored {
            self.focused = target;
        } else if let Some(first) = order.first() {
            self.focused = *first;
        }
        self.focused
    }

    /// Move one control forward or backward, wrapping at the ends.
    pub fn step(&mut self, order: &[PlayFocusTarget], forward: bool) -> PlayFocusTarget {
        if order.is_empty() {
            return self.focused;
        }
        let next = order
            .iter()
            .position(|target| *target == self.focused)
            .map_or(0, |index| {
                if forward {
                    (index + 1) % order.len()
                } else {
                    (index + order.len() - 1) % order.len()
                }
            });
        self.focused = order[next];
        self.focused
    }

    /// Jump to the first reachable control.
    pub fn home(&mut self, order: &[PlayFocusTarget]) -> PlayFocusTarget {
        if let Some(first) = order.first() {
            self.focused = *first;
        }
        self.focused
    }

    /// Jump to the last reachable control.
    pub fn end(&mut self, order: &[PlayFocusTarget]) -> PlayFocusTarget {
        if let Some(last) = order.last() {
            self.focused = *last;
        }
        self.focused
    }

    /// Apply one logical key to the focus and return the resulting action.
    pub fn handle_key(&mut self, key: PlayKey, order: &[PlayFocusTarget]) -> PlayAction {
        match key {
            PlayKey::Next => PlayAction::FocusChanged(self.step(order, true)),
            PlayKey::Previous => PlayAction::FocusChanged(self.step(order, false)),
            PlayKey::Home => PlayAction::FocusChanged(self.home(order)),
            PlayKey::End => PlayAction::FocusChanged(self.end(order)),
            PlayKey::Activate => PlayAction::Activate(self.focused),
            PlayKey::Choose(index) => PlayAction::Choose(index),
            PlayKey::Restart => PlayAction::Restart,
            PlayKey::TogglePlaytest => PlayAction::TogglePlaytest,
            PlayKey::TogglePresentation => PlayAction::TogglePresentation,
            PlayKey::ToggleDiagnostics => PlayAction::ToggleDiagnostics,
            PlayKey::ScaleUp => PlayAction::StepTextScale(1),
            PlayKey::ScaleDown => PlayAction::StepTextScale(-1),
            PlayKey::ToggleMotion => PlayAction::ToggleMotion,
            PlayKey::SkipReveal => PlayAction::SkipReveal,
            PlayKey::RevealSlower => PlayAction::StepRevealSpeed(-1),
            PlayKey::RevealFaster => PlayAction::StepRevealSpeed(1),
            PlayKey::ToggleAutoAdvance => PlayAction::ToggleAutoAdvance,
        }
    }
}

/// Accessibility metadata for the focused control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayAccessibility {
    /// Toolkit-independent role.
    pub role: &'static str,
    /// Author-facing label.
    pub label: String,
    /// One-based position in the focus order.
    pub position: usize,
    /// Number of reachable controls.
    pub total: usize,
    /// Documented shortcut hint.
    pub keyboard_hint: &'static str,
}

/// Describe the focused control for assistive output.
#[must_use]
pub fn play_accessibility(
    order: &[PlayFocusTarget],
    focused: PlayFocusTarget,
) -> PlayAccessibility {
    PlayAccessibility {
        role: focused.role(),
        label: focused.label(),
        position: order
            .iter()
            .position(|target| *target == focused)
            .map_or(1, |index| index + 1),
        total: order.len(),
        keyboard_hint: PLAY_KEYBOARD_HINT,
    }
}

/// Severity of one Play diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlayDiagnosticSeverity {
    /// Informational fallback.
    Note,
    /// A frame degraded without losing story state.
    Warning,
    /// The last valid frame is shown because the current build failed.
    Error,
}

impl PlayDiagnosticSeverity {
    /// Author-facing severity name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// One diagnostic reachable from the Play author-control region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayDiagnostic {
    /// Severity, ordered from information to critical.
    pub severity: PlayDiagnosticSeverity,
    /// Redaction-safe summary.
    pub summary: String,
}

/// Collect diagnostics in deterministic order: compile issue, cue notes, then
/// resolved-presentation diagnostics. The surface lists them below the scene,
/// never over it, so the playable frame stays visible.
#[must_use]
pub fn play_diagnostics(
    scene: &PlayScene,
    rows: Option<&PresentationDrawerRows>,
) -> Vec<PlayDiagnostic> {
    let mut diagnostics = Vec::new();
    if let Some(issue) = &scene.issue {
        diagnostics.push(PlayDiagnostic {
            severity: PlayDiagnosticSeverity::Error,
            summary: issue.clone(),
        });
    }
    for note in &scene.notes {
        diagnostics.push(PlayDiagnostic {
            severity: PlayDiagnosticSeverity::Warning,
            summary: note.clone(),
        });
    }
    if let Some(rows) = rows {
        for diagnostic in &rows.diagnostics {
            let severity = if diagnostic.kind == PresentationDiagnosticKind::Fallback {
                PlayDiagnosticSeverity::Note
            } else {
                PlayDiagnosticSeverity::Warning
            };
            diagnostics.push(PlayDiagnostic {
                severity,
                summary: format!(
                    "{} · {} — {}",
                    diagnostic.kind.label(),
                    diagnostic.source,
                    diagnostic.detail
                ),
            });
        }
    }
    diagnostics
}

/// Whether any diagnostic reports a failed build or runtime.
#[must_use]
pub fn has_critical_diagnostics(diagnostics: &[PlayDiagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == PlayDiagnosticSeverity::Error)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::preview::PreviewSession;
    use weave_core::Span;
    use weave_core::ir::{
        CueIr, DomainExportIr, DomainExportSourceIr, DomainModuleIr, DomainValueIr, Instruction,
        InstructionKind, KnotIr, SceneIr, StageIr, StoryIr, Template,
    };

    fn branching_story() -> StoryIr {
        let source = r#"
FLAG boarded = false

=== start ===
Mara: The ferry leaves at dusk.
* [Board the ferry]
    SET boarded = true
    Mara: Then we cross together.
    -> END
* [Wait for morning]
    -> start
"#;
        weave_compiler::compile(source, &weave_compiler::CompileOptions::default())
            .expect("fixture compiles")
            .story
    }

    fn session() -> PreviewSession {
        let mut session = PreviewSession::from_story(branching_story(), 7, 1);
        session.run();
        session
    }

    #[test]
    fn a_named_line_splits_into_speaker_and_text() {
        assert_eq!(
            split_speaker("Mara: The ferry leaves at dusk."),
            ("Mara".to_owned(), "The ferry leaves at dusk.".to_owned())
        );
    }

    #[test]
    fn a_sentence_with_a_colon_stays_narration() {
        let (speaker, line) = split_speaker("The rule was simple: wait for morning.");
        assert_eq!(speaker, NARRATOR);
        assert_eq!(line, "The rule was simple: wait for morning.");
    }

    #[test]
    fn a_long_prefix_is_not_a_character_name() {
        let (speaker, _) = split_speaker(
            "A very long introductory clause that is really prose: and then it ended.",
        );
        assert_eq!(speaker, NARRATOR);
    }

    #[test]
    fn the_scene_shows_the_last_line_and_the_pending_choices() {
        let session = session();
        let scene = scene_from_session(&session);
        assert_eq!(scene.speaker, "Mara");
        assert_eq!(scene.line, "The ferry leaves at dusk.");
        assert_eq!(scene.choices.len(), 2);
        assert_eq!(scene.status, PreviewStatus::WaitingForChoice);
        assert!(!scene.ended);
    }

    #[test]
    fn the_scene_matches_the_debug_session_after_a_choice() {
        let mut session = session();
        let before = scene_from_session(&session);
        assert_eq!(before.choices, session.choices());

        session.choose(0);
        let after = scene_from_session(&session);
        assert_eq!(after.choices, session.choices());
        assert_eq!(after.speaker, "Mara");
        assert_eq!(after.line, "Then we cross together.");
        assert!(after.ended);
        assert_eq!(after.status, PreviewStatus::Ended);
        assert_eq!(
            session
                .variables()
                .iter()
                .find(|(name, _)| name == "boarded")
                .map(|(_, value)| value.clone()),
            Some(weave_runtime::Value::Bool(true))
        );
    }

    #[test]
    fn the_last_valid_frame_survives_a_compile_failure() {
        let mut session = session();
        assert!(!session.compile_source("=== broken", None, 2));
        let scene = scene_from_session(&session);
        assert_eq!(scene.line, "The ferry leaves at dusk.");
        assert!(scene.issue.is_some());
    }

    #[test]
    fn an_empty_session_has_an_empty_narrated_scene() {
        let scene = scene_from_session(&PreviewSession::empty(1));
        assert_eq!(scene.speaker, NARRATOR);
        assert!(scene.line.is_empty());
        assert!(scene.choices.is_empty());
        assert!(!scene.ended);
        assert!(scene.stages.is_empty());
        assert_eq!(scene.fit, Fit::Cover);
        assert_eq!(scene.anchor, AnchorIr::default());
    }

    fn cued_story() -> StoryIr {
        let source = r#"
scene ferry_dusk {
    background: "art/ferry_dusk.png"
}

stage mara {
    image: "art/mara_neutral.png"
    name: "Mara"
}

=== start ===
@scene ferry_dusk
@show mara
@speaker mara
Mara: The ferry leaves at dusk.
-> END
"#;
        weave_compiler::compile(source, &weave_compiler::CompileOptions::default())
            .expect("fixture compiles")
            .story
    }

    #[test]
    fn cues_stage_the_background_character_and_speaker() {
        let mut session = PreviewSession::from_story(cued_story(), 3, 1);
        session.run();
        let scene = scene_from_session(&session);
        assert_eq!(scene.background.as_deref(), Some("ferry_dusk"));
        assert_eq!(scene.stage.as_deref(), Some("mara"));
        assert_eq!(scene.stages.len(), 1);
        assert_eq!(scene.stages[0].name, "Mara");
        assert_eq!(scene.stages[0].slot, Slot::Center);
        assert_eq!(scene.speaker, "Mara");
        assert_eq!(scene.line, "The ferry leaves at dusk.");
    }

    #[test]
    fn an_explicit_speaker_cue_beats_the_prose_prefix() {
        let source = r#"
stage kell {
    name: "Kell"
}

=== start ===
@speaker kell
Mara: The ferry leaves at dusk.
-> END
"#;
        let story = weave_compiler::compile(source, &weave_compiler::CompileOptions::default())
            .expect("fixture compiles")
            .story;
        let mut session = PreviewSession::from_story(story, 3, 1);
        session.run();
        let scene = scene_from_session(&session);
        assert_eq!(scene.speaker, "Kell");
        assert_eq!(scene.line, "The ferry leaves at dusk.");
    }

    #[test]
    fn hiding_a_stage_clears_it() {
        let source = r#"
stage mara {
    name: "Mara"
}

=== start ===
@show mara
@hide mara
The stage is empty.
-> END
"#;
        let story = weave_compiler::compile(source, &weave_compiler::CompileOptions::default())
            .expect("fixture compiles")
            .story;
        let mut session = PreviewSession::from_story(story, 3, 1);
        session.run();
        let scene = scene_from_session(&session);
        assert_eq!(scene.stage, None);
        assert!(scene.stages.is_empty());
        assert_eq!(scene.speaker, NARRATOR);
    }

    #[test]
    fn a_restart_clears_the_staged_scene() {
        let mut session = PreviewSession::from_story(cued_story(), 3, 1);
        session.run();
        assert!(scene_from_session(&session).background.is_some());
        session.restart();
        let scene = scene_from_session(&session);
        assert_eq!(scene.background, None);
        assert_eq!(scene.stage, None);
        assert!(scene.stages.is_empty());
    }

    #[test]
    fn scene_tones_are_stable_and_distinct() {
        assert_eq!(scene_tone(None), 0x0e1118);
        assert_eq!(
            scene_tone(Some("ferry_dusk")),
            scene_tone(Some("ferry_dusk"))
        );
        assert_ne!(scene_tone(Some("ferry_dusk")), 0x0e1118);
    }

    #[test]
    fn the_mode_toggles_between_debug_and_play() {
        assert_eq!(PlayMode::default(), PlayMode::Debug);
        assert_eq!(PlayMode::Debug.other(), PlayMode::Play);
        assert_eq!(PlayMode::Play.other(), PlayMode::Debug);
    }

    #[test]
    fn a_fresh_session_is_not_playable_until_it_runs() {
        let mut session = PreviewSession::from_story(branching_story(), 7, 1);
        let scene = scene_from_session(&session);
        assert!(scene.line.is_empty());
        assert_eq!(session.status(), PreviewStatus::Ready);

        let _ = session.step();
        assert_eq!(scene_from_session(&session).speaker, "Mara");
    }

    #[derive(Default)]
    struct FixtureStage {
        name: Option<String>,
        slot: Option<Slot>,
        layer: Option<u8>,
        image: Option<String>,
        variants: BTreeMap<String, String>,
    }

    #[derive(Default)]
    struct FixtureLookup {
        scenes: BTreeMap<String, (Fit, AnchorIr)>,
        stages: BTreeMap<String, FixtureStage>,
    }

    impl PresentationLookup for FixtureLookup {
        fn scene_fit(&self, id: &str) -> Option<Fit> {
            self.scenes.get(id).map(|(fit, _)| *fit)
        }

        fn scene_anchor(&self, id: &str) -> Option<AnchorIr> {
            self.scenes.get(id).map(|(_, anchor)| *anchor)
        }

        fn stage_name(&self, id: &str) -> Option<String> {
            self.stages.get(id)?.name.clone()
        }

        fn stage_slot(&self, id: &str) -> Option<Slot> {
            self.stages.get(id)?.slot
        }

        fn stage_layer(&self, id: &str) -> Option<u8> {
            self.stages.get(id)?.layer
        }

        fn stage_image(&self, id: &str) -> Option<&str> {
            self.stages.get(id)?.image.as_deref()
        }

        fn stage_variant_image(&self, stage: &str, variant: &str) -> Option<&str> {
            self.stages
                .get(stage)?
                .variants
                .get(variant)
                .map(String::as_str)
        }
    }

    fn staging_fixture() -> FixtureLookup {
        let mut lookup = FixtureLookup::default();
        lookup.scenes.insert(
            "deck".to_owned(),
            (Fit::Contain, AnchorIr { x: 0.2, y: 0.7 }),
        );
        lookup.stages.insert(
            "mara".to_owned(),
            FixtureStage {
                name: Some("Mara".to_owned()),
                slot: Some(Slot::Left),
                layer: Some(2),
                image: Some("art/mara_neutral.png".to_owned()),
                variants: BTreeMap::from([(
                    "surprised".to_owned(),
                    "art/mara_surprised.png".to_owned(),
                )]),
            },
        );
        lookup.stages.insert(
            "keeper".to_owned(),
            FixtureStage {
                name: Some("The Keeper".to_owned()),
                slot: Some(Slot::Right),
                layer: Some(5),
                image: Some("art/keeper_neutral.png".to_owned()),
                variants: BTreeMap::new(),
            },
        );
        lookup
    }

    #[test]
    fn folding_a_cue_sequence_produces_the_expected_scene() {
        let lookup = staging_fixture();
        let cues = vec![
            PresentationEvent::Scene("deck".to_owned()),
            PresentationEvent::Show("mara".to_owned()),
            PresentationEvent::Show("keeper".to_owned()),
            PresentationEvent::Move {
                stage: "keeper".to_owned(),
                to: Slot::Center,
            },
            PresentationEvent::Swap {
                stage: "mara".to_owned(),
                variant: "surprised".to_owned(),
            },
            PresentationEvent::Swap {
                stage: "keeper".to_owned(),
                variant: "ghost".to_owned(),
            },
            PresentationEvent::Transition {
                kind: TransitionKind::Fade,
                ms: 240,
            },
            PresentationEvent::Move {
                stage: "mara".to_owned(),
                to: Slot::Right,
            },
        ];

        let state = fold_cues(&cues, &lookup);
        assert_eq!(state.background.as_deref(), Some("deck"));
        assert_eq!(state.fit, Fit::Contain);
        assert_eq!(state.anchor, AnchorIr { x: 0.2, y: 0.7 });
        assert_eq!(state.stages.len(), 2);

        let mara = state
            .stages
            .iter()
            .find(|stage| stage.id == "mara")
            .expect("mara is staged");
        assert_eq!(mara.name, "Mara");
        assert_eq!(mara.slot, Slot::Right);
        assert_eq!(mara.layer, 2);
        assert_eq!(mara.image.as_deref(), Some("art/mara_surprised.png"));
        assert_eq!(mara.variant.as_deref(), Some("surprised"));
        assert_eq!(
            mara.transition,
            Some(PlayTransition::new(TransitionKind::Fade, 240))
        );

        let keeper = state
            .stages
            .iter()
            .find(|stage| stage.id == "keeper")
            .expect("keeper is staged");
        assert_eq!(keeper.slot, Slot::Center);
        assert_eq!(keeper.layer, 5);
        assert_eq!(keeper.image.as_deref(), Some("art/keeper_neutral.png"));
        assert_eq!(keeper.variant, None);

        assert_eq!(state.notes.len(), 1);
        assert!(state.notes[0].contains("keeper"));
        assert!(state.notes[0].contains("ghost"));
        assert!(state.notes[0].contains("keeping the current image"));
        assert_eq!(
            state.transition, None,
            "the final move consumed the pending transition"
        );

        let ordered = ordered_stages(&state.stages)
            .iter()
            .map(|stage| stage.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ordered, ["mara", "keeper"]);
    }

    #[test]
    fn a_transition_changes_no_staging() {
        let lookup = staging_fixture();
        let staging = vec![
            PresentationEvent::Scene("deck".to_owned()),
            PresentationEvent::Show("mara".to_owned()),
            PresentationEvent::Move {
                stage: "mara".to_owned(),
                to: Slot::Right,
            },
            PresentationEvent::Swap {
                stage: "mara".to_owned(),
                variant: "surprised".to_owned(),
            },
        ];
        let mut with_transition = staging.clone();
        with_transition.push(PresentationEvent::Transition {
            kind: TransitionKind::Wipe,
            ms: 9_000,
        });

        let plain = fold_cues(&staging, &lookup);
        let transitioned = fold_cues(&with_transition, &lookup);
        assert_eq!(plain.background, transitioned.background);
        assert_eq!(plain.fit, transitioned.fit);
        assert_eq!(plain.anchor, transitioned.anchor);
        assert_eq!(plain.stages, transitioned.stages);
        assert_eq!(
            transitioned.transition,
            Some(PlayTransition::new(TransitionKind::Wipe, MAX_TRANSITION_MS)),
            "the duration is clamped to the contract bound"
        );
    }

    #[test]
    fn stages_in_one_slot_are_ordered_by_layer_then_insertion() {
        let mut lookup = FixtureLookup::default();
        for (id, layer) in [("front", 3_u8), ("back", 1_u8), ("middle", 1_u8)] {
            lookup.stages.insert(
                id.to_owned(),
                FixtureStage {
                    slot: Some(Slot::Left),
                    layer: Some(layer),
                    ..FixtureStage::default()
                },
            );
        }
        let cues = vec![
            PresentationEvent::Show("front".to_owned()),
            PresentationEvent::Show("back".to_owned()),
            PresentationEvent::Show("middle".to_owned()),
        ];
        let state = fold_cues(&cues, &lookup);

        let ordered = stages_in_slot(&state.stages, Slot::Left)
            .iter()
            .map(|stage| stage.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ordered, ["back", "middle", "front"]);
        assert!(stages_in_slot(&state.stages, Slot::Right).is_empty());
    }

    #[test]
    fn slots_land_on_the_normalized_grid() {
        let fractions = SLOT_ORDER
            .iter()
            .copied()
            .map(slot_fraction)
            .collect::<Vec<_>>();
        assert_eq!(fractions, [0.1, 0.3, 0.5, 0.7, 0.9]);
        assert_eq!(slot_index(Slot::FarLeft), 0);
        assert_eq!(slot_index(Slot::FarRight), 4);
        for pair in SLOT_ORDER.windows(2) {
            assert!(slot_index(pair[0]) < slot_index(pair[1]));
        }
    }

    #[test]
    fn an_undeclared_scene_falls_back_to_the_contract_defaults() {
        let lookup = FixtureLookup::default();
        let state = fold_cues(&[PresentationEvent::Scene("unknown".to_owned())], &lookup);
        assert_eq!(state.background.as_deref(), Some("unknown"));
        assert_eq!(state.fit, Fit::Cover);
        assert_eq!(state.anchor, AnchorIr { x: 0.5, y: 0.5 });

        let source = r#"
scene plain {
    background: "art/plain.png"
}

=== start ===
@scene plain
The crossing.
-> END
"#;
        let story = weave_compiler::compile(source, &weave_compiler::CompileOptions::default())
            .expect("fixture compiles")
            .story;
        let mut session = PreviewSession::from_story(story, 3, 1);
        session.run();
        assert_eq!(session.scene_fit("plain"), Some(Fit::Cover));
        assert_eq!(
            session.scene_anchor("plain"),
            Some(AnchorIr { x: 0.5, y: 0.5 })
        );
        assert_eq!(session.scene_fit("missing"), None);
        assert_eq!(session.scene_anchor("missing"), None);
        assert_eq!(session.stage_slot("missing"), None);
        assert_eq!(session.stage_layer("missing"), None);
        assert_eq!(session.stage_variant_image("missing", "ghost"), None);
        let scene = scene_from_session(&session);
        assert_eq!(scene.fit, Fit::Cover);
        assert_eq!(scene.anchor, AnchorIr { x: 0.5, y: 0.5 });
    }

    #[test]
    fn declared_fit_anchor_slot_layer_and_variants_resolve() {
        let source = r#"
scene deck {
    background: "art/deck.png"
    fit: contain
    anchor: 0.0, 1.0
}

stage mara {
    image: "art/mara_neutral.png"
    name: "Mara"
    slot: far_right
    layer: 9
    variant surprised {
        image: "art/mara_surprised.png"
    }
}

=== start ===
@scene deck
@show mara
Mara: The ferry leaves at dusk.
-> END
"#;
        let story = weave_compiler::compile(source, &weave_compiler::CompileOptions::default())
            .expect("fixture compiles")
            .story;
        let mut session = PreviewSession::from_story(story, 3, 1);
        session.run();
        assert_eq!(session.scene_fit("deck"), Some(Fit::Contain));
        assert_eq!(
            session.scene_anchor("deck"),
            Some(AnchorIr { x: 0.0, y: 1.0 })
        );
        assert_eq!(session.stage_slot("mara"), Some(Slot::FarRight));
        assert_eq!(session.stage_layer("mara"), Some(9));
        assert_eq!(
            session.stage_variant_image("mara", "surprised"),
            Some("art/mara_surprised.png")
        );
        assert_eq!(session.stage_variant_image("mara", "ghost"), None);

        let scene = scene_from_session(&session);
        assert_eq!(scene.background.as_deref(), Some("deck"));
        assert_eq!(scene.fit, Fit::Contain);
        assert_eq!(scene.anchor, AnchorIr { x: 0.0, y: 1.0 });
        assert_eq!(scene.stages[0].slot, Slot::FarRight);
        assert_eq!(scene.stages[0].layer, 9);
    }

    fn story_with_missing_variant() -> StoryIr {
        let mut story = StoryIr::new("start");
        story.presentation.stages.insert(
            "mara".to_owned(),
            StageIr {
                image: Some("art/mara_neutral.png".to_owned()),
                name: Some("Mara".to_owned()),
                slot: Slot::Left,
                layer: 1,
                variants: BTreeMap::new(),
            },
        );
        let span = Span::new(0, 0, 1, 1);
        story.knots.insert(
            "start".to_owned(),
            KnotIr {
                content: vec![
                    Instruction {
                        span,
                        kind: InstructionKind::Cue(CueIr::Show("mara".to_owned())),
                    },
                    Instruction {
                        span,
                        kind: InstructionKind::Cue(CueIr::Swap {
                            stage: "mara".to_owned(),
                            variant: "ghost".to_owned(),
                        }),
                    },
                    Instruction {
                        span,
                        kind: InstructionKind::Text(Template::text("The ferry leaves at dusk.")),
                    },
                    Instruction {
                        span,
                        kind: InstructionKind::End,
                    },
                ],
            },
        );
        story
    }

    #[test]
    fn a_restart_clears_stages_and_fallback_notes() {
        let mut session = PreviewSession::from_story(story_with_missing_variant(), 3, 1);
        session.run();
        let scene = scene_from_session(&session);
        assert_eq!(scene.stages.len(), 1);
        assert_eq!(
            scene.stages[0].image.as_deref(),
            Some("art/mara_neutral.png"),
            "a missing variant keeps the previous image"
        );
        assert_eq!(scene.stages[0].variant, None);
        assert_eq!(scene.notes.len(), 1);
        assert!(scene.notes[0].contains("mara"));
        assert!(scene.notes[0].contains("ghost"));

        session.restart();
        let scene = scene_from_session(&session);
        assert!(scene.stages.is_empty());
        assert!(scene.notes.is_empty());
        assert_eq!(scene.background, None);
    }

    #[test]
    fn a_swap_on_a_hidden_stage_is_ignored_without_a_note() {
        let lookup = staging_fixture();
        let state = fold_cues(
            &[PresentationEvent::Swap {
                stage: "keeper".to_owned(),
                variant: "ghost".to_owned(),
            }],
            &lookup,
        );
        assert!(state.stages.is_empty());
        assert!(state.notes.is_empty());
    }

    #[test]
    fn a_removal_consumes_the_pending_transition() {
        let lookup = staging_fixture();
        let state = fold_cues(
            &[
                PresentationEvent::Show("mara".to_owned()),
                PresentationEvent::Transition {
                    kind: TransitionKind::Fade,
                    ms: 200,
                },
                PresentationEvent::Hide("mara".to_owned()),
                PresentationEvent::Show("keeper".to_owned()),
            ],
            &lookup,
        );

        assert!(
            state.stages.iter().all(|stage| stage.id != "mara"),
            "the removal still takes effect"
        );
        assert_eq!(
            state.transition, None,
            "the removal consumes the transition instead of leaking it"
        );
        let keeper = state
            .stages
            .iter()
            .find(|stage| stage.id == "keeper")
            .expect("keeper is staged");
        assert_eq!(
            keeper.transition, None,
            "the later show must not inherit the removal's transition"
        );
    }

    #[test]
    fn a_later_transition_replaces_an_unapplied_one() {
        let lookup = staging_fixture();
        let state = fold_cues(
            &[
                PresentationEvent::Transition {
                    kind: TransitionKind::Fade,
                    ms: 200,
                },
                PresentationEvent::Transition {
                    kind: TransitionKind::Wipe,
                    ms: 50,
                },
                PresentationEvent::Show("mara".to_owned()),
            ],
            &lookup,
        );

        let mara = state
            .stages
            .iter()
            .find(|stage| stage.id == "mara")
            .expect("mara is staged");
        assert_eq!(
            mara.transition,
            Some(PlayTransition::new(TransitionKind::Wipe, 50)),
            "only the last transition before a staged operation applies"
        );
        assert_eq!(state.transition, None);
    }

    #[test]
    fn a_cue_on_a_hidden_stage_changes_no_frame_and_keeps_the_transition() {
        let lookup = staging_fixture();
        let state = fold_cues(
            &[
                PresentationEvent::Transition {
                    kind: TransitionKind::Fade,
                    ms: 200,
                },
                PresentationEvent::Move {
                    stage: "keeper".to_owned(),
                    to: Slot::Center,
                },
                PresentationEvent::Swap {
                    stage: "keeper".to_owned(),
                    variant: "ghost".to_owned(),
                },
                PresentationEvent::Hide("keeper".to_owned()),
                PresentationEvent::Show("mara".to_owned()),
            ],
            &lookup,
        );

        let mara = state
            .stages
            .iter()
            .find(|stage| stage.id == "mara")
            .expect("mara is staged");
        assert_eq!(
            mara.transition,
            Some(PlayTransition::new(TransitionKind::Fade, 200)),
            "a move, swap, or hide of a hidden stage changes no frame"
        );
    }

    #[test]
    fn an_out_of_range_or_non_finite_anchor_lands_on_the_image() {
        assert_eq!(
            sanitize_anchor(AnchorIr { x: 2.0, y: -1.0 }),
            AnchorIr { x: 1.0, y: 0.0 }
        );
        assert_eq!(
            sanitize_anchor(AnchorIr {
                x: f64::NAN,
                y: f64::INFINITY
            }),
            AnchorIr { x: 0.5, y: 0.5 }
        );

        let mut lookup = FixtureLookup::default();
        lookup.scenes.insert(
            "tilted".to_owned(),
            (Fit::Cover, AnchorIr { x: 4.0, y: -2.0 }),
        );
        let state = fold_cues(&[PresentationEvent::Scene("tilted".to_owned())], &lookup);
        assert_eq!(state.anchor, AnchorIr { x: 1.0, y: 0.0 });
    }

    #[test]
    fn a_show_replaces_a_visible_stage_and_keeps_its_insertion_order() {
        let lookup = staging_fixture();
        let state = fold_cues(
            &[
                PresentationEvent::Show("keeper".to_owned()),
                PresentationEvent::Show("mara".to_owned()),
                PresentationEvent::Swap {
                    stage: "mara".to_owned(),
                    variant: "surprised".to_owned(),
                },
                PresentationEvent::Show("mara".to_owned()),
            ],
            &lookup,
        );

        let mara = state
            .stages
            .iter()
            .find(|stage| stage.id == "mara")
            .expect("mara is staged");
        assert_eq!(mara.order, 1, "a repeat show keeps the first insertion");
        assert_eq!(
            mara.image.as_deref(),
            Some("art/mara_neutral.png"),
            "a repeat show restores the base image"
        );
        assert_eq!(mara.variant, None);
        assert_eq!(
            state
                .stages
                .iter()
                .map(|stage| stage.id.as_str())
                .collect::<Vec<_>>(),
            ["keeper", "mara"]
        );
    }

    #[test]
    fn cues_at_one_story_step_fold_in_source_order() {
        let lookup = staging_fixture();
        let forward = fold_cues(
            &[
                PresentationEvent::Show("mara".to_owned()),
                PresentationEvent::Swap {
                    stage: "mara".to_owned(),
                    variant: "surprised".to_owned(),
                },
                PresentationEvent::Move {
                    stage: "mara".to_owned(),
                    to: Slot::Right,
                },
            ],
            &lookup,
        );
        let mara = forward
            .stages
            .iter()
            .find(|stage| stage.id == "mara")
            .expect("mara is staged");
        assert_eq!(mara.image.as_deref(), Some("art/mara_surprised.png"));
        assert_eq!(mara.slot, Slot::Right);
        assert_eq!(forward.notes, Vec::<String>::new());

        let swap_first = fold_cues(
            &[
                PresentationEvent::Swap {
                    stage: "mara".to_owned(),
                    variant: "surprised".to_owned(),
                },
                PresentationEvent::Show("mara".to_owned()),
            ],
            &lookup,
        );
        assert_eq!(
            swap_first.stages[0].image.as_deref(),
            Some("art/mara_neutral.png"),
            "a swap before the show has no visible stage to replace"
        );
        assert!(swap_first.notes.is_empty());
    }

    fn presentation_story() -> StoryIr {
        let mut story = StoryIr::new("start");
        story.presentation.scenes.insert(
            "harbour".to_owned(),
            SceneIr {
                background: Some("art/harbour.png".to_owned()),
                fit: Fit::Cover,
                anchor: AnchorIr::default(),
            },
        );
        story.presentation.scenes.insert(
            "cabin".to_owned(),
            SceneIr {
                background: Some("art/cabin.png".to_owned()),
                fit: Fit::Contain,
                anchor: AnchorIr::default(),
            },
        );
        story.presentation.stages.insert(
            "mara".to_owned(),
            StageIr {
                image: Some("art/mara_neutral.png".to_owned()),
                name: Some("Mara".to_owned()),
                slot: Slot::Left,
                layer: 1,
                variants: BTreeMap::new(),
            },
        );
        story.presentation.stages.insert(
            "keeper".to_owned(),
            StageIr {
                image: Some("art/keeper_neutral.png".to_owned()),
                name: Some("The Keeper".to_owned()),
                slot: Slot::Right,
                layer: 2,
                variants: BTreeMap::new(),
            },
        );
        story.modules.insert(
            "world".to_owned(),
            DomainModuleIr {
                id: "org.weave.world".to_owned(),
                version: "1.0.0".to_owned(),
                pack_id: "example.world".to_owned(),
                pack_version: "1.0.0".to_owned(),
                exports: BTreeMap::from([(
                    "seed".to_owned(),
                    DomainExportIr {
                        source: DomainExportSourceIr::Pack,
                        value: DomainValueIr::Object(BTreeMap::from([(
                            "terrain".to_owned(),
                            DomainValueIr::Symbol("harbour".to_owned()),
                        )])),
                    },
                )]),
                authored_overrides: Vec::new(),
            },
        );
        story
    }

    fn presentation_snapshot() -> BTreeMap<String, Value> {
        BTreeMap::from([("speaker".to_owned(), Value::String("mara".to_owned()))])
    }

    #[test]
    fn drawer_rows_show_world_and_character_references_with_values() {
        let story = presentation_story();
        let variables = presentation_snapshot();
        let world = WorldPresentation {
            selector: ValueRef::Module {
                alias: "world".to_owned(),
                path: vec!["seed".to_owned(), "terrain".to_owned()],
            },
            scenes: BTreeMap::from([("harbour".to_owned(), "harbour".to_owned())]),
            fallback: Some("cabin".to_owned()),
        };
        let character = CharacterPresentation {
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
            fallback_stage: Some("keeper".to_owned()),
        };

        let plan = resolve_presentation(&story, Some(&world), Some(&character), Some(&variables));
        let rows = presentation_drawer_rows(&plan, &story, Some(&variables));
        assert!(rows.diagnostics.is_empty());

        let scene = rows
            .references
            .iter()
            .find(|row| row.target == ResolvedTarget::Scene)
            .expect("world selection row");
        assert_eq!(scene.reference, "module `world` export `seed.terrain`");
        assert_eq!(scene.id, "harbour");
        assert_eq!(scene.value.as_deref(), Some("harbour"));
        assert!(!scene.fallback);
        assert_eq!(
            scene.summary(),
            "module `world` export `seed.terrain` → scene `harbour` = harbour"
        );

        let stage = rows
            .references
            .iter()
            .find(|row| row.target == ResolvedTarget::Stage)
            .expect("character selection row");
        assert_eq!(stage.reference, "variable `speaker`");
        assert_eq!(stage.id, "mara");
        assert_eq!(stage.value.as_deref(), Some("mara"));
        assert!(!stage.fallback);
    }

    #[test]
    fn drawer_rows_show_a_fallback_and_never_echo_the_rejected_value() {
        let story = presentation_story();
        let secret = "never-print-this-token";
        let variables = BTreeMap::from([("place".to_owned(), Value::String(secret.to_owned()))]);
        let world = WorldPresentation {
            selector: ValueRef::Variable {
                name: "place".to_owned(),
            },
            scenes: BTreeMap::new(),
            fallback: Some("cabin".to_owned()),
        };

        let plan = resolve_presentation(&story, Some(&world), None, Some(&variables));
        let rows = presentation_drawer_rows(&plan, &story, Some(&variables));

        let scene = &rows.references[0];
        assert_eq!(scene.id, "cabin");
        assert!(scene.fallback);
        assert_eq!(scene.value, None, "a rejected value is never repeated");
        assert!(scene.summary().ends_with("(fallback)"));
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
        assert_eq!(rows.diagnostics[0].kind.label(), "unmapped_value");
        assert_eq!(rows.diagnostics[0].source, "variable `place`");
        for diagnostic in &rows.diagnostics {
            assert!(!diagnostic.source.contains(secret));
            assert!(!diagnostic.detail.contains(secret));
        }
        assert!(!format!("{rows:?}").contains(secret));
    }

    #[test]
    fn a_missing_reference_reports_its_diagnostic_kind_and_label() {
        let story = presentation_story();
        let world = WorldPresentation {
            selector: ValueRef::Variable {
                name: "absent".to_owned(),
            },
            scenes: BTreeMap::new(),
            fallback: Some("cabin".to_owned()),
        };

        let plan = resolve_presentation(&story, Some(&world), None, None);
        let rows = presentation_drawer_rows(&plan, &story, None);

        assert_eq!(
            rows.diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.kind.label(), diagnostic.source.as_str()))
                .collect::<Vec<_>>(),
            [
                ("unavailable_value", "variable `absent`"),
                ("fallback", "variable `absent`")
            ]
        );
        assert_eq!(rows.references[0].id, "cabin");
        assert!(rows.references[0].fallback);
        assert_eq!(rows.references[0].value, None);
    }

    // --- Accessibility and interaction ------------------------------------------

    fn play_context(playtest_expanded: bool) -> PlayFocusContext {
        PlayFocusContext {
            mode: PlayMode::Play,
            playtest_expanded,
            ..PlayFocusContext::default()
        }
    }

    fn ready_scene() -> PlayScene {
        let mut scene = scene_from_session(&session());
        scene.status = PreviewStatus::Ready;
        scene.choices.clear();
        scene
    }

    #[test]
    fn focus_order_is_documented_and_covers_every_visible_control() {
        let scene = scene_from_session(&session());
        let order = play_focus_order(&scene, play_context(false));

        assert_eq!(order[0], PlayFocusTarget::Mode(PlayMode::Debug));
        assert_eq!(order[1], PlayFocusTarget::Mode(PlayMode::Play));
        assert_eq!(order[2], PlayFocusTarget::Advance);
        assert_eq!(
            order
                .iter()
                .filter(|target| matches!(target, PlayFocusTarget::Choice(_)))
                .count(),
            scene.choices.len(),
            "every pending choice is reachable in numeric order"
        );
        assert!(order.contains(&PlayFocusTarget::PlaytestToggle));
        assert!(order.contains(&PlayFocusTarget::PresentationToggle));
        assert!(order.contains(&PlayFocusTarget::DiagnosticsToggle));
        assert!(
            !order.contains(&PlayFocusTarget::PlaytestCapture),
            "a closed drawer exposes no focus stops"
        );

        let open = play_focus_order(&scene, play_context(true));
        let toggle = open
            .iter()
            .position(|target| *target == PlayFocusTarget::PlaytestToggle)
            .expect("the toggle is reachable");
        assert_eq!(open[toggle + 1], PlayFocusTarget::PlaytestCapture);
        assert_eq!(
            open[toggle + 6],
            PlayFocusTarget::PlaytestReplay,
            "the drawer buttons follow the toggle in draw order"
        );

        let mut ended_session = session();
        ended_session.choose(0);
        let ended = scene_from_session(&ended_session);
        let ended_order = play_focus_order(&ended, play_context(false));
        assert!(ended_order.contains(&PlayFocusTarget::Restart));
        assert!(!ended_order.contains(&PlayFocusTarget::Advance));
    }

    #[test]
    fn debug_mode_exposes_every_debug_control_in_draw_order() {
        let scene = scene_from_session(&session());
        let order = play_focus_order(
            &scene,
            PlayFocusContext {
                mode: PlayMode::Debug,
                debug_choices: scene.choices.len(),
                debug_knots: 2,
                has_issue: true,
                ..PlayFocusContext::default()
            },
        );

        assert_eq!(order[0], PlayFocusTarget::Mode(PlayMode::Debug));
        assert_eq!(order[2], PlayFocusTarget::DebugStep);
        assert_eq!(order[7], PlayFocusTarget::DebugSeedUp);
        let choice_index = order
            .iter()
            .position(|target| matches!(target, PlayFocusTarget::DebugChoice(_)))
            .expect("a choice control");
        let issue_index = order
            .iter()
            .position(|target| *target == PlayFocusTarget::DebugIssue)
            .expect("an issue control");
        let knot_index = order
            .iter()
            .position(|target| matches!(target, PlayFocusTarget::DebugKnot(_)))
            .expect("a knot control");
        assert!(choice_index < issue_index);
        assert!(issue_index < knot_index, "focus follows the draw order");
    }

    #[test]
    fn tab_shift_tab_and_home_end_follow_the_documented_order() {
        let scene = scene_from_session(&session());
        let order = play_focus_order(&scene, play_context(false));
        let mut focus = PlayFocus::default();
        focus.focus(PlayFocusTarget::Advance);

        assert_eq!(
            focus.step(&order, true),
            PlayFocusTarget::Choice(0),
            "Tab moves forward one control"
        );
        assert_eq!(
            focus.step(&order, false),
            PlayFocusTarget::Advance,
            "Shift+Tab moves backward one control"
        );
        assert_eq!(focus.home(&order), PlayFocusTarget::Mode(PlayMode::Debug));
        assert_eq!(
            focus.step(&order, false),
            PlayFocusTarget::DiagnosticsToggle,
            "the order wraps at the start"
        );
        assert_eq!(focus.end(&order), PlayFocusTarget::DiagnosticsToggle);
        assert_eq!(
            focus.step(&order, true),
            PlayFocusTarget::Mode(PlayMode::Debug)
        );
    }

    #[test]
    fn keyboard_number_keys_and_enter_select_a_choice() {
        let scene = scene_from_session(&session());
        let order = play_focus_order(&scene, play_context(false));

        assert_eq!(
            play_key_for("2", Some("2"), false, false, false, false),
            Some(PlayKey::Choose(1)),
            "number keys select the matching choice"
        );
        assert_eq!(
            play_key_for("9", Some("9"), false, false, false, false),
            Some(PlayKey::Choose(8))
        );
        assert_eq!(
            play_key_for("0", Some("0"), false, false, false, false),
            None,
            "there is no zero-th choice"
        );

        let mut focus = PlayFocus::default();
        focus.focus(PlayFocusTarget::Choice(1));
        assert_eq!(
            focus.handle_key(PlayKey::Activate, &order),
            PlayAction::Activate(PlayFocusTarget::Choice(1)),
            "Enter or Space activates the focused choice"
        );
        assert_eq!(
            focus.handle_key(PlayKey::Choose(1), &order),
            PlayAction::Choose(1)
        );
    }

    #[test]
    fn focus_restores_to_the_drawer_toggle_when_a_control_closes() {
        let scene = scene_from_session(&session());
        let open = play_focus_order(&scene, play_context(true));
        let closed = play_focus_order(&scene, play_context(false));
        assert!(open.contains(&PlayFocusTarget::PlaytestExportJson));
        assert!(!closed.contains(&PlayFocusTarget::PlaytestExportJson));

        let mut focus = PlayFocus::default();
        focus.focus(PlayFocusTarget::PlaytestExportJson);
        assert_eq!(
            focus.reconcile(&closed),
            PlayFocusTarget::PlaytestToggle,
            "a vanished drawer control restores focus to its toggle"
        );

        focus.focus(PlayFocusTarget::DebugStep);
        assert_eq!(
            focus.reconcile(&closed),
            PlayFocusTarget::Mode(PlayMode::Debug),
            "an unreachable control restores a reachable one"
        );

        let after_choice = play_focus_order(&ready_scene(), play_context(false));
        focus.focus(PlayFocusTarget::Choice(0));
        assert_eq!(
            focus.reconcile(&after_choice),
            PlayFocusTarget::Advance,
            "a chosen option restores focus to the next scene action"
        );
    }

    #[test]
    fn accent_filled_controls_use_a_contrasting_ring_and_label() {
        assert_eq!(focus_indicator_colour_on_fill(true), DARK_THEME.workspace);
        assert_eq!(on_accent_text_colour(), DARK_THEME.workspace);
        assert!(
            contrast_ratio(on_accent_text_colour(), DARK_THEME.accent) >= 4.5,
            "label on the accent fill must reach 4.5:1"
        );
        assert!(
            contrast_ratio(focus_indicator_colour_on_fill(true), DARK_THEME.accent) >= 3.0,
            "focus ring on the accent fill must reach 3:1"
        );
    }

    #[test]
    fn focus_indicator_and_body_text_meet_minimum_contrast() {
        assert_eq!(focus_indicator_colour(true), DARK_THEME.accent);
        assert_eq!(focus_indicator_colour(false), DARK_THEME.border);
        assert!(
            contrast_ratio(DARK_THEME.text, DARK_THEME.panel) >= MIN_TEXT_CONTRAST,
            "dialogue text on the panel meets WCAG AA"
        );
        assert!(
            contrast_ratio(DARK_THEME.text, DARK_THEME.chrome) >= MIN_TEXT_CONTRAST,
            "dialogue text on chrome meets WCAG AA"
        );
        assert!(
            contrast_ratio(DARK_THEME.accent, DARK_THEME.panel) >= MIN_FOCUS_CONTRAST,
            "the focus indicator meets the non-text minimum on the panel"
        );
        assert!(
            contrast_ratio(DARK_THEME.accent, DARK_THEME.border) >= MIN_FOCUS_CONTRAST,
            "the focus indicator meets the non-text minimum beside a control border"
        );
        assert!(contrast_ratio(0x000000, 0xffffff) > 20.0);
    }

    #[test]
    fn text_scale_steps_are_monotonic_and_bounded() {
        assert_eq!(scaled_text_px(20.0, DEFAULT_TEXT_SCALE), 20.0);
        assert!(scaled_text_px(20.0, 1.5) > scaled_text_px(20.0, 1.0));
        assert!(scaled_dialogue_height(DIALOGUE_MAX_HEIGHT_PX, 1.5) > DIALOGUE_MAX_HEIGHT_PX);

        assert_eq!(step_text_scale(1.0, 1), 1.15);
        assert_eq!(step_text_scale(1.0, -1), 0.85);
        assert_eq!(step_text_scale(1.5, 1), 1.5);
        assert_eq!(step_text_scale(0.85, -1), 0.85);
        assert_eq!(clamp_text_scale(f32::NAN), DEFAULT_TEXT_SCALE);
        assert_eq!(clamp_text_scale(9.0), 1.5);
    }

    #[test]
    fn reduced_motion_suppresses_transitions_without_reordering_cues() {
        let lookup = staging_fixture();
        let cues = vec![
            PresentationEvent::Scene("deck".to_owned()),
            PresentationEvent::Show("mara".to_owned()),
            PresentationEvent::Show("keeper".to_owned()),
            PresentationEvent::Transition {
                kind: TransitionKind::Fade,
                ms: 240,
            },
            PresentationEvent::Move {
                stage: "mara".to_owned(),
                to: Slot::Right,
            },
        ];
        let state = fold_cues(&cues, &lookup);
        let full_order = ordered_stages(&state.stages)
            .iter()
            .map(|stage| stage.id.as_str())
            .collect::<Vec<_>>();
        let reduced = MotionPreference::Reduced;
        let reduced_order = vec!["mara", "keeper"];
        assert_eq!(
            full_order, reduced_order,
            "cue order is fixed by the cue list, not by the motion preference"
        );
        assert_eq!(
            reduced.transition(state.stages[0].transition),
            None,
            "reduced motion suppresses the transition, not the cue"
        );

        let transition = Some(PlayTransition::new(TransitionKind::Fade, 240));
        assert_eq!(MotionPreference::Full.transition(transition), transition);
        assert_eq!(MotionPreference::Reduced.transition(transition), None);
        assert_eq!(
            reduced.stage_opacity(&state.stages[0]),
            1.0,
            "reduced motion settles the figure immediately"
        );
        assert_eq!(
            PlayScene::stage_opacity(&state.stages[0]),
            1.0,
            "a clock-free surface settles at the final opacity"
        );

        let mut scene = ready_scene();
        scene.background_transition = transition;
        assert_eq!(scene.background_opacity(), 1.0);
        assert_eq!(reduced.background_opacity(&scene), 1.0);
        assert!(reduced.reveal(TextReveal::new(5)).instant);
    }

    #[test]
    fn text_reveal_honours_speed_instant_mode_and_character_boundaries() {
        let reveal = TextReveal::new(10);
        assert_eq!(
            reveal.visible_chars("abcdefghij", Duration::ZERO),
            0,
            "nothing is revealed before the first tick"
        );
        assert_eq!(
            reveal.visible_chars("abcdefghij", Duration::from_millis(500)),
            5
        );
        assert_eq!(
            reveal.visible_chars("abcdefghij", Duration::from_secs(5)),
            10,
            "reveal never exceeds the line"
        );
        assert!(reveal.is_complete("abc", Duration::from_secs(1)));
        assert_eq!(
            TextReveal::instant().visible_chars("abcdef", Duration::ZERO),
            6,
            "instant reveal shows the whole line at once"
        );
        assert_eq!(reveal.with_speed(0).chars_per_second, MIN_REVEAL_CPS);
        assert_eq!(reveal.with_speed(9999).chars_per_second, MAX_REVEAL_CPS);

        let multibyte = "é中🙂x";
        assert_eq!(
            TextReveal::new(10).visible_chars(multibyte, Duration::from_millis(200)),
            2
        );
        assert_eq!(
            TextReveal::new(10).reveal(multibyte, Duration::from_millis(200)),
            "é中",
            "multi-byte characters are not split"
        );
    }

    #[test]
    fn auto_advance_is_bounded_and_stops_at_choices_and_the_end() {
        let waiting = scene_from_session(&session());
        let ready = ready_scene();
        let policy = AutoAdvancePolicy {
            enabled: true,
            delay: Duration::from_millis(100),
            max_steps: 2,
        };
        let mut state = AutoAdvanceState::new(policy);

        assert!(
            !state.can_advance(&waiting, Duration::from_secs(1)),
            "auto-advance stops at a choice"
        );
        assert!(
            !state.can_advance(&ready, Duration::from_millis(99)),
            "the delay is respected"
        );
        assert!(state.can_advance(&ready, Duration::from_millis(100)));

        assert!(state.record_advance());
        assert!(state.record_advance());
        assert!(!state.record_advance(), "the third step exceeds the bound");
        assert_eq!(state.advanced(), 2);
        assert!(!state.can_advance(&ready, Duration::from_secs(1)));

        state.reset();
        let mut ended = ready.clone();
        ended.ended = true;
        ended.status = PreviewStatus::Ended;
        assert!(!state.can_advance(&ended, Duration::from_secs(1)));

        state.reset();
        let mut issue = ready;
        issue.issue = Some("compile failed".to_owned());
        assert!(!state.can_advance(&issue, Duration::from_secs(1)));

        let clamped = AutoAdvanceState::new(AutoAdvancePolicy {
            enabled: true,
            delay: Duration::ZERO,
            max_steps: u32::MAX,
        });
        assert_eq!(clamped.policy().max_steps, MAX_AUTO_ADVANCE_STEPS);
    }

    #[test]
    fn diagnostics_are_critical_first_and_reachable_from_the_control_region() {
        let mut scene = scene_from_session(&session());
        scene.issue = Some("Last valid frame kept: E001: broken".to_owned());
        scene
            .notes
            .push("stage \"mara\" — the image is missing".to_owned());
        let rows = PresentationDrawerRows {
            references: Vec::new(),
            diagnostics: vec![PresentationDiagnostic {
                kind: PresentationDiagnosticKind::UnmappedValue,
                source: "variable `place`".to_owned(),
                detail: "the selected value has no host scene mapping".to_owned(),
            }],
        };

        let diagnostics = play_diagnostics(&scene, Some(&rows));
        assert_eq!(diagnostics.len(), 3);
        assert_eq!(
            diagnostics[0].severity,
            PlayDiagnosticSeverity::Error,
            "the compile issue is the first critical diagnostic"
        );
        assert!(has_critical_diagnostics(&diagnostics));
        assert_eq!(diagnostics[1].severity, PlayDiagnosticSeverity::Warning);
        assert_eq!(diagnostics[2].severity, PlayDiagnosticSeverity::Warning);
        assert_eq!(diagnostics[0].severity.label(), "error");

        let order = play_focus_order(&scene, play_context(false));
        assert!(order.contains(&PlayFocusTarget::DiagnosticsToggle));
        assert!(
            play_diagnostics(&ready_scene(), None).is_empty(),
            "a clean frame reports no diagnostics"
        );
    }

    #[test]
    fn accessibility_metadata_names_the_focused_control() {
        let scene = scene_from_session(&session());
        let order = play_focus_order(&scene, play_context(false));
        let metadata = play_accessibility(&order, PlayFocusTarget::PlaytestToggle);

        assert_eq!(metadata.role, "button");
        assert_eq!(metadata.label, "Playtest drawer");
        assert_eq!(
            metadata.position,
            order
                .iter()
                .position(|target| *target == PlayFocusTarget::PlaytestToggle)
                .expect("the toggle is in the order")
                + 1
        );
        assert_eq!(metadata.total, order.len());
        assert!(metadata.keyboard_hint.contains("Tab"));
        assert!(metadata.keyboard_hint.contains("Enter"));
        for target in &order {
            assert!(!target.label().is_empty());
            assert!(!target.role().is_empty());
        }
    }

    #[test]
    fn keyboard_hint_documents_every_shortcut_and_chords_are_left_to_the_host() {
        for shortcut in [
            "Tab",
            "Enter",
            "choose",
            "restart",
            "playtest",
            "diagnostics",
            "reveal speed",
            "auto-advance",
        ] {
            assert!(
                PLAY_KEYBOARD_HINT.contains(shortcut),
                "the hint documents {shortcut}"
            );
        }
        assert_eq!(
            play_key_for("tab", None, true, false, false, false),
            Some(PlayKey::Previous)
        );
        assert_eq!(
            play_key_for("space", None, false, false, false, false),
            Some(PlayKey::Activate)
        );
        assert_eq!(
            play_key_for("[", Some("["), false, false, false, false),
            Some(PlayKey::RevealSlower)
        );
        assert_eq!(
            play_key_for("]", Some("]"), false, false, false, false),
            Some(PlayKey::RevealFaster)
        );
        let mut reveal = TextReveal::new(DEFAULT_REVEAL_CPS);
        reveal = reveal.with_speed(reveal.chars_per_second + REVEAL_SPEED_STEP);
        assert!(reveal.chars_per_second > DEFAULT_REVEAL_CPS);
        assert_eq!(
            play_key_for("r", Some("r"), true, true, false, false),
            None,
            "control chords are left to the host"
        );
        assert_eq!(
            play_key_for("d", Some("d"), false, false, false, true),
            None,
            "platform chords are left to the host"
        );
    }
}
