use weave_web::{Player, PlayerFrame};
#[cfg(not(target_arch = "wasm32"))]
use weave_web::{PlayerError, WEB_STATE_VERSION};

const STORY_JSON: &str = include_str!("../../../examples/web-player/story.json");
const PRESENTATION_JSON: &str = include_str!("fixtures/presentation.json");
const ERROR_PATH_JSON: &str = include_str!("fixtures/error-path.json");

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn loads_json_and_exposes_the_complete_player_flow() {
    exercise_player_flow();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn wasm_loads_json_and_exposes_the_complete_player_flow() {
    exercise_player_flow();
}

fn exercise_player_flow() {
    let mut player = Player::from_json(STORY_JSON, 17).expect("load compiled JSON");
    let first = player.advance().expect("first line");
    let PlayerFrame::Line { text, draws, .. } = first.clone() else {
        panic!("expected opening line");
    };
    assert!(text.contains("observatory"));
    assert_eq!(draws.len(), 1);

    let choices = player.advance().expect("choice boundary");
    let PlayerFrame::Choices { choices, .. } = choices else {
        panic!("expected choices");
    };
    assert_eq!(choices.len(), 2);
    assert_eq!(player.choices(), choices);

    player.choose(0).expect("choose first branch");
    assert!(matches!(
        player.advance().expect("branch line"),
        PlayerFrame::Line { .. }
    ));
    assert!(matches!(
        player.advance().expect("ending"),
        PlayerFrame::Ended { .. }
    ));

    player.restart().expect("restart");
    assert_eq!(player.advance().expect("replayed opening"), first);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn save_state_round_trips_and_preserves_the_original_seed() {
    let mut original = Player::from_json(STORY_JSON, 29).expect("load story");
    let _ = original.advance().expect("opening");
    let _ = original.advance().expect("choices");
    original.choose(1).expect("choose second branch");
    let saved = original.serialize_state().expect("serialize state");
    let expected = original.advance().expect("advance original");

    let mut restored = Player::from_json(STORY_JSON, 999).expect("load replacement");
    restored.restore_state(&saved).expect("restore state");
    assert_eq!(restored.seed(), 29);
    assert_eq!(restored.advance().expect("advance restored"), expected);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn malformed_and_incompatible_inputs_fail_safely() {
    assert!(matches!(
        Player::from_json("{}", 0),
        Err(PlayerError::StoryJson(_))
    ));

    let player = Player::from_json(STORY_JSON, 3).expect("load story");
    let mut saved: serde_json::Value = serde_json::from_str(
        &player
            .serialize_state()
            .expect("serialize compatible state"),
    )
    .expect("state is JSON");
    saved["version"] = serde_json::Value::from(WEB_STATE_VERSION + 1);
    let mut replacement = Player::from_json(STORY_JSON, 3).expect("load replacement");
    assert!(matches!(
        replacement.restore_state(&saved.to_string()),
        Err(PlayerError::StateVersion { .. })
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn presentation_cues_are_drained_into_the_frame_and_leave_nothing_pending() {
    use weave_runtime::PresentationEvent;

    let mut player = Player::from_json(PRESENTATION_JSON, 23).expect("load presentation story");
    assert!(player.pending_presentation().is_empty());

    let frame = player.advance().expect("first line");
    let PlayerFrame::Line {
        text, presentation, ..
    } = frame
    else {
        panic!("expected a line");
    };
    assert_eq!(text, "The ferry leaves at dusk.");
    assert_eq!(
        presentation,
        vec![
            PresentationEvent::Scene("ferry_dusk".to_owned()),
            PresentationEvent::Show("mara".to_owned()),
            PresentationEvent::Speaker(Some("mara".to_owned())),
            PresentationEvent::Move {
                stage: "mara".to_owned(),
                to: weave_core::ir::Slot::Center,
            },
            PresentationEvent::Swap {
                stage: "mara".to_owned(),
                variant: "surprised".to_owned(),
            },
            PresentationEvent::Transition {
                kind: weave_core::ir::TransitionKind::Wipe,
                ms: 320,
            },
        ]
    );
    assert!(
        player.pending_presentation().is_empty(),
        "the host must drain every cue it received"
    );

    let mut repeat = Player::from_json(PRESENTATION_JSON, 23).expect("load presentation story");
    assert_eq!(
        repeat.advance().expect("first line"),
        PlayerFrame::Line {
            text,
            draws: Vec::new(),
            presentation: vec![
                PresentationEvent::Scene("ferry_dusk".to_owned()),
                PresentationEvent::Show("mara".to_owned()),
                PresentationEvent::Speaker(Some("mara".to_owned())),
                PresentationEvent::Move {
                    stage: "mara".to_owned(),
                    to: weave_core::ir::Slot::Center,
                },
                PresentationEvent::Swap {
                    stage: "mara".to_owned(),
                    variant: "surprised".to_owned(),
                },
                PresentationEvent::Transition {
                    kind: weave_core::ir::TransitionKind::Wipe,
                    ms: 320,
                },
            ],
        }
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn serialized_frames_and_failures_expose_cues_in_the_binding_shape() {
    let mut player = Player::from_json(PRESENTATION_JSON, 23).expect("load presentation story");
    let frame = serde_json::to_value(player.advance().expect("first line")).expect("frame JSON");
    assert_eq!(frame["kind"], "line");
    assert_eq!(
        frame["presentation"],
        serde_json::json!([
            {"kind": "scene", "value": "ferry_dusk"},
            {"kind": "show", "value": "mara"},
            {"kind": "speaker", "value": "mara"},
            {"kind": "move", "value": {"stage": "mara", "to": "center"}},
            {"kind": "swap", "value": {"stage": "mara", "variant": "surprised"}},
            {"kind": "transition", "value": {"kind": "wipe", "ms": 320}},
        ]),
        "the JavaScript binding receives ordered cue objects, not opaque values"
    );

    let mut failing = Player::from_json(ERROR_PATH_JSON, 5).expect("load failing story");
    let failure = failing.advance().expect_err("division by zero must fail");
    assert_eq!(
        serde_json::to_value(&failure.presentation).expect("cue JSON"),
        serde_json::json!([
            {"kind": "scene", "value": "ferry_dusk"},
            {"kind": "speaker", "value": null},
        ]),
        "a bare speaker cue reaches the host as a null value"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_failed_advance_reports_the_cues_that_ran_before_it() {
    use weave_runtime::PresentationEvent;

    let mut player = Player::from_json(ERROR_PATH_JSON, 5).expect("load failing story");
    assert!(player.pending_presentation().is_empty());

    let failure = player.advance().expect_err("division by zero must fail");
    assert!(matches!(failure.error, PlayerError::Runtime(_)));
    assert_eq!(
        failure.presentation,
        vec![
            PresentationEvent::Scene("ferry_dusk".to_owned()),
            PresentationEvent::Speaker(None),
        ],
        "the host receives every cue produced before the failure, in order"
    );
    assert!(
        player.pending_presentation().is_empty(),
        "a failure must not strand cues for a later frame"
    );

    let next = player
        .advance()
        .expect("execution resumes after the failure");
    assert_eq!(
        next,
        PlayerFrame::Line {
            text: "The ferry leaves at dusk.".to_owned(),
            draws: Vec::new(),
            presentation: Vec::new(),
        },
        "the delivered cues are not replayed into a later frame"
    );
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn wasm_frames_carry_ordered_cues_and_failures_keep_earlier_cues() {
    use wasm_bindgen::{JsCast, JsValue};

    fn cues(value: &JsValue, property: &str) -> Vec<(String, Option<String>)> {
        let array = js_sys::Reflect::get(value, &JsValue::from_str(property))
            .expect("cue property")
            .dyn_into::<js_sys::Array>()
            .expect("cue array");
        array
            .iter()
            .map(|cue| {
                let kind = js_sys::Reflect::get(&cue, &JsValue::from_str("kind"))
                    .expect("cue kind")
                    .as_string()
                    .expect("kind string");
                let value =
                    js_sys::Reflect::get(&cue, &JsValue::from_str("value")).expect("cue value");
                let value = if value.is_null() || value.is_undefined() {
                    None
                } else if let Some(text) = value.as_string() {
                    Some(text)
                } else {
                    Some(
                        js_sys::JSON::stringify(&value)
                            .ok()
                            .and_then(|text| text.as_string())
                            .expect("cue value JSON"),
                    )
                };
                (kind, value)
            })
            .collect()
    }

    let mut player = weave_web::WasmStoryPlayer::new(PRESENTATION_JSON, 23).expect("load story");
    let frame = player.continue_story().expect("first line");
    assert_eq!(
        cues(&frame, "presentation"),
        vec![
            ("scene".to_owned(), Some("ferry_dusk".to_owned())),
            ("show".to_owned(), Some("mara".to_owned())),
            ("speaker".to_owned(), Some("mara".to_owned())),
            (
                "move".to_owned(),
                Some("{\"stage\":\"mara\",\"to\":\"center\"}".to_owned())
            ),
            (
                "swap".to_owned(),
                Some("{\"stage\":\"mara\",\"variant\":\"surprised\"}".to_owned())
            ),
            (
                "transition".to_owned(),
                Some("{\"kind\":\"wipe\",\"ms\":320}".to_owned())
            ),
        ]
    );

    let mut failing =
        weave_web::WasmStoryPlayer::new(ERROR_PATH_JSON, 5).expect("load failing story");
    let thrown = failing
        .continue_story()
        .expect_err("division by zero must throw");
    let error = thrown
        .clone()
        .dyn_into::<js_sys::Error>()
        .expect("a JavaScript Error is thrown");
    assert!(
        String::from(error.message()).contains("division"),
        "the thrown message keeps the runtime failure"
    );
    assert_eq!(
        cues(&thrown, "presentation"),
        vec![
            ("scene".to_owned(), Some("ferry_dusk".to_owned())),
            ("speaker".to_owned(), None),
        ],
        "cues that ran before the failure are attached to the thrown error"
    );
}
