//! Debounced compiler integration and isolated live story playback panel.

use std::time::{Duration, Instant};

use gpui::{AnyElement, Context, EventEmitter, Window, div, prelude::*, px, rgb};
use weave_compiler::{CompileOptions, compile};
use weave_core::ir::StoryIr;
use weave_core::{Diagnostic, Span};
use weave_patterns::DrawResult;
use weave_runtime::{ChoiceView, Story, StoryEvent, Value};

use crate::theme::DARK_THEME;

const MAX_CONTINUE_BOUNDARIES: usize = 10_000;

/// Transcript item retained for the current preview run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewTranscript {
    Line(String),
    Choice(String),
    Jump(String),
}

/// Current playback boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewStatus {
    Ready,
    Running,
    WaitingForChoice,
    Ended,
    Stopped,
    Error,
}

impl PreviewStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::WaitingForChoice => "Waiting for choice",
            Self::Ended => "Ended",
            Self::Stopped => "Stopped",
            Self::Error => "Error",
        }
    }
}

/// Actionable compiler/runtime failure shown without discarding the last valid preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewIssue {
    pub message: String,
    pub span: Option<Span>,
}

/// Quiet-period tracker used by source edits before recompiling preview state.
#[derive(Debug, Clone)]
pub struct PreviewCompileDebouncer {
    delay: Duration,
    pending: Option<(u64, Instant)>,
}

impl PreviewCompileDebouncer {
    #[must_use]
    pub const fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: None,
        }
    }

    pub fn schedule(&mut self, revision: u64, now: Instant) {
        self.pending = Some((revision, now));
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn take_ready(&mut self, now: Instant) -> Option<u64> {
        let (revision, scheduled) = self.pending?;
        if now.saturating_duration_since(scheduled) < self.delay {
            return None;
        }
        self.pending = None;
        Some(revision)
    }
}

#[derive(Debug, Clone)]
struct PendingCompile {
    source: String,
    source_name: Option<String>,
    revision: u64,
}

/// Testable live preview session isolated from the editor's pattern browser.
#[derive(Debug, Clone)]
pub struct PreviewSession {
    last_valid_story: Option<StoryIr>,
    runtime: Option<Story>,
    seed: u64,
    compiled_revision: Option<u64>,
    stale: bool,
    status: PreviewStatus,
    transcript: Vec<PreviewTranscript>,
    pattern_draws: Vec<DrawResult>,
    issue: Option<PreviewIssue>,
}

impl PreviewSession {
    #[must_use]
    pub fn empty(seed: u64) -> Self {
        Self {
            last_valid_story: None,
            runtime: None,
            seed,
            compiled_revision: None,
            stale: false,
            status: PreviewStatus::Ready,
            transcript: Vec::new(),
            pattern_draws: Vec::new(),
            issue: None,
        }
    }

