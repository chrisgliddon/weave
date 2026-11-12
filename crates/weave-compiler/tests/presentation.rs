use std::collections::BTreeMap;

use weave_compiler::{CompileOptions, compile, to_json, to_ron};
use weave_core::ir::{
    AnchorIr, CueIr, Fit, InstructionKind, SceneIr, Slot, StageIr, StoryIr, TransitionKind,
};

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

const STAGING_SOURCE: &str = r#"scene ferry_dusk {
    background: "art/ferry_dusk.png"
    fit: contain
    anchor: 0.25, 0.75
}

stage mara {
    image: "art/mara_neutral.png"
    name: "Mara"
    slot: left
    layer: 1
    variant surprised {
        image: "art/mara_surprised.png"
    }
}

stage keeper {
    image: "art/keeper_neutral.png"
    name: "The Keeper"
    slot: right
    layer: 2
}

=== start ===
@scene ferry_dusk
@show mara
@show keeper
@speaker mara
@move mara center
@swap mara surprised
@transition fade 240
The ferry leaves at dusk.
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
fn lowers_scene_fit_anchor_slots_layers_variants_and_staged_cues() {
    let story = compile_source(STAGING_SOURCE);

    assert_eq!(
        story.presentation.scenes["ferry_dusk"],
        SceneIr {
            background: Some("art/ferry_dusk.png".to_owned()),
            fit: Fit::Contain,
            anchor: AnchorIr { x: 0.25, y: 0.75 },
        }
    );
    assert_eq!(
        story.presentation.stages["mara"],
        StageIr {
            image: Some("art/mara_neutral.png".to_owned()),
            name: Some("Mara".to_owned()),
            slot: Slot::Left,
            layer: 1,
            variants: BTreeMap::from([(
                "surprised".to_owned(),
                "art/mara_surprised.png".to_owned()
            )]),
        }
    );
    assert_eq!(story.presentation.stages["keeper"].slot, Slot::Right);
    assert_eq!(story.presentation.stages["keeper"].layer, 2);

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
            CueIr::Show("keeper".to_owned()),
            CueIr::Speaker(Some("mara".to_owned())),
            CueIr::Move {
                stage: "mara".to_owned(),
                to: Slot::Center,
            },
            CueIr::Swap {
                stage: "mara".to_owned(),
                variant: "surprised".to_owned(),
            },
            CueIr::Transition {
                kind: TransitionKind::Fade,
                ms: 240,
            },
        ]
    );
}

#[test]
fn staged_presentation_rejections_have_stable_codes_and_spans() {
    for (label, source, code, line, column) in [
        (
            "unknown slot",
            "stage mara {\n    slot: middle\n}\n=== start ===\n-> END\n",
            "W2205",
            2,
            11,
        ),
        (
            "unknown fit",
            "scene ferry {\n    fit: stretch\n}\n=== start ===\n-> END\n",
            "W2206",
            2,
            10,
        ),
        (
            "bad anchor",
            "scene ferry {\n    anchor: 1.5, 0.5\n}\n=== start ===\n-> END\n",
            "W2207",
            2,
            13,
        ),
        (
            "negative anchor",
            "scene ferry {\n    anchor: -0.5, 0.5\n}\n=== start ===\n-> END\n",
            "W2207",
            2,
            13,
        ),
        (
            "layer 10",
            "stage mara {\n    layer: 10\n}\n=== start ===\n-> END\n",
            "W2208",
            2,
            12,
        ),
        (
            "negative layer",
            "stage mara {\n    layer: -1\n}\n=== start ===\n-> END\n",
            "W2208",
            2,
            12,
        ),
        (
            "transition too long",
            "=== start ===\n@transition fade 2001\nText.\n-> END\n",
            "W2210",
            2,
            18,
        ),
        (
            "unknown transition kind",
            "=== start ===\n@transition blur 200\nText.\n-> END\n",
            "W2209",
            2,
            13,
        ),
        (
            "missing variant",
            "stage mara {\n    image: \"art/mara.png\"\n}\n=== start ===\n@swap mara surprised\nText.\n-> END\n",
            "W2211",
            5,
            12,
        ),
        (
            "unknown move target",
            "=== start ===\n@move ghost left\nText.\n-> END\n",
            "W2213",
            2,
            7,
        ),
        (
            "unsafe variant image",
            "stage mara {\n    variant surprised {\n        image: \"art/con.png\"\n    }\n}\n=== start ===\n-> END\n",
            "W2204",
            3,
            16,
        ),
    ] {
        let error = match compile(source, &CompileOptions::default()) {
            Ok(_) => panic!("{label} compiled unexpectedly"),
            Err(error) => error,
        };
        let diagnostic = error
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.0 == code)
            .unwrap_or_else(|| panic!("{label} did not produce {code}: {error:?}"));
        let span = diagnostic
            .span
            .unwrap_or_else(|| panic!("{label} diagnostic has no span"));
        assert_eq!(span.line, line, "{label} line");
        assert_eq!(span.column, column, "{label} column");
    }
}

