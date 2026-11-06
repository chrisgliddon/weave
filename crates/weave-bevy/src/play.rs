//! Engine-neutral projection of Weave presentation cues into one renderable frame.
//!
//! The editor Play view folds the same [`PresentationEvent`] cues in source
//! order. This module keeps that projection portable for any Bevy host: it
//! reads only compiled [`StoryIr`] presentation data and runtime cues, and it
//! never depends on editor or GPUI types.
//!
//! The fold is deterministic. One cue list applies in source order, stages keep
//! first-show insertion order, and [`PlayFrame::ordered_stages`] paints by
//! layer then by that stable order. A transition records how the next staged
//! operation is drawn; it never changes which stages are visible.
//!
//! Fallbacks are total. An undeclared scene uses the contract defaults, an
//! undeclared stage uses its identifier as its name with the default slot and
//! layer, and an unknown variant keeps the current image and adds a note.
//! Renderers therefore never need to panic on a missing declaration.

use weave_runtime::PresentationEvent;

pub use weave_core::ir::{AnchorIr, Fit, SceneIr, Slot, StageIr, StoryIr, TransitionKind};

/// Longest transition the presentation contract allows.
pub const MAX_TRANSITION_MS: u16 = 2_000;

/// Display name used when a line has no speaker.
pub const NARRATOR: &str = "Narrator";

/// Normalized left-to-right centre of each slot.
pub const SLOT_FRACTIONS: [f32; 5] = [0.1, 0.3, 0.5, 0.7, 0.9];

/// Flat tones used when a declared scene has no loaded image.
pub const SCENE_TONES: [u32; 4] = [0x101722, 0x171420, 0x141d1c, 0x1d1814];

/// One bounded transition recorded for the next staged operation.
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

    /// Stable style name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        transition_label(self.kind)
    }

    /// Opacity a clock-free renderer applies to the element that just changed.
    ///
    /// A cut is immediate. A fade or wipe settles one bounded step short of
    /// opaque. A slide needs a clock to be visible and collapses to a cut here.
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
    /// Paint order declared by the stage.
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

/// Everything a renderer draws for one story step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayFrame {
    /// Staged background scene id, when a cue set one.
    pub background: Option<String>,
    /// Fit declared by the staged background, or the contract default.
    pub fit: Fit,
    /// Anchor declared by the staged background, or the contract default.
    pub anchor: AnchorIr,
    /// Visible stages in first-show order.
    pub stages: Vec<StagedCharacter>,
    /// Transition recorded for the next staged operation.
    pub transition: Option<PlayTransition>,
    /// Transition that introduced the current background.
    pub background_transition: Option<PlayTransition>,
    /// Speaker declared by the last speaker cue; outer `None` means no cue.
    pub speaker: Option<Option<String>>,
    /// Fallback notes raised while folding, in cue order.
    pub notes: Vec<String>,
    /// Stable labels for every applied cue, in source order.
    pub cue_log: Vec<String>,
    next_order: usize,
}

