use weave_compiler::{CompileOptions, compile, compile_to_ron, json_schema, to_json, to_ron};
use weave_core::ir::{InstructionKind, StoryIr};

const SOURCE: &str = include_str!("fixtures/golden.weave");
const EXPECTED: &str = include_str!("fixtures/golden.ron");
const EXPECTED_JSON: &str = include_str!("fixtures/golden.json");
const EXPECTED_SCHEMA: &str = include_str!("../../../schemas/weave-story-ir-v4.schema.json");
const PATTERN_SOURCE: &str = include_str!("../../../examples/stories/patterns.weave");

#[test]
fn source_to_ron_matches_the_reviewed_golden_file() {
    let (actual, diagnostics) = compile_to_ron(
        SOURCE,
        &CompileOptions {
            source_name: Some("crates/weave-compiler/tests/fixtures/golden.weave".to_owned()),
        },
    )
    .expect("golden source should compile");
    assert!(diagnostics.is_empty());
    assert_eq!(actual, EXPECTED);
}

#[test]
fn source_to_json_matches_the_reviewed_golden_file() {
    let compiled = compile(
        SOURCE,
        &CompileOptions {
            source_name: Some("crates/weave-compiler/tests/fixtures/golden.weave".to_owned()),
        },
    )
    .expect("golden source should compile");
    assert!(compiled.diagnostics.is_empty());
    assert_eq!(
        to_json(&compiled.story).expect("serialize JSON"),
        EXPECTED_JSON
    );
}

#[test]
fn ron_and_json_decode_to_equivalent_pattern_models() {
    let compiled = compile(PATTERN_SOURCE, &CompileOptions::default())
        .expect("pattern example should compile");
    let ron = to_ron(&compiled.story).expect("serialize RON");
    let json = to_json(&compiled.story).expect("serialize JSON");
    let from_ron: StoryIr = ron::from_str(&ron).expect("decode RON");
    let from_json: StoryIr = serde_json::from_str(&json).expect("decode JSON");

    assert_eq!(from_ron, from_json);
    assert_eq!(from_ron.patterns.len(), 4);
}

#[test]
fn choice_identifiers_ignore_source_names_comments_and_blank_lines() {
    let compact = "=== start ===\nHello.\n* [Continue] -> END\n";
    let with_trivia = "=== start ===\n\n// Greeting shown first.\nHello.\n\n// Navigation.\n* [Continue] -> END\n";
    let first = compile(
        compact,
        &CompileOptions {
            source_name: Some("first.weave".to_owned()),
        },
    )
    .expect("compact source compiles");
    let second = compile(
        with_trivia,
        &CompileOptions {
            source_name: Some("nested/second.weave".to_owned()),
        },
    )
    .expect("source with trivia compiles");

    assert_eq!(first_choice_id(&first.story), "start:root.1");
    assert_eq!(
        first_choice_id(&first.story),
        first_choice_id(&second.story)
    );
}

#[test]
fn generated_schema_matches_the_published_schema() {
    let actual = json_schema().expect("generate schema");
    assert_eq!(actual, EXPECTED_SCHEMA);

    let schema: serde_json::Value = serde_json::from_str(&actual).expect("schema is JSON");
    assert_eq!(schema["$id"], "urn:weave:schema:story-ir:4");
    assert_eq!(schema["properties"]["version"]["const"], 4);
}

fn first_choice_id(story: &StoryIr) -> &str {
    story.knots["start"]
        .content
        .iter()
        .find_map(|instruction| match &instruction.kind {
            InstructionKind::Choice(choice) => Some(choice.id.as_str()),
            _ => None,
        })
        .expect("story contains a choice")
}
