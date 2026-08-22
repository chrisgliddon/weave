//! Deterministic standalone execution of compiled Weave stories.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use weave_core::Span;
use weave_core::ir::{
    BinaryOperatorIr, ChoiceIr, ConditionalIr, DeclarationIr, DomainExportSourceIr, DomainModuleIr,
    DomainValueIr, Expression, IR_VERSION, Instruction, InstructionKind, ListOperationIr, StoryIr,
    Template, TemplatePartIr, UnaryOperatorIr, ValueLiteral, VariableKindIr,
};
use weave_patterns::{
    DrawRequest, DrawResult, PatternState, PatternSystem, PatternValue, SeededRandom,
    system_from_ir,
};

/// Runtime version corresponding to the serialized IR version.
pub const RUNTIME_VERSION: u32 = IR_VERSION;
const MAX_GRAMMAR_DEPTH: usize = 64;
const MAX_THREAD_DEPTH: usize = 256;

/// A dynamically typed story value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Value {
    /// Null.
    Null,
    /// Boolean.
    Bool(bool),
    /// Finite number.
    Number(f64),
    /// UTF-8 text.
    String(String),
    /// Semantic symbol.
    Symbol(String),
    /// Ordered values.
    List(Vec<Value>),
    /// Deterministically ordered fields.
    Object(BTreeMap<String, Value>),
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => formatter.write_str("null"),
            Self::Bool(value) => value.fmt(formatter),
            Self::Number(value) => {
                if value.fract() == 0.0 {
                    write!(formatter, "{value:.0}")
                } else {
                    value.fmt(formatter)
                }
            }
            Self::String(value) | Self::Symbol(value) => formatter.write_str(value),
            Self::List(values) => {
                formatter.write_str("[")?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str(", ")?;
                    }
                    value.fmt(formatter)?;
                }
                formatter.write_str("]")
            }
            Self::Object(fields) => {
                formatter.write_str("{")?;
                for (index, (name, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str(", ")?;
                    }
                    write!(formatter, "{name}: {value}")?;
                }
                formatter.write_str("}")
            }
        }
    }
}

/// Observable state for one declared value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableState {
    /// Declaration kind used for runtime validation.
    pub kind: VariableKindIr,
    /// Current value.
    pub value: Value,
    /// Allowed values for a state machine.
    pub allowed_states: Vec<String>,
}

/// Serializable mutable state owned by one exact domain-module release.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainModuleState {
    /// Exact module semantic version that owns these values.
    pub version: String,
    /// Mutable exports in deterministic name order.
    pub values: BTreeMap<String, Value>,
}

/// Serializable mutable state, kept separate from immutable [`StoryIr`] data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoryState {
    variables: BTreeMap<String, VariableState>,
    patterns: BTreeMap<String, PatternState>,
    modules: BTreeMap<String, DomainModuleState>,
    selected_once: BTreeSet<String>,
    frames: Vec<ExecutionFrame>,
    pending_choices: Vec<PendingChoice>,
    seed: u64,
    random_draws: u64,
    ended: bool,
}

impl StoryState {
    /// Current story variables in deterministic name order.
    #[must_use]
    pub const fn variables(&self) -> &BTreeMap<String, VariableState> {
        &self.variables
    }

    /// Mutable draw state for each declared pattern system.
    #[must_use]
    pub const fn patterns(&self) -> &BTreeMap<String, PatternState> {
        &self.patterns
    }

    /// Isolated mutable domain-module state keyed by stable module identity.
    #[must_use]
    pub const fn modules(&self) -> &BTreeMap<String, DomainModuleState> {
        &self.modules
    }