impl PlayFrame {
    /// Apply one presentation cue in source order and return its stable label.
    pub fn apply(&mut self, cue: &PresentationEvent, story: &StoryIr) -> String {
        let label = cue_label(cue);
        match cue {
            PresentationEvent::Scene(id) => {
                self.background = Some(id.clone());
                let scene = story.presentation.scenes.get(id);
                self.fit = scene.map_or_else(Fit::default, |scene| scene.fit);
                self.anchor =
                    sanitize_anchor(scene.map_or_else(AnchorIr::default, |scene| scene.anchor));
                self.background_transition = self.transition.take();
            }
            PresentationEvent::Show(id) => {
                let stage = story.presentation.stages.get(id);
                let slot = stage.map_or_else(Slot::default, |stage| stage.slot);
                let layer = stage.map_or(0, |stage| stage.layer);
                let name = stage
                    .and_then(|stage| stage.name.clone())
                    .unwrap_or_else(|| id.clone());
                let image = stage.and_then(|stage| stage.image.clone());
                let pending = self.transition.take();
                if let Some(staged) = self.stages.iter_mut().find(|staged| staged.id == *id) {
                    staged.slot = slot;
                    staged.layer = layer;
                    staged.name = name;
                    staged.image = image;
                    staged.variant = None;
                    staged.transition = pending;
                } else {
                    self.stages.push(StagedCharacter {
                        id: id.clone(),
                        name,
                        slot,
                        layer,
                        image,
                        variant: None,
                        order: self.next_order,
                        transition: pending,
                    });
                    self.next_order += 1;
                }
            }
            PresentationEvent::Hide(id) => {
                let before = self.stages.len();
                self.stages.retain(|stage| stage.id != *id);
                if self.stages.len() != before {
                    // A visible removal settles immediately; a hidden stage has
                    // no frame left to animate.
                    self.transition = None;
                }
            }
            PresentationEvent::Speaker(name) => {
                self.speaker = Some(name.clone());
            }
            PresentationEvent::Move { stage, to } => {
                let pending = self.transition.take();
                if let Some(staged) = self.stages.iter_mut().find(|staged| staged.id == *stage) {
                    staged.slot = *to;
                    staged.transition = pending;
                } else {
                    // A hidden stage has no frame to move, so the transition
                    // stays pending for the next operation that changes it.
                    self.transition = pending;
                }
            }
            PresentationEvent::Swap { stage, variant } => {
                let pending = self.transition.take();
                let position = self.stages.iter().position(|staged| staged.id == *stage);
                if let Some(position) = position {
                    let image = story
                        .presentation
                        .stages
                        .get(stage)
                        .and_then(|declared| declared.variants.get(variant))
                        .cloned();
                    match image {
                        Some(path) => {
                            self.stages[position].image = Some(path);
                            self.stages[position].variant = Some(variant.clone());
                        }
                        None => self.notes.push(format!(
                            "stage \"{stage}\" · variant \"{variant}\" — the stage does not declare this variant; keeping the current image"
                        )),
                    }
                    self.stages[position].transition = pending;
                } else {
                    self.transition = pending;
                }
            }
            PresentationEvent::Transition { kind, ms } => {
                self.transition = Some(PlayTransition::new(*kind, *ms));
            }
        }
        self.cue_log.push(label.clone());
        label
    }

    /// Visible stages ordered by paint layer, then by stable insertion order.
    #[must_use]
    pub fn ordered_stages(&self) -> Vec<&StagedCharacter> {
        let mut ordered: Vec<&StagedCharacter> = self.stages.iter().collect();
        ordered.sort_by_key(|stage| (stage.layer, stage.order));
        ordered
    }

    /// Resolved display name of the current speaker, when a cue named one.
    ///
    /// A bare speaker cue resolves to [`NARRATOR`]. `None` means no cue has run,
    /// so a renderer may fall back to a prose prefix.
    #[must_use]
    pub fn speaker_name(&self, story: &StoryIr) -> Option<String> {
        match &self.speaker {
            Some(Some(id)) => Some(
                story
                    .presentation
                    .stages
                    .get(id)
                    .and_then(|stage| stage.name.clone())
                    .unwrap_or_else(|| id.clone()),
            ),
            Some(None) => Some(NARRATOR.to_owned()),
            None => None,
        }
    }

    /// Opacity the renderer applies to one stage figure.
    #[must_use]
    pub fn stage_opacity(stage: &StagedCharacter) -> f32 {
        stage.transition.map_or(1.0, PlayTransition::opacity)
    }

    /// Opacity the renderer applies to the background.
    #[must_use]
    pub fn background_opacity(&self) -> f32 {
        self.background_transition
            .map_or(1.0, PlayTransition::opacity)
    }
}

/// Fold one ordered cue list into a frame.
#[must_use]
pub fn fold_cues(cues: &[PresentationEvent], story: &StoryIr) -> PlayFrame {
    let mut frame = PlayFrame::default();
    for cue in cues {
        frame.apply(cue, story);
    }
    frame
}

