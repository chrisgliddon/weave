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
