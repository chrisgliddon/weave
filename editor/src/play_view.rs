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

use weave_core::ir::{AnchorIr, Fit, Slot, TransitionKind};
use weave_runtime::{ChoiceView, PresentationEvent};

use crate::preview::{PreviewSession, PreviewStatus, PreviewTranscript};

/// Which presentation the preview panel uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    /// A cut is immediate. A fade or wipe settles one bounded step short of
    /// opaque; a renderer with a clock may interpolate to `1.0` over
    /// [`PlayTransition::ms`] and still reach the same final staging. A slide
    /// needs a clock to be visible and collapses to a cut here.
    #[must_use]
    pub const fn opacity(self) -> f32 {
        match self.kind {
            TransitionKind::Cut | TransitionKind::Slide => 1.0,
            TransitionKind::Fade => 0.92,
            TransitionKind::Wipe => 0.96,
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

/// Flat background tones the Play surface uses for declared scenes.
///
/// The cue carries an asset reference; until the asset lifecycle lands the
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

/// Fold one ordered cue list into presentation state.
///
/// Cues apply in source order. A swap replaces only the named stage's current
/// image; when the stage does not declare the variant, the previous image is
/// kept and a note names the stage and the variant. A transition is recorded
/// for the next staged operation and never changes staging itself.
#[must_use]
pub fn fold_cues(cues: &[PresentationEvent], lookup: &dyn PresentationLookup) -> PresentationState {
    let mut state = PresentationState::default();
    let mut insertion = 0_usize;

    for cue in cues {
        match cue {
            PresentationEvent::Scene(id) => {
                state.background = Some(id.clone());
                state.fit = lookup.scene_fit(id).unwrap_or_default();
                state.anchor = lookup.scene_anchor(id).unwrap_or_default();
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
                state.stages.retain(|stage| stage.id != *id);
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::preview::PreviewSession;
    use weave_core::Span;
    use weave_core::ir::{CueIr, Instruction, InstructionKind, KnotIr, StageIr, StoryIr, Template};

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
}