#[test]
fn transition_durations_separate_out_of_range_from_malformed_tokens() {
    for (token, code) in [
        ("-1", "W2210"),
        ("2001", "W2210"),
        ("65536", "W2210"),
        ("999999999999999999999999999999999999999999", "W2210"),
        ("+240", "W1046"),
        ("twenty", "W1046"),
        ("1.5", "W1046"),
    ] {
        let source = format!("=== start ===\n@transition fade {token}\nText.\n-> END\n");
        let error = compile(&source, &CompileOptions::default())
            .expect_err("an invalid duration must not compile");
        let diagnostic = error
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.0 == code)
            .unwrap_or_else(|| panic!("`{token}` did not produce {code}: {error:?}"));
        let span = diagnostic
            .span
            .unwrap_or_else(|| panic!("`{token}` diagnostic has no span"));
        assert_eq!(span.line, 2, "`{token}` line");
        assert_eq!(span.column, 18, "`{token}` column");
        if code == "W2210" {
            assert!(
                !error
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code.0 == "W1046"),
                "`{token}` must not be reported as malformed: {error:?}"
            );
        }
    }

    for token in ["0", "2000"] {
        let source = format!("=== start ===\n@transition fade {token}\nText.\n-> END\n");
        assert!(
            compile(&source, &CompileOptions::default()).is_ok(),
            "`{token}` is inside the contract bound"
        );
    }
}

#[test]
fn negative_zero_anchors_lower_and_serialize_as_positive_zero() {
    let source = "scene ferry {\n    anchor: -0.0, 0.5\n}\n=== start ===\n-> END\n";
    let story = compile_source(source);
    let anchor = story.presentation.scenes["ferry"].anchor;
    assert_eq!(anchor.x, 0.0);
    assert!(
        anchor.x.is_sign_positive(),
        "negative zero survived lowering"
    );

    let json = to_json(&story).expect("serialize JSON");
    let ron = to_ron(&story).expect("serialize RON");
    assert!(!json.contains("-0.0"), "{json}");
    assert!(!ron.contains("-0.0"), "{ron}");
}

#[test]
fn duplicate_stage_variants_are_rejected_with_a_span() {
    let source = r#"stage mara {
    variant surprised {
        image: "art/mara_surprised.png"
    }
    variant surprised {
        image: "art/mara_alt.png"
    }
}
=== start ===
-> END
"#;
    let error = compile(source, &CompileOptions::default()).expect_err("duplicate variant fails");
    let diagnostic = error
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.0 == "W2212")
        .expect("duplicate variant diagnostic");
    assert_eq!(diagnostic.span.expect("span").line, 5);
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

#[test]
fn staged_scene_round_trips_through_ron_and_json() {
    let story = compile_source(STAGING_SOURCE);
    let ron = to_ron(&story).expect("serialize RON");
    let json = to_json(&story).expect("serialize JSON");

    let from_ron: StoryIr = ron::from_str(&ron).expect("decode RON");
    let from_json: StoryIr = serde_json::from_str(&json).expect("decode JSON");
    assert_eq!(from_ron, from_json);
    assert_eq!(from_ron, story);
    assert_eq!(from_ron.version, weave_core::ir::IR_VERSION);
    assert_eq!(from_ron.presentation.stages.len(), 2);
    assert_eq!(
        from_ron.presentation.stages["mara"].variants["surprised"],
        "art/mara_surprised.png"
    );
}
