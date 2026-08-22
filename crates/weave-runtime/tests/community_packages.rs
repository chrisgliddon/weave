use std::collections::BTreeMap;
use std::path::Path;

use weave_compiler::{CompileOptions, compile_with_patterns};
use weave_patterns::load_package;
use weave_runtime::{Story, StoryEvent};

const STORY: &str = include_str!("../../../patterns/community/ember-omens/example.weave");

#[test]
fn installed_package_data_compiles_and_executes_through_the_standard_runtime() {
    let package_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../patterns/community/ember-omens/package.weave-pattern.json");
    let package = load_package(package_path).expect("load sample package");
    let patterns = BTreeMap::from([(
        package.metadata.id.clone(),
        package.pattern_ir().expect("lower package"),
    )]);
    let compiled = compile_with_patterns(STORY, &CompileOptions::default(), &patterns)
        .expect("compile package-backed story");
    let mut story = Story::with_seed(compiled.story, 19).expect("start story");

    let draws = story.take_pattern_draws();
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].entries.len(), 3);
    assert_eq!(
        draws[0]
            .entries
            .iter()
            .map(|entry| entry.position.as_deref())
            .collect::<Vec<_>>(),
        [Some("kindling"), Some("flame"), Some("embers")]
    );

    let mut lines = Vec::new();
    loop {
        match story.advance().expect("advance story") {
            StoryEvent::Line(line) => lines.push(line),
            StoryEvent::Ended => break,
            StoryEvent::Choices(_) => panic!("sample has no choices"),
        }
    }
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|line| !line.contains("{reading")));
}
