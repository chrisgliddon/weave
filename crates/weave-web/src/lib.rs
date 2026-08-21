//! WebAssembly host boundary for deterministic Weave story playback.

use serde::{Deserialize, Serialize};
use weave_core::ir::StoryIr;
use weave_patterns::DrawResult;
use weave_runtime::{ChoiceView, RuntimeError, Story, StoryEvent, StoryState};

/// Current browser save-envelope version.
pub const WEB_STATE_VERSION: u32 = 1;

/// One host-visible playback boundary plus pattern draws produced while reaching it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PlayerFrame {
    /// One rendered narrative line.
    Line {
        /// Rendered text.
        text: String,
        /// Structured pattern draws produced before this boundary.
        draws: Vec<DrawResult>,
    },
    /// The story is waiting for one host selection.
    Choices {
        /// Currently eligible choices.
        choices: Vec<ChoiceView>,
        /// Structured pattern draws produced before this boundary.
        draws: Vec<DrawResult>,
    },
    /// The story has ended.
    Ended {
        /// Structured pattern draws produced before this boundary.
        draws: Vec<DrawResult>,
    },
}

/// Browser-player loading, execution, or save-state failure.
#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    /// Compiled story JSON could not be decoded.
    #[error("invalid compiled story JSON: {0}")]
    StoryJson(String),
    /// Saved browser state JSON could not be decoded.
    #[error("invalid saved story state JSON: {0}")]
    StateJson(String),
    /// The save envelope uses an unsupported version.
    #[error("unsupported web state version {found}; expected {expected}")]
    StateVersion {
        /// Version found in the save envelope.
        found: u32,
        /// Version supported by this player.
        expected: u32,
    },
    /// The save belongs to a different story IR version.
    #[error("saved story IR version {found} does not match loaded version {expected}")]
    StoryVersion {
        /// Version found in the save envelope.
        found: u32,
        /// Version of the currently loaded story.
        expected: u32,
    },
    /// The standalone runtime rejected an operation.
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedState {
    version: u32,
    story_version: u32,
    state: StoryState,
}

/// Deterministic player with no browser-global or desktop dependencies.
#[derive(Debug, Clone)]
pub struct Player {
    data: StoryIr,
    story: Story,
    seed: u64,
}

impl Player {
    /// Decode compiled JSON and start at the story entry knot.
    pub fn from_json(source: &str, seed: u64) -> Result<Self, PlayerError> {
        let data = serde_json::from_str::<StoryIr>(source)
            .map_err(|error| PlayerError::StoryJson(error.to_string()))?;
        let story = Story::with_seed(data.clone(), seed)?;
        Ok(Self { data, story, seed })
    }

    /// Advance to the next line, choice set, or ending.
    pub fn advance(&mut self) -> Result<PlayerFrame, PlayerError> {
        let event = self.story.advance()?;
        let draws = self.story.take_pattern_draws();
        Ok(match event {
            StoryEvent::Line(text) => PlayerFrame::Line { text, draws },
            StoryEvent::Choices(choices) => PlayerFrame::Choices { choices, draws },
            StoryEvent::Ended => PlayerFrame::Ended { draws },
        })
    }

    /// Return the currently eligible choices without advancing.
    #[must_use]
    pub fn choices(&self) -> Vec<ChoiceView> {
        self.story.choices()
    }

    /// Select one currently eligible choice by zero-based index.
    pub fn choose(&mut self, index: usize) -> Result<(), PlayerError> {
        self.story.choose(index)?;
        Ok(())
    }

    /// Restart from the entry knot with the current seed.
    pub fn restart(&mut self) -> Result<(), PlayerError> {
        self.story = Story::with_seed(self.data.clone(), self.seed)?;
        Ok(())
    }

    /// Restart from the entry knot with a new deterministic seed.
    pub fn restart_with_seed(&mut self, seed: u64) -> Result<(), PlayerError> {
        self.seed = seed;
        self.restart()
    }

