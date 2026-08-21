use weave_web::{Player, PlayerFrame};
#[cfg(not(target_arch = "wasm32"))]
use weave_web::{PlayerError, WEB_STATE_VERSION};

const STORY_JSON: &str = include_str!("../../../examples/web-player/story.json");

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
    let PlayerFrame::Line { text, draws } = first.clone() else {
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