    /// Original deterministic random seed.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Whether execution has ended.
    #[must_use]
    pub const fn is_ended(&self) -> bool {
        self.ended
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BlockRef {
    knot: String,
    segments: Vec<BlockSegment>,
}

impl BlockRef {
    fn knot(name: impl Into<String>) -> Self {
        Self {
            knot: name.into(),
            segments: Vec::new(),
        }
    }

    fn child(&self, segment: BlockSegment) -> Self {
        let mut child = self.clone();
        child.segments.push(segment);
        child
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum BlockSegment {
    Choice { instruction: usize },
    ConditionalBranch { instruction: usize, branch: usize },
    ConditionalFallback { instruction: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ExecutionFrame {
    block: BlockRef,
    index: usize,
    on_complete: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PendingChoice {
    reference: BlockRef,
    instruction: usize,
    view: ChoiceView,
}

/// One currently eligible choice exposed to a host application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceView {
    /// Stable compiler-generated choice identifier.
    pub id: String,
    /// Rendered label.
    pub text: String,
    /// Whether the choice disappears after it is selected.
    pub once: bool,
}

/// One host-visible result of advancing a story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryEvent {
    /// One rendered narrative line.
    Line(String),
    /// Execution is waiting for one of these choices.
    Choices(Vec<ChoiceView>),
    /// Story execution is complete.
    Ended,
}

/// Structured failure category for malformed IR or invalid runtime behavior.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeErrorKind {
    /// The serialized IR version is unsupported.
    #[error("unsupported story IR version {found}; expected {expected}")]
    UnsupportedVersion { found: u32, expected: u32 },
    /// RON input could not be decoded.
    #[error("invalid story RON: {0}")]
    InvalidRon(String),
    /// File input/output failed.
    #[error("story I/O failed: {0}")]
    Io(String),
    /// Compiled control-flow data is invalid.
    #[error("invalid compiled story: {0}")]
    InvalidStory(String),
    /// A referenced knot does not exist.
    #[error("unknown knot `{0}`")]
    UnknownKnot(String),
    /// A referenced grammar or rule does not exist.
    #[error("unknown grammar rule `{grammar}.{rule}`")]
    UnknownGrammarRule { grammar: String, rule: String },
    /// A variable or field path cannot be resolved.
    #[error("unknown value path `{0}`")]
    UnknownPath(String),
    /// An operation received the wrong concrete value type.
    #[error("type error: {0}")]
    Type(String),
    /// Numeric division or remainder by zero.
    #[error("division or remainder by zero")]
    DivideByZero,
    /// Grammar expansion exceeded the recursion limit.
    #[error("grammar expansion exceeded {MAX_GRAMMAR_DEPTH} levels")]
    GrammarDepth,
    /// Thread execution exceeded the call-frame limit.
    #[error("thread execution exceeded {MAX_THREAD_DEPTH} frames")]
    ThreadDepth,
    /// The host selected an unavailable choice.
    #[error("choice index {index} is unavailable; {available} choices are pending")]
    InvalidChoice { index: usize, available: usize },
    /// Pattern definition or execution failed.
    #[error("pattern `{system}` failed: {message}")]
    Pattern {
        /// Declared pattern identity.
        system: String,
        /// Structured pattern-layer error rendered for host diagnostics.
        message: String,
    },
    /// A state-machine transition violates its declaration.
    #[error("state `{name}` does not allow `{value}`")]
    InvalidState { name: String, value: String },
}

/// Runtime failure with an optional author-facing source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    /// Failure category.
    pub kind: RuntimeErrorKind,
    /// Instruction that caused the failure, when available.
    pub span: Option<Span>,
}

impl RuntimeError {
    fn new(kind: RuntimeErrorKind) -> Self {
        Self { kind, span: None }
    }

    fn at(kind: RuntimeErrorKind, span: Span) -> Self {
        Self {
            kind,
            span: Some(span),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(formatter)?;
        if let Some(span) = self.span {
            write!(formatter, " at {}:{}", span.line, span.column)?;
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeError {}

/// A running story over immutable compiled data and isolated mutable state.
#[derive(Debug, Clone)]
pub struct Story {
    data: Arc<StoryIr>,
    patterns: BTreeMap<String, Arc<dyn PatternSystem>>,
    state: StoryState,
    pending_pattern_draws: Vec<DrawResult>,
}

impl Story {
    /// Start a compiled story with deterministic seed zero.
    pub fn new(data: StoryIr) -> Result<Self, RuntimeError> {
        Self::with_seed(data, 0)
    }

    /// Start a compiled story with an explicit deterministic seed.
    pub fn with_seed(data: StoryIr, seed: u64) -> Result<Self, RuntimeError> {
        validate_story(&data)?;
        let patterns = build_patterns(&data)?;
        let entry = data.entry.clone();
        let modules = initial_domain_state(&data);
        let mut story = Self {
            data: Arc::new(data),
            state: StoryState {
                patterns: patterns
                    .keys()
                    .map(|name| (name.clone(), PatternState::default()))
                    .collect(),
                variables: BTreeMap::new(),
                modules,
                selected_once: BTreeSet::new(),
                frames: vec![ExecutionFrame {
                    block: BlockRef::knot(entry),
                    index: 0,
                    on_complete: None,
                }],
                pending_choices: Vec::new(),
                seed,
                random_draws: 0,
                ended: false,
            },
            patterns,
            pending_pattern_draws: Vec::new(),
        };
        story.execute_globals()?;
        Ok(story)
    }

    /// Restore mutable state against the same immutable compiled story.
    pub fn restore(data: StoryIr, mut state: StoryState) -> Result<Self, RuntimeError> {
        validate_story(&data)?;
        let patterns = build_patterns(&data)?;
        reconcile_pattern_state(&patterns, &mut state.patterns);
        reconcile_domain_state(&data, &mut state.modules);
        let story = Self {
            data: Arc::new(data),
            patterns,
            state,
            pending_pattern_draws: Vec::new(),
        };
        story.validate_state()?;
        Ok(story)
    }

    /// Decode and start a compiled RON story.
    pub fn from_ron(source: &str) -> Result<Self, RuntimeError> {
        let data = ron::from_str::<StoryIr>(source)
            .map_err(|error| RuntimeError::new(RuntimeErrorKind::InvalidRon(error.to_string())))?;
        Self::new(data)
    }

    /// Load and start a compiled RON story from disk.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let source = fs::read_to_string(path.as_ref())
            .map_err(|error| RuntimeError::new(RuntimeErrorKind::Io(error.to_string())))?;
        Self::from_ron(&source)
    }

    /// Immutable compiled story data.
    #[must_use]
    pub fn data(&self) -> &StoryIr {
        &self.data
    }

    /// Read one active module export by alias and nested field path.
    ///
    /// An empty path returns an object containing every export. State-backed exports come from
    /// the isolated save state; pack-backed exports remain immutable IR data.
    #[must_use]
    pub fn module_value(&self, alias: &str, path: &[&str]) -> Option<Value> {
        let module = self.data.modules.get(alias)?;
        self.module_value_from(module, path)
    }

    /// Saveable mutable execution state.
    #[must_use]
    pub const fn state(&self) -> &StoryState {
        &self.state
    }

    /// Whether execution can advance without a host choice.
    #[must_use]
    pub fn can_continue(&self) -> bool {
        !self.state.ended && self.state.pending_choices.is_empty()
    }

    /// Whether execution is waiting for a choice.
    #[must_use]
    pub fn has_choices(&self) -> bool {
        !self.state.pending_choices.is_empty()
    }

    /// Currently eligible rendered choices.
    #[must_use]
    pub fn choices(&self) -> Vec<ChoiceView> {
        self.state
            .pending_choices
            .iter()
            .map(|choice| choice.view.clone())
            .collect()
    }

    /// Drain pattern draws produced since the previous call in narrative evaluation order.
    #[must_use]
    pub fn take_pattern_draws(&mut self) -> Vec<DrawResult> {
        std::mem::take(&mut self.pending_pattern_draws)
    }

    /// Advance until a line, choice set, or end boundary is reached.
    pub fn advance(&mut self) -> Result<StoryEvent, RuntimeError> {
        if self.state.ended {
            return Ok(StoryEvent::Ended);
        }
        if !self.state.pending_choices.is_empty() {
            return Ok(StoryEvent::Choices(self.choices()));
        }

        loop {
            let Some((block, index)) = self.next_position()? else {
                return Ok(StoryEvent::Ended);
            };
            let instruction = self.instruction(&block, index)?.clone();
            self.increment_frame()?;
            let span = instruction.span;
            match instruction.kind {
                InstructionKind::Text(template) => {
                    return self
                        .render_template(&template, 0)
                        .map(StoryEvent::Line)
                        .map_err(|error| with_span(error, span));
                }
                InstructionKind::Declare(declaration) => {
                    self.execute_declaration(&declaration)
                        .map_err(|error| with_span(error, span))?;
                }
                InstructionKind::Assign { name, value } => {
                    let value = self
                        .evaluate(&value)
                        .map_err(|error| with_span(error, span))?;
                    self.assign(&name, value)
                        .map_err(|error| with_span(error, span))?;
                }
                InstructionKind::MutateList {
                    operation,
                    name,
                    value,
                } => {
                    let value = self
                        .evaluate(&value)
                        .map_err(|error| with_span(error, span))?;
                    self.mutate_list(operation, &name, value)
                        .map_err(|error| with_span(error, span))?;
                }
                InstructionKind::Choice(_) => {
                    if let Some(event) = self
                        .prepare_choices(block, index)
                        .map_err(|error| with_span(error, span))?
                    {
                        return Ok(event);
                    }
                }
                InstructionKind::Conditional(conditional) => {
                    self.enter_conditional(&block, index, &conditional)
                        .map_err(|error| with_span(error, span))?;
                }
                InstructionKind::Divert(target) => self.jump(&target)?,
                InstructionKind::Thread(target) => self.thread(&target, span)?,
                InstructionKind::End => {
                    self.end();
                    return Ok(StoryEvent::Ended);
                }
            }
        }
    }

    /// Advance and require the next boundary to be a narrative line.
    pub fn continue_line(&mut self) -> Result<Option<String>, RuntimeError> {
        match self.advance()? {
            StoryEvent::Line(line) => Ok(Some(line)),
            StoryEvent::Choices(_) | StoryEvent::Ended => Ok(None),
        }
    }

    /// Select one currently pending choice by display index.
    pub fn choose(&mut self, index: usize) -> Result<(), RuntimeError> {
        let available = self.state.pending_choices.len();
        let Some(selected) = self.state.pending_choices.get(index).cloned() else {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidChoice {
                index,
                available,
            }));
        };
        let instruction = self
            .instruction(&selected.reference, selected.instruction)?
            .clone();
        let InstructionKind::Choice(choice) = instruction.kind else {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                "saved choice points to a non-choice instruction".to_owned(),
            )));
        };
        self.state.pending_choices.clear();
        if choice.once {
            self.state.selected_once.insert(choice.id.clone());
        }
        if choice.body.is_empty() {
            if let Some(target) = choice.divert {
                self.jump(&target)?;
            }
        } else {
            self.state.frames.push(ExecutionFrame {
                block: selected.reference.child(BlockSegment::Choice {
                    instruction: selected.instruction,
                }),
                index: 0,
                on_complete: choice.divert,
            });
        }
        Ok(())
    }

    /// Replace the current flow stack with a named knot.
    pub fn jump(&mut self, target: &str) -> Result<(), RuntimeError> {
        if target == "END" {
            self.end();
            return Ok(());
        }
        if !self.data.knots.contains_key(target) {
            return Err(RuntimeError::new(RuntimeErrorKind::UnknownKnot(
                target.to_owned(),
            )));
        }
        self.state.frames.clear();
        self.state.frames.push(ExecutionFrame {
            block: BlockRef::knot(target),
            index: 0,
            on_complete: None,
        });
        self.state.pending_choices.clear();
        self.state.ended = false;
        Ok(())
    }

    fn execute_globals(&mut self) -> Result<(), RuntimeError> {
        for instruction in self.data.globals.clone() {
            match instruction.kind {
                InstructionKind::Declare(declaration) => self
                    .execute_declaration(&declaration)
                    .map_err(|error| with_span(error, instruction.span))?,
                _ => {
                    return Err(RuntimeError::at(
                        RuntimeErrorKind::InvalidStory(
                            "global content must contain declarations only".to_owned(),
                        ),
                        instruction.span,
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_state(&self) -> Result<(), RuntimeError> {
        for frame in &self.state.frames {
            let block = self.block(&frame.block)?;
            if frame.index > block.len() {
                return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                    "saved execution cursor is out of range".to_owned(),
                )));
            }
        }
        for pending in &self.state.pending_choices {
            if !matches!(
                self.instruction(&pending.reference, pending.instruction)?
                    .kind,
                InstructionKind::Choice(_)
            ) {
                return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                    "saved choice points to a non-choice instruction".to_owned(),
                )));
            }
        }
        if self.state.patterns.len() != self.patterns.len()
            || self
                .patterns
                .keys()
                .any(|name| !self.state.patterns.contains_key(name))
        {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                "saved pattern state does not match the compiled story".to_owned(),
            )));
        }
        for (name, state) in &self.state.patterns {
            let Some(system) = self.patterns.get(name) else {
                return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                    "saved pattern state references an unknown system".to_owned(),
                )));
            };
            if state.last_draw.iter().any(|id| {
                !system
                    .definition()
                    .elements
                    .iter()
                    .any(|element| &element.id == id)
            }) {
                return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(format!(
                    "saved pattern state for `{name}` references stale elements"
                ))));
            }
        }
        if self.state.modules.len() != self.data.modules.len() {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                "saved domain state does not match the compiled story".to_owned(),
            )));
        }
        for module in self.data.modules.values() {
            let Some(state) = self.state.modules.get(&module.id) else {
                return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                    "saved domain state is missing an active module".to_owned(),
                )));
            };
            if state.version != module.version {
                return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                    "saved domain state belongs to a different module version".to_owned(),
                )));
            }
            let expected = module
                .exports
                .iter()
                .filter(|(_, export)| export.source == DomainExportSourceIr::State)
                .collect::<BTreeMap<_, _>>();
            if state.values.len() != expected.len() {
                return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                    "saved domain state has stale exports".to_owned(),
                )));
            }
            for (name, export) in expected {
                let Some(value) = state.values.get(name) else {
                    return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                        "saved domain state is missing an export".to_owned(),
                    )));
                };
                if !same_value_type(value, &domain_value(&export.value)) {
                    return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                        "saved domain export has an incompatible value type".to_owned(),
                    )));
                }
            }
        }
        Ok(())
    }

    fn next_position(&mut self) -> Result<Option<(BlockRef, usize)>, RuntimeError> {
        loop {
            let Some(frame) = self.state.frames.last().cloned() else {
                self.state.ended = true;
                return Ok(None);
            };
            let length = self.block(&frame.block)?.len();
            if frame.index < length {
                return Ok(Some((frame.block, frame.index)));
            }
            let completed = self.state.frames.pop().ok_or_else(|| {
                RuntimeError::new(RuntimeErrorKind::InvalidStory(
                    "execution stack changed unexpectedly".to_owned(),
                ))
            })?;
            if let Some(target) = completed.on_complete {
                self.jump(&target)?;
            } else if self.state.frames.is_empty() {
                self.state.ended = true;
                return Ok(None);
            }
        }
    }

    fn increment_frame(&mut self) -> Result<(), RuntimeError> {
        let Some(frame) = self.state.frames.last_mut() else {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                "execution has no active frame".to_owned(),
            )));
        };
        frame.index = frame.index.saturating_add(1);
        Ok(())
    }

    fn block(&self, reference: &BlockRef) -> Result<&[Instruction], RuntimeError> {
        let knot = self.data.knots.get(&reference.knot).ok_or_else(|| {
            RuntimeError::new(RuntimeErrorKind::UnknownKnot(reference.knot.clone()))
        })?;
        let mut block = knot.content.as_slice();
        for segment in &reference.segments {
            block = match *segment {
                BlockSegment::Choice { instruction } => {
                    let Some(Instruction {
                        kind: InstructionKind::Choice(choice),
                        ..
                    }) = block.get(instruction)
                    else {
                        return Err(invalid_block_reference());
                    };
                    &choice.body
                }
                BlockSegment::ConditionalBranch {
                    instruction,
                    branch,
                } => {
                    let Some(Instruction {
                        kind: InstructionKind::Conditional(conditional),
                        ..
                    }) = block.get(instruction)
                    else {
                        return Err(invalid_block_reference());
                    };
                    conditional
                        .branches
                        .get(branch)
                        .map(|branch| branch.body.as_slice())
                        .ok_or_else(invalid_block_reference)?
                }
                BlockSegment::ConditionalFallback { instruction } => {
                    let Some(Instruction {
                        kind: InstructionKind::Conditional(conditional),
                        ..
                    }) = block.get(instruction)
                    else {
                        return Err(invalid_block_reference());
                    };
                    conditional
                        .fallback
                        .as_deref()
                        .ok_or_else(invalid_block_reference)?
                }
            };
        }
        Ok(block)
    }

    fn instruction(
        &self,
        reference: &BlockRef,
        index: usize,
    ) -> Result<&Instruction, RuntimeError> {
        self.block(reference)?
            .get(index)
            .ok_or_else(invalid_block_reference)
    }

    fn prepare_choices(
        &mut self,
        block: BlockRef,
        first_index: usize,
    ) -> Result<Option<StoryEvent>, RuntimeError> {
        let mut index = first_index;
        let mut choices = Vec::new();
        while let Some(instruction) = self.block(&block)?.get(index).cloned() {
            let InstructionKind::Choice(choice) = instruction.kind else {
                break;
            };
            if index > first_index {
                self.increment_frame()?;
            }
            if self.choice_is_eligible(&choice)? {
                let text = self.render_template(&choice.text, 0)?;
                choices.push(PendingChoice {
                    reference: block.clone(),
                    instruction: index,
                    view: ChoiceView {
                        id: choice.id,
                        text,
                        once: choice.once,
                    },
                });
            }
            index = index.saturating_add(1);
        }
        if choices.is_empty() {
            Ok(None)
        } else {
            self.state.pending_choices = choices;
            Ok(Some(StoryEvent::Choices(self.choices())))
        }
    }

    fn choice_is_eligible(&mut self, choice: &ChoiceIr) -> Result<bool, RuntimeError> {
        if choice.once && self.state.selected_once.contains(&choice.id) {
            return Ok(false);
        }
        match &choice.condition {
            Some(condition) => expect_bool(self.evaluate(condition)?, "choice condition"),
            None => Ok(true),
        }
    }

    fn enter_conditional(
        &mut self,
        block: &BlockRef,
        instruction: usize,
        conditional: &ConditionalIr,
    ) -> Result<(), RuntimeError> {
        for (branch_index, branch) in conditional.branches.iter().enumerate() {
            if expect_bool(self.evaluate(&branch.condition)?, "conditional branch")? {
                if !branch.body.is_empty() {
                    self.state.frames.push(ExecutionFrame {
                        block: block.child(BlockSegment::ConditionalBranch {
                            instruction,
                            branch: branch_index,
                        }),
                        index: 0,
                        on_complete: None,
                    });
                }
                return Ok(());
            }
        }
        if conditional
            .fallback
            .as_ref()
            .is_some_and(|body| !body.is_empty())
        {
            self.state.frames.push(ExecutionFrame {
                block: block.child(BlockSegment::ConditionalFallback { instruction }),
                index: 0,
                on_complete: None,
            });
        }
        Ok(())
    }

    fn thread(&mut self, target: &str, span: Span) -> Result<(), RuntimeError> {
        if !self.data.knots.contains_key(target) {
            return Err(RuntimeError::at(
                RuntimeErrorKind::UnknownKnot(target.to_owned()),
                span,
            ));
        }
        if self.state.frames.len() >= MAX_THREAD_DEPTH {
            return Err(RuntimeError::at(RuntimeErrorKind::ThreadDepth, span));
        }
        self.state.frames.push(ExecutionFrame {
            block: BlockRef::knot(target),
            index: 0,
            on_complete: None,
        });
        Ok(())
    }

    fn end(&mut self) {
        self.state.frames.clear();
        self.state.pending_choices.clear();
        self.state.ended = true;
    }

    fn execute_declaration(&mut self, declaration: &DeclarationIr) -> Result<(), RuntimeError> {
        let value = self.evaluate(&declaration.value)?;
        validate_declared_value(declaration, &value)?;
        if let Some(existing) = self.state.variables.get(&declaration.name)
            && existing.kind != declaration.kind
        {
            return Err(RuntimeError::new(RuntimeErrorKind::Type(format!(
                "`{}` was redeclared with another kind",
                declaration.name
            ))));
        }
        self.state.variables.insert(
            declaration.name.clone(),
            VariableState {
                kind: declaration.kind,
                value,
                allowed_states: declaration.allowed_states.clone(),
            },
        );
        Ok(())
    }

    fn assign(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        let Some(variable) = self.state.variables.get_mut(name) else {
            return Err(RuntimeError::new(RuntimeErrorKind::UnknownPath(
                name.to_owned(),
            )));
        };
        validate_assignment(name, variable, &value)?;
        variable.value = value;
        Ok(())
    }

    fn mutate_list(
        &mut self,
        operation: ListOperationIr,
        name: &str,
        value: Value,
    ) -> Result<(), RuntimeError> {
        let Some(variable) = self.state.variables.get_mut(name) else {
            return Err(RuntimeError::new(RuntimeErrorKind::UnknownPath(
                name.to_owned(),
            )));
        };
        let Value::List(values) = &mut variable.value else {
            return Err(RuntimeError::new(RuntimeErrorKind::Type(format!(
                "list mutation target `{name}` is not a list"
            ))));
        };
        match operation {
            ListOperationIr::Push => values.push(value),
            ListOperationIr::Remove => {
                if let Some(index) = values.iter().position(|member| member == &value) {
                    values.remove(index);
                }
            }
        }
        Ok(())
    }

    fn evaluate(&mut self, expression: &Expression) -> Result<Value, RuntimeError> {
        match expression {
            Expression::Literal(literal) => Ok(literal_value(literal)),
            Expression::List(values) => values
                .iter()
                .map(|value| self.evaluate(value))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List),
            Expression::Path(path) => self.resolve_path(path),
            Expression::GrammarRef { grammar, rule } => {
                let Some(grammar) = grammar else {
                    return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                        "runtime grammar reference is not qualified".to_owned(),
                    )));
                };
                self.expand_grammar(grammar, rule, 0).map(Value::String)
            }
            Expression::Call { path, arguments } => {
                if !arguments.is_empty() {
                    return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                        "compiled pattern draw contains unsupported arguments".to_owned(),
                    )));
                }
                self.draw_pattern(path)
            }
            Expression::Unary { operator, operand } => {
                let operand = self.evaluate(operand)?;
                match (operator, operand) {
                    (UnaryOperatorIr::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
                    (UnaryOperatorIr::Negate, Value::Number(value)) => Ok(Value::Number(-value)),
                    (operator, value) => Err(RuntimeError::new(RuntimeErrorKind::Type(format!(
                        "operator {operator:?} cannot be applied to {}",
                        value_kind(&value)
                    )))),
                }
            }
            Expression::Binary {
                left,
                operator,
                right,
            } => self.evaluate_binary(left, *operator, right),
        }
    }

    fn evaluate_binary(
        &mut self,
        left: &Expression,
        operator: BinaryOperatorIr,
        right: &Expression,
    ) -> Result<Value, RuntimeError> {
        let left_value = self.evaluate(left)?;
        if operator == BinaryOperatorIr::And {
            let left = expect_bool(left_value, "left operand of `and`")?;
            return if left {
                expect_bool(self.evaluate(right)?, "right operand of `and`").map(Value::Bool)
            } else {
                Ok(Value::Bool(false))
            };
        }
        if operator == BinaryOperatorIr::Or {
            let left = expect_bool(left_value, "left operand of `or`")?;
            return if left {
                Ok(Value::Bool(true))
            } else {
                expect_bool(self.evaluate(right)?, "right operand of `or`").map(Value::Bool)
            };
        }
        let right_value = self.evaluate(right)?;
        evaluate_eager_binary(left_value, operator, right_value)
    }

    fn resolve_path(&self, path: &[String]) -> Result<Value, RuntimeError> {
        let Some(root) = path.first() else {
            return Err(RuntimeError::new(RuntimeErrorKind::UnknownPath(
                String::new(),
            )));
        };
        let Some(variable) = self.state.variables.get(root) else {
            if let Some(module) = self.data.modules.get(root) {
                let fields = path[1..].iter().map(String::as_str).collect::<Vec<_>>();
                return self.module_value_from(module, &fields).ok_or_else(|| {
                    RuntimeError::new(RuntimeErrorKind::UnknownPath(path.join(".")))
                });
            }
            if path.len() == 1 {
                return Ok(Value::Symbol(root.clone()));
            }
            return Err(RuntimeError::new(RuntimeErrorKind::UnknownPath(
                path.join("."),
            )));
        };
        let mut value = &variable.value;
        for field in &path[1..] {
            let Value::Object(fields) = value else {
                return Err(RuntimeError::new(RuntimeErrorKind::UnknownPath(
                    path.join("."),
                )));
            };
            let Some(next) = fields.get(field) else {
                return Err(RuntimeError::new(RuntimeErrorKind::UnknownPath(
                    path.join("."),
                )));
            };
            value = next;
        }
        Ok(value.clone())
    }

    fn module_value_from(&self, module: &DomainModuleIr, path: &[&str]) -> Option<Value> {
        if path.is_empty() {
            let values = module
                .exports
                .iter()
                .map(|(name, export)| {
                    self.module_export_value(module, name, export.source)
                        .map(|value| (name.clone(), value))
                })
                .collect::<Option<BTreeMap<_, _>>>()?;
            return Some(Value::Object(values));
        }
        let (export_name, fields) = path.split_first()?;
        let export = module.exports.get(*export_name)?;
        let mut value = self.module_export_value(module, export_name, export.source)?;
        for field in fields {
            let Value::Object(values) = value else {
                return None;
            };
            value = values.get(*field)?.clone();
        }
        Some(value)
    }

    fn module_export_value(
        &self,
        module: &DomainModuleIr,
        name: &str,
        source: DomainExportSourceIr,
    ) -> Option<Value> {
        match source {
            DomainExportSourceIr::Pack => module
                .exports
                .get(name)
                .map(|export| domain_value(&export.value)),
            DomainExportSourceIr::State => self
                .state
                .modules
                .get(&module.id)
                .and_then(|state| state.values.get(name))
                .cloned(),
        }
    }

    fn render_template(
        &mut self,
        template: &Template,
        depth: usize,
    ) -> Result<String, RuntimeError> {
        if depth > MAX_GRAMMAR_DEPTH {
            return Err(RuntimeError::new(RuntimeErrorKind::GrammarDepth));
        }
        let mut output = String::new();
        for part in &template.parts {
            match part {
                TemplatePartIr::Text(text) => output.push_str(text),
                TemplatePartIr::GrammarRef { grammar, rule } => {
                    let Some(grammar) = grammar else {
                        return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                            "runtime grammar reference is not qualified".to_owned(),
                        )));
                    };
                    output.push_str(&self.expand_grammar(grammar, rule, depth + 1)?);
                }
                TemplatePartIr::Expression(expression) => {
                    output.push_str(&self.evaluate(expression)?.to_string());
                }
            }
        }
        Ok(output)
    }

    fn expand_grammar(
        &mut self,
        grammar: &str,
        rule: &str,
        depth: usize,
    ) -> Result<String, RuntimeError> {
        if depth > MAX_GRAMMAR_DEPTH {
            return Err(RuntimeError::new(RuntimeErrorKind::GrammarDepth));
        }
        let alternatives = self
            .data
            .grammars
            .get(grammar)
            .and_then(|grammar| grammar.rules.get(rule))
            .cloned()
            .ok_or_else(|| {
                RuntimeError::new(RuntimeErrorKind::UnknownGrammarRule {
                    grammar: grammar.to_owned(),
                    rule: rule.to_owned(),
                })
            })?;
        if alternatives.is_empty() {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(format!(
                "grammar rule `{grammar}.{rule}` has no alternatives"
            ))));
        }
        let index = self.random_index(alternatives.len());
        self.render_template(&alternatives[index], depth + 1)
    }

    fn draw_pattern(&mut self, path: &[String]) -> Result<Value, RuntimeError> {
        let (system_name, spread) = match path {
            [system, draw] if draw == "draw" => (system.as_str(), None),
            [system, spread_keyword, spread, draw]
                if spread_keyword == "spread" && draw == "draw" =>
            {
                (system.as_str(), Some(spread.clone()))
            }
            _ => {
                return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(format!(
                    "unsupported compiled call `{}`",
                    path.join(".")
                ))));
            }
        };
        let system = self.patterns.get(system_name).cloned().ok_or_else(|| {
            RuntimeError::new(RuntimeErrorKind::Pattern {
                system: system_name.to_owned(),
                message: "the compiled story does not define this system".to_owned(),
            })
        })?;
        let pattern_state = self.state.patterns.get_mut(system_name).ok_or_else(|| {
            RuntimeError::new(RuntimeErrorKind::InvalidStory(format!(
                "pattern `{system_name}` has no mutable story state"
            )))
        })?;
        let mut random = SeededRandom::new(self.state.seed, self.state.random_draws);
        let result = system
            .draw(
                &DrawRequest {
                    spread,
                    ..DrawRequest::default()
                },
                pattern_state,
                &mut random,
            )
            .map_err(|error| {
                RuntimeError::new(RuntimeErrorKind::Pattern {
                    system: system_name.to_owned(),
                    message: error.to_string(),
                })
            })?;
        self.state.random_draws = self.state.random_draws.saturating_add(random.consumed());
        let value = draw_result_value(&result)?;
        self.pending_pattern_draws.push(result);
        Ok(value)
    }

    fn random_index(&mut self, length: usize) -> usize {
        let mut random = ChaCha8Rng::seed_from_u64(self.state.seed);
        for _ in 0..self.state.random_draws {
            let _ = random.next_u64();
        }
        let value = random.next_u64();
        self.state.random_draws = self.state.random_draws.saturating_add(1);
        (value % length as u64) as usize
    }
}

