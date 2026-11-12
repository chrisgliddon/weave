//! Round trip through the exported C functions, exactly as a C or C++ host calls them.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char};
use std::ptr;

use serde_json::Value;
use weave_ffi::*;

const STORY_JSON: &str = include_str!("../../../examples/web-player/story.json");

fn take(text: *mut c_char) -> Value {
    assert!(!text.is_null());
    // SAFETY: returned by the library; released right after copying.
    let value = serde_json::from_str(unsafe { CStr::from_ptr(text) }.to_str().unwrap()).unwrap();
    unsafe { weave_string_free(text) };
    value
}

#[test]
fn plays_saves_restores_and_ends_through_the_c_abi() {
    assert_eq!(weave_ffi_abi_version(), WEAVE_FFI_ABI_VERSION);
    let source = CString::new(STORY_JSON).unwrap();
    let mut handle = ptr::null_mut();
    let created = take(unsafe { weave_player_new(source.as_ptr(), 7, &mut handle) });
    assert_eq!(created["ok"], true, "{created}");
    assert!(!handle.is_null());

    let mut saved = None;
    let mut lines = 0;
    for _ in 0..500 {
        let step = take(unsafe { weave_player_advance(handle) });
        assert_eq!(step["ok"], true, "{step}");
        match step["frame"]["kind"].as_str().unwrap() {
            "line" => lines += 1,
            "choices" => {
                let listed = take(unsafe { weave_player_choices(handle) });
                assert!(!listed["choices"].as_array().unwrap().is_empty());
                if saved.is_none() {
                    let s = take(unsafe { weave_player_save(handle) });
                    saved = Some(CString::new(s["state"].as_str().unwrap()).unwrap());
                }
                assert_eq!(take(unsafe { weave_player_choose(handle, 0) })["ok"], true);
            }
            "ended" => break,
            other => panic!("unexpected frame {other}"),
        }
    }
    assert!(lines > 0);
    let state = saved.expect("story offers at least one choice");
    assert_eq!(
        take(unsafe { weave_player_restore(handle, state.as_ptr()) })["ok"],
        true
    );
    assert!(
        !take(unsafe { weave_player_choices(handle) })["choices"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    unsafe { weave_player_free(handle) };
}

#[test]
fn reports_errors_instead_of_crashing() {
    let bad = CString::new("{not json").unwrap();
    let mut handle = ptr::null_mut();
    let created = take(unsafe { weave_player_new(bad.as_ptr(), 1, &mut handle) });
    assert_eq!(created["ok"], false);
    assert!(handle.is_null());
    assert_eq!(
        take(unsafe { weave_player_advance(ptr::null_mut()) })["ok"],
        false
    );
    assert_eq!(
        take(unsafe { weave_player_new(ptr::null(), 1, &mut handle) })["ok"],
        false
    );
    assert!(!unsafe { weave_player_can_continue(ptr::null_mut()) });
    unsafe { weave_player_free(ptr::null_mut()) };
    unsafe { weave_string_free(ptr::null_mut()) };
}

const WRITER_JSON: &str = include_str!("../../../examples/writer-metadata/lighthouse.story.json");

fn writer_player() -> *mut WeavePlayer {
    let source = CString::new(WRITER_JSON).unwrap();
    let mut handle = ptr::null_mut();
    let created = take(unsafe { weave_player_new(source.as_ptr(), 0, &mut handle) });
    assert_eq!(created["ok"], true, "{created}");
    handle
}

#[test]
fn abi_two_carries_structured_lines_strings_and_channels() {
    assert_eq!(WEAVE_FFI_ABI_VERSION, 2);
    let handle = writer_player();

    let first = take(unsafe { weave_player_advance(handle) });
    let frame = &first["frame"];
    assert_eq!(frame["kind"], "line");
    assert_eq!(frame["id"], "shore.1");
    assert_eq!(frame["channel"], "narration");
    assert_eq!(frame["tags"], serde_json::json!(["ambient"]));
    assert_eq!(frame["meta"]["sfx"], "wind");

    let sign = take(unsafe { weave_player_advance(handle) });
    let frame = &sign["frame"];
    assert_eq!(frame["id"], "shore.sign");
    assert_eq!(frame["channel"], "sign");
    assert_eq!(frame["text"], "GULL POINT LIGHT Keeper on duty");
    assert_eq!(frame["rich"], "GULL POINT LIGHT <b>Keeper on duty</>");
    assert_eq!(frame["beats"][0]["at"], 17);
    assert_eq!(frame["spans"][0]["style"], "b");

    let keeper = take(unsafe { weave_player_advance(handle) });
    assert_eq!(keeper["frame"]["speaker"], "keeper");
    assert_eq!(keeper["frame"]["channel"], "dialog");
    take(unsafe { weave_player_advance(handle) });
    let choices = take(unsafe { weave_player_advance(handle) });
    assert_eq!(choices["frame"]["kind"], "choices");
    assert_eq!(
        choices["frame"]["choices"][0]["tags"],
        serde_json::json!(["curious"])
    );

    let table = CString::new("ui").unwrap();
    let key = CString::new("menu.quit").unwrap();
    let quit = take(unsafe { weave_player_string(handle, table.as_ptr(), key.as_ptr()) });
    assert_eq!(quit["ok"], true, "{quit}");
    assert_eq!(quit["string"]["text"], "Leave the shore");
    assert_eq!(quit["string"]["meta"]["confirm"], "true");
    let missing = CString::new("menu.nope").unwrap();
    let unknown = take(unsafe { weave_player_string(handle, table.as_ptr(), missing.as_ptr()) });
    assert_eq!(unknown["ok"], false);
    assert_eq!(
        take(unsafe { weave_player_string(handle, ptr::null(), key.as_ptr()) })["ok"],
        false
    );

    let channels = take(unsafe { weave_player_channels(handle) });
    assert_eq!(channels["ok"], true);
    let channels = &channels["channels"];
    assert_eq!(channels["narration"]["animated"], true);
    assert_eq!(channels["dialog"]["cps"], 40.0);
    assert_eq!(channels["dialog"]["meta"]["box"], "bottom");
    assert_eq!(channels["sign"]["animated"], false);
    assert_eq!(channels["sign"]["meta"]["frame"], "brass");
    assert_eq!(
        take(unsafe { weave_player_channels(ptr::null_mut()) })["ok"],
        false
    );
    unsafe { weave_player_free(handle) };
}

#[test]
fn channel_and_speaker_survive_save_and_restore() {
    let handle = writer_player();
    for _ in 0..5 {
        take(unsafe { weave_player_advance(handle) });
    }
    let saved = take(unsafe { weave_player_save(handle) });
    let state = CString::new(saved["state"].as_str().unwrap()).unwrap();
    assert_eq!(take(unsafe { weave_player_restart(handle, 0) })["ok"], true);
    assert_eq!(
        take(unsafe { weave_player_restore(handle, state.as_ptr()) })["ok"],
        true
    );
    assert_eq!(take(unsafe { weave_player_choose(handle, 0) })["ok"], true);
    // The choice body sets the speaker but not the channel, so `dialog` comes from the save.
    let line = take(unsafe { weave_player_advance(handle) });
    assert_eq!(line["frame"]["channel"], "dialog", "{line}");
    assert_eq!(line["frame"]["speaker"], "visitor");
    unsafe { weave_player_free(handle) };
}