    pub fn from_story(story: StoryIr, seed: u64, revision: u64) -> Self {
        let mut session = Self::empty(seed);
        session.install_story(story, revision);
        session
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub const fn status(&self) -> PreviewStatus {
        self.status
    }

    #[must_use]
    pub const fn is_stale(&self) -> bool {
        self.stale
    }

    #[must_use]
    pub const fn compiled_revision(&self) -> Option<u64> {
        self.compiled_revision
    }

    #[must_use]
    pub fn transcript(&self) -> &[PreviewTranscript] {
        &self.transcript
    }

    #[must_use]
    pub fn pattern_draws(&self) -> &[DrawResult] {
        &self.pattern_draws
    }

    #[must_use]
    pub fn issue(&self) -> Option<&PreviewIssue> {
        self.issue.as_ref()
    }

    #[must_use]
    pub fn choices(&self) -> Vec<ChoiceView> {
        self.runtime.as_ref().map_or_else(Vec::new, Story::choices)
    }

    #[must_use]
    pub fn knots(&self) -> Vec<String> {
        self.last_valid_story
            .as_ref()
            .map_or_else(Vec::new, |story| story.knots.keys().cloned().collect())
    }

    #[must_use]
    pub fn variables(&self) -> Vec<(String, Value)> {
        self.runtime.as_ref().map_or_else(Vec::new, |runtime| {
            runtime
                .state()
                .variables()
                .iter()
                .map(|(name, state)| (name.clone(), state.value.clone()))
                .collect()
        })
    }

    /// Compile current source. Failures preserve the complete last valid runtime and transcript.
    pub fn compile_source(
        &mut self,
        source: &str,
        source_name: Option<String>,
        revision: u64,
    ) -> bool {
        match compile(source, &CompileOptions { source_name }) {
            Ok(compiled) => {
                self.install_story(compiled.story, revision);
                true
            }
            Err(error) => {
                self.stale = self.runtime.is_some();
                self.issue = Some(issue_from_diagnostics(&error.diagnostics));
                if self.runtime.is_none() {
                    self.status = PreviewStatus::Error;
                }
                false
            }
        }
    }

    pub fn step(&mut self) -> bool {
        if self.status == PreviewStatus::Stopped {
            return false;
        }
        let Some(mut runtime) = self.runtime.take() else {
            self.fail("No valid compiled story is available", None);
            return false;
        };
        let event = runtime.advance();
        self.pattern_draws.extend(runtime.take_pattern_draws());
        self.runtime = Some(runtime);
        match event {
            Ok(StoryEvent::Line(line)) => {
                self.transcript.push(PreviewTranscript::Line(line));
                self.status = PreviewStatus::Ready;
                if !self.stale {
                    self.issue = None;
                }
                true
            }
            Ok(StoryEvent::Choices(_)) => {
                self.status = PreviewStatus::WaitingForChoice;
                true
            }
            Ok(StoryEvent::Ended) => {
                self.status = PreviewStatus::Ended;
                true
            }
            Err(error) => {
                self.fail(error.to_string(), error.span);
                false
            }
        }
    }

    /// Continue through narrative lines until a choice, end, error, or safety boundary.
    pub fn run(&mut self) -> bool {
        if self.status == PreviewStatus::Stopped {
            self.status = PreviewStatus::Ready;
        }
        self.status = PreviewStatus::Running;
        for _ in 0..MAX_CONTINUE_BOUNDARIES {
            if self
                .runtime
                .as_ref()
                .is_some_and(|runtime| runtime.has_choices())
            {
                self.status = PreviewStatus::WaitingForChoice;
                return true;
            }
            if !self.step() {
                return false;
            }
            if matches!(
                self.status,
                PreviewStatus::WaitingForChoice | PreviewStatus::Ended | PreviewStatus::Error
            ) {
                return true;
            }
            self.status = PreviewStatus::Running;
        }
        self.fail(
            format!("Preview stopped after {MAX_CONTINUE_BOUNDARIES} output boundaries"),
            None,
        );
        false
    }

    pub fn choose(&mut self, index: usize) -> bool {
        let choice = self.choices().get(index).cloned();
        let Some(runtime) = self.runtime.as_mut() else {
            self.fail("No valid compiled story is available", None);
            return false;
        };
        match runtime.choose(index) {
            Ok(()) => {
                if let Some(choice) = choice {
                    self.transcript.push(PreviewTranscript::Choice(choice.text));
                }
                self.status = PreviewStatus::Ready;
                self.run()
            }
            Err(error) => {
                let span = error.span;
                self.fail(error.to_string(), span);
                false
            }
        }
    }

    pub fn restart(&mut self) -> bool {
        let Some(story) = self.last_valid_story.clone() else {
            self.fail("No valid compiled story is available", None);
            return false;
        };
        let stale = self.stale;
        let issue = self.issue.clone();
        let installed = self.install_story(story, self.compiled_revision.unwrap_or_default());
        if installed && stale {
            self.stale = true;
            self.issue = issue;
        }
        installed
    }

    /// Restart global state, then begin playback at one named knot.
    pub fn jump(&mut self, knot: &str) -> bool {
        let Some(story) = self.last_valid_story.clone() else {
            self.fail("No valid compiled story is available", None);
            return false;
        };
        let stale = self.stale;
        let stale_issue = self.issue.clone();
        let mut runtime = match Story::with_seed(story, self.seed) {
            Ok(runtime) => runtime,
            Err(error) => {
                let span = error.span;
                self.fail(error.to_string(), span);
                return false;
            }
        };
        if let Err(error) = runtime.jump(knot) {
            let span = error.span;
            self.fail(error.to_string(), span);
            return false;
        }
        self.transcript.clear();
        self.transcript
            .push(PreviewTranscript::Jump(knot.to_owned()));
        self.pattern_draws = runtime.take_pattern_draws();
        self.runtime = Some(runtime);
        self.status = PreviewStatus::Ready;
        self.stale = stale;
        self.issue = if stale { stale_issue } else { None };
        self.run()
    }

    pub fn stop(&mut self) {
        self.status = PreviewStatus::Stopped;
    }

    pub fn change_seed(&mut self, delta: i64) -> bool {
        self.seed = self.seed.saturating_add_signed(delta);
        self.restart()
    }

    fn install_story(&mut self, story: StoryIr, revision: u64) -> bool {
        match Story::with_seed(story.clone(), self.seed) {
            Ok(mut runtime) => {
                self.pattern_draws = runtime.take_pattern_draws();
                self.last_valid_story = Some(story);
                self.runtime = Some(runtime);
                self.compiled_revision = Some(revision);
                self.stale = false;
                self.status = PreviewStatus::Ready;
                self.transcript.clear();
                self.issue = None;
                true
            }
            Err(error) => {
                self.stale = self.runtime.is_some();
                let span = error.span;
                self.fail(error.to_string(), span);
                false
            }
        }
    }

    fn fail(&mut self, message: impl Into<String>, span: Option<Span>) {
        self.issue = Some(PreviewIssue {
            message: message.into(),
            span,
        });
        self.status = PreviewStatus::Error;
    }
}

/// Cross-panel event emitted by actionable preview diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewEvent {
    RevealSource(Span),
}

