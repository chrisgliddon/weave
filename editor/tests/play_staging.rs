//! End-to-end staging tests for the Play view.
//!
//! The fixtures are synthetic and declare no external assets. The asset tests
//! create their own small PNG files in a temporary project directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use image::{ImageFormat, Rgba, RgbaImage};
use weave_core::ir::{AnchorIr, Fit, Slot, StoryIr, TransitionKind};
use weave_editor::{
    MAX_TRANSITION_MS, NARRATOR, PlayScene, PlayTransition, PresentationAssets, PresentationLookup,
    PreviewSession, SLOT_ORDER, fold_cues, ordered_stages, scene_from_session, slot_fraction,
    stages_in_slot,
};
use weave_runtime::PresentationEvent;

/// A branching scene with two characters, variants, moves, and two scenes.
///
/// The first branch stops on a nested choice, so the intermediate frame with
/// both characters and the swapped variant is observable.
const STAGING_SOURCE: &str = r#"
scene harbour {
    background: "art/harbour.png"
    fit: cover
    anchor: 0.5, 0.35
}

scene cabin {
    background: "art/cabin.png"
    fit: contain
}

stage mara {
    image: "art/mara_neutral.png"
    name: "Mara"
    slot: left
    layer: 1
    variant surprised {
        image: "art/mara_surprised.png"
    }
}

stage keeper {
    image: "art/keeper_neutral.png"
    name: "The Keeper"
    slot: right
    layer: 3
    variant angry {
        image: "art/keeper_angry.png"
    }
}

=== start ===
@scene harbour
@show mara
@show keeper
@speaker mara
Mara: The ferry leaves at dusk.
* [Ask the keeper]
    @move keeper center
    @swap mara surprised
    @transition fade 240
    @speaker keeper
    The Keeper: Then we cross together.
    * [Cross at once]
        @transition wipe 300
        @scene cabin
        @hide mara
        @swap keeper angry
        @speaker
        Then the water stills.
        -> END
    * [Wait on the shore]
        @hide keeper
        @hide mara
        @speaker
        We wait for morning.
        -> END
* [Leave quietly]
    @hide keeper
    @hide mara
    @speaker
    We wait for morning.
    -> END
"#;

fn compile(source: &str) -> StoryIr {
    weave_compiler::compile(source, &weave_compiler::CompileOptions::default())
        .expect("fixture compiles")
        .story
}

fn drive(story: StoryIr, choices: &[usize]) -> PreviewSession {
    let mut session = PreviewSession::from_story(story, 7, 1);
    assert!(session.run(), "the story reaches its first choice");
    for &choice in choices {
        assert!(
            session.choose(choice),
            "choice {choice} advances to the next boundary"
        );
    }
    session
}

fn stage<'a>(scene: &'a PlayScene, id: &str) -> &'a weave_editor::StagedCharacter {
    scene
        .stages
        .iter()
        .find(|stage| stage.id == id)
        .unwrap_or_else(|| panic!("stage {id} is visible"))
}

#[test]
fn a_multi_character_scene_enters_leaves_and_switches_variants() {
    let mut session = drive(compile(STAGING_SOURCE), &[]);
    let opening = scene_from_session(&session);
    assert_eq!(opening.background.as_deref(), Some("harbour"));
    assert_eq!(opening.fit, Fit::Cover);
    assert_eq!(opening.anchor, AnchorIr { x: 0.5, y: 0.35 });
    assert_eq!(opening.speaker, "Mara");
    assert_eq!(
        opening
            .ordered_stages()
            .iter()
            .map(|stage| stage.id.as_str())
            .collect::<Vec<_>>(),
        ["mara", "keeper"],
        "paint order follows the declared layers"
    );
    assert_eq!(stage(&opening, "mara").slot, Slot::Left);
    assert_eq!(stage(&opening, "keeper").slot, Slot::Right);

    assert!(
        session.choose(0),
        "the first branch reaches a nested choice"
    );
    let middle = scene_from_session(&session);
    assert_eq!(middle.speaker, "The Keeper");
    assert_eq!(
        stage(&middle, "mara").image.as_deref(),
        Some("art/mara_surprised.png")
    );
    assert_eq!(stage(&middle, "mara").variant.as_deref(), Some("surprised"));
    assert_eq!(stage(&middle, "keeper").slot, Slot::Center);
    assert_eq!(
        middle.transition,
        Some(PlayTransition::new(TransitionKind::Fade, 240)),
        "the transition waits for the next staged operation"
    );

    assert!(session.choose(0), "the branch reaches the ending");
    let ending = scene_from_session(&session);
    assert_eq!(ending.background.as_deref(), Some("cabin"));
    assert_eq!(ending.fit, Fit::Contain);
    assert_eq!(
        ending.background_transition,
        Some(PlayTransition::new(TransitionKind::Wipe, 300))
    );
    assert_eq!(ending.speaker, NARRATOR);
    assert_eq!(ending.stages.len(), 1, "Mara has left the scene");
    let keeper = stage(&ending, "keeper");
    assert_eq!(keeper.image.as_deref(), Some("art/keeper_angry.png"));
    assert_eq!(keeper.variant.as_deref(), Some("angry"));
    assert!(ending.notes.is_empty());
    assert!(ending.ended);
}