fn build_patterns(
    data: &StoryIr,
) -> Result<BTreeMap<String, Arc<dyn PatternSystem>>, RuntimeError> {
    data.patterns
        .iter()
        .map(|(name, definition)| {
            system_from_ir(name, definition)
                .map(|system| (name.clone(), system))
                .map_err(|error| {
                    RuntimeError::new(RuntimeErrorKind::Pattern {
                        system: name.clone(),
                        message: error.to_string(),
                    })
                })
        })
        .collect()
}

fn initial_domain_state(data: &StoryIr) -> BTreeMap<String, DomainModuleState> {
    data.modules
        .values()
        .map(|module| {
            (
                module.id.clone(),
                DomainModuleState {
                    version: module.version.clone(),
                    values: initial_module_values(module),
                },
            )
        })
        .collect()
}

fn initial_module_values(module: &DomainModuleIr) -> BTreeMap<String, Value> {
    module
        .exports
        .iter()
        .filter(|(_, export)| export.source == DomainExportSourceIr::State)
        .map(|(name, export)| (name.clone(), domain_value(&export.value)))
        .collect()
}

fn reconcile_domain_state(data: &StoryIr, states: &mut BTreeMap<String, DomainModuleState>) {
    let active = data
        .modules
        .values()
        .map(|module| module.id.as_str())
        .collect::<BTreeSet<_>>();
    states.retain(|id, _| active.contains(id.as_str()));
    for module in data.modules.values() {
        let initial = initial_module_values(module);
        let state_is_current = states
            .get(&module.id)
            .is_some_and(|state| state.version == module.version);
        if !state_is_current {
            states.insert(
                module.id.clone(),
                DomainModuleState {
                    version: module.version.clone(),
                    values: initial,
                },
            );
            continue;
        }
        let Some(state) = states.get_mut(&module.id) else {
            continue;
        };
        state.values.retain(|name, value| {
            initial
                .get(name)
                .is_some_and(|expected| same_value_type(value, expected))
        });
        for (name, value) in initial {
            state.values.entry(name).or_insert(value);
        }
    }
}