/// Native live-play surface with a debounced pending source compiler.
pub struct PreviewSurface {
    session: PreviewSession,
    debouncer: PreviewCompileDebouncer,
    pending: Option<PendingCompile>,
}

impl PreviewSurface {
    pub fn new(story: Option<StoryIr>, seed: u64, revision: u64, _cx: &mut Context<Self>) -> Self {
        Self {
            session: story.map_or_else(
                || PreviewSession::empty(seed),
                |story| PreviewSession::from_story(story, seed, revision),
            ),
            debouncer: PreviewCompileDebouncer::new(Duration::from_millis(250)),
            pending: None,
        }
    }

    #[must_use]
    pub const fn session(&self) -> &PreviewSession {
        &self.session
    }

    pub fn schedule_compile(
        &mut self,
        source: String,
        source_name: Option<String>,
        revision: u64,
        now: Instant,
        cx: &mut Context<Self>,
    ) {
        self.pending = Some(PendingCompile {
            source,
            source_name,
            revision,
        });
        self.debouncer.schedule(revision, now);
        cx.notify();
    }

    pub fn poll_compile(&mut self, now: Instant, cx: &mut Context<Self>) -> bool {
        let Some(revision) = self.debouncer.take_ready(now) else {
            return false;
        };
        let Some(pending) = self.pending.take() else {
            return false;
        };
        if pending.revision != revision {
            return false;
        }
        let compiled =
            self.session
                .compile_source(&pending.source, pending.source_name, pending.revision);
        cx.notify();
        compiled
    }

    pub fn compile_now(
        &mut self,
        source: &str,
        source_name: Option<String>,
        revision: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        self.pending = None;
        self.debouncer.cancel();
        let compiled = self.session.compile_source(source, source_name, revision);
        cx.notify();
        compiled
    }

    pub fn run(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.session.run();
        cx.notify();
        changed
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.session.stop();
        cx.notify();
    }

    fn step(&mut self, cx: &mut Context<Self>) {
        self.session.step();
        cx.notify();
    }