#[test]
fn leaving_a_branch_clears_both_stages() {
    let session = drive(compile(STAGING_SOURCE), &[1]);
    let scene = scene_from_session(&session);
    assert!(scene.stages.is_empty());
    assert_eq!(scene.speaker, NARRATOR);
    assert_eq!(
        scene.background.as_deref(),
        Some("harbour"),
        "a branch that changes no scene keeps the staged background"
    );
    assert!(scene.ended);
}

#[test]
fn replaying_the_same_seed_and_choices_emits_identical_cues() {
    let first = drive(compile(STAGING_SOURCE), &[0, 0]);
    let second = drive(compile(STAGING_SOURCE), &[0, 0]);

    assert_eq!(first.presentation(), second.presentation());
    assert_eq!(
        scene_from_session(&first),
        scene_from_session(&second),
        "the same cues fold to the same frame"
    );

    let other = drive(compile(STAGING_SOURCE), &[1]);
    assert_ne!(
        first.presentation(),
        other.presentation(),
        "a different choice produces a different cue sequence"
    );
}

#[test]
fn a_restart_discards_the_previous_staging_and_replays_the_same_cues() {
    let mut session = drive(compile(STAGING_SOURCE), &[0, 0]);
    let first = session.presentation().to_vec();

    assert!(session.restart());
    assert!(session.run());
    assert!(session.choose(0));
    assert!(session.choose(0));
    assert_eq!(session.presentation(), first);

    let scene = scene_from_session(&session);
    assert_eq!(scene.background.as_deref(), Some("cabin"));
    assert!(scene.stages.iter().all(|stage| stage.id != "mara"));
}

#[test]
fn a_compiled_story_round_trips_and_replays_the_same_cues() {
    let story = compile(STAGING_SOURCE);

    let encoded = ron::to_string(&story).expect("the story encodes as RON");
    let from_ron: StoryIr = ron::from_str(&encoded).expect("the RON story decodes");
    assert_eq!(from_ron, story);

    let encoded = serde_json::to_string(&story).expect("the story encodes as JSON");
    let from_json: StoryIr = serde_json::from_str(&encoded).expect("the JSON story decodes");
    assert_eq!(from_json, story);

    let original = drive(story, &[0, 0]);
    let ron_session = drive(from_ron, &[0, 0]);
    let json_session = drive(from_json, &[0, 0]);
    assert_eq!(original.presentation(), ron_session.presentation());
    assert_eq!(original.presentation(), json_session.presentation());
    assert_eq!(
        scene_from_session(&original),
        scene_from_session(&json_session)
    );
}