fn reconcile_pattern_state(
    patterns: &BTreeMap<String, Arc<dyn PatternSystem>>,
    states: &mut BTreeMap<String, PatternState>,
) {
    states.retain(|name, _| patterns.contains_key(name));
    for (name, system) in patterns {
        let state_is_current = states.get(name).is_some_and(|state| {
            state.last_draw.iter().all(|id| {
                system
                    .definition()
                    .elements
                    .iter()
                    .any(|element| &element.id == id)
            })
        });
        if !state_is_current {
            states.insert(name.clone(), PatternState::default());
        }
    }
}

fn draw_result_value(result: &DrawResult) -> Result<Value, RuntimeError> {
    if result.spread.is_none() {
        let [entry] = result.entries.as_slice() else {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                "single pattern draw did not return exactly one element".to_owned(),
            )));
        };
        return Ok(drawn_element_value(entry));
    }
    let mut positions = BTreeMap::new();
    for entry in &result.entries {
        let Some(position) = &entry.position else {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                "spread pattern draw returned an unpositioned element".to_owned(),
            )));
        };
        if positions
            .insert(position.clone(), drawn_element_value(entry))
            .is_some()
        {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                "spread pattern draw repeated a result position".to_owned(),
            )));
        }
    }
    Ok(Value::Object(positions))
}