    fn restart(&mut self, cx: &mut Context<Self>) {
        self.session.restart();
        cx.notify();
    }

    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        self.session.choose(index);
        cx.notify();
    }

    fn jump(&mut self, knot: &str, cx: &mut Context<Self>) {
        self.session.jump(knot);
        cx.notify();
    }

    fn change_seed(&mut self, delta: i64, cx: &mut Context<Self>) {
        self.session.change_seed(delta);
        cx.notify();
    }

    fn transcript_panel(&self) -> AnyElement {
        let mut transcript = div()
            .id("preview-transcript-scroll")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .pr_3();
        if self.session.transcript().is_empty() {
            transcript = transcript.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("Press Continue to play the current compiled story."),
            );
        }
        for (index, entry) in self.session.transcript().iter().enumerate() {
            let (prefix, text, color) = match entry {
                PreviewTranscript::Line(line) => ("", line.as_str(), DARK_THEME.text),
                PreviewTranscript::Choice(choice) => ("› ", choice.as_str(), DARK_THEME.accent),
                PreviewTranscript::Jump(knot) => ("↪ ", knot.as_str(), DARK_THEME.warning),
            };
            transcript = transcript.child(
                div()
                    .id(("preview-transcript", index))
                    .text_sm()
                    .text_color(rgb(color))
                    .child(format!("{prefix}{text}")),
            );
        }
        transcript.into_any_element()
    }

    fn controls_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let stale = if self.session.is_stale() {
            " · showing last valid build"
        } else {
            ""
        };
        let mut panel = div()
            .w(px(310.0))
            .h_full()
            .flex_none()
            .px_3()
            .border_l_1()
            .border_color(rgb(DARK_THEME.border))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(format!("{}{}", self.session.status().label(), stale)),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(preview_button("Step", "preview-step", cx, |this, cx| {
                        this.step(cx);
                    }))
                    .child(preview_button(
                        "Continue",
                        "preview-continue",
                        cx,
                        |this, cx| {
                            this.run(cx);
                        },
                    ))
                    .child(preview_button(
                        "Restart",
                        "preview-restart",
                        cx,
                        |this, cx| {
                            this.restart(cx);
                        },
                    ))
                    .child(preview_button("Stop", "preview-stop", cx, |this, cx| {
                        this.stop(cx);
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(preview_button(
                        "Seed −",
                        "preview-seed-down",
                        cx,
                        |this, cx| {
                            this.change_seed(-1, cx);
                        },
                    ))
                    .child(
                        div()
                            .px_2()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(self.session.seed().to_string()),
                    )
                    .child(preview_button(
                        "Seed +",
                        "preview-seed-up",
                        cx,
                        |this, cx| {
                            this.change_seed(1, cx);
                        },
                    )),
            );

        for (index, choice) in self.session.choices().into_iter().enumerate() {
            panel = panel.child(
                div()
                    .id(("preview-choice", index))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.accent))
                    .cursor_pointer()
                    .text_xs()
                    .child(choice.text)
                    .on_click(cx.listener(move |this, _, _, cx| this.choose(index, cx))),
            );
        }

        if let Some(issue) = self.session.issue() {
            let span = issue.span;
            panel = panel.child(
                div()
                    .id("preview-issue")
                    .p_2()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.error))
                    .cursor_pointer()
                    .text_xs()
                    .child(issue.message.clone())
                    .when_some(span, |issue, span| {
                        issue.on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(PreviewEvent::RevealSource(span));
                        }))
                    }),
            );
        }

        let mut knots = div()
            .id("preview-knots-scroll")
            .max_h(px(64.0))
            .overflow_y_scroll();
        for (index, knot) in self.session.knots().into_iter().enumerate() {
            let target = knot.clone();
            knots = knots.child(
                div()
                    .id(("preview-knot", index))
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.accent))
                    .child(format!("Replay {knot}"))
                    .on_click(cx.listener(move |this, _, _, cx| this.jump(&target, cx))),
            );
        }
        panel.child(knots).into_any_element()
    }

    fn state_panel(&self) -> AnyElement {
        let mut state = div()
            .id("preview-state-scroll")
            .w(px(310.0))
            .h_full()
            .flex_none()
            .pl_3()
            .border_l_1()
            .border_color(rgb(DARK_THEME.border))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().child("State"));
        let variables = self.session.variables();
        if variables.is_empty() {
            state = state.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("No variables"),
            );
        }
        for (name, value) in variables {
            state = state.child(div().text_xs().child(format!("{name} = {value}")));
        }
        if !self.session.pattern_draws().is_empty() {
            state = state.child(
                div()
                    .mt_2()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("PATTERN DRAWS"),
            );
        }
        for draw in self.session.pattern_draws().iter().rev().take(12) {
            state = state.child(pattern_draw_summary(draw));
        }
        state.into_any_element()
    }
}

impl EventEmitter<PreviewEvent> for PreviewSurface {}

impl gpui::Render for PreviewSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("play-preview")
            .size_full()
            .p_3()
            .flex()
            .gap_3()
            .bg(rgb(DARK_THEME.panel))
            .border_t_1()
            .border_color(rgb(DARK_THEME.border))
            .child(self.transcript_panel())
            .child(self.controls_panel(cx))
            .child(self.state_panel())
    }
}

fn preview_button(
    label: &str,
    id: &'static str,
    cx: &mut Context<PreviewSurface>,
    action: impl Fn(&mut PreviewSurface, &mut Context<PreviewSurface>) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgb(DARK_THEME.chrome))
        .cursor_pointer()
        .text_xs()
        .child(label.to_owned())
        .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
        .into_any_element()
}

