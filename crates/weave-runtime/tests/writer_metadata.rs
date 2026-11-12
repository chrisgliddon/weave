//! Structured lines: ids, channels, speakers, markup, and string tables at runtime.

use weave_compiler::{CompileOptions, compile};
use weave_runtime::{Line, Story, StoryState};

fn start(source: &str) -> Story {
    let compiled = compile(source, &CompileOptions::default()).expect("story should compile");
    Story::new(compiled.story).expect("story should start")
}

fn next(story: &mut Story) -> Line {
    story
        .continue_line()
        .expect("line should render")
        .expect("story should have another line")
}

const SOURCE: &str = r#"channel radio { animated: false, cps: 25, tint: "green" }
stage ana {
    name: "Ana"
}
VAR who = "<b>Mallory</b> & ^id:x"
STRINGS ui
    ok = "<em>OK</em> & go" ^button

=== start ===
Plain narration. ^calm
@to radio
@speaker ana
Static. <pause 0.5> Then {who}. ^id:start.static
* [Answer] ^bold
    Over.
    -> later

=== later ===
Still on the radio.
@to
@speaker
Back to narration.
-> END
"#;

#[test]
fn lines_carry_ids_tags_channel_and_speaker() {
    let mut story = start(SOURCE);
    let first = next(&mut story);
    assert_eq!(first.id, "start.1");
    assert_eq!(first.channel, "narration");
    assert_eq!(first.speaker, None);
    assert_eq!(first.tags, ["calm"]);
    assert_eq!(first.text, "Plain narration.");

    let second = next(&mut story);
    assert_eq!(second.id, "start.static");
    assert_eq!(second.channel, "radio");
    assert_eq!(second.speaker.as_deref(), Some("ana"));
    assert_eq!(second.beats.len(), 1);
    assert_eq!(second.beats[0].pause, 0.5);
}

#[test]
fn interpolated_values_are_never_markup_or_tags() {
    let mut story = start(SOURCE);
    next(&mut story);
    let line = next(&mut story);
    assert_eq!(line.text, "Static. Then <b>Mallory</b> & ^id:x.");
    assert!(line.spans.is_empty(), "{line:?}");
    assert_eq!(
        line.rich,
        "Static. Then &lt;b&gt;Mallory&lt;/b&gt; &amp; ^id:x."
    );
    assert_eq!(line.id, "start.static");
    assert!(line.meta.is_empty());
}

#[test]
fn channel_and_speaker_persist_across_knots_and_saves() {
    let mut story = start(SOURCE);
    next(&mut story);
    next(&mut story);
    assert!(story.continue_line().expect("reaches the choice").is_none());
    let choices = story.choices().to_vec();
    assert_eq!(choices[0].tags, ["bold"]);
    let saved = ron::to_string(story.state()).expect("state serializes");
    story.choose(0).expect("choice is valid");
    assert_eq!(next(&mut story).channel, "radio");
    let carried = next(&mut story);
    assert_eq!(carried.id, "later.1");
    assert_eq!(carried.channel, "radio");
    assert_eq!(carried.speaker.as_deref(), Some("ana"));
    let back = next(&mut story);
    assert_eq!(back.channel, "narration");
    assert_eq!(back.speaker, None);

    let compiled = compile(SOURCE, &CompileOptions::default()).unwrap();
    let state: StoryState = ron::from_str(&saved).expect("state deserializes");
    assert_eq!(state.channel(), "radio");
    assert_eq!(state.speaker(), Some("ana"));
    let mut restored = Story::restore(compiled.story, state).expect("state restores");
    restored.choose(0).expect("restored choice is valid");
    let line = next(&mut restored);
    assert_eq!(
        (line.channel.as_str(), line.speaker.as_deref()),
        ("radio", Some("ana"))
    );
}

#[test]
fn rejects_a_saved_channel_the_story_does_not_declare() {
    let compiled = compile(SOURCE, &CompileOptions::default()).unwrap();
    let story = start(SOURCE);
    let saved = ron::to_string(story.state()).unwrap();
    let tampered = saved.replace("channel:\"narration\"", "channel:\"ghost\"");
    assert_ne!(
        saved, tampered,
        "fixture should contain the channel field: {saved}"
    );
    let state: StoryState = ron::from_str(&tampered).unwrap();
    assert!(Story::restore(compiled.story, state).is_err());
}