fn drawn_element_value(entry: &weave_patterns::DrawnElement) -> Value {
    let mut fields = entry
        .fields
        .iter()
        .map(|(name, value)| (name.clone(), pattern_value(value)))
        .collect::<BTreeMap<_, _>>();
    fields.insert("id".to_owned(), Value::String(entry.id.clone()));
    fields.insert("reversed".to_owned(), Value::Bool(entry.reversed));
    if let Some(position) = &entry.position {
        fields.insert("position".to_owned(), Value::Symbol(position.clone()));
    }
    Value::Object(fields)
}

fn pattern_value(value: &PatternValue) -> Value {
    match value {
        PatternValue::Null => Value::Null,
        PatternValue::Bool(value) => Value::Bool(*value),
        PatternValue::Number(value) => Value::Number(*value),
        PatternValue::String(value) => Value::String(value.clone()),
        PatternValue::Symbol(value) => Value::Symbol(value.clone()),
        PatternValue::List(values) => Value::List(values.iter().map(pattern_value).collect()),
        PatternValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), pattern_value(value)))
                .collect(),
        ),
    }
}

fn validate_story(data: &StoryIr) -> Result<(), RuntimeError> {
    if data.version != IR_VERSION {
        return Err(RuntimeError::new(RuntimeErrorKind::UnsupportedVersion {
            found: data.version,
            expected: IR_VERSION,
        }));
    }
    if !data.knots.contains_key(&data.entry) {
        return Err(RuntimeError::new(RuntimeErrorKind::UnknownKnot(
            data.entry.clone(),
        )));
    }
    let mut module_ids = BTreeSet::new();
    for (alias, module) in &data.modules {
        if alias.is_empty()
            || module.id.is_empty()
            || module.version.is_empty()
            || module.pack_id.is_empty()
            || module.pack_version.is_empty()
        {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                "compiled domain module metadata is incomplete".to_owned(),
            )));
        }
        if !module_ids.insert(&module.id) {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                "compiled story activates one domain module more than once".to_owned(),
            )));
        }
        if module
            .exports
            .values()
            .any(|export| !valid_domain_value(&export.value))
        {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                "compiled domain module contains an invalid value".to_owned(),
            )));
        }
        let mut previous = None;
        for authored in &module.authored_overrides {
            if authored.path.is_empty()
                || authored.path.iter().any(String::is_empty)
                || previous.is_some_and(|path: &[String]| path >= authored.path.as_slice())
                || !valid_domain_value(&authored.value)
            {
                return Err(RuntimeError::new(RuntimeErrorKind::InvalidStory(
                    "compiled domain module contains invalid authored metadata".to_owned(),
                )));
            }
            previous = Some(authored.path.as_slice());
        }
    }
    Ok(())
}

