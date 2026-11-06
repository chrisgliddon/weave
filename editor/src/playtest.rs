//! Versioned author playtest records: capture, portable serialization, and
//! deterministic replay.
//!
//! A playtest record is a small, self-contained description of one author
//! playthrough: the compiled story's content digest, the deterministic random
//! seed, the ordered selected choice indices, checkpoints, transcript events,
//! presentation cues, and the terminal outcome. It carries no filesystem path
//! and performs no I/O. Every function is pure apart from the in-memory
//! runtime it drives.
//!
//! Records are **not** save states. The saved-game format is owned by
//! [`weave_runtime::StoryState`] and has its own compatibility guarantee. A
//! playtest record deliberately stores a replay cursor rather than runtime
//! state, so a future save-state change cannot invalidate a captured record and
//! a playtest never depends on saved-game compatibility.
//!
//! ## Formats
//!
//! [`PlaytestRecord::to_json`] and [`PlaytestRecord::to_ron`] emit the same
//! model with a stable field order and the same [`PLAYTEST_FORMAT_VERSION`].
//! Both decoders reject unknown fields and unsupported versions. JSON and RON
//! therefore round-trip to equal records with byte-identical output.
//!
//! ## Replay
//!
//! [`PlaytestRecord::replay`] runs the record against a compiled story and
//! returns the reproduced state, the remaining choices, the cue order, and the
//! outcome. It stops at the first incompatibility and returns an actionable
//! [`ReplayMismatch`]: a changed story digest, a different seed, a choice index
//! the story no longer offers, a cue or transcript difference, or a different
//! outcome.
//!
//! ## Source navigation
//!
//! Every recorded transcript event and presentation cue carries the knot that
//! produced it and, when the compiled story still exposes it, the source
//! [`Span`]. A host can open that location without guessing from rendered text.

use std::fmt;

use serde::{Deserialize, Serialize};
use weave_core::Span;
use weave_core::ir::{CueIr, Instruction, InstructionKind, KnotIr, StoryIr};
use weave_runtime::{ChoiceView, PresentationEvent, RuntimeError, Story, StoryEvent, StoryState};

/// Current playtest record format version.
pub const PLAYTEST_FORMAT_VERSION: u32 = 1;

/// Maximum number of story advances one capture may perform.
///
/// The bound keeps a cyclic story from allocating without limit and makes a
/// capture failure deterministic.
pub const MAX_CAPTURE_STEPS: usize = 100_000;

/// Digest algorithm prefix recorded beside the digest hex.
const DIGEST_ALGORITHM: &str = "fnv1a128:";

/// FNV-1a 128-bit offset basis.
const FNV_OFFSET_BASIS: u128 = 0x6c62272e07bb014262b821756295c58d;

/// FNV-1a 128-bit prime.
const FNV_PRIME: u128 = 0x0000000001000000000000000000013b;

/// One host-visible transcript event with its originating source location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaytestEvent {
    /// What happened.
    pub kind: PlaytestEventKind,
    /// Knot the event came from.
    pub knot: String,
    /// Source location in the compiled story, when one is known.
    pub span: Option<Span>,
}

/// Transcript event payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum PlaytestEventKind {
    /// One rendered narrative line.
    Line {
        /// Rendered line text.
        text: String,
    },
    /// One selected choice.
    Choice {
        /// Zero-based display index that was selected.
        index: usize,
        /// Rendered choice label.
        text: String,
    },
}

/// One presentation cue in source order, with its source location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaytestCue {
    /// Cue emitted by the runtime.
    pub cue: PresentationEvent,
    /// Knot the cue came from.
    pub knot: String,
    /// Source location in the compiled story, when one is known.
    pub span: Option<Span>,
}

/// Deterministic replay cursor at a choice boundary.
///
/// A checkpoint stores no runtime state. Restoring one replays the record from
/// the start through the recorded choices, which keeps the playtest format
/// independent of saved-game compatibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaytestCheckpoint {
    /// Number of transcript events recorded when the checkpoint was taken.
    pub event: usize,
    /// Number of selected choices applied when the checkpoint was taken.
    pub choice: usize,
    /// Knot at the checkpoint.
    pub knot: String,
    /// Source location at the checkpoint, when one is known.
    pub span: Option<Span>,
}

/// How the captured playthrough stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum PlaytestOutcome {
    /// The story reached its end.
    Ended,
    /// The capture ran out of supplied choices while the story waited.
    WaitingForChoice {
        /// Rendered labels offered at the stopping point.
        choices: Vec<String>,
    },
}

impl PlaytestOutcome {
    /// Short author-facing description of where the playthrough stopped.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::Ended => "ended".to_owned(),
            Self::WaitingForChoice { choices } if choices.is_empty() => {
                "waiting for choice".to_owned()
            }
            Self::WaitingForChoice { choices } => {
                format!("waiting for choice: {}", choices.join(", "))
            }
        }
    }
}

/// One complete, portable author playtest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaytestRecord {
    /// Playtest record format version, not a story or save-state version.
    pub format_version: u32,
    /// Stable digest of the compiled story content.
    pub story_digest: String,
    /// Deterministic seed used for the playthrough.
    pub seed: u64,
    /// Knot where playback began.
    pub start_knot: String,
    /// Ordered selected choice indices.
    pub choices: Vec<usize>,
    /// Transcript events in playback order.
    pub events: Vec<PlaytestEvent>,
    /// Presentation cues in emission order.
    pub cues: Vec<PlaytestCue>,
    /// Replay cursors at the start and after each choice.
    pub checkpoints: Vec<PlaytestCheckpoint>,
    /// Terminal state of the capture.
    pub outcome: PlaytestOutcome,
}

/// Reproduced terminal state of one successful replay.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayReport {
    /// Final runtime state.
    pub state: StoryState,
    /// Choices still available at the stopping point.
    pub choices: Vec<ChoiceView>,
    /// Presentation cues reproduced in order.
    pub cues: Vec<PlaytestCue>,
    /// Reproduced terminal outcome.
    pub outcome: PlaytestOutcome,
}

/// Runtime state reproduced at one checkpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckpointReport {
    /// Runtime state at the checkpoint boundary.
    pub state: StoryState,
    /// Choices pending at the checkpoint boundary.
    pub choices: Vec<ChoiceView>,
    /// Presentation cues emitted before the checkpoint.
    pub cues: Vec<PlaytestCue>,
}