#[test]
fn a_transition_duration_is_presentation_only_and_bounded() {
    let lookup = LayoutLookup::default();
    let without_transition_cues = vec![
        PresentationEvent::Show("one".to_owned()),
        PresentationEvent::Move {
            stage: "one".to_owned(),
            to: Slot::FarRight,
        },
    ];
    let mut cues = without_transition_cues.clone();
    cues.insert(
        1,
        PresentationEvent::Transition {
            kind: TransitionKind::Slide,
            ms: 5_000,
        },
    );

    let state = fold_cues(&cues, &lookup);
    assert_eq!(
        state.stages[0].transition,
        Some(PlayTransition::new(
            TransitionKind::Slide,
            MAX_TRANSITION_MS
        )),
        "the duration is clamped to the contract bound"
    );
    assert_eq!(state.stages[0].slot, Slot::FarRight);

    let without_transition = fold_cues(&without_transition_cues, &lookup);
    let staging = |state: &weave_editor::PresentationState| {
        state
            .stages
            .iter()
            .map(|stage| {
                (
                    stage.id.clone(),
                    stage.slot,
                    stage.layer,
                    stage.image.clone(),
                    stage.variant.clone(),
                    stage.order,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        staging(&without_transition),
        staging(&state),
        "a transition never changes the final staging"
    );
    assert_eq!(
        PlayTransition::new(TransitionKind::Cut, 10).opacity(),
        1.0,
        "a renderer without a clock still reaches the final frame"
    );
}

#[test]
fn layout_slots_and_layers_order_every_figure() {
    let mut lookup = LayoutLookup::default();
    for (id, slot, layer) in [
        ("front", Slot::Left, 3_u8),
        ("back", Slot::Left, 1_u8),
        ("middle", Slot::Left, 1_u8),
        ("far", Slot::Right, 0_u8),
    ] {
        lookup
            .stages
            .insert(id.to_owned(), LayoutStage { slot, layer });
    }
    let cues = vec![
        PresentationEvent::Show("front".to_owned()),
        PresentationEvent::Show("back".to_owned()),
        PresentationEvent::Show("middle".to_owned()),
        PresentationEvent::Show("far".to_owned()),
    ];
    let state = fold_cues(&cues, &lookup);

    let left = stages_in_slot(&state.stages, Slot::Left)
        .iter()
        .map(|stage| stage.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(left, ["back", "middle", "front"]);

    let paint_order = ordered_stages(&state.stages)
        .iter()
        .map(|stage| stage.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paint_order, ["far", "back", "middle", "front"]);

    let fractions = SLOT_ORDER
        .iter()
        .copied()
        .map(slot_fraction)
        .collect::<Vec<_>>();
    assert!(
        fractions.windows(2).all(|pair| pair[0] < pair[1]),
        "slots run left to right"
    );
}

#[test]
fn an_anchor_from_another_producer_stays_on_the_image() {
    let mut lookup = LayoutLookup::default();
    lookup
        .anchors
        .insert("tilted".to_owned(), AnchorIr { x: 3.0, y: -4.0 });
    let state = fold_cues(&[PresentationEvent::Scene("tilted".to_owned())], &lookup);
    assert_eq!(state.anchor, AnchorIr { x: 1.0, y: 0.0 });

    lookup.anchors.insert(
        "broken".to_owned(),
        AnchorIr {
            x: f64::NAN,
            y: f64::INFINITY,
        },
    );
    let state = fold_cues(&[PresentationEvent::Scene("broken".to_owned())], &lookup);
    assert_eq!(state.anchor, AnchorIr { x: 0.5, y: 0.5 });
}

fn write_png(path: &Path, colour: [u8; 4], width: u32, height: u32) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("asset directory");
    }
    RgbaImage::from_pixel(width, height, Rgba(colour))
        .save_with_format(path, ImageFormat::Png)
        .expect("png fixture");
}

#[test]
fn staged_assets_cache_invalidate_and_recover() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().to_path_buf();
    let background = root.join("art/harbour.png");
    write_png(&background, [20, 40, 60, 255], 2, 1);

    let session = drive(compile(STAGING_SOURCE), &[]);
    let scene = scene_from_session(&session);
    assert_eq!(scene.background.as_deref(), Some("harbour"));
    let declared = session
        .scene_background("harbour")
        .expect("the scene declares a background");

    let mut assets = PresentationAssets::new(root.clone());
    let first = assets
        .background("harbour", declared)
        .expect("the background decodes");
    let cached = assets
        .background("harbour", declared)
        .expect("the cached background returns");
    assert!(Arc::ptr_eq(&first, &cached), "the frame is decoded once");
    assert_eq!(first.size(0).width.0, 2);
    assert!(!assets.refresh());

    write_png(&background, [60, 40, 20, 255], 3, 2);
    assert!(assets.refresh(), "a rewritten file invalidates the entry");
    let replaced = assets
        .background("harbour", declared)
        .expect("the replacement decodes");
    assert!(!Arc::ptr_eq(&first, &replaced));
    assert_eq!(replaced.size(0).width.0, 3);
    assert!(assets.failure(declared).is_none());

    let moved = root.join("art/harbour-moved.png");
    fs::rename(&background, &moved).expect("move the asset");
    assert!(assets.refresh(), "a moved file invalidates the entry");
    assert!(assets.background("harbour", declared).is_none());
    assert!(
        assets
            .failure(declared)
            .is_some_and(|note| note.contains("not found")),
        "an unavailable asset names the declaration and the reason"
    );

    fs::rename(&moved, &background).expect("restore the asset");
    assert!(assets.refresh(), "a restored file invalidates the failure");
    assert!(assets.background("harbour", declared).is_some());
}

struct LayoutStage {
    slot: Slot,
    layer: u8,
}

#[derive(Default)]
struct LayoutLookup {
    anchors: BTreeMap<String, AnchorIr>,
    stages: BTreeMap<String, LayoutStage>,
}

impl PresentationLookup for LayoutLookup {
    fn scene_fit(&self, _id: &str) -> Option<Fit> {
        None
    }

    fn scene_anchor(&self, id: &str) -> Option<AnchorIr> {
        self.anchors.get(id).copied()
    }

    fn stage_name(&self, _id: &str) -> Option<String> {
        None
    }

    fn stage_slot(&self, id: &str) -> Option<Slot> {
        self.stages.get(id).map(|stage| stage.slot)
    }

    fn stage_layer(&self, id: &str) -> Option<u8> {
        self.stages.get(id).map(|stage| stage.layer)
    }

    fn stage_image(&self, _id: &str) -> Option<&str> {
        None
    }

    fn stage_variant_image(&self, _stage: &str, _variant: &str) -> Option<&str> {
        None
    }
}
