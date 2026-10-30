use proptest::prelude::*;
use weave_core::ast::{Cue, Statement};
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
fn parses_domain_module_activation_and_typed_paths_as_shared_source_syntax() {
    let source = include_str!("../../../examples/domain-modules/contract/tracer.weave");
    let document = parse_document(source).expect("tracer source parses");
    assert_eq!(document.modules().count(), 1);
    assert_eq!(
        document.modules().next().expect("module").node.alias,
        "constellation"
    );
    let result = type_check(&document);
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error),
        "generic source tooling should accept module paths before artifact resolution: {result:#?}"
    );
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

#[test]
fn parses_scene_stage_declarations_and_presentation_cues() {
    let source = r#"scene ferry_dusk {
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
    let document = parse_document(source).expect("presentation source parses");

    let scenes = document.scenes().collect::<Vec<_>>();
    assert_eq!(scenes.len(), 1);
    assert_eq!(scenes[0].node.name, "ferry_dusk");
    assert_eq!(
        scenes[0].node.background.as_deref(),
        Some("art/ferry_dusk.png")
    );
    assert!(scenes[0].span.start < scenes[0].span.end);

    let stages = document.stages().collect::<Vec<_>>();
    assert_eq!(stages.len(), 1);
    assert_eq!(stages[0].node.name, "mara");
    assert_eq!(
        stages[0].node.image.as_deref(),
        Some("art/mara_neutral.png")
    );
    assert_eq!(stages[0].node.display_name.as_deref(), Some("Mara"));
    assert!(stages[0].span.start < stages[0].span.end);

    let knot = document.knots().next().expect("knot");
    let cues = knot
        .node
        .body
        .iter()
        .filter_map(|statement| match &statement.node {
            Statement::Cue(cue) => Some((cue.clone(), statement.span)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cues.iter().map(|(cue, _)| cue.clone()).collect::<Vec<_>>(),
        vec![
            Cue::Scene("ferry_dusk".to_owned()),
            Cue::Show("mara".to_owned()),
            Cue::Speaker(Some("mara".to_owned())),
            Cue::Hide("mara".to_owned()),
            Cue::Speaker(None),
        ]
    );
    assert!(cues.iter().all(|(_, span)| span.start < span.end));
    assert!(
        cues.windows(2)
            .all(|pair| pair[0].1.start < pair[1].1.start)
    );
}

#[test]
fn rejects_unknown_presentation_directives() {
    let diagnostics =
        parse_document("=== start ===\n@shout mara\n").expect_err("unknown directive must fail");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.0 == "W1040")
    );
}

proptest! {
    #[test]
    fn arbitrary_utf8_input_never_panics(source in any::<String>()) {
        let _ = parse_document(&source);
    }
}