/// Successful replay or the first incompatibility found.
pub type ReplayResult = Result<ReplayReport, ReplayMismatch>;

/// First reason a record could not reproduce its playthrough.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplayMismatch {
    /// The compiled story content differs from the captured story.
    StoryDigest {
        /// Digest stored in the record.
        recorded: String,
        /// Digest of the supplied story.
        actual: String,
    },
    /// The replay seed differs from the captured seed.
    Seed {
        /// Seed stored in the record.
        recorded: u64,
        /// Seed supplied to replay.
        attempted: u64,
    },
    /// The recorded start knot is no longer defined.
    StartKnot {
        /// Missing knot name.
        knot: String,
    },
    /// The story could not be digested.
    StoryUnreadable {
        /// Encoder detail.
        detail: String,
    },
    /// A recorded choice index is absent at that boundary.
    ChoiceOutOfRange {
        /// Zero-based choice boundary position.
        position: usize,
        /// Recorded selection index.
        index: usize,
        /// Number of choices the story offers.
        available: usize,
        /// Knot where the mismatch occurred.
        knot: String,
        /// Source location where the mismatch occurred.
        span: Option<Span>,
    },
    /// The runtime failed while replaying.
    Runtime {
        /// Number of events reproduced before the failure.
        position: usize,
        /// Runtime error text.
        detail: String,
    },
    /// The cue sequence differs.
    Cue {
        /// Zero-based cue position.
        position: usize,
        /// Cue recorded in the playtest.
        expected: Option<Box<PlaytestCue>>,
        /// Cue the story produced.
        actual: Option<Box<PlaytestCue>>,
    },
    /// The transcript sequence differs.
    Event {
        /// Zero-based event position.
        position: usize,
        /// Event recorded in the playtest.
        expected: Option<Box<PlaytestEvent>>,
        /// Event the story produced.
        actual: Option<Box<PlaytestEventKind>>,
    },
    /// The terminal outcome differs.
    Outcome {
        /// Recorded outcome.
        expected: PlaytestOutcome,
        /// Reproduced outcome.
        actual: PlaytestOutcome,
    },
    /// Choices remain but the story ended first.
    UnusedChoices {
        /// Zero-based position of the first unused choice.
        position: usize,
        /// Number of unused choices.
        remaining: usize,
    },
    /// The requested checkpoint index does not exist.
    UnknownCheckpoint {
        /// Requested checkpoint index.
        index: usize,
        /// Number of checkpoints in the record.
        available: usize,
    },
    /// The record's own checkpoints were not reached during replay.
    CheckpointNotFound {
        /// Requested checkpoint index.
        index: usize,
        /// Recorded event position.
        event: usize,
        /// Recorded choice position.
        choice: usize,
    },
}

impl fmt::Display for ReplayMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StoryDigest { recorded, actual } => write!(
                formatter,
                "story content changed: the record was captured against {recorded}, but this story digests to {actual}; reopen the story this record was captured from, or capture a new playtest"
            ),
            Self::Seed {
                recorded,
                attempted,
            } => write!(
                formatter,
                "seed changed: the record was captured with seed {recorded}, but replay requested seed {attempted}; replay with the recorded seed to reproduce this playthrough"
            ),
            Self::StartKnot { knot } => write!(
                formatter,
                "start knot `{knot}` is not defined in this story; the story changed or the record was edited"
            ),
            Self::StoryUnreadable { detail } => {
                write!(formatter, "the story could not be digested: {detail}")
            }
            Self::ChoiceOutOfRange {
                position,
                index,
                available,
                knot,
                ..
            } => write!(
                formatter,
                "choice #{position} selects index {index}, but only {available} choices are available at knot `{knot}`; check the record or replay against the story it was captured from"
            ),
            Self::Runtime { position, detail } => {
                write!(formatter, "the story failed at event {position}: {detail}")
            }
            Self::Cue {
                position,
                expected,
                actual,
            } => write!(
                formatter,
                "cue #{position} differs: {}, but the story produced {}",
                describe_cue(expected.as_deref()),
                describe_cue(actual.as_deref())
            ),
            Self::Event {
                position,
                expected,
                actual,
            } => write!(
                formatter,
                "event #{position} differs: {}, but the story produced {}",
                describe_event(expected.as_deref()),
                describe_kind(actual.as_deref())
            ),
            Self::Outcome { expected, actual } => write!(
                formatter,
                "outcome differs: the record ended as {expected:?}, but the story ended as {actual:?}"
            ),
            Self::UnusedChoices {
                position,
                remaining,
            } => write!(
                formatter,
                "the story ended with {remaining} of the record's choices unused, starting at position {position}; the record does not describe this story"
            ),
            Self::UnknownCheckpoint { index, available } => write!(
                formatter,
                "checkpoint #{index} does not exist; the record has {available} checkpoints"
            ),
            Self::CheckpointNotFound {
                index,
                event,
                choice,
            } => write!(
                formatter,
                "checkpoint #{index} at event {event} and choice {choice} was not reached; the record is internally inconsistent"
            ),
        }
    }
}

impl std::error::Error for ReplayMismatch {}

/// Capture failed before a complete record could be produced.
#[derive(Debug, thiserror::Error)]
pub enum PlaytestError {
    /// The runtime rejected the story.
    #[error("the runtime rejected the story: {0}")]
    Runtime(#[from] RuntimeError),
    /// The capture exceeded its advance bound.
    #[error(
        "capture exceeded {limit} story advances; the story may loop without reaching a line, choice, or end"
    )]
    StepLimit {
        /// The configured bound.
        limit: usize,
    },
    /// A supplied choice index is absent at that boundary.
    #[error("choice #{position} selects index {index}, but only {available} choices are available")]
    ChoiceOutOfRange {
        /// Zero-based choice boundary position.
        position: usize,
        /// Supplied selection index.
        index: usize,
        /// Number of choices offered.
        available: usize,
    },
    /// The story ended before every supplied choice was used.
    #[error("{supplied} choices were supplied, but the story ended after {consumed}")]
    UnusedChoices {
        /// Number of choices actually applied.
        consumed: usize,
        /// Number of choices supplied.
        supplied: usize,
    },
    /// The requested start knot does not exist.
    #[error("start knot `{0}` is not defined in this story")]
    UnknownStartKnot(String),
    /// The story could not be digested.
    #[error("the story could not be digested: {0}")]
    Digest(String),
}