fn pattern_draw_summary(draw: &DrawResult) -> AnyElement {
    let entries = draw
        .entries
        .iter()
        .map(|entry| {
            let name = entry.name().unwrap_or(&entry.id);
            entry
                .position
                .as_ref()
                .map_or_else(|| name.to_owned(), |position| format!("{position}: {name}"))
        })
        .collect::<Vec<_>>()
        .join(" · ");
    div()
        .p_2()
        .rounded_md()
        .bg(rgb(DARK_THEME.chrome))
        .child(
            div()
                .text_xs()
                .text_color(rgb(DARK_THEME.accent))
                .child(draw.system.clone()),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child(entries),
        )
        .into_any_element()
}

fn issue_from_diagnostics(diagnostics: &[Diagnostic]) -> PreviewIssue {
    let first = diagnostics.first();
    PreviewIssue {
        message: first.map_or_else(
            || "Compilation failed without a diagnostic".to_owned(),
            |diagnostic| {
                format!(
                    "{}: {}{}",
                    diagnostic.code,
                    diagnostic.message,
                    if diagnostics.len() > 1 {
                        format!(" (+{} more)", diagnostics.len() - 1)
                    } else {
                        String::new()
                    }
                )
            },
        ),
        span: first.and_then(|diagnostic| diagnostic.span),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STORY: &str = r#"grammar names {
    value: ["Mira", "Aldric"]
}
pattern runes {
    builtin: elder_futhark
    draw: uniform
    reversals: false
}
VAR score = 0
VAR omen = runes.draw()
=== start ===
Hello, #names.value#.
* [Continue]
    SET score = 1
    -> finish
=== finish ===
Done.
-> END
"#;

    fn session(seed: u64) -> PreviewSession {
        let compiled = compile(STORY, &CompileOptions::default()).expect("story compiles");
        PreviewSession::from_story(compiled.story, seed, 1)
    }

    #[test]
    fn playback_steps_chooses_restarts_and_exposes_state() {
        let mut preview = session(7);
        assert!(preview.run());
        assert_eq!(preview.status(), PreviewStatus::WaitingForChoice);
        assert_eq!(preview.choices().len(), 1);
        assert!(preview.choose(0));
        assert_eq!(preview.status(), PreviewStatus::Ended);
        assert!(
            preview
                .variables()
                .iter()
                .any(|(name, value)| name == "score" && *value == Value::Number(1.0))
        );
        assert!(!preview.pattern_draws().is_empty());
        assert!(preview.restart());
        assert!(preview.transcript().is_empty());
        assert!(preview.jump("finish"));
        assert!(
            preview
                .transcript()
                .iter()
                .any(|entry| matches!(entry, PreviewTranscript::Jump(knot) if knot == "finish"))
        );
    }

    #[test]
    fn seed_replays_are_deterministic() {
        let mut first = session(42);
        first.run();
        let first_transcript = first.transcript().to_vec();
        let first_draws = first.pattern_draws().to_vec();
        first.restart();
        first.run();
        assert_eq!(first.transcript(), first_transcript);
        assert_eq!(first.pattern_draws(), first_draws);
    }

    #[test]
    fn compiler_failure_preserves_last_valid_preview() {
        let mut preview = session(5);
        preview.run();
        let transcript = preview.transcript().to_vec();
        let revision = preview.compiled_revision();
        assert!(!preview.compile_source("=== broken", None, 2));
        assert!(preview.is_stale());
        assert_eq!(preview.compiled_revision(), revision);
        assert_eq!(preview.transcript(), transcript);
        assert!(preview.issue().is_some());
    }

    #[test]
    fn runtime_failure_is_actionable_without_crashing_the_session() {
        let mut preview = session(5);
        let invalid_runtime =
            "VAR score = 0\nVAR divisor = 0\n=== start ===\nSET score = 1 / divisor\n-> END\n";
        assert!(preview.compile_source(invalid_runtime, None, 2));
        assert!(!preview.run());
        assert_eq!(preview.status(), PreviewStatus::Error);
        assert!(
            preview
                .issue()
                .is_some_and(|issue| issue.message.contains("division"))
        );
    }

    #[test]
    fn compile_debounce_uses_the_latest_revision_and_quiet_period() {
        let start = Instant::now();
        let mut debounce = PreviewCompileDebouncer::new(Duration::from_millis(250));
        debounce.schedule(1, start);
        assert_eq!(
            debounce.take_ready(start + Duration::from_millis(249)),
            None
        );
        debounce.schedule(2, start + Duration::from_millis(100));
        assert_eq!(
            debounce.take_ready(start + Duration::from_millis(349)),
            None
        );
        assert_eq!(
            debounce.take_ready(start + Duration::from_millis(350)),
            Some(2)
        );
    }
}
