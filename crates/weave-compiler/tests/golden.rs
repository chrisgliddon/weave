use weave_compiler::{CompileOptions, compile_to_ron};

const SOURCE: &str = include_str!("fixtures/golden.weave");
const EXPECTED: &str = include_str!("fixtures/golden.ron");

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
