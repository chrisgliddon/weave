use weave_compiler::{CompileOptions, compile};
use weave_runtime::{PresentationEvent, Story, StoryEvent};

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

fn compile_story(source: &str) -> Story {
    let compiled = compile(source, &CompileOptions::default()).expect("story should compile");
    Story::new(compiled.story).expect("story should start")
}

#[test]
fn cues_accumulate_across_lines_and_drain_in_source_order() {
    let mut story = compile_story(SOURCE);
    assert!(story.take_presentation().is_empty());

    assert_eq!(
        story.advance().expect("first line"),
        StoryEvent::Line("Before the crossing.".to_owned())
    );
    assert_eq!(
        story.take_presentation(),
        vec![PresentationEvent::Scene("ferry_dusk".to_owned())]
    );
    assert!(story.take_presentation().is_empty());

    assert_eq!(
        story.advance().expect("second line"),
        StoryEvent::Line("Mara: The ferry leaves at dusk.".to_owned())
    );
    assert_eq!(
        story.take_presentation(),
        vec![
            PresentationEvent::Show("mara".to_owned()),
            PresentationEvent::Speaker(Some("mara".to_owned())),
        ]
    );

    assert_eq!(
        story.advance().expect("third line"),
        StoryEvent::Line("After she leaves.".to_owned())
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
        story.advance().expect("line after presentation cues"),
        StoryEvent::Line("Still running.".to_owned())
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