/// Normalized left-to-right centre of one slot.
#[must_use]
pub const fn slot_fraction(slot: Slot) -> f32 {
    match slot {
        Slot::FarLeft => SLOT_FRACTIONS[0],
        Slot::Left => SLOT_FRACTIONS[1],
        Slot::Center => SLOT_FRACTIONS[2],
        Slot::Right => SLOT_FRACTIONS[3],
        Slot::FarRight => SLOT_FRACTIONS[4],
    }
}

/// Stable slot name used by labels and tests.
#[must_use]
pub const fn slot_label(slot: Slot) -> &'static str {
    match slot {
        Slot::FarLeft => "far_left",
        Slot::Left => "left",
        Slot::Center => "center",
        Slot::Right => "right",
        Slot::FarRight => "far_right",
    }
}

/// Stable fit name used by labels and tests.
#[must_use]
pub const fn fit_label(fit: Fit) -> &'static str {
    match fit {
        Fit::Cover => "cover",
        Fit::Contain => "contain",
    }
}

/// Stable transition name used by labels and tests.
#[must_use]
pub const fn transition_label(kind: TransitionKind) -> &'static str {
    match kind {
        TransitionKind::Cut => "cut",
        TransitionKind::Slide => "slide",
        TransitionKind::Fade => "fade",
        TransitionKind::Wipe => "wipe",
    }
}

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
#[must_use]
pub fn sanitize_anchor(anchor: AnchorIr) -> AnchorIr {
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

/// Split a leading `Speaker: ` prefix from a line.
///
/// Weave lines carry no speaker field, so a host may name a line the way prose
/// does: a short capitalised prefix ending in a colon. Anything else is
/// narration.
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

fn cue_label(cue: &PresentationEvent) -> String {
    match cue {
        PresentationEvent::Scene(id) => format!("scene:{id}"),
        PresentationEvent::Show(id) => format!("show:{id}"),
        PresentationEvent::Hide(id) => format!("hide:{id}"),
        PresentationEvent::Speaker(Some(id)) => format!("speaker:{id}"),
        PresentationEvent::Speaker(None) => "speaker".to_owned(),
        PresentationEvent::Move { stage, to } => {
            format!("move:{stage}:{}", slot_label(*to))
        }
        PresentationEvent::Swap { stage, variant } => format!("swap:{stage}:{variant}"),
        PresentationEvent::Transition { kind, ms } => {
            format!("transition:{}:{ms}", transition_label(*kind))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn story() -> StoryIr {
        let mut story = StoryIr::new("start");
        story.presentation.scenes.insert(
            "tavern".to_owned(),
            SceneIr {
                background: Some("art/tavern_placeholder.png".to_owned()),
                fit: Fit::Contain,
                anchor: AnchorIr { x: 0.25, y: 0.75 },
            },
        );
        story.presentation.stages.insert(
            "innkeeper".to_owned(),
            StageIr {
                image: Some("art/innkeeper_placeholder.png".to_owned()),
                name: Some("The Innkeeper".to_owned()),
                slot: Slot::Left,
                layer: 1,
                variants: BTreeMap::from([(
                    "amused".to_owned(),
                    "art/innkeeper_amused_placeholder.png".to_owned(),
                )]),
            },
        );
        story.presentation.stages.insert(
            "traveller".to_owned(),
            StageIr {
                image: None,
                name: Some("Traveller".to_owned()),
                slot: Slot::Right,
                layer: 2,
                variants: BTreeMap::new(),
            },
        );
        story
    }

    fn staging_cues() -> Vec<PresentationEvent> {
        vec![
            PresentationEvent::Transition {
                kind: TransitionKind::Fade,
                ms: 220,
            },
            PresentationEvent::Scene("tavern".to_owned()),
            PresentationEvent::Show("innkeeper".to_owned()),
            PresentationEvent::Speaker(Some("innkeeper".to_owned())),
            PresentationEvent::Swap {
                stage: "innkeeper".to_owned(),
                variant: "amused".to_owned(),
            },
            PresentationEvent::Move {
                stage: "innkeeper".to_owned(),
                to: Slot::Center,
            },
            PresentationEvent::Transition {
                kind: TransitionKind::Wipe,
                ms: 9_000,
            },
            PresentationEvent::Show("traveller".to_owned()),
            PresentationEvent::Speaker(Some("traveller".to_owned())),
        ]
    }

    #[test]
    fn a_cue_sequence_produces_the_expected_frame_and_order() {
        let story = story();
        let frame = fold_cues(&staging_cues(), &story);

        assert_eq!(frame.background.as_deref(), Some("tavern"));
        assert_eq!(frame.fit, Fit::Contain);
        assert_eq!(frame.anchor, AnchorIr { x: 0.25, y: 0.75 });
        assert_eq!(
            frame.background_transition,
            Some(PlayTransition::new(TransitionKind::Fade, 220))
        );
        assert_eq!(frame.stages.len(), 2);
        let innkeeper = frame
            .stages
            .iter()
            .find(|stage| stage.id == "innkeeper")
            .expect("innkeeper is staged");
        assert_eq!(innkeeper.name, "The Innkeeper");
        assert_eq!(innkeeper.slot, Slot::Center);
        assert_eq!(innkeeper.layer, 1);
        assert_eq!(innkeeper.variant.as_deref(), Some("amused"));
        assert_eq!(
            innkeeper.image.as_deref(),
            Some("art/innkeeper_amused_placeholder.png")
        );
        assert_eq!(innkeeper.order, 0);

        let traveller = frame
            .stages
            .iter()
            .find(|stage| stage.id == "traveller")
            .expect("traveller is staged");
        assert_eq!(traveller.slot, Slot::Right);
        assert_eq!(
            traveller.transition,
            Some(PlayTransition::new(TransitionKind::Wipe, MAX_TRANSITION_MS)),
            "the duration is clamped to the contract bound"
        );
        assert_eq!(frame.speaker_name(&story).as_deref(), Some("Traveller"));
        assert_eq!(
            frame
                .ordered_stages()
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["innkeeper", "traveller"]
        );
        assert_eq!(
            frame.cue_log,
            [
                "transition:fade:220",
                "scene:tavern",
                "show:innkeeper",
                "speaker:innkeeper",
                "swap:innkeeper:amused",
                "move:innkeeper:center",
                "transition:wipe:9000",
                "show:traveller",
                "speaker:traveller",
            ]
        );
    }

    #[test]
    fn the_same_cues_always_fold_to_the_same_frame() {
        let story = story();
        assert_eq!(
            fold_cues(&staging_cues(), &story),
            fold_cues(&staging_cues(), &story)
        );
    }

    #[test]
    fn an_unknown_variant_keeps_the_current_image_and_notes_it() {
        let story = story();
        let frame = fold_cues(
            &[
                PresentationEvent::Show("innkeeper".to_owned()),
                PresentationEvent::Swap {
                    stage: "innkeeper".to_owned(),
                    variant: "furious".to_owned(),
                },
            ],
            &story,
        );
        let innkeeper = &frame.stages[0];
        assert_eq!(
            innkeeper.image.as_deref(),
            Some("art/innkeeper_placeholder.png")
        );
        assert_eq!(innkeeper.variant, None);
        assert_eq!(frame.notes.len(), 1);
        assert!(frame.notes[0].contains("innkeeper"));
        assert!(frame.notes[0].contains("furious"));
        assert!(frame.notes[0].contains("keeping the current image"));
    }

    #[test]
    fn undeclared_scenes_and_stages_use_the_contract_defaults() {
        let story = story();
        let frame = fold_cues(
            &[
                PresentationEvent::Scene("missing_scene".to_owned()),
                PresentationEvent::Show("missing_stage".to_owned()),
            ],
            &story,
        );
        assert_eq!(frame.background.as_deref(), Some("missing_scene"));
        assert_eq!(frame.fit, Fit::Cover);
        assert_eq!(frame.anchor, AnchorIr::default());
        assert_eq!(frame.stages.len(), 1);
        assert_eq!(frame.stages[0].name, "missing_stage");
        assert_eq!(frame.stages[0].slot, Slot::Center);
        assert_eq!(frame.stages[0].layer, 0);
        assert_eq!(frame.stages[0].image, None);
    }

    #[test]
    fn a_missing_image_path_is_carried_without_failure() {
        let mut story = story();
        story.presentation.stages.insert(
            "ghost".to_owned(),
            StageIr {
                image: Some("art/does_not_exist.png".to_owned()),
                name: None,
                slot: Slot::Center,
                layer: 0,
                variants: BTreeMap::new(),
            },
        );
        let frame = fold_cues(&[PresentationEvent::Show("ghost".to_owned())], &story);
        assert_eq!(
            frame.stages[0].image.as_deref(),
            Some("art/does_not_exist.png")
        );
        assert!(frame.notes.is_empty());
    }

    #[test]
    fn a_removal_consumes_the_pending_transition() {
        let story = story();
        let frame = fold_cues(
            &[
                PresentationEvent::Show("innkeeper".to_owned()),
                PresentationEvent::Transition {
                    kind: TransitionKind::Fade,
                    ms: 200,
                },
                PresentationEvent::Hide("innkeeper".to_owned()),
                PresentationEvent::Show("traveller".to_owned()),
            ],
            &story,
        );
        assert!(frame.stages.iter().all(|stage| stage.id != "innkeeper"));
        assert_eq!(frame.transition, None);
        assert_eq!(frame.stages[0].transition, None);
    }

    #[test]
    fn a_cue_on_a_hidden_stage_keeps_the_transition_pending() {
        let story = story();
        let frame = fold_cues(
            &[
                PresentationEvent::Transition {
                    kind: TransitionKind::Fade,
                    ms: 200,
                },
                PresentationEvent::Move {
                    stage: "traveller".to_owned(),
                    to: Slot::Left,
                },
                PresentationEvent::Show("innkeeper".to_owned()),
            ],
            &story,
        );
        assert_eq!(
            frame.stages[0].transition,
            Some(PlayTransition::new(TransitionKind::Fade, 200))
        );
    }

    #[test]
    fn repeated_shows_keep_insertion_order_and_restore_the_base_image() {
        let story = story();
        let frame = fold_cues(
            &[
                PresentationEvent::Show("innkeeper".to_owned()),
                PresentationEvent::Swap {
                    stage: "innkeeper".to_owned(),
                    variant: "amused".to_owned(),
                },
                PresentationEvent::Show("innkeeper".to_owned()),
            ],
            &story,
        );
        assert_eq!(frame.stages[0].order, 0);
        assert_eq!(
            frame.stages[0].image.as_deref(),
            Some("art/innkeeper_placeholder.png")
        );
        assert_eq!(frame.stages[0].variant, None);
    }

    #[test]
    fn slots_land_on_the_normalized_grid() {
        assert_eq!(
            [
                Slot::FarLeft,
                Slot::Left,
                Slot::Center,
                Slot::Right,
                Slot::FarRight,
            ]
            .map(slot_fraction),
            SLOT_FRACTIONS
        );
    }

    #[test]
    fn anchors_are_clamped_and_non_finite_components_center() {
        assert_eq!(
            sanitize_anchor(AnchorIr { x: 2.0, y: -1.0 }),
            AnchorIr { x: 1.0, y: 0.0 }
        );
        assert_eq!(
            sanitize_anchor(AnchorIr {
                x: f64::NAN,
                y: f64::INFINITY,
            }),
            AnchorIr { x: 0.5, y: 0.5 }
        );
    }

    #[test]
    fn prose_speaker_prefixes_split_only_when_clean() {
        assert_eq!(
            split_speaker("The Innkeeper: Welcome in."),
            ("The Innkeeper".to_owned(), "Welcome in.".to_owned())
        );
        assert_eq!(
            split_speaker("The rule was simple: wait for morning."),
            (
                NARRATOR.to_owned(),
                "The rule was simple: wait for morning.".to_owned()
            )
        );
        assert_eq!(
            split_speaker(
                "A very long introductory clause that is really prose: and then it ended."
            ),
            (
                NARRATOR.to_owned(),
                "A very long introductory clause that is really prose: and then it ended."
                    .to_owned()
            )
        );
    }

    #[test]
    fn an_empty_cue_list_yields_the_default_frame() {
        let story = StoryIr::new("start");
        assert_eq!(fold_cues(&[], &story), PlayFrame::default());
    }
}
