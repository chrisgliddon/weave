use std::error::Error;

use weave_compiler::{CompileOptions, compile};
use weave_runtime::{Story, StoryEvent};

const STORY_SOURCE: &str = include_str!("../../stories/patterns.weave");

fn main() -> Result<(), Box<dyn Error>> {
    let compiled = compile(
        STORY_SOURCE,
        &CompileOptions {
            source_name: Some("examples/stories/patterns.weave".to_owned()),
        },
    )?;
    let mut story = Story::with_seed(compiled.story, 7)?;

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
}
