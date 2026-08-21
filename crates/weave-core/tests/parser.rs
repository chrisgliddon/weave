use proptest::prelude::*;
use weave_core::{Severity, parse_document, type_check};

const README_STORY: &str = include_str!("fixtures/fortune_teller.weave");

#[test]
fn parses_and_checks_the_complete_readme_story() {
    let document = parse_document(README_STORY)
        .unwrap_or_else(|diagnostics| panic!("README story did not parse: {diagnostics:#?}"));
    let result = type_check(&document);
    let errors = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "README story did not check: {errors:#?}");
    assert_eq!(document.knots().count(), 4);
}

#[test]
fn accepts_unicode_comments_crlf_and_tabs() {
    let source = "// привет\r\ngrammar names {\r\n\tfirst: [\"Mira\", \"李\"]\r\n}\r\n\r\n=== start ===\r\nHello, #names.first# — café.\r\n-> END\r\n";
    let document = parse_document(source);
    assert!(document.is_ok(), "{document:#?}");
}

#[test]
fn malformed_input_has_a_source_location() {
    let diagnostics = parse_document("=== start ===\n{true:\n    missing close\n")
        .expect_err("unterminated conditional must fail");
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.span.is_some())
    );
}

proptest! {
    #[test]
    fn arbitrary_utf8_input_never_panics(source in any::<String>()) {
        let _ = parse_document(&source);
    }
}