const MAX_DOMAIN_VALUE_DEPTH: usize = 32;
const MAX_DOMAIN_VALUE_NODES: usize = 262_144;

fn valid_domain_value(value: &DomainValueIr) -> bool {
    let mut nodes = 0;
    valid_domain_value_at(value, 0, &mut nodes)
}

fn valid_domain_value_at(value: &DomainValueIr, depth: usize, nodes: &mut usize) -> bool {
    *nodes += 1;
    if depth > MAX_DOMAIN_VALUE_DEPTH || *nodes > MAX_DOMAIN_VALUE_NODES {
        return false;
    }
    match value {
        DomainValueIr::Number(value) => value.is_finite(),
        DomainValueIr::List(values) => values
            .iter()
            .all(|value| valid_domain_value_at(value, depth + 1, nodes)),
        DomainValueIr::Object(fields) => fields
            .values()
            .all(|value| valid_domain_value_at(value, depth + 1, nodes)),
        DomainValueIr::Null
        | DomainValueIr::Bool(_)
        | DomainValueIr::String(_)
        | DomainValueIr::Symbol(_) => true,
    }
}

fn validate_declared_value(declaration: &DeclarationIr, value: &Value) -> Result<(), RuntimeError> {
    match declaration.kind {
        VariableKindIr::Variable => Ok(()),
        VariableKindIr::List if matches!(value, Value::List(_)) => Ok(()),
        VariableKindIr::Flag if matches!(value, Value::Bool(_)) => Ok(()),
        VariableKindIr::State => {
            let Value::Symbol(symbol) = value else {
                return Err(RuntimeError::new(RuntimeErrorKind::Type(format!(
                    "STATE `{}` requires a symbol",
                    declaration.name
                ))));
            };
            if declaration.allowed_states.contains(symbol) {
                Ok(())
            } else {
                Err(RuntimeError::new(RuntimeErrorKind::InvalidState {
                    name: declaration.name.clone(),
                    value: symbol.clone(),
                }))
            }
        }
        _ => Err(RuntimeError::new(RuntimeErrorKind::Type(format!(
            "initializer for `{}` does not match its declaration kind",
            declaration.name
        )))),
    }
}

fn validate_assignment(
    name: &str,
    variable: &VariableState,
    value: &Value,
) -> Result<(), RuntimeError> {
    match variable.kind {
        VariableKindIr::Variable => {
            if same_value_type(&variable.value, value) {
                Ok(())
            } else {
                Err(RuntimeError::new(RuntimeErrorKind::Type(format!(
                    "assignment to `{name}` changes its concrete type"
                ))))
            }
        }
        VariableKindIr::List if matches!(value, Value::List(_)) => Ok(()),
        VariableKindIr::Flag if matches!(value, Value::Bool(_)) => Ok(()),
        VariableKindIr::State => {
            let Value::Symbol(symbol) = value else {
                return Err(RuntimeError::new(RuntimeErrorKind::Type(format!(
                    "STATE `{name}` requires a symbol"
                ))));
            };
            if variable.allowed_states.contains(symbol) {
                Ok(())
            } else {
                Err(RuntimeError::new(RuntimeErrorKind::InvalidState {
                    name: name.to_owned(),
                    value: symbol.clone(),
                }))
            }
        }
        _ => Err(RuntimeError::new(RuntimeErrorKind::Type(format!(
            "assignment to `{name}` has the wrong value type"
        )))),
    }
}

fn evaluate_eager_binary(
    left: Value,
    operator: BinaryOperatorIr,
    right: Value,
) -> Result<Value, RuntimeError> {
    match operator {
        BinaryOperatorIr::Add => match (left, right) {
            (Value::Number(left), Value::Number(right)) => Ok(Value::Number(left + right)),
            (Value::String(left), Value::String(right)) => Ok(Value::String(left + &right)),
            (left, right) => binary_type_error(operator, &left, &right),
        },
        BinaryOperatorIr::Subtract
        | BinaryOperatorIr::Multiply
        | BinaryOperatorIr::Divide
        | BinaryOperatorIr::Remainder => {
            let (Value::Number(left), Value::Number(right)) = (&left, &right) else {
                return binary_type_error(operator, &left, &right);
            };
            if matches!(
                operator,
                BinaryOperatorIr::Divide | BinaryOperatorIr::Remainder
            ) && *right == 0.0
            {
                return Err(RuntimeError::new(RuntimeErrorKind::DivideByZero));
            }
            let value = match operator {
                BinaryOperatorIr::Subtract => left - right,
                BinaryOperatorIr::Multiply => left * right,
                BinaryOperatorIr::Divide => left / right,
                BinaryOperatorIr::Remainder => left % right,
                _ => unreachable!("operator filtered by outer match"),
            };
            if value.is_finite() {
                Ok(Value::Number(value))
            } else {
                Err(RuntimeError::new(RuntimeErrorKind::Type(
                    "arithmetic result is not finite".to_owned(),
                )))
            }
        }
        BinaryOperatorIr::Equal => Ok(Value::Bool(left == right)),
        BinaryOperatorIr::NotEqual => Ok(Value::Bool(left != right)),
        BinaryOperatorIr::Less
        | BinaryOperatorIr::LessEqual
        | BinaryOperatorIr::Greater
        | BinaryOperatorIr::GreaterEqual => compare_values(left, operator, right),
        BinaryOperatorIr::In => match (left, right) {
            (value, Value::List(values)) => Ok(Value::Bool(values.contains(&value))),
            (Value::String(needle), Value::String(haystack)) => {
                Ok(Value::Bool(haystack.contains(&needle)))
            }
            (left, right) => binary_type_error(operator, &left, &right),
        },
        BinaryOperatorIr::And | BinaryOperatorIr::Or => Err(RuntimeError::new(
            RuntimeErrorKind::InvalidStory("short-circuit operator evaluated eagerly".to_owned()),
        )),
    }
}