    /// Serialize runtime state into a versioned JSON envelope owned by the host.
    pub fn serialize_state(&self) -> Result<String, PlayerError> {
        serde_json::to_string(&SavedState {
            version: WEB_STATE_VERSION,
            story_version: self.data.version,
            state: self.story.state().clone(),
        })
        .map_err(|error| PlayerError::StateJson(error.to_string()))
    }

    /// Restore a state envelope against the currently loaded immutable story.
    pub fn restore_state(&mut self, source: &str) -> Result<(), PlayerError> {
        let saved = serde_json::from_str::<SavedState>(source)
            .map_err(|error| PlayerError::StateJson(error.to_string()))?;
        if saved.version != WEB_STATE_VERSION {
            return Err(PlayerError::StateVersion {
                found: saved.version,
                expected: WEB_STATE_VERSION,
            });
        }
        if saved.story_version != self.data.version {
            return Err(PlayerError::StoryVersion {
                found: saved.story_version,
                expected: self.data.version,
            });
        }
        self.seed = saved.state.seed();
        self.story = Story::restore(self.data.clone(), saved.state)?;
        Ok(())
    }

    /// Seed used by restart and deterministic random draws.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Whether execution can advance without a choice.
    #[must_use]
    pub fn can_continue(&self) -> bool {
        self.story.can_continue()
    }
}

#[cfg(target_arch = "wasm32")]
mod bindings {
    use serde::Serialize;
    use wasm_bindgen::prelude::*;

    use super::{Player, PlayerError};

    /// JavaScript-facing Weave story player.
    #[wasm_bindgen(js_name = StoryPlayer)]
    pub struct WasmStoryPlayer {
        inner: Player,
    }

    #[wasm_bindgen(js_class = StoryPlayer)]
    impl WasmStoryPlayer {
        /// Load compiled story JSON with an unsigned 32-bit deterministic seed.
        #[wasm_bindgen(constructor)]
        pub fn new(story_json: &str, seed: u32) -> Result<Self, JsError> {
            Ok(Self {
                inner: Player::from_json(story_json, u64::from(seed)).map_err(js_error)?,
            })
        }

        /// Advance to the next host-visible story boundary.
        #[wasm_bindgen(js_name = continue)]
        pub fn continue_story(&mut self) -> Result<JsValue, JsError> {
            to_js(&self.inner.advance().map_err(js_error)?)
        }

        /// Return the currently eligible choices.
        pub fn choices(&self) -> Result<JsValue, JsError> {
            to_js(&self.inner.choices())
        }

        /// Select one currently eligible choice by zero-based index.
        pub fn choose(&mut self, index: usize) -> Result<(), JsError> {
            self.inner.choose(index).map_err(js_error)
        }

        /// Restart from the entry knot with the current seed.
        pub fn restart(&mut self) -> Result<(), JsError> {
            self.inner.restart().map_err(js_error)
        }

        /// Restart from the entry knot with a new unsigned 32-bit seed.
        #[wasm_bindgen(js_name = restartWithSeed)]
        pub fn restart_with_seed(&mut self, seed: u32) -> Result<(), JsError> {
            self.inner
                .restart_with_seed(u64::from(seed))
                .map_err(js_error)
        }

        /// Serialize runtime state; the JavaScript host decides whether and where to store it.
        #[wasm_bindgen(js_name = serializeState)]
        pub fn serialize_state(&self) -> Result<String, JsError> {
            self.inner.serialize_state().map_err(js_error)
        }

        /// Restore a previously serialized state envelope.
        #[wasm_bindgen(js_name = restoreState)]
        pub fn restore_state(&mut self, state_json: &str) -> Result<(), JsError> {
            self.inner.restore_state(state_json).map_err(js_error)
        }

        /// Whether execution can advance without a choice.
        #[wasm_bindgen(js_name = canContinue)]
        pub fn can_continue(&self) -> bool {
            self.inner.can_continue()
        }
    }

    fn to_js(value: &impl Serialize) -> Result<JsValue, JsError> {
        value
            .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
            .map_err(|error| JsError::new(&error.to_string()))
    }

    fn js_error(error: PlayerError) -> JsError {
        JsError::new(&error.to_string())
    }
}
