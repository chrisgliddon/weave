//! Play mode: the preview session presented as a scene rather than a transcript.
//!
//! The scene is derived from the same session the debug transcript reads, so
//! the two views cannot disagree about the story state. This module holds the
//! derived data and the naming rules; the surface only draws it.

use weave_runtime::ChoiceView;

use crate::preview::{PreviewSession, PreviewStatus, PreviewTranscript};

/// Which presentation the preview panel uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    /// The diagnostic transcript.
    #[default]
    Debug,
    /// The staged scene.
    Play,
}

impl PlayMode {
    /// Toggle label for the other mode.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Debug => Self::Play,
            Self::Play => Self::Debug,
        }
    }

    /// Mode name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Debug => "Debug",
            Self::Play => "Play",
        }
    }
}

/// Everything the Play surface draws, derived from one preview session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayScene {
    /// Speaker of the current line.
    pub speaker: String,
    /// Current line, without its speaker prefix.
    pub line: String,
    /// Choices waiting for the author.
    pub choices: Vec<ChoiceView>,
    /// Session status.
    pub status: PreviewStatus,
    /// Whether the story has ended.
    pub ended: bool,
    /// Diagnostic shown without discarding the last valid frame.
    pub issue: Option<String>,
}

/// The name used when a line has no explicit speaker.
pub const NARRATOR: &str = "Narrator";

/// Build the scene for the current session state.
///
/// The scene reads the same runtime and transcript the debug view reads, so
/// advancing, choosing, restarting, and jumping produce the same state in both
/// presentations.
#[must_use]
pub fn scene_from_session(session: &PreviewSession) -> PlayScene {
    let line = session
        .transcript()
        .iter()
        .rev()
        .find_map(|entry| match entry {
            PreviewTranscript::Line(line) => Some(line.clone()),
            PreviewTranscript::Choice(_) | PreviewTranscript::Jump(_) => None,
        })
        .unwrap_or_default();
    let (speaker, line) = split_speaker(&line);
    PlayScene {
        speaker,
        line,
        choices: session.choices(),
        status: session.status(),
        ended: session.status() == PreviewStatus::Ended,
        issue: session.issue().map(|issue| issue.message.clone()),
    }
}

/// Split a leading `Speaker: ` prefix from a line.
///
/// Weave lines carry no speaker field, so Play names a line the way prose does:
/// a short prefix ending in a colon before the text. Anything else is
/// narration, which keeps a diagnostic or a long sentence from becoming a
/// character name.
#[must_use]
pub fn split_speaker(line: &str) -> (String, String) {
    let trimmed = line.trim_start();
    if let Some(index) = trimmed.find(": ")
        && index > 0
        && index <= 40
    {
        let candidate = &trimmed[..index];
        let words: Vec<&str> = candidate.split_whitespace().collect();
        let capitalised = words.iter().all(|word| {
            word.chars()
                .next()
                .is_some_and(|first| first.is_uppercase())
        });
        let clean = !candidate.contains(['.', '!', '?', ',', ';'])
            && !words.is_empty()
            && words.len() <= 3
            && capitalised
            && candidate.chars().all(|character| {
                character.is_alphanumeric() || character.is_whitespace() || character == '-'
            });
        if clean {
            return (
                candidate.to_owned(),
                trimmed[index + 2..].trim_start().to_owned(),
            );
        }
    }
    (NARRATOR.to_owned(), line.trim_start().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::{PreviewSession, PreviewStatus};
    use weave_core::ir::StoryIr;

    fn branching_story() -> StoryIr {
        let source = r#"
FLAG boarded = false

=== start ===
Mara: The ferry leaves at dusk.
* [Board the ferry]
    SET boarded = true
    Mara: Then we cross together.
    -> END
* [Wait for morning]
    -> start
"#;
        weave_compiler::compile(source, &weave_compiler::CompileOptions::default())
            .expect("fixture compiles")
            .story
    }

    fn session() -> PreviewSession {
        let mut session = PreviewSession::from_story(branching_story(), 7, 1);
        session.run();
        session
    }

    #[test]
    fn a_named_line_splits_into_speaker_and_text() {
        assert_eq!(
            split_speaker("Mara: The ferry leaves at dusk."),
            ("Mara".to_owned(), "The ferry leaves at dusk.".to_owned())
        );
    }

    #[test]
    fn a_sentence_with_a_colon_stays_narration() {
        let (speaker, line) = split_speaker("The rule was simple: wait for morning.");
        assert_eq!(speaker, NARRATOR);
        assert_eq!(line, "The rule was simple: wait for morning.");
    }

    #[test]
    fn a_long_prefix_is_not_a_character_name() {
        let (speaker, _) = split_speaker(
            "A very long introductory clause that is really prose: and then it ended.",
        );
        assert_eq!(speaker, NARRATOR);
    }

    #[test]
    fn the_scene_shows_the_last_line_and_the_pending_choices() {
        let session = session();
        let scene = scene_from_session(&session);
        assert_eq!(scene.speaker, "Mara");
        assert_eq!(scene.line, "The ferry leaves at dusk.");
        assert_eq!(scene.choices.len(), 2);
        assert_eq!(scene.status, PreviewStatus::WaitingForChoice);
        assert!(!scene.ended);
    }

    #[test]
    fn the_scene_matches_the_debug_session_after_a_choice() {
        let mut session = session();
        let before = scene_from_session(&session);
        assert_eq!(before.choices, session.choices());

        session.choose(0);
        let after = scene_from_session(&session);
        assert_eq!(after.choices, session.choices());
        assert_eq!(after.speaker, "Mara");
        assert_eq!(after.line, "Then we cross together.");
        assert!(after.ended);
        assert_eq!(after.status, PreviewStatus::Ended);
        assert_eq!(
            session
                .variables()
                .iter()
                .find(|(name, _)| name == "boarded")
                .map(|(_, value)| value.clone()),
            Some(weave_runtime::Value::Bool(true))
        );
    }

    #[test]
    fn the_last_valid_frame_survives_a_compile_failure() {
        let mut session = session();
        assert!(!session.compile_source("=== broken", None, 2));
        let scene = scene_from_session(&session);
        assert_eq!(scene.line, "The ferry leaves at dusk.");
        assert!(scene.issue.is_some());
    }

    #[test]
    fn an_empty_session_has_an_empty_narrated_scene() {
        let scene = scene_from_session(&PreviewSession::empty(1));
        assert_eq!(scene.speaker, NARRATOR);
        assert!(scene.line.is_empty());
        assert!(scene.choices.is_empty());
        assert!(!scene.ended);
    }

    #[test]
    fn the_mode_toggles_between_debug_and_play() {
        assert_eq!(PlayMode::default(), PlayMode::Debug);
        assert_eq!(PlayMode::Debug.other(), PlayMode::Play);
        assert_eq!(PlayMode::Play.other(), PlayMode::Debug);
    }

    #[test]
    fn a_fresh_session_is_not_playable_until_it_runs() {
        let mut session = PreviewSession::from_story(branching_story(), 7, 1);
        let scene = scene_from_session(&session);
        assert!(scene.line.is_empty());
        assert_eq!(session.status(), PreviewStatus::Ready);

        let _ = session.step();
        assert_eq!(scene_from_session(&session).speaker, "Mara");
    }
}