fn compare_values(
    left: Value,
    operator: BinaryOperatorIr,
    right: Value,
) -> Result<Value, RuntimeError> {
    let ordering = match (&left, &right) {
        (Value::Number(left), Value::Number(right)) => left.partial_cmp(right),
        (Value::String(left), Value::String(right))
        | (Value::Symbol(left), Value::Symbol(right)) => Some(left.cmp(right)),
        _ => return binary_type_error(operator, &left, &right),
    };
    let Some(ordering) = ordering else {
        return Err(RuntimeError::new(RuntimeErrorKind::Type(
            "values cannot be ordered".to_owned(),
        )));
    };
    let result = match operator {
        BinaryOperatorIr::Less => ordering.is_lt(),
        BinaryOperatorIr::LessEqual => ordering.is_le(),
        BinaryOperatorIr::Greater => ordering.is_gt(),
        BinaryOperatorIr::GreaterEqual => ordering.is_ge(),
        _ => false,
    };
    Ok(Value::Bool(result))
}

fn binary_type_error(
    operator: BinaryOperatorIr,
    left: &Value,
    right: &Value,
) -> Result<Value, RuntimeError> {
    Err(RuntimeError::new(RuntimeErrorKind::Type(format!(
        "operator {operator:?} does not accept {} and {}",
        value_kind(left),
        value_kind(right)
    ))))
}

fn expect_bool(value: Value, context: &str) -> Result<bool, RuntimeError> {
    match value {
        Value::Bool(value) => Ok(value),
        value => Err(RuntimeError::new(RuntimeErrorKind::Type(format!(
            "{context} requires Bool, found {}",
            value_kind(&value)
        )))),
    }
}

fn literal_value(literal: &ValueLiteral) -> Value {
    match literal {
        ValueLiteral::Null => Value::Null,
        ValueLiteral::Bool(value) => Value::Bool(*value),
        ValueLiteral::Number(value) => Value::Number(*value),
        ValueLiteral::String(value) => Value::String(value.clone()),
        ValueLiteral::Symbol(value) => Value::Symbol(value.clone()),
    }
}

fn domain_value(value: &DomainValueIr) -> Value {
    match value {
        DomainValueIr::Null => Value::Null,
        DomainValueIr::Bool(value) => Value::Bool(*value),
        DomainValueIr::Number(value) => Value::Number(*value),
        DomainValueIr::String(value) => Value::String(value.clone()),
        DomainValueIr::Symbol(value) => Value::Symbol(value.clone()),
        DomainValueIr::List(values) => Value::List(values.iter().map(domain_value).collect()),
        DomainValueIr::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), domain_value(value)))
                .collect(),
        ),
    }
}

fn same_value_type(left: &Value, right: &Value) -> bool {
    matches!(
        (left, right),
        (Value::Null, Value::Null)
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Symbol(_), Value::Symbol(_))
            | (Value::List(_), Value::List(_))
            | (Value::Object(_), Value::Object(_))
    )
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "Null",
        Value::Bool(_) => "Bool",
        Value::Number(_) => "Number",
        Value::String(_) => "String",
        Value::Symbol(_) => "Symbol",
        Value::List(_) => "List",
        Value::Object(_) => "Object",
    }
}

fn invalid_block_reference() -> RuntimeError {
    RuntimeError::new(RuntimeErrorKind::InvalidStory(
        "saved block reference is invalid".to_owned(),
    ))
}

fn with_span(mut error: RuntimeError, span: Span) -> RuntimeError {
    if error.span.is_none() {
        error.span = Some(span);
    }
    error
}

#[cfg(test)]
mod tests {
    use weave_compiler::{CompileOptions, compile};
    use weave_domain::{DomainCatalog, DomainPack, ModuleManifest};

    use super::*;

    const README_STORY: &str = include_str!("../../weave-core/tests/fixtures/fortune_teller.weave");

    fn compile_story(source: &str, seed: u64) -> Story {
        let compiled = compile(source, &CompileOptions::default()).expect("story should compile");
        Story::with_seed(compiled.story, seed).expect("story should start")
    }

    #[test]
    fn module_exports_seed_globals_drive_narrative_and_remain_host_visible() {
        let source = include_str!("../../../examples/domain-modules/contract/tracer.weave");
        let manifest = ModuleManifest::from_json(include_str!(
            "../../../examples/domain-modules/contract/module.weave-module.json"
        ))
        .expect("canonical manifest");
        let pack = DomainPack::from_json(include_str!(
            "../../../examples/domain-modules/contract/pack.weave-domain.json"
        ))
        .expect("canonical pack");
        let catalog = DomainCatalog::from_artifacts([manifest], [pack]).expect("catalog");
        let compiled =
            weave_compiler::compile_with_modules(source, &CompileOptions::default(), &catalog)
                .expect("compile tracer");
        let mut story = Story::new(compiled.story).expect("start tracer");

        assert_eq!(
            story.module_value("constellation", &["phase"]),
            Some(Value::Symbol("twilight".to_owned()))
        );
        assert_eq!(
            story.state().variables()["observed_phase"].value,
            Value::Symbol("twilight".to_owned())
        );
        assert_eq!(
            story.continue_line().expect("advance tracer"),
            Some("The Glasswing constellation glows at 0.625 intensity.".to_owned())
        );
    }

    #[test]
    fn rejects_domain_values_beyond_the_contract_depth_limit() {
        let mut value = DomainValueIr::Null;
        for _ in 0..=MAX_DOMAIN_VALUE_DEPTH {
            value = DomainValueIr::List(vec![value]);
        }
        let mut compiled = compile("=== start ===\n-> END\n", &CompileOptions::default())
            .expect("minimal story compiles")
            .story;
        compiled.modules.insert(
            "synthetic".to_owned(),
            DomainModuleIr {
                id: "org.weave.synthetic".to_owned(),
                version: "1.0.0".to_owned(),
                pack_id: "depth_probe".to_owned(),
                pack_version: "1.0.0".to_owned(),
                exports: BTreeMap::from([(
                    "probe".to_owned(),
                    weave_core::ir::DomainExportIr {
                        source: DomainExportSourceIr::Pack,
                        value,
                    },
                )]),
                authored_overrides: Vec::new(),
            },
        );

        let error = Story::new(compiled).expect_err("over-deep module value must fail");
        assert!(matches!(error.kind, RuntimeErrorKind::InvalidStory(_)));
    }

