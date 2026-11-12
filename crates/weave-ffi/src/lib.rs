//! C ABI over [`weave_web::Player`], for hosts such as Unreal Engine.
//!
//! Every function that can fail returns a NUL-terminated UTF-8 JSON envelope owned by
//! the caller, who must release it with [`weave_string_free`]:
//!
//! - success: `{"ok": true, ...}` with call-specific members;
//! - failure: `{"ok": false, "error": "message"}`, plus `"presentation"` for a failed
//!   advance (cues produced before the failure, which the host applies first).
//!
//! Panics never cross the boundary; they become failure envelopes. Players are not
//! thread-safe: use each from one thread at a time.

use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::{Value, json};
use weave_web::Player;

/// Version of this C ABI. Hosts check it once after loading the library.
pub const WEAVE_FFI_ABI_VERSION: u32 = 1;

/// Opaque player handle.
pub struct WeavePlayer(Player);

fn into_c(value: &Value) -> *mut c_char {
    let text = value.to_string().replace('\0', "\u{FFFD}");
    CString::new(text).map_or(std::ptr::null_mut(), CString::into_raw)
}

fn fail(message: impl ToString) -> Value {
    json!({ "ok": false, "error": message.to_string() })
}

fn guarded(body: impl FnOnce() -> Value) -> *mut c_char {
    let value = catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<&str>()
            .map(ToString::to_string)
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".to_owned());
        fail(format!("weave-ffi panic: {message}"))
    });
    into_c(&value)
}

#[allow(unsafe_code)]
fn read_str<'a>(ptr: *const c_char, what: &str) -> Result<&'a str, Value> {
    if ptr.is_null() {
        return Err(fail(format!("{what} is null")));
    }
    // SAFETY: the caller guarantees a valid NUL-terminated string that outlives the call.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| fail(format!("{what} is not UTF-8")))
}

#[allow(unsafe_code)]
fn player<'a>(handle: *mut WeavePlayer) -> Result<&'a mut Player, Value> {
    if handle.is_null() {
        return Err(fail("player handle is null"));
    }
    // SAFETY: the caller passes a live handle from `weave_player_new`, used by one thread.
    Ok(unsafe { &mut (*handle).0 })
}

macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(error) => return error,
        }
    };
}

/// Return [`WEAVE_FFI_ABI_VERSION`].
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn weave_ffi_abi_version() -> u32 {
    WEAVE_FFI_ABI_VERSION
}

/// Create a player from compiled story JSON (`weavec --format json`) and a seed.
///
/// On success writes the handle to `out_player` and returns `{"ok":true}`; release the
/// handle with [`weave_player_free`].
///
/// # Safety
/// `story_json` is a valid NUL-terminated string; `out_player` is a valid writable pointer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn weave_player_new(
    story_json: *const c_char,
    seed: u64,
    out_player: *mut *mut WeavePlayer,
) -> *mut c_char {
    guarded(|| {
        if out_player.is_null() {
            return fail("out_player is null");
        }
        let source = tri!(read_str(story_json, "story_json"));
        match Player::from_json(source, seed) {
            Ok(created) => {
                // SAFETY: checked non-null above; the caller owns the slot.
                unsafe { *out_player = Box::into_raw(Box::new(WeavePlayer(created))) };
                json!({ "ok": true })
            }
            Err(error) => fail(error),
        }
    })
}

/// Release a player. Null is ignored.
///
/// # Safety
/// `handle` came from [`weave_player_new`] and is not used afterwards.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn weave_player_free(handle: *mut WeavePlayer) {
    if !handle.is_null() {
        // SAFETY: ownership returns to Rust exactly once, per the contract above.
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Release a string returned by this library. Null is ignored.
///
/// # Safety
/// `text` came from this library and is not used afterwards.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn weave_string_free(text: *mut c_char) {
    if !text.is_null() {
        // SAFETY: the string was produced by `CString::into_raw` in this library.
        drop(unsafe { CString::from_raw(text) });
    }
}

/// Advance to the next line, choice set or ending: `{"ok":true,"frame":{"kind":"line"|"choices"|"ended",...}}`.
///
/// # Safety
/// `handle` is a live player.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn weave_player_advance(handle: *mut WeavePlayer) -> *mut c_char {
    guarded(|| {
        let p = tri!(player(handle));
        match p.advance() {
            Ok(frame) => json!({ "ok": true, "frame": frame }),
            Err(failure) => json!({
                "ok": false,
                "error": failure.error.to_string(),
                "presentation": failure.presentation,
            }),
        }
    })
}

/// Currently eligible choices: `{"ok":true,"choices":[{"id","text","once"}...]}`.
///
/// # Safety
/// `handle` is a live player.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn weave_player_choices(handle: *mut WeavePlayer) -> *mut c_char {
    guarded(|| {
        let p = tri!(player(handle));
        json!({ "ok": true, "choices": p.choices() })
    })
}

/// Select an eligible choice by zero-based index.
///
/// # Safety
/// `handle` is a live player.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn weave_player_choose(
    handle: *mut WeavePlayer,
    index: usize,
) -> *mut c_char {
    guarded(|| {
        let p = tri!(player(handle));
        match p.choose(index) {
            Ok(()) => json!({ "ok": true }),
            Err(error) => fail(error),
        }
    })
}

/// Whether the story can advance without a choice. Null handles return false.
///
/// # Safety
/// `handle` is null or a live player.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn weave_player_can_continue(handle: *mut WeavePlayer) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        player(handle).is_ok_and(|p| p.can_continue())
    }))
    .unwrap_or(false)
}

/// Restart from the entry knot with a new seed.
///
/// # Safety
/// `handle` is a live player.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn weave_player_restart(handle: *mut WeavePlayer, seed: u64) -> *mut c_char {
    guarded(|| {
        let p = tri!(player(handle));
        match p.restart_with_seed(seed) {
            Ok(()) => json!({ "ok": true }),
            Err(error) => fail(error),
        }
    })
}

/// Save state: `{"ok":true,"state":"<versioned JSON envelope as a string>"}`.
///
/// # Safety
/// `handle` is a live player.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn weave_player_save(handle: *mut WeavePlayer) -> *mut c_char {
    guarded(|| {
        let p = tri!(player(handle));
        match p.serialize_state() {
            Ok(state) => json!({ "ok": true, "state": state }),
            Err(error) => fail(error),
        }
    })
}

/// Restore state produced by [`weave_player_save`] against the same story.
///
/// # Safety
/// `handle` is a live player; `state_json` is a valid NUL-terminated string.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn weave_player_restore(
    handle: *mut WeavePlayer,
    state_json: *const c_char,
) -> *mut c_char {
    guarded(|| {
        let p = tri!(player(handle));
        let state = tri!(read_str(state_json, "state_json"));
        match p.restore_state(state) {
            Ok(()) => json!({ "ok": true }),
            Err(error) => fail(error),
        }
    })
}