/// Record encoding or decoding failed.
#[derive(Debug, thiserror::Error)]
pub enum PlaytestFormatError {
    /// JSON encoding failed.
    #[error("could not encode the playtest as JSON: {0}")]
    JsonEncode(String),
    /// JSON decoding failed.
    #[error("invalid playtest JSON: {0}")]
    JsonDecode(String),
    /// RON encoding failed.
    #[error("could not encode the playtest as RON: {0}")]
    RonEncode(String),
    /// RON decoding failed.
    #[error("invalid playtest RON: {0}")]
    RonDecode(String),
    /// The record uses a format version this build cannot read.
    #[error("unsupported playtest format version {found}; this build reads version {expected}")]
    UnsupportedVersion {
        /// Version found in the document.
        found: u32,
        /// Version this build reads.
        expected: u32,
    },
}

impl PlaytestRecord {
    /// Capture one playthrough from the story entry knot.
    ///
    /// The runtime drives the story deterministically with `seed` and selects
    /// `choices` in order. If the choices run out while the story waits, the
    /// record's outcome is [`PlaytestOutcome::WaitingForChoice`]. Supplying
    /// more choices than the story uses is an error.
    pub fn capture(story: &StoryIr, seed: u64, choices: &[usize]) -> Result<Self, PlaytestError> {
        let entry = story.entry.clone();
        Self::capture_from(story, seed, &entry, choices)
    }

