use weave_compiler::{CompileOptions, compile};
use weave_core::ir::{Slot, TransitionKind};
use weave_runtime::{PresentationEvent, Story, StoryEvent};

const STAGING_SOURCE: &str = r#"scene ferry_dusk {
    background: "art/ferry_dusk.png"
    fit: cover
    anchor: 0.5, 0.5
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
}

=== start ===
@scene ferry_dusk
@show mara
@show keeper
@speaker mara
@move mara center
@swap mara surprised
@transition fade 240
The ferry leaves at dusk.
-> END
"#;

const SOURCE: &str = r#"scene ferry_dusk {
    background: "art/ferry_dusk.png"
}

stage mara {
    image: "art/mara_neutral.png"
    name: "Mara"
}

=== start ===
@scene ferry_dusk
Before the crossing.
@show mara
@speaker mara
Mara: The ferry leaves at dusk.
@hide mara
@speaker
After she leaves.
-> END
"#;

trait LineText {
    fn line_text(&self) -> Option<&str>;
}

impl LineText for StoryEvent {
    fn line_text(&self) -> Option<&str> {
        match self {
            StoryEvent::Line(line) => Some(&line.text),
            _ => None,
        }
    }
}

fn compile_story(source: &str) -> Story {
    let compiled = compile(source, &CompileOptions::default()).expect("story should compile");
    Story::new(compiled.story).expect("story should start")
}

#[test]
fn cues_accumulate_across_lines_and_drain_in_source_order() {
    let mut story = compile_story(SOURCE);
    assert!(story.take_presentation().is_empty());

    assert_eq!(
        story.advance().expect("first line").line_text(),
        Some("Before the crossing.")
    );
    assert_eq!(
        story.take_presentation(),
        vec![PresentationEvent::Scene("ferry_dusk".to_owned())]
    );
    assert!(story.take_presentation().is_empty());

    assert_eq!(
        story.advance().expect("second line").line_text(),
        Some("Mara: The ferry leaves at dusk.")
    );
    assert_eq!(
        story.take_presentation(),
        vec![
            PresentationEvent::Show("mara".to_owned()),
            PresentationEvent::Speaker(Some("mara".to_owned())),
        ]
    );

    assert_eq!(
        story.advance().expect("third line").line_text(),
        Some("After she leaves.")
    );
    assert_eq!(
        story.take_presentation(),
        vec![
            PresentationEvent::Hide("mara".to_owned()),
            PresentationEvent::Speaker(None),
        ]
    );

    assert_eq!(story.advance().expect("story ends"), StoryEvent::Ended);
    assert!(story.take_presentation().is_empty());
}

#[test]
fn cue_instructions_continue_instead_of_ending_the_story() {
    let source = r#"stage ghostly {
}

=== start ===
@speaker
@hide ghostly
Still running.
-> END
"#;
    let mut story = compile_story(source);
    assert_eq!(
        story
            .advance()
            .expect("line after presentation cues")
            .line_text(),
        Some("Still running.")
    );
    assert_eq!(
        story.take_presentation(),
        vec![
            PresentationEvent::Speaker(None),
            PresentationEvent::Hide("ghostly".to_owned()),
        ]
    );
}

#[test]
fn staged_cues_emit_in_source_order_and_never_end_the_story() {
    let mut story = compile_story(STAGING_SOURCE);
    assert_eq!(
        story.advance().expect("line after staged cues").line_text(),
        Some("The ferry leaves at dusk.")
    );
    assert_eq!(
        story.take_presentation(),
        vec![
            PresentationEvent::Scene("ferry_dusk".to_owned()),
            PresentationEvent::Show("mara".to_owned()),
            PresentationEvent::Show("keeper".to_owned()),
            PresentationEvent::Speaker(Some("mara".to_owned())),
            PresentationEvent::Move {
                stage: "mara".to_owned(),
                to: Slot::Center,
            },
            PresentationEvent::Swap {
                stage: "mara".to_owned(),
                variant: "surprised".to_owned(),
            },
            PresentationEvent::Transition {
                kind: TransitionKind::Fade,
                ms: 240,
            },
        ]
    );
    assert!(story.take_presentation().is_empty(), "cues drain once");
    assert_eq!(story.advance().expect("story ends"), StoryEvent::Ended);
}

#[test]
fn staged_cue_order_survives_restore_and_repeated_runs() {
    fn run(story: &mut Story) -> Vec<(StoryEvent, Vec<PresentationEvent>)> {
        let mut observed = Vec::new();
        loop {
            let event = story.advance().expect("advance");
            let cues = story.take_presentation();
            let ended = event == StoryEvent::Ended;
            observed.push((event, cues));
            if ended {
                return observed;
            }
        }
    }

    let compiled = compile(STAGING_SOURCE, &CompileOptions::default()).expect("staging compiles");
    let first = run(&mut Story::with_seed(compiled.story.clone(), 41).expect("start first"));
    let second = run(&mut Story::with_seed(compiled.story.clone(), 41).expect("start second"));
    assert_eq!(first, second);

    let pristine = Story::with_seed(compiled.story.clone(), 41).expect("start pristine");
    let saved = pristine.state().clone();
    let mut restored = Story::restore(compiled.story, saved).expect("restore saved runtime state");
    assert_eq!(run(&mut restored), first);
}

#[test]
fn cue_stream_is_deterministic_for_the_same_source_and_seed() {
    fn run() -> Vec<(StoryEvent, Vec<PresentationEvent>)> {
        let mut story = compile_story(SOURCE);
        let mut observed = Vec::new();
        loop {
            let event = story.advance().expect("advance");
            let cues = story.take_presentation();
            let ended = event == StoryEvent::Ended;
            observed.push((event, cues));
            if ended {
                return observed;
            }
        }
    }

    let first = run();
    let second = run();
    assert_eq!(first, second);
    assert!(first.iter().any(|(_, cues)| !cues.is_empty()));
}
