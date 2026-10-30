use weave_compiler::{CompileOptions, compile, to_json, to_ron};
use weave_core::ir::{CueIr, InstructionKind, StoryIr};

const PRESENTATION_SOURCE: &str = r#"scene ferry_dusk {
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

fn compile_source(source: &str) -> StoryIr {
    compile(source, &CompileOptions::default())
        .expect("source should compile")
        .story
}

#[test]
fn lowers_scene_stage_declarations_and_ordered_cues() {
    let story = compile_source(PRESENTATION_SOURCE);

    assert_eq!(
        story.presentation.scenes["ferry_dusk"]
            .background
            .as_deref(),
        Some("art/ferry_dusk.png")
    );
    assert_eq!(
        story.presentation.stages["mara"].image.as_deref(),
        Some("art/mara_neutral.png")
    );
    assert_eq!(
        story.presentation.stages["mara"].name.as_deref(),
        Some("Mara")
    );

    let cues = story.knots["start"]
        .content
        .iter()
        .filter_map(|instruction| match &instruction.kind {
            InstructionKind::Cue(cue) => Some(cue.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cues,
        vec![
            CueIr::Scene("ferry_dusk".to_owned()),
            CueIr::Show("mara".to_owned()),
            CueIr::Speaker(Some("mara".to_owned())),
            CueIr::Hide("mara".to_owned()),
            CueIr::Speaker(None),
        ]
    );
}

#[test]
fn unknown_scene_and_stage_cues_report_source_spans() {
    let source = "=== start ===\n@scene missing_scene\n@show missing_stage\nText.\n-> END\n";
    let error = compile(source, &CompileOptions::default()).expect_err("cues must resolve");

    let scene = error
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.0 == "W2202")
        .expect("unknown scene diagnostic");
    let scene_span = scene.span.expect("scene diagnostic has a span");
    assert_eq!(scene_span.line, 2);

    let stage = error
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.0 == "W2203")
        .expect("unknown stage diagnostic");
    let stage_span = stage.span.expect("stage diagnostic has a span");
    assert_eq!(stage_span.line, 3);
}

#[test]
fn rejects_non_portable_asset_paths_with_specific_reasons() {
    for (path, expected) in [
        ("", "empty"),
        ("~/.ssh/id_rsa", "`~`"),
        ("art/%2e%2e/secret.png", "percent-encoded"),
        ("art\\mara.png", "`\\`"),
        ("art/\u{1}mara.png", "control characters"),
        ("art/\t/mara.png", "control characters"),
        ("art/mara./image.png", "end with `.`"),
        ("art/mara /image.png", "end with a space"),
        ("art/mara.png ", "end with a space"),
        ("art/con.png", "reserved device name"),
        ("art/COM1", "reserved device name"),
        ("art/aux.tar.gz", "reserved device name"),
        ("art/PRN", "reserved device name"),
        ("art/lpt9.txt", "reserved device name"),
        ("art/ｎｕｌ.png", "ASCII"),
        ("art/\u{202E}mara.png", "ASCII"),
        ("art//mara.png", "empty segment"),
        ("art/./mara.png", "`.` segment"),
        ("art/../mara.png", "`..` segment"),
        ("/etc/passwd", "cannot start with `/`"),
        ("https://example.com/x.png", "`:` scheme"),
        ("art/mara/", "end with `/`"),
        ("art/mara?.png", "only ASCII letters"),
        ("art/mara .png", "only ASCII letters"),
    ] {
        let literal = path.replace('\\', "\\\\");
        let source = format!(
            "scene ferry_dusk {{\n    background: \"{literal}\"\n}}\n=== start ===\n-> END\n"
        );
        let error = compile(&source, &CompileOptions::default())
            .expect_err("non-portable asset path must fail");
        let diagnostic = error
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.0 == "W2204")
            .unwrap_or_else(|| panic!("missing asset-path diagnostic for {path}"));
        assert!(
            diagnostic.message.contains(expected),
            "diagnostic for {path} did not name {expected}: {}",
            diagnostic.message
        );
        let span = diagnostic.span.expect("asset diagnostic has a span");
        assert_eq!(
            span.line, 2,
            "diagnostic should point at the literal for {path}"
        );
        assert_eq!(
            span.column, 17,
            "diagnostic should point at the literal for {path}"
        );
    }
}

#[test]
fn asset_path_diagnostics_escape_control_characters() {
    let source = "scene ferry_dusk {\n    background: \"art/\u{1b}[31mred\u{0}.png\"\n}\n=== start ===\n-> END\n";
    let error = compile(source, &CompileOptions::default()).expect_err("control characters fail");
    let diagnostic = error
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.0 == "W2204")
        .expect("asset-path diagnostic");
    assert!(
        !diagnostic
            .message
            .chars()
            .any(|character| character.is_control()),
        "the rejected path must not reach the terminal verbatim: {:?}",
        diagnostic.message
    );
    assert!(
        diagnostic.message.contains("\\u{1b}[31mred\\0.png"),
        "the path should be debug-quoted: {}",
        diagnostic.message
    );
}

#[test]
fn stage_image_diagnostics_point_at_the_image_literal() {
    let source = "stage mara {\n    image: \"art/con.png\"\n}\n=== start ===\n-> END\n";
    let error = compile(source, &CompileOptions::default()).expect_err("reserved name must fail");
    let diagnostic = error
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.0 == "W2204")
        .expect("asset-path diagnostic");
    let span = diagnostic.span.expect("asset diagnostic has a span");
    assert_eq!(span.line, 2);
    assert_eq!(span.column, 12);
}

#[test]
fn presentation_ir_round_trips_through_ron_and_json() {
    let story = compile_source(PRESENTATION_SOURCE);
    let ron = to_ron(&story).expect("serialize RON");
    let json = to_json(&story).expect("serialize JSON");

    let from_ron: StoryIr = ron::from_str(&ron).expect("decode RON");
    let from_json: StoryIr = serde_json::from_str(&json).expect("decode JSON");
    assert_eq!(from_ron, from_json);
    assert_eq!(from_ron, story);
}
