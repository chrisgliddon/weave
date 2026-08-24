use std::error::Error;

use weave_compiler::{CompileOptions, compile};
use weave_core::ir::StoryIr;
use weave_runtime::{Story, StoryEvent};

const STORY_SOURCE: &str = include_str!("../../stories/patterns.weave");
const TABLETOP_STORY: &str =
    include_str!("../../tabletop-adapters/plug-and-play/runtime/ember-vale.story.ron");

fn main() -> Result<(), Box<dyn Error>> {
    let compiled = compile(
        STORY_SOURCE,
        &CompileOptions {
            source_name: Some("examples/stories/patterns.weave".to_owned()),
        },
    )?;
    run_story(Story::with_seed(compiled.story, 7)?)?;
    println!("--- Plug-And-Play portable story ---");
    run_story(Story::new(ron::from_str::<StoryIr>(TABLETOP_STORY)?)?)?;
    Ok(())
}

fn run_story(mut story: Story) -> Result<(), Box<dyn Error>> {
    loop {
        match story.advance()? {
            StoryEvent::Line(line) => println!("{line}"),
            StoryEvent::Choices(choices) => {
                for (index, choice) in choices.iter().enumerate() {
                    println!("  {index}: {}", choice.text);
                }
                story.choose(0)?;
            }
            StoryEvent::Ended => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_story_compiles_and_starts() {
        let compiled = compile(STORY_SOURCE, &CompileOptions::default()).expect("example compiles");
        let mut story = Story::new(compiled.story).expect("example starts");
        assert!(matches!(story.advance(), Ok(StoryEvent::Line(_))));
    }

    #[test]
    fn generated_tabletop_story_loads_and_starts() {
        let compiled = ron::from_str::<StoryIr>(TABLETOP_STORY).expect("checked tabletop RON");
        let mut story = Story::new(compiled).expect("tabletop example starts");
        assert!(matches!(story.advance(), Ok(StoryEvent::Line(_))));
    }
}