#[test]
fn exposes_strings_and_channels() {
    let story = start(SOURCE);
    let ok = story.string("ui", "ok").expect("string exists");
    assert_eq!(ok.text, "OK & go");
    assert_eq!(ok.rich, "<em>OK</> &amp; go");
    assert_eq!(ok.tags, ["button"]);
    assert!(story.string("ui", "missing").is_none());
    assert!(story.string("other", "ok").is_none());
    let channels = story.channels();
    assert!(channels.contains_key("narration"));
    assert!(!channels["radio"].animated);
    assert_eq!(channels["radio"].cps, Some(25.0));
}

#[test]
fn rejects_a_saved_speaker_that_is_not_a_stage() {
    let compiled = compile(SOURCE, &CompileOptions::default()).unwrap();
    let mut story = start(SOURCE);
    next(&mut story);
    next(&mut story);
    let saved = ron::to_string(story.state()).unwrap();
    let tampered = saved.replace("speaker:Some(\"ana\")", "speaker:Some(\"<b>nobody</b>\")");
    assert_ne!(
        saved, tampered,
        "fixture should contain the speaker: {saved}"
    );
    let state: StoryState = ron::from_str(&tampered).unwrap();
    assert!(Story::restore(compiled.story, state).is_err());
}

#[test]
fn trims_whitespace_left_by_trailing_markup() {
    let mut story = start("=== start ===\nDone. <pause 1>\n<pause 0.5> Begun.\n-> END\n");
    let first = next(&mut story);
    assert_eq!(first.text, "Done.");
    assert_eq!(first.beats[0].at, 5);
    let second = next(&mut story);
    assert_eq!(second.text, "Begun.");
    assert_eq!(second.beats[0].at, 0);
}

#[test]
fn rejects_hand_edited_ir_with_invalid_writer_metadata() {
    let compiled = compile(SOURCE, &CompileOptions::default()).unwrap();
    let json = serde_json::to_value(&compiled.story).unwrap();
    let load = |value: serde_json::Value| {
        let story: weave_core::ir::StoryIr = serde_json::from_value(value).unwrap();
        Story::new(story)
    };
    assert!(load(json.clone()).is_ok());

    let mut no_narration = json.clone();
    no_narration["channels"]
        .as_object_mut()
        .unwrap()
        .remove("narration");
    assert!(load(no_narration).is_err());

    let mut bad_cps = json.clone();
    bad_cps["channels"]["radio"]["cps"] = serde_json::json!(-1.0);
    assert!(load(bad_cps).is_err());

    let text = serde_json::to_string(&json).unwrap();
    let bad_pause = text.replacen("\"value\":0.5", "\"value\":99.0", 1);
    assert_ne!(text, bad_pause, "fixture should contain a pause");
    assert!(load(serde_json::from_str(&bad_pause).unwrap()).is_err());

    let bad_id = text.replacen("\"start.static\"", "\"9bad\"", 1);
    assert_ne!(text, bad_id);
    assert!(load(serde_json::from_str(&bad_id).unwrap()).is_err());

    let styled = compile(
        "=== start ===\nA <b>bold</b> move.\n-> END\n",
        &CompileOptions::default(),
    )
    .unwrap();
    let styled = serde_json::to_string(&styled.story).unwrap();
    let bad_style = styled.replacen("\"value\":\"b\"", "\"value\":\"b><img src=x>\"", 1);
    assert_ne!(
        styled, bad_style,
        "fixture should contain a style: {styled}"
    );
    assert!(load(serde_json::from_str(&bad_style).unwrap()).is_err());
}

#[test]
fn string_values_keep_their_quoted_edge_spaces() {
    let story = start(
        "STRINGS ui\n    score = \"Score: \"\n    escaped = \"\\<b> & co\"\n=== start ===\nHi.\n-> END\n",
    );
    assert_eq!(story.string("ui", "score").unwrap().text, "Score: ");
    let escaped = story.string("ui", "escaped").unwrap();
    assert_eq!(escaped.text, "<b> & co");
    assert_eq!(escaped.rich, "&lt;b&gt; &amp; co");
}