    /// Capture one playthrough beginning at `start_knot`.
    pub fn capture_from(
        story: &StoryIr,
        seed: u64,
        start_knot: &str,
        choices: &[usize],
    ) -> Result<Self, PlaytestError> {
        if !story.knots.contains_key(start_knot) {
            return Err(PlaytestError::UnknownStartKnot(start_knot.to_owned()));
        }

        let digest = story_digest(story)?;
        let mut runtime = Story::with_seed(story.clone(), seed)?;
        if start_knot != story.entry {
            runtime.jump(start_knot)?;
        }

        let mut events = Vec::new();
        let mut cues = Vec::new();
        let mut selected = Vec::new();
        let mut checkpoints = vec![PlaytestCheckpoint {
            event: 0,
            choice: 0,
            knot: start_knot.to_owned(),
            span: first_span(story, start_knot),
        }];
        let mut last_knot = start_knot.to_owned();

        for _ in 0..MAX_CAPTURE_STEPS {
            let event = runtime.advance()?;
            let emitted = runtime.take_presentation();
            let probe = probe_runtime(&runtime);
            let current_knot = top_knot(probe.as_ref()).unwrap_or_else(|| last_knot.clone());

            for cue in emitted {
                let (knot, span) = locate_cue(story, &current_knot, &cue);
                cues.push(PlaytestCue { cue, knot, span });
            }

            match event {
                StoryEvent::Line(text) => {
                    let (knot, span) = line_location(story, probe.as_ref(), &current_knot);
                    last_knot = knot.clone();
                    events.push(PlaytestEvent {
                        kind: PlaytestEventKind::Line { text },
                        knot,
                        span,
                    });
                }
                StoryEvent::Choices(available) => {
                    let position = selected.len();
                    let Some(&index) = choices.get(position) else {
                        return Ok(Self::assemble(
                            digest,
                            seed,
                            start_knot,
                            selected,
                            events,
                            cues,
                            checkpoints,
                            PlaytestOutcome::WaitingForChoice {
                                choices: available
                                    .iter()
                                    .map(|choice| choice.text.clone())
                                    .collect(),
                            },
                        ));
                    };
                    let Some(choice) = available.get(index) else {
                        return Err(PlaytestError::ChoiceOutOfRange {
                            position,
                            index,
                            available: available.len(),
                        });
                    };
                    let (knot, span) = choice_location(story, probe.as_ref(), index, &current_knot);
                    last_knot = knot.clone();
                    events.push(PlaytestEvent {
                        kind: PlaytestEventKind::Choice {
                            index,
                            text: choice.text.clone(),
                        },
                        knot,
                        span,
                    });
                    runtime.choose(index)?;
                    selected.push(index);

                    let post = probe_runtime(&runtime);
                    let (knot, span) = cursor_location(story, post.as_ref(), &last_knot);
                    last_knot = knot.clone();
                    checkpoints.push(PlaytestCheckpoint {
                        event: events.len(),
                        choice: selected.len(),
                        knot,
                        span,
                    });
                }
                StoryEvent::Ended => {
                    if selected.len() < choices.len() {
                        return Err(PlaytestError::UnusedChoices {
                            consumed: selected.len(),
                            supplied: choices.len(),
                        });
                    }
                    return Ok(Self::assemble(
                        digest,
                        seed,
                        start_knot,
                        selected,
                        events,
                        cues,
                        checkpoints,
                        PlaytestOutcome::Ended,
                    ));
                }
            }
        }

        Err(PlaytestError::StepLimit {
            limit: MAX_CAPTURE_STEPS,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble(
        story_digest: String,
        seed: u64,
        start_knot: &str,
        choices: Vec<usize>,
        events: Vec<PlaytestEvent>,
        cues: Vec<PlaytestCue>,
        checkpoints: Vec<PlaytestCheckpoint>,
        outcome: PlaytestOutcome,
    ) -> Self {
        Self {
            format_version: PLAYTEST_FORMAT_VERSION,
            story_digest,
            seed,
            start_knot: start_knot.to_owned(),
            choices,
            events,
            cues,
            checkpoints,
            outcome,
        }
    }

    /// Encode as stable, pretty JSON.
    pub fn to_json(&self) -> Result<String, PlaytestFormatError> {
        let mut output = serde_json::to_string_pretty(self)
            .map_err(|error| PlaytestFormatError::JsonEncode(error.to_string()))?;
        output.push('\n');
        Ok(output)
    }

    /// Decode stable JSON and reject an unsupported format version.
    pub fn from_json(source: &str) -> Result<Self, PlaytestFormatError> {
        let record = serde_json::from_str::<Self>(source)
            .map_err(|error| PlaytestFormatError::JsonDecode(error.to_string()))?;
        record.validate_version()?;
        Ok(record)
    }

    /// Encode as stable, pretty RON.
    pub fn to_ron(&self) -> Result<String, PlaytestFormatError> {
        let config = ron::ser::PrettyConfig::new()
            .new_line("\n")
            .indentor("    ")
            .struct_names(false)
            .enumerate_arrays(false)
            .compact_arrays(false)
            .compact_maps(false);
        let mut output = ron::ser::to_string_pretty(self, config)
            .map_err(|error| PlaytestFormatError::RonEncode(error.to_string()))?;
        output.push('\n');
        Ok(output)
    }

    /// Decode stable RON and reject an unsupported format version.
    pub fn from_ron(source: &str) -> Result<Self, PlaytestFormatError> {
        let record = ron::from_str::<Self>(source)
            .map_err(|error| PlaytestFormatError::RonDecode(error.to_string()))?;
        record.validate_version()?;
        Ok(record)
    }

    /// Replay against the story and the recorded seed.
    ///
    /// Returns the reproduced state, remaining choices, cue order, and
    /// outcome, or the first mismatch.
    pub fn replay(&self, story: &StoryIr) -> ReplayResult {
        self.replay_with_seed(story, self.seed)
    }

    /// Replay against the story with an explicit seed.
    ///
    /// A seed different from the recorded seed is reported as
    /// [`ReplayMismatch::Seed`] before any story execution.
    pub fn replay_with_seed(&self, story: &StoryIr, seed: u64) -> ReplayResult {
        self.verify(story, seed)?;
        match self.walk(story, seed, WalkStop::Terminal)? {
            WalkOutcome::Terminal(report) => Ok(report),
            WalkOutcome::Checkpoint(_) => Err(ReplayMismatch::CheckpointNotFound {
                index: 0,
                event: 0,
                choice: 0,
            }),
        }
    }

    /// Replay from the start to one checkpoint and return its runtime state.
    ///
    /// The caller may pass the returned state to
    /// [`weave_runtime::Story::restore`] to continue from that boundary.
    pub fn restore_checkpoint(
        &self,
        story: &StoryIr,
        checkpoint: usize,
    ) -> Result<CheckpointReport, ReplayMismatch> {
        let expected =
            self.checkpoints
                .get(checkpoint)
                .ok_or(ReplayMismatch::UnknownCheckpoint {
                    index: checkpoint,
                    available: self.checkpoints.len(),
                })?;
        self.verify(story, self.seed)?;
        match self.walk(story, self.seed, WalkStop::Checkpoint(checkpoint))? {
            WalkOutcome::Checkpoint(report) => Ok(report),
            WalkOutcome::Terminal(_) => Err(ReplayMismatch::CheckpointNotFound {
                index: checkpoint,
                event: expected.event,
                choice: expected.choice,
            }),
        }
    }

    fn validate_version(&self) -> Result<(), PlaytestFormatError> {
        if self.format_version != PLAYTEST_FORMAT_VERSION {
            return Err(PlaytestFormatError::UnsupportedVersion {
                found: self.format_version,
                expected: PLAYTEST_FORMAT_VERSION,
            });
        }
        Ok(())
    }

    fn verify(&self, story: &StoryIr, seed: u64) -> Result<(), ReplayMismatch> {
        let actual = story_digest(story).map_err(|error| ReplayMismatch::StoryUnreadable {
            detail: error.to_string(),
        })?;
        if actual != self.story_digest {
            return Err(ReplayMismatch::StoryDigest {
                recorded: self.story_digest.clone(),
                actual,
            });
        }
        if seed != self.seed {
            return Err(ReplayMismatch::Seed {
                recorded: self.seed,
                attempted: seed,
            });
        }
        Ok(())
    }

    fn open(&self, story: &StoryIr, seed: u64) -> Result<Story, ReplayMismatch> {
        let mut runtime =
            Story::with_seed(story.clone(), seed).map_err(|error| ReplayMismatch::Runtime {
                position: 0,
                detail: error.to_string(),
            })?;
        if self.start_knot != story.entry {
            if !story.knots.contains_key(&self.start_knot) {
                return Err(ReplayMismatch::StartKnot {
                    knot: self.start_knot.clone(),
                });
            }
            runtime
                .jump(&self.start_knot)
                .map_err(|error| ReplayMismatch::Runtime {
                    position: 0,
                    detail: error.to_string(),
                })?;
        }
        Ok(runtime)
    }

    fn walk(
        &self,
        story: &StoryIr,
        seed: u64,
        stop: WalkStop,
    ) -> Result<WalkOutcome, ReplayMismatch> {
        let mut runtime = self.open(story, seed)?;
        let mut event_position = 0_usize;
        let mut cue_position = 0_usize;
        let mut choice_position = 0_usize;
        let mut reproduced_cues = Vec::new();

        if let WalkStop::Checkpoint(index) = stop
            && let Some(checkpoint) = self.checkpoints.get(index)
            && checkpoint.event == 0
            && checkpoint.choice == 0
        {
            return Ok(WalkOutcome::Checkpoint(CheckpointReport {
                state: runtime.state().clone(),
                choices: runtime.choices(),
                cues: reproduced_cues,
            }));
        }

        loop {
            let event = runtime.advance().map_err(|error| ReplayMismatch::Runtime {
                position: event_position,
                detail: error.to_string(),
            })?;

            for cue in runtime.take_presentation() {
                match self.cues.get(cue_position) {
                    None => {
                        return Err(ReplayMismatch::Cue {
                            position: cue_position,
                            expected: None,
                            actual: Some(Box::new(PlaytestCue {
                                cue,
                                knot: String::new(),
                                span: None,
                            })),
                        });
                    }
                    Some(recorded) if recorded.cue != cue => {
                        return Err(ReplayMismatch::Cue {
                            position: cue_position,
                            expected: Some(Box::new(recorded.clone())),
                            actual: Some(Box::new(PlaytestCue {
                                cue,
                                knot: recorded.knot.clone(),
                                span: recorded.span,
                            })),
                        });
                    }
                    Some(recorded) => {
                        reproduced_cues.push(recorded.clone());
                        cue_position += 1;
                    }
                }
            }

            match event {
                StoryEvent::Line(text) => {
                    let expected = self.events.get(event_position);
                    match expected.map(|event| &event.kind) {
                        Some(PlaytestEventKind::Line { text: recorded }) if *recorded == text => {
                            event_position += 1;
                        }
                        _ => {
                            return Err(ReplayMismatch::Event {
                                position: event_position,
                                expected: expected.map(|event| Box::new(event.clone())),
                                actual: Some(Box::new(PlaytestEventKind::Line { text })),
                            });
                        }
                    }
                }
                StoryEvent::Choices(available) => {
                    let Some(&chosen) = self.choices.get(choice_position) else {
                        let actual = PlaytestOutcome::WaitingForChoice {
                            choices: available.iter().map(|choice| choice.text.clone()).collect(),
                        };
                        if self.outcome != actual {
                            return Err(ReplayMismatch::Outcome {
                                expected: self.outcome.clone(),
                                actual,
                            });
                        }
                        return self.finish(
                            runtime.state(),
                            available,
                            reproduced_cues,
                            event_position,
                            cue_position,
                            actual,
                        );
                    };

                    let recorded = self.events.get(event_position);
                    let knot = recorded.map_or_else(String::new, |event| event.knot.clone());
                    let span = recorded.and_then(|event| event.span);

                    if chosen >= available.len() {
                        return Err(ReplayMismatch::ChoiceOutOfRange {
                            position: choice_position,
                            index: chosen,
                            available: available.len(),
                            knot,
                            span,
                        });
                    }

                    let actual = PlaytestEventKind::Choice {
                        index: chosen,
                        text: available[chosen].text.clone(),
                    };
                    match recorded.map(|event| &event.kind) {
                        Some(PlaytestEventKind::Choice { index, text })
                            if *index == chosen && *text == available[chosen].text => {}
                        _ => {
                            return Err(ReplayMismatch::Event {
                                position: event_position,
                                expected: recorded.map(|event| Box::new(event.clone())),
                                actual: Some(Box::new(actual)),
                            });
                        }
                    }

                    runtime
                        .choose(chosen)
                        .map_err(|error| ReplayMismatch::Runtime {
                            position: event_position,
                            detail: error.to_string(),
                        })?;
                    event_position += 1;
                    choice_position += 1;

                    if let WalkStop::Checkpoint(index) = stop
                        && let Some(checkpoint) = self.checkpoints.get(index)
                        && checkpoint.event == event_position
                        && checkpoint.choice == choice_position
                    {
                        return Ok(WalkOutcome::Checkpoint(CheckpointReport {
                            state: runtime.state().clone(),
                            choices: runtime.choices(),
                            cues: reproduced_cues,
                        }));
                    }
                }
                StoryEvent::Ended => {
                    if choice_position < self.choices.len() {
                        return Err(ReplayMismatch::UnusedChoices {
                            position: choice_position,
                            remaining: self.choices.len() - choice_position,
                        });
                    }
                    let actual = PlaytestOutcome::Ended;
                    if self.outcome != actual {
                        return Err(ReplayMismatch::Outcome {
                            expected: self.outcome.clone(),
                            actual,
                        });
                    }
                    return self.finish(
                        runtime.state(),
                        runtime.choices(),
                        reproduced_cues,
                        event_position,
                        cue_position,
                        actual,
                    );
                }
            }
        }
    }

    fn finish(
        &self,
        state: &StoryState,
        choices: Vec<ChoiceView>,
        cues: Vec<PlaytestCue>,
        event_position: usize,
        cue_position: usize,
        outcome: PlaytestOutcome,
    ) -> Result<WalkOutcome, ReplayMismatch> {
        if let Some(expected) = self.events.get(event_position) {
            return Err(ReplayMismatch::Event {
                position: event_position,
                expected: Some(Box::new(expected.clone())),
                actual: None,
            });
        }
        if let Some(expected) = self.cues.get(cue_position) {
            return Err(ReplayMismatch::Cue {
                position: cue_position,
                expected: Some(Box::new(expected.clone())),
                actual: None,
            });
        }
        Ok(WalkOutcome::Terminal(ReplayReport {
            state: state.clone(),
            choices,
            cues,
            outcome,
        }))
    }
}

/// Where a replay walk stops.
#[derive(Clone, Copy)]
enum WalkStop {
    /// Stop at the terminal state.
    Terminal,
    /// Stop at one checkpoint index.
    Checkpoint(usize),
}

/// Result of one replay walk.
enum WalkOutcome {
    /// The walk reached the terminal state.
    Terminal(ReplayReport),
    /// The walk reached the requested checkpoint.
    Checkpoint(CheckpointReport),
}

/// Compute the stable content digest of a compiled story.
///
/// The source name is excluded so the same content hashes equally when it is
/// opened from a different machine or location. The digest is stable across
/// runs and platforms; it is an identity check, not a security primitive.
pub fn story_digest(story: &StoryIr) -> Result<String, PlaytestError> {
    let mut normalized = story.clone();
    normalized.source_name = None;
    let encoded = serde_json::to_vec(&normalized)
        .map_err(|error| PlaytestError::Digest(error.to_string()))?;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in encoded {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    Ok(format!("{DIGEST_ALGORITHM}{hash:032x}"))
}

/// Serializable mirror of the runtime state fields source navigation needs.
///
/// The runtime exposes no public cursor accessor, so capture reads the same
/// serialized state the runtime already exposes for save and inspection.
#[derive(Debug, Deserialize)]
struct StateProbe {
    #[serde(default)]
    frames: Vec<FrameProbe>,
    #[serde(default)]
    pending_choices: Vec<PendingChoiceProbe>,
}

#[derive(Debug, Deserialize)]
struct FrameProbe {
    block: BlockProbe,
    index: usize,
}

#[derive(Debug, Deserialize)]
struct BlockProbe {
    knot: String,
    #[serde(default)]
    segments: Vec<SegmentProbe>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum SegmentProbe {
    Choice { instruction: usize },
    ConditionalBranch { instruction: usize, branch: usize },
    ConditionalFallback { instruction: usize },
}

#[derive(Debug, Deserialize)]
struct PendingChoiceProbe {
    reference: BlockProbe,
    instruction: usize,
}

fn probe_runtime(runtime: &Story) -> Option<StateProbe> {
    let value = serde_json::to_value(runtime.state()).ok()?;
    serde_json::from_value(value).ok()
}

fn top_knot(probe: Option<&StateProbe>) -> Option<String> {
    let frame = probe?.frames.last()?;
    Some(frame.block.knot.clone())
}

fn first_span(story: &StoryIr, knot: &str) -> Option<Span> {
    story
        .knots
        .get(knot)?
        .content
        .first()
        .map(|instruction| instruction.span)
}

fn line_location(
    story: &StoryIr,
    probe: Option<&StateProbe>,
    fallback_knot: &str,
) -> (String, Option<Span>) {
    let Some(frame) = probe.and_then(|probe| probe.frames.last()) else {
        return (fallback_knot.to_owned(), None);
    };
    let span = frame
        .index
        .checked_sub(1)
        .and_then(|index| span_at(story, &frame.block, index));
    (frame.block.knot.clone(), span)
}

fn choice_location(
    story: &StoryIr,
    probe: Option<&StateProbe>,
    index: usize,
    fallback_knot: &str,
) -> (String, Option<Span>) {
    let Some(choice) = probe.and_then(|probe| probe.pending_choices.get(index)) else {
        return (fallback_knot.to_owned(), None);
    };
    let span = span_at(story, &choice.reference, choice.instruction);
    (choice.reference.knot.clone(), span)
}

fn cursor_location(
    story: &StoryIr,
    probe: Option<&StateProbe>,
    fallback_knot: &str,
) -> (String, Option<Span>) {
    let Some(frame) = probe.and_then(|probe| probe.frames.last()) else {
        return (fallback_knot.to_owned(), None);
    };
    let span = span_at(story, &frame.block, frame.index);
    (frame.block.knot.clone(), span)
}

/// Resolve the instruction span at one cursor position inside a block.
fn span_at(story: &StoryIr, block: &BlockProbe, index: usize) -> Option<Span> {
    block_instructions(story, block)?
        .get(index)
        .map(|instruction| instruction.span)
}

/// Descend a serialized block reference to its instruction list.
fn block_instructions<'a>(story: &'a StoryIr, block: &BlockProbe) -> Option<&'a [Instruction]> {
    let mut instructions: &'a [Instruction] = &story.knots.get(&block.knot)?.content;
    for segment in &block.segments {
        instructions = match segment {
            SegmentProbe::Choice { instruction } => match &instructions.get(*instruction)?.kind {
                InstructionKind::Choice(choice) => &choice.body,
                _ => return None,
            },
            SegmentProbe::ConditionalBranch {
                instruction,
                branch,
            } => match &instructions.get(*instruction)?.kind {
                InstructionKind::Conditional(conditional) => {
                    &conditional.branches.get(*branch)?.body
                }
                _ => return None,
            },
            SegmentProbe::ConditionalFallback { instruction } => {
                match &instructions.get(*instruction)?.kind {
                    InstructionKind::Conditional(conditional) => conditional.fallback.as_deref()?,
                    _ => return None,
                }
            }
        };
    }
    Some(instructions)
}

/// Find the first source instruction that produced one cue.
///
/// The preferred knot is searched first, then every knot in sorted order. A
/// cue that is not found keeps the preferred knot and no span, which is still
/// enough for navigation to the knot.
fn locate_cue(
    story: &StoryIr,
    preferred_knot: &str,
    cue: &PresentationEvent,
) -> (String, Option<Span>) {
    if let Some(knot) = story.knots.get(preferred_knot)
        && let Some(span) = find_cue_span(knot, cue)
    {
        return (preferred_knot.to_owned(), Some(span));
    }
    for (name, knot) in &story.knots {
        if let Some(span) = find_cue_span(knot, cue) {
            return (name.clone(), Some(span));
        }
    }
    (preferred_knot.to_owned(), None)
}

/// Depth-first, source-ordered search for one cue instruction.
fn find_cue_span(knot: &KnotIr, cue: &PresentationEvent) -> Option<Span> {
    let mut stack: Vec<&Instruction> = knot.content.iter().rev().collect();
    while let Some(instruction) = stack.pop() {
        if let InstructionKind::Cue(candidate) = &instruction.kind
            && cue_matches(candidate, cue)
        {
            return Some(instruction.span);
        }
        match &instruction.kind {
            InstructionKind::Choice(choice) => {
                stack.extend(choice.body.iter().rev());
            }
            InstructionKind::Conditional(conditional) => {
                if let Some(fallback) = &conditional.fallback {
                    stack.extend(fallback.iter().rev());
                }
                for branch in conditional.branches.iter().rev() {
                    stack.extend(branch.body.iter().rev());
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether one compiled cue is the source of one runtime cue.
fn cue_matches(compiled: &CueIr, emitted: &PresentationEvent) -> bool {
    match (compiled, emitted) {
        (CueIr::Scene(a), PresentationEvent::Scene(b))
        | (CueIr::Show(a), PresentationEvent::Show(b))
        | (CueIr::Hide(a), PresentationEvent::Hide(b)) => a == b,
        (CueIr::Speaker(a), PresentationEvent::Speaker(b)) => a == b,
        (
            CueIr::Move {
                stage: a,
                to: slot_a,
            },
            PresentationEvent::Move {
                stage: b,
                to: slot_b,
            },
        ) => a == b && slot_a == slot_b,
        (
            CueIr::Swap {
                stage: a,
                variant: variant_a,
            },
            PresentationEvent::Swap {
                stage: b,
                variant: variant_b,
            },
        ) => a == b && variant_a == variant_b,
        (
            CueIr::Transition {
                kind: kind_a,
                ms: ms_a,
            },
            PresentationEvent::Transition {
                kind: kind_b,
                ms: ms_b,
            },
        ) => kind_a == kind_b && ms_a == ms_b,
        _ => false,
    }
}

fn describe_cue(cue: Option<&PlaytestCue>) -> String {
    match cue {
        Some(cue) => format!("the record has {:?} at knot `{}`", cue.cue, cue.knot),
        None => "the record has no further cue".to_owned(),
    }
}

fn describe_event(event: Option<&PlaytestEvent>) -> String {
    match event {
        Some(event) => format!(
            "the record has {} at knot `{}`",
            describe_kind(Some(&event.kind)),
            event.knot
        ),
        None => "the record has no further event".to_owned(),
    }
}

fn describe_kind(kind: Option<&PlaytestEventKind>) -> String {
    match kind {
        Some(PlaytestEventKind::Line { text }) => format!("line \"{text}\""),
        Some(PlaytestEventKind::Choice { index, text }) => {
            format!("choice #{index} \"{text}\"")
        }
        None => "no further event".to_owned(),
    }
}

impl fmt::Display for PlaytestEventKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&describe_kind(Some(self)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_compiler::CompileOptions;
    use weave_core::ir::TemplatePartIr;
    use weave_runtime::Value;

    const SOURCE: &str = r#"
FLAG boarded = false

scene ferry_dusk {
    background: "art/ferry_dusk.png"
}

scene crossing {
    background: "art/crossing.png"
}

stage mara {
    image: "art/mara_neutral.png"
    name: "Mara"
}

=== start ===
@scene ferry_dusk
@show mara
Mara: The ferry leaves at dusk.
* [Board the ferry]
    SET boarded = true
    @scene crossing
    Mara: Then we cross together.
    -> END
* [Wait for morning]
    -> start

=== landing ===
The landing is empty.
-> END
"#;

    fn story() -> StoryIr {
        compile(SOURCE, None)
    }

    fn compile(source: &str, source_name: Option<&str>) -> StoryIr {
        let options = CompileOptions {
            source_name: source_name.map(str::to_owned),
        };
        weave_compiler::compile(source, &options)
            .expect("fixture compiles")
            .story
    }

    fn record() -> PlaytestRecord {
        PlaytestRecord::capture(&story(), 7, &[0]).expect("capture")
    }

    #[test]
    fn a_captured_record_holds_every_contract_field() {
        let record = record();
        assert_eq!(record.format_version, PLAYTEST_FORMAT_VERSION);
        assert!(record.story_digest.starts_with(DIGEST_ALGORITHM));
        assert_eq!(record.seed, 7);
        assert_eq!(record.start_knot, "start");
        assert_eq!(record.choices, [0]);
        assert_eq!(record.checkpoints.len(), 2);
        assert_eq!(record.outcome, PlaytestOutcome::Ended);
        assert_eq!(
            record
                .cues
                .iter()
                .map(|cue| cue.cue.clone())
                .collect::<Vec<_>>(),
            [
                PresentationEvent::Scene("ferry_dusk".to_owned()),
                PresentationEvent::Show("mara".to_owned()),
                PresentationEvent::Scene("crossing".to_owned()),
            ]
        );
        assert!(matches!(
            record.events.last().map(|event| &event.kind),
            Some(PlaytestEventKind::Line { text }) if text == "Mara: Then we cross together."
        ));
    }

    #[test]
    fn a_record_round_trips_through_json_and_ron() {
        let record = record();
        let json = record.to_json().expect("encode JSON");
        let ron = record.to_ron().expect("encode RON");

        assert_eq!(
            PlaytestRecord::from_json(&json).expect("decode JSON"),
            record
        );
        assert_eq!(PlaytestRecord::from_ron(&ron).expect("decode RON"), record);
        assert_eq!(
            PlaytestRecord::from_json(&json)
                .expect("decode JSON")
                .to_json()
                .expect("re-encode JSON"),
            json
        );
        assert_eq!(
            PlaytestRecord::from_ron(&ron)
                .expect("decode RON")
                .to_ron()
                .expect("re-encode RON"),
            ron
        );

        assert!(json.starts_with("{\n  \"format_version\": "));
        assert!(
            json.find("\"format_version\"").expect("version field")
                < json.find("\"story_digest\"").expect("digest field")
        );
        assert!(ron.starts_with("(\n    format_version: "));
    }

    #[test]
    fn a_record_never_embeds_a_machine_specific_source_name() {
        let named = compile(SOURCE, Some("/Users/example/secret.weave"));
        let unnamed = story();
        assert_eq!(
            story_digest(&named).expect("named digest"),
            story_digest(&unnamed).expect("unnamed digest"),
            "the source name is not part of story identity"
        );

        let record = PlaytestRecord::capture(&named, 7, &[0]).expect("capture named story");
        let json = record.to_json().expect("JSON");
        let ron = record.to_ron().expect("RON");
        for encoded in [&json, &ron] {
            assert!(!encoded.contains("/Users/"));
            assert!(!encoded.contains("secret.weave"));
        }
    }

    #[test]
    fn replay_reproduces_state_cues_and_outcome() {
        let story = story();
        let record = record();
        let report = record.replay(&story).expect("replay");

        assert_eq!(report.outcome, PlaytestOutcome::Ended);
        assert!(report.choices.is_empty());
        assert_eq!(report.state.variables()["boarded"].value, Value::Bool(true));
        assert_eq!(
            report
                .cues
                .iter()
                .map(|cue| cue.cue.clone())
                .collect::<Vec<_>>(),
            [
                PresentationEvent::Scene("ferry_dusk".to_owned()),
                PresentationEvent::Show("mara".to_owned()),
                PresentationEvent::Scene("crossing".to_owned()),
            ]
        );
    }

    #[test]
    fn replay_reports_a_changed_story_digest() {
        let record = record();
        let mut changed = story();
        changed
            .knots
            .get_mut("start")
            .expect("start knot")
            .content
            .push(Instruction {
                span: Span::new(0, 0, 1, 1),
                kind: InstructionKind::End,
            });

        let error = record.replay(&changed).expect_err("changed story");
        assert!(matches!(error, ReplayMismatch::StoryDigest { .. }));
        assert!(error.to_string().contains("story content changed"));
    }

    #[test]
    fn replay_reports_a_changed_seed() {
        let record = record();
        let error = record
            .replay_with_seed(&story(), 8)
            .expect_err("changed seed");
        assert!(matches!(
            error,
            ReplayMismatch::Seed {
                recorded: 7,
                attempted: 8
            }
        ));
        assert!(error.to_string().contains("replay with the recorded seed"));
    }

    #[test]
    fn replay_reports_an_out_of_range_choice() {
        let mut record = record();
        record.choices[0] = 9;
        let error = record.replay(&story()).expect_err("bad choice");
        assert!(matches!(
            error,
            ReplayMismatch::ChoiceOutOfRange {
                position: 0,
                index: 9,
                available: 2,
                ..
            }
        ));
        assert!(error.to_string().contains("only 2 choices are available"));
    }

    #[test]
    fn replay_reports_a_cue_mismatch() {
        let mut record = record();
        record.cues[0].cue = PresentationEvent::Scene("elsewhere".to_owned());
        let error = record.replay(&story()).expect_err("cue mismatch");
        assert!(matches!(error, ReplayMismatch::Cue { position: 0, .. }));
        assert!(error.to_string().contains("cue #0 differs"));
    }

    #[test]
    fn replay_reports_a_transcript_mismatch() {
        let mut record = record();
        if let PlaytestEventKind::Line { text } = &mut record.events[0].kind {
            *text = "A different line.".to_owned();
        }
        let error = record.replay(&story()).expect_err("event mismatch");
        assert!(matches!(error, ReplayMismatch::Event { position: 0, .. }));
    }

    #[test]
    fn a_checkpoint_restores_the_story_at_that_boundary() {
        let story = story();
        let record = record();
        assert_eq!(record.checkpoints[1].choice, 1);
        assert_eq!(record.checkpoints[1].event, 2);

        let checkpoint = record
            .restore_checkpoint(&story, 1)
            .expect("restore checkpoint");
        assert!(checkpoint.choices.is_empty());

        let mut runtime = Story::restore(story.clone(), checkpoint.state).expect("restore runtime");
        let mut lines = Vec::new();
        while let StoryEvent::Line(line) = runtime.advance().expect("advance") {
            lines.push(line);
        }
        assert_eq!(lines, ["Mara: Then we cross together."]);
        assert_eq!(
            runtime.state().variables()["boarded"].value,
            Value::Bool(true)
        );
    }

    #[test]
    fn capture_records_source_navigation_for_events_and_cues() {
        let story = story();
        let record = record();

        let expected_line = story
            .knots
            .get("start")
            .expect("start knot")
            .content
            .iter()
            .find_map(|instruction| match &instruction.kind {
                InstructionKind::Text(template)
                    if matches!(
                        template.parts.as_slice(),
                        [TemplatePartIr::Text(text)]
                            if text == "Mara: The ferry leaves at dusk."
                    ) =>
                {
                    Some(instruction.span)
                }
                _ => None,
            })
            .expect("line span");

        let line = record
            .events
            .iter()
            .find(|event| {
                matches!(
                    &event.kind,
                    PlaytestEventKind::Line { text } if text.contains("ferry leaves")
                )
            })
            .expect("line event");
        assert_eq!(line.knot, "start");
        assert_eq!(line.span, Some(expected_line));

        let choice = record
            .events
            .iter()
            .find(|event| matches!(event.kind, PlaytestEventKind::Choice { .. }))
            .expect("choice event");
        assert_eq!(choice.knot, "start");
        assert!(choice.span.is_some(), "a compiled choice has a span");

        for cue in &record.cues {
            assert_eq!(cue.knot, "start");
            assert!(
                cue.span.is_some(),
                "a compiled cue has a source span: {cue:?}"
            );
        }
    }

    #[test]
    fn a_partial_playtest_stops_waiting_for_choice() {
        let story = story();
        let record = PlaytestRecord::capture(&story, 7, &[]).expect("capture");
        assert_eq!(
            record.outcome,
            PlaytestOutcome::WaitingForChoice {
                choices: vec!["Board the ferry".to_owned(), "Wait for morning".to_owned()]
            }
        );
        let report = record.replay(&story).expect("replay");
        assert!(matches!(
            report.outcome,
            PlaytestOutcome::WaitingForChoice { .. }
        ));
        assert_eq!(report.choices.len(), 2);
    }

    #[test]
    fn extra_supplied_choices_are_rejected() {
        let error = PlaytestRecord::capture(&story(), 7, &[0, 0]).expect_err("extra choice");
        assert!(matches!(
            error,
            PlaytestError::UnusedChoices {
                consumed: 1,
                supplied: 2
            }
        ));
    }

    #[test]
    fn a_capture_from_another_start_knot_replays_from_there() {
        let story = story();
        let record =
            PlaytestRecord::capture_from(&story, 7, "landing", &[]).expect("capture from knot");
        assert_eq!(record.start_knot, "landing");
        assert_eq!(record.events.len(), 1);
        assert_eq!(record.events[0].knot, "landing");
        let report = record.replay(&story).expect("replay");
        assert_eq!(report.outcome, PlaytestOutcome::Ended);
        assert_eq!(
            report.state.variables()["boarded"].value,
            Value::Bool(false)
        );
    }

    #[test]
    fn an_unknown_start_knot_is_rejected() {
        let error = PlaytestRecord::capture_from(&story(), 7, "missing", &[])
            .expect_err("unknown start knot");
        assert!(matches!(error, PlaytestError::UnknownStartKnot(knot) if knot == "missing"));
    }

    #[test]
    fn an_unsupported_format_version_is_rejected() {
        let record = record();
        let json = record
            .to_json()
            .expect("JSON")
            .replace("\"format_version\": 1", "\"format_version\": 99");
        assert!(matches!(
            PlaytestRecord::from_json(&json),
            Err(PlaytestFormatError::UnsupportedVersion {
                found: 99,
                expected: 1
            })
        ));

        let ron = record
            .to_ron()
            .expect("RON")
            .replace("format_version: 1", "format_version: 99");
        assert!(matches!(
            PlaytestRecord::from_ron(&ron),
            Err(PlaytestFormatError::UnsupportedVersion {
                found: 99,
                expected: 1
            })
        ));
    }

    #[test]
    fn unknown_record_fields_are_rejected() {
        let json = record().to_json().expect("JSON").replace(
            "\"format_version\": 1,",
            "\"format_version\": 1,\n  \"unexpected\": true,",
        );
        assert!(matches!(
            PlaytestRecord::from_json(&json),
            Err(PlaytestFormatError::JsonDecode(_))
        ));
    }
}