    #[test]
    fn complete_readme_story_compiles_and_runs_its_tarot_spread() {
        let mut story = compile_story(README_STORY, 2026);
        assert!(matches!(story.advance(), Ok(StoryEvent::Line(_))));
        let draws = story.take_pattern_draws();
        assert_eq!(draws.len(), 1);
        assert_eq!(
            draws[0]
                .entries
                .iter()
                .map(|entry| entry.position.as_deref())
                .collect::<Vec<_>>(),
            [Some("past"), Some("present"), Some("future")]
        );
    }

    #[test]
    fn runs_variables_conditions_choices_and_state() {
        let source = r#"
VAR score = 1
LIST inventory = [key]
FLAG open = false
STATE quest = dormant [dormant, active, complete]
=== start ===
SET score = score + 1
* [Open] {if key in inventory}
    SET open = true
    SET quest = active
    Door open: {open}; score: {score}; quest: {quest}.
    -> END
+ [Wait]
    Still waiting.
    -> start
"#;
        let mut story = compile_story(source, 9);
        assert!(matches!(story.advance(), Ok(StoryEvent::Choices(_))));
        story.choose(0).expect("choice should be valid");
        assert_eq!(
            story.advance().expect("story should advance"),
            StoryEvent::Line("Door open: true; score: 2; quest: active.".to_owned())
        );
        assert_eq!(
            story.advance().expect("story should end"),
            StoryEvent::Ended
        );
    }

    #[test]
    fn nested_choices_and_threads_resume_in_order() {
        let source = r#"
=== start ===
Before.
<- aside
* [Ask]
    Asked.
    * [Finish] -> ending
=== aside ===
Aside.
=== ending ===
Done.
-> END
"#;
        let mut story = compile_story(source, 0);
        assert_eq!(
            story.continue_line().expect("line"),
            Some("Before.".to_owned())
        );
        assert_eq!(
            story.continue_line().expect("line"),
            Some("Aside.".to_owned())
        );
        assert!(matches!(story.advance(), Ok(StoryEvent::Choices(_))));
        story.choose(0).expect("outer choice");
        assert_eq!(
            story.continue_line().expect("line"),
            Some("Asked.".to_owned())
        );
        assert!(matches!(story.advance(), Ok(StoryEvent::Choices(_))));
        story.choose(0).expect("inner choice");
        assert_eq!(
            story.continue_line().expect("line"),
            Some("Done.".to_owned())
        );
    }

    #[test]
    fn seeded_grammar_is_reproducible_and_state_restores() {
        let source = r#"
grammar coin {
    side: ["heads", "tails"]
}
=== start ===
#coin.side# #coin.side#
* [Again] -> start
"#;
        let compiled = compile(source, &CompileOptions::default()).expect("story should compile");
        let mut first = Story::with_seed(compiled.story.clone(), 42).expect("story should start");
        let mut second = Story::with_seed(compiled.story.clone(), 42).expect("story should start");
        assert_eq!(
            first.advance().expect("line"),
            second.advance().expect("line")
        );
        assert!(matches!(first.advance(), Ok(StoryEvent::Choices(_))));
        let encoded = ron::to_string(first.state()).expect("state should serialize");
        let state: StoryState = ron::from_str(&encoded).expect("state should deserialize");
        let restored = Story::restore(compiled.story, state).expect("state should restore");
        assert_eq!(restored.choices(), first.choices());
    }

    #[test]
    fn rejects_unknown_versions_and_executes_pattern_calls() {
        let mut invalid = StoryIr::new("start");
        invalid.version = IR_VERSION + 1;
        let error = Story::new(invalid).expect_err("unknown version should fail");
        assert!(matches!(
            error.kind,
            RuntimeErrorKind::UnsupportedVersion { .. }
        ));

        let source = r#"
pattern cards {
    deck: [
        (name: "One", meaning: first),
        (name: "Two", meaning: second),
    ]
    spread single { positions: [card] }
}
VAR draw = cards.spread.single.draw()
=== start ===
{draw.card.name}: {draw.card.meaning}
-> END
"#;
        let mut story = compile_story(source, 0);
        assert!(matches!(
            story.advance(),
            Ok(StoryEvent::Line(line)) if line == "One: first" || line == "Two: second"
        ));
        let draws = story.take_pattern_draws();
        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0].entries[0].position.as_deref(), Some("card"));
        assert_eq!(story.state().patterns()["cards"].draws, 1);
    }

    #[test]
    fn custom_weighted_spreads_are_deterministic_and_branch_on_meaning() {
        let source = r#"
pattern weather_omens {
    omens: [
        (name: "Storm Crow", meaning: ill_tidings, severity: 3),
        (name: "Sun Dog", meaning: good_fortune, severity: 1),
        (name: "Frost Wolf", meaning: harsh_winter, severity: 4),
    ]
    draw: weighted_by_severity
    spread day_omen { positions: [dawn, noon, dusk] }
}
VAR omen = weather_omens.spread.day_omen.draw()
=== start ===
{omen.dawn.meaning == ill_tidings:
    Warning: {omen.dawn.name}.
- else:
    Clear: {omen.dawn.name}.
}
-> END
"#;
        let mut first = compile_story(source, 71);
        let mut second = compile_story(source, 71);
        assert_eq!(
            first.advance().expect("line"),
            second.advance().expect("line")
        );
        assert_eq!(first.take_pattern_draws(), second.take_pattern_draws());
        assert_eq!(first.state().patterns()["weather_omens"].last_draw.len(), 3);
    }

    #[test]
    fn built_in_systems_expose_structured_fields_to_story_expressions() {
        let source = r#"
pattern tarot_reading {
    builtin: tarot
    reversals: true
}
pattern changes {
    builtin: i_ching
    draw: three_coin
}
pattern runes {
    builtin: elder_futhark
}
VAR cards = tarot_reading.spread.three_card.draw()
VAR hexagram = changes.draw()
VAR rune = runes.draw()
=== start ===
{cards.past.name}|{cards.future.meaning}|{hexagram.meaning}|{hexagram.changing_lines}|{hexagram.transformed.meaning}|{rune.transliteration}
-> END
"#;
        let mut story = compile_story(source, 314_159);
        let StoryEvent::Line(line) = story.advance().expect("built-ins run") else {
            panic!("expected a line");
        };
        assert_eq!(line.split('|').count(), 6);
        let draws = story.take_pattern_draws();
        assert_eq!(draws.len(), 3);
        assert_eq!(draws[0].system, "tarot_reading");
        assert_eq!(draws[1].system, "changes");
        assert_eq!(draws[2].system, "runes");
    }

    #[test]
    fn restore_removes_stale_pattern_state_after_definition_changes() {
        let original = r#"
pattern omens {
    signs: [(name: "Crow", meaning: warning)]
}
VAR omen = omens.draw()
=== start ===
{omen.name}
"#;
        let mut story = compile_story(original, 5);
        assert_eq!(story.state().patterns()["omens"].last_draw, ["crow"]);
        let state = story.state().clone();
        let replacement = r#"
pattern omens {
    signs: [(name: "Wolf", meaning: hardship)]
}
VAR omen = omens.draw()
=== start ===
{omen.name}
"#;
        let compiled =
            compile(replacement, &CompileOptions::default()).expect("replacement compiles");
        story = Story::restore(compiled.story, state).expect("state reconciles");
        assert!(story.state().patterns()["omens"].last_draw.is_empty());
    }

    #[test]
    fn reports_division_by_zero_without_panicking() {
        let source = "=== start ===\nVAR value = 1 / 0\n{value}\n";
        let mut story = compile_story(source, 0);
        let error = story.advance().expect_err("division by zero should fail");
        assert_eq!(error.kind, RuntimeErrorKind::DivideByZero);
    }
}
