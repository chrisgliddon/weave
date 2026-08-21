use weave_compiler::{CompileOptions, compile, to_json, to_ron};
use weave_core::ir::StoryIr;
use weave_runtime::{Story, StoryEvent};

const PATTERN_SOURCE: &str = include_str!("../../../examples/stories/patterns.weave");

#[test]
fn ron_and_json_stories_produce_identical_pattern_results() {
    let compiled = compile(PATTERN_SOURCE, &CompileOptions::default())
        .expect("pattern example should compile");
    let ron_ir: StoryIr =
        ron::from_str(&to_ron(&compiled.story).expect("serialize RON")).expect("decode RON");
    let json_ir: StoryIr = serde_json::from_str(&to_json(&compiled.story).expect("serialize JSON"))
        .expect("decode JSON");
    let mut ron_story = Story::with_seed(ron_ir, 73).expect("start RON story");
    let mut json_story = Story::with_seed(json_ir, 73).expect("start JSON story");

    assert_eq!(ron_story.state(), json_story.state());
    for _ in 0..16 {
        let ron_event = ron_story.advance().expect("advance RON story");
        let json_event = json_story.advance().expect("advance JSON story");
        assert_eq!(ron_event, json_event);
        assert_eq!(ron_story.state(), json_story.state());
        if ron_event == StoryEvent::Ended {
            return;
        }
    }
    panic!("pattern example did not end within the expected event budget");
}
