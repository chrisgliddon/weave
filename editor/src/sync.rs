//! Canonical source/AST/graph synchronization with revision conflicts and coherent history.

use std::collections::{BTreeMap, BTreeSet};

use weave_core::ast::{Choice, Document, Item, Knot, Span, Spanned, Statement};
use weave_core::{Diagnostic, parse_document};

use crate::graph::{GraphDocument, GraphPoint};

const HISTORY_LIMIT: usize = 500;

/// Semantic edit requested by the graph surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphEdit {
    /// Append a new knot with a generated identifier.
    AddKnot { preferred_name: String },
    /// Append a choice to a knot that diverts to `target`.
    AddChoice {
        knot: String,
        label: String,
        target: String,
    },
    /// Replace the tail of one knot with a divert or thread to another.
    ConnectKnots {
        source: String,
        target: String,
        thread: bool,
    },
    /// Replace the knot's first prose line, inserting one when absent.
    ///
    /// Empty text removes the first prose line.
    SetKnotText { knot: String, text: String },
    /// Rename a knot and every divert, thread, and choice target that names it.
    RenameKnot { knot: String, name: String },
    /// Replace the label of the choice at `path`.
    SetChoiceText {
        knot: String,
        path: Vec<usize>,
        text: String,
    },
    /// Remove a knot that nothing references.
    RemoveKnot { knot: String },
    /// Remove the choice at `path`.
    RemoveChoice { knot: String, path: Vec<usize> },
    /// Point the choice at `path` at another knot.
    ConnectChoice {
        knot: String,
        path: Vec<usize>,
        target: String,
    },
    /// Replace the prose at `path`. Empty text removes that prose line.
    SetStatementText {
        knot: String,
        path: Vec<usize>,
        text: String,
    },
    /// Remove the non-choice statement at `path`.
    RemoveStatement { knot: String, path: Vec<usize> },
    /// Append a prose line to the knot body.
    AppendProse { knot: String, text: String },
    /// Replace the knot body with a raw source fragment that excludes the header.
    ReplaceKnotSource { knot: String, source: String },
}

/// Result of accepting source from the text view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSync {
    Unchanged {
        revision: u64,
    },
    Applied {
        revision: u64,
    },
    Invalid {
        revision: u64,
        diagnostics: Vec<Diagnostic>,
    },
}

impl TextSync {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        match self {
            Self::Unchanged { revision }
            | Self::Applied { revision }
            | Self::Invalid { revision, .. } => *revision,
        }
    }
}

/// Accepted graph edit and its canonical formatted source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSync {
    pub revision: u64,
    pub source: String,
    pub focus_node: Option<String>,
    pub summary: String,
}

/// A view tried to edit an obsolete revision or mutate an invalid source tree.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyncConflict {
    #[error(
        "view revision {expected} is stale; canonical project is at revision {actual}; refresh before editing"
    )]
    StaleRevision { expected: u64, actual: u64 },
    #[error("graph edit is blocked until the current text has valid syntax")]
    InvalidText,
    #[error("graph edit refers to missing knot `{0}`")]
    MissingKnot(String),
    #[error("cannot connect knot `{0}` to itself")]
    SelfConnection(String),
    #[error(
        "`{0}` is not a valid knot name; start with a letter or underscore and use only ASCII letters, digits, and underscores"
    )]
    InvalidIdentifier(String),
    #[error("a knot named `{0}` already exists")]
    DuplicateKnot(String),
    #[error("choice {path:?} does not exist in knot `{knot}`")]
    MissingChoice { knot: String, path: Vec<usize> },
    #[error("statement {path:?} in knot `{knot}` cannot be edited this way")]
    MissingStatement { knot: String, path: Vec<usize> },
    #[error(
        "knot `{knot}` is still referenced by {references} connection(s); remove those connections first"
    )]
    KnotInUse { knot: String, references: usize },
    #[error("a story needs at least one knot")]
    LastKnot,
    #[error("`{0}` is reserved for ending a story; choose another knot name")]
    ReservedName(String),
    #[error(
        "the edit cannot be stored as story prose because Weave would read it as structure; reword it or use the graph controls"
    )]
    NotRepresentable,
    #[error("the edited source could not be re-parsed; nothing was changed")]
    UnparsableEdit,
}

#[derive(Debug, Clone)]
struct Snapshot {
    source: String,
    document: Option<Document>,
    last_valid_document: Document,
    layout: BTreeMap<String, GraphPoint>,
}

/// The single editor-owned project representation.
///
/// Raw source is authoritative for persistence. The current parsed document is absent while a
/// writer has incomplete syntax; the last valid document keeps the graph stable during that
/// interval. Stable node positions are editor metadata and never rewrite source by themselves.
#[derive(Debug, Clone)]
pub struct CanonicalProjectModel {
    source: String,
    document: Option<Document>,
    last_valid_document: Document,
    layout: BTreeMap<String, GraphPoint>,
    revision: u64,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl CanonicalProjectModel {
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        let source = source.into();
        let document = parse_document(&source).ok();
        let last_valid_document = document.clone().unwrap_or_default();
        let mut model = Self {
            source,
            document,
            last_valid_document,
            layout: BTreeMap::new(),
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        };
        model.reconcile_layout();
        model
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    #[must_use]
    pub const fn graph_document(&self) -> &Document {
        &self.last_valid_document
    }

    #[must_use]
    pub fn layout(&self) -> &BTreeMap<String, GraphPoint> {
        &self.layout
    }

    #[must_use]
    pub const fn can_edit_graph(&self) -> bool {
        self.document.is_some()
    }

    /// Accept source only if the caller edited the revision it last observed.
    pub fn apply_text(
        &mut self,
        source: impl Into<String>,
        expected_revision: u64,
    ) -> Result<TextSync, SyncConflict> {
        let source = source.into();
        if source == self.source {
            return Ok(TextSync::Unchanged {
                revision: self.revision,
            });
        }
        self.ensure_revision(expected_revision)?;
        let parsed = parse_document(&source);
        self.push_undo();
        self.source = source;
        self.revision = self.revision.saturating_add(1);
        match parsed {
            Ok(document) => {
                self.document = Some(document.clone());
                self.last_valid_document = document;
                self.reconcile_layout();
                Ok(TextSync::Applied {
                    revision: self.revision,
                })
            }
            Err(diagnostics) => {
                self.document = None;
                Ok(TextSync::Invalid {
                    revision: self.revision,
                    diagnostics,
                })
            }
        }
    }

    /// Apply one semantic graph action and return canonical formatted source.
    pub fn apply_graph_edit(
        &mut self,
        edit: &GraphEdit,
        expected_revision: u64,
    ) -> Result<GraphSync, SyncConflict> {
        self.ensure_revision(expected_revision)?;
        let mut document = self.document.clone().ok_or(SyncConflict::InvalidText)?;
        let (focus_node, summary) = mutate_document(&mut document, edit)?;
        let source = weave_fmt::format_document(&document);
        // Reject any edit the parser would read differently from the mutation. Prose that
        // happens to look like a choice, divert, or knot header would otherwise be written
        // verbatim and reparse as structure.
        let reparsed = parse_document(&source).map_err(|_| SyncConflict::UnparsableEdit)?;
        if !same_structure(&reparsed, &document) {
            return Err(SyncConflict::NotRepresentable);
        }
        if let GraphEdit::RenameKnot { knot, name } = edit {
            self.remap_knot_layout(knot, name);
        }
        self.push_undo();
        self.source = source.clone();
        self.document = Some(reparsed.clone());
        self.last_valid_document = reparsed;
        self.revision = self.revision.saturating_add(1);
        self.reconcile_layout();
        Ok(GraphSync {
            revision: self.revision,
            source,
            focus_node,
            summary,
        })
    }

    /// Capture current node positions without rewriting source or invalidating text revisions.
    pub fn capture_layout(&mut self, positions: BTreeMap<String, GraphPoint>) -> bool {
        let known = GraphDocument::from_ast(&self.last_valid_document)
            .nodes()
            .iter()
            .map(|node| node.id.clone())
            .collect::<BTreeSet<_>>();
        let positions = positions
            .into_iter()
            .filter(|(id, _)| known.contains(id))
            .collect::<BTreeMap<_, _>>();
        if positions == self.layout {
            return false;
        }
        self.layout = positions;
        self.reconcile_layout();
        true
    }

    /// Undo one accepted text or semantic graph transaction.
    pub fn undo(&mut self) -> Option<String> {
        let snapshot = self.undo.pop()?;
        let current = self.snapshot();
        self.redo.push(current);
        self.restore(snapshot);
        Some(self.source.clone())
    }

    /// Redo one canonical transaction.
    pub fn redo(&mut self) -> Option<String> {
        let snapshot = self.redo.pop()?;
        let current = self.snapshot();
        self.undo.push(current);
        self.restore(snapshot);
        Some(self.source.clone())
    }

    fn ensure_revision(&self, expected: u64) -> Result<(), SyncConflict> {
        if expected == self.revision {
            Ok(())
        } else {
            Err(SyncConflict::StaleRevision {
                expected,
                actual: self.revision,
            })
        }
    }

    fn push_undo(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            source: self.source.clone(),
            document: self.document.clone(),
            last_valid_document: self.last_valid_document.clone(),
            layout: self.layout.clone(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.source = snapshot.source;
        self.document = snapshot.document;
        self.last_valid_document = snapshot.last_valid_document;
        self.layout = snapshot.layout;
        self.revision = self.revision.saturating_add(1);
        self.reconcile_layout();
    }

    fn remap_knot_layout(&mut self, old: &str, new: &str) {
        let old_knot = format!("knot:{old}");
        let old_choice = format!("choice:{old}:");
        let new_knot = format!("knot:{new}");
        let new_choice = format!("choice:{new}:");
        let previous = std::mem::take(&mut self.layout);
        self.layout = previous
            .into_iter()
            .map(|(id, point)| {
                if id == old_knot {
                    (new_knot.clone(), point)
                } else if let Some(rest) = id.strip_prefix(&old_choice) {
                    (format!("{new_choice}{rest}"), point)
                } else {
                    (id, point)
                }
            })
            .collect();
    }

    fn reconcile_layout(&mut self) {
        let graph = GraphDocument::from_ast(&self.last_valid_document);
        let previous = std::mem::take(&mut self.layout);
        self.layout = graph
            .nodes()
            .iter()
            .map(|node| {
                (
                    node.id.clone(),
                    previous.get(&node.id).copied().unwrap_or(node.position),
                )
            })
            .collect();
    }
}

fn mutate_document(
    document: &mut Document,
    edit: &GraphEdit,
) -> Result<(Option<String>, String), SyncConflict> {
    match edit {
        GraphEdit::AddKnot { preferred_name } => {
            let name = unique_knot_name(document, preferred_name);
            let knot = Knot {
                name: name.clone(),
                body: vec![Spanned::new(Statement::End, Span::default())],
            };
            document.items.push(Spanned::new(
                Item::Knot(Spanned::new(knot, Span::default())),
                Span::default(),
            ));
            Ok((Some(format!("knot:{name}")), format!("Added knot {name}")))
        }
        GraphEdit::AddChoice {
            knot,
            label,
            target,
        } => {
            let owner = find_knot_mut(document, knot)?;
            let insert_at = owner
                .node
                .body
                .iter()
                .position(|statement| matches!(statement.node, Statement::End))
                .unwrap_or(owner.node.body.len());
            owner.node.body.insert(
                insert_at,
                Spanned::new(
                    Statement::Choice(Choice {
                        once: true,
                        text: label.clone(),
                        condition: None,
                        body: Vec::new(),
                        divert: Some(target.clone()),
                    }),
                    Span::default(),
                ),
            );
            Ok((
                Some(format!("knot:{knot}")),
                format!("Added a choice to {knot}"),
            ))
        }
        GraphEdit::ConnectKnots {
            source,
            target,
            thread,
        } => {
            if source == target {
                return Err(SyncConflict::SelfConnection(source.clone()));
            }
            if !document.knots().any(|knot| knot.node.name == *target) {
                return Err(SyncConflict::MissingKnot(target.clone()));
            }
            let owner = find_knot_mut(document, source)?;
            if owner
                .node
                .body
                .last()
                .is_some_and(|statement| matches!(statement.node, Statement::End))
            {
                owner.node.body.pop();
            }
            let statement = if *thread {
                Statement::Thread(target.clone())
            } else {
                Statement::Divert(target.clone())
            };
            owner
                .node
                .body
                .push(Spanned::new(statement, Span::default()));
            Ok((
                Some(format!("knot:{source}")),
                format!(
                    "Connected {source} {} {target}",
                    if *thread { "as a thread to" } else { "to" }
                ),
            ))
        }
        GraphEdit::SetKnotText { knot, text } => {
            let owner = find_knot_mut(document, knot)?;
            let text = single_line(text);
            let position = owner
                .node
                .body
                .iter()
                .position(|statement| matches!(statement.node, Statement::Text(_)));
            match position {
                Some(index) if text.is_empty() => {
                    owner.node.body.remove(index);
                }
                Some(index) => {
                    owner.node.body[index].node = Statement::Text(text);
                }
                None if !text.is_empty() => {
                    owner
                        .node
                        .body
                        .insert(0, Spanned::new(Statement::Text(text), Span::default()));
                }
                None => {}
            }
            Ok((Some(format!("knot:{knot}")), format!("Edited knot {knot}")))
        }
        GraphEdit::RenameKnot { knot, name } => {
            if name == "END" {
                return Err(SyncConflict::ReservedName(name.clone()));
            }
            if !is_valid_identifier(name) {
                return Err(SyncConflict::InvalidIdentifier(name.clone()));
            }
            if name != knot && document.knots().any(|other| other.node.name == *name) {
                return Err(SyncConflict::DuplicateKnot(name.clone()));
            }
            let owner = find_knot_mut(document, knot)?;
            owner.node.name.clone_from(name);
            for owner in document
                .items
                .iter_mut()
                .filter_map(|item| match &mut item.node {
                    Item::Knot(knot) => Some(knot),
                    _ => None,
                })
            {
                rename_references(&mut owner.node.body, knot, name);
            }
            Ok((
                Some(format!("knot:{name}")),
                format!("Renamed {knot} to {name}"),
            ))
        }
        GraphEdit::SetChoiceText { knot, path, text } => {
            let choice = find_choice_mut(document, knot, path)?;
            choice.text = single_line(text);
            Ok((
                Some(choice_focus(knot, path)),
                format!("Edited a choice in {knot}"),
            ))
        }
        GraphEdit::RemoveKnot { knot } => {
            if document.knots().count() <= 1 {
                return Err(SyncConflict::LastKnot);
            }
            let references = document
                .knots()
                .map(|owner| count_references(&owner.node.body, knot))
                .sum::<usize>();
            if references > 0 {
                return Err(SyncConflict::KnotInUse {
                    knot: knot.clone(),
                    references,
                });
            }
            let position = document.items.iter().position(
                |item| matches!(&item.node, Item::Knot(owner) if owner.node.name == *knot),
            );
            let Some(position) = position else {
                return Err(SyncConflict::MissingKnot(knot.clone()));
            };
            document.items.remove(position);
            Ok((None, format!("Removed knot {knot}")))
        }
        GraphEdit::RemoveChoice { knot, path } => {
            let owner = find_knot_mut(document, knot)?;
            let (parent, index) =
                find_choice_slot(&mut owner.node.body, path).ok_or_else(|| {
                    SyncConflict::MissingChoice {
                        knot: knot.clone(),
                        path: path.clone(),
                    }
                })?;
            parent.remove(index);
            Ok((None, format!("Removed a choice from {knot}")))
        }
        GraphEdit::ConnectChoice { knot, path, target } => {
            if !document.knots().any(|owner| owner.node.name == *target) {
                return Err(SyncConflict::MissingKnot(target.clone()));
            }
            let choice = find_choice_mut(document, knot, path)?;
            choice.divert = Some(target.clone());
            Ok((
                Some(choice_focus(knot, path)),
                format!("Connected a choice in {knot} to {target}"),
            ))
        }
        GraphEdit::SetStatementText { knot, path, text } => {
            let text = single_line(text);
            let owner = find_knot_mut(document, knot)?;
            let index = top_level_index(knot, path)?;
            let is_text = owner
                .node
                .body
                .get(index)
                .is_some_and(|statement| matches!(statement.node, Statement::Text(_)));
            if !is_text {
                return Err(missing_statement(knot, path));
            }
            if text.is_empty() {
                owner.node.body.remove(index);
            } else {
                let statement = &mut owner.node.body[index];
                if let Statement::Text(current) = &mut statement.node {
                    *current = text;
                }
            }
            Ok((
                Some(format!("knot:{knot}")),
                format!("Edited prose in {knot}"),
            ))
        }
        GraphEdit::RemoveStatement { knot, path } => {
            let owner = find_knot_mut(document, knot)?;
            let index = top_level_index(knot, path)?;
            let removable = owner.node.body.get(index).is_some_and(|statement| {
                matches!(
                    statement.node,
                    Statement::Text(_)
                        | Statement::Comment(_)
                        | Statement::Blank
                        | Statement::Cue(_)
                        | Statement::End
                )
            });
            if !removable {
                return Err(missing_statement(knot, path));
            }
            owner.node.body.remove(index);
            Ok((None, format!("Removed a statement from {knot}")))
        }
        GraphEdit::ReplaceKnotSource { knot, source } => {
            let fragment = format!("=== {knot} ===\n{source}\n");
            let parsed = parse_document(&fragment).map_err(|_| SyncConflict::UnparsableEdit)?;
            let mut knots = parsed.knots();
            let Some(owner) = knots.next() else {
                return Err(SyncConflict::UnparsableEdit);
            };
            if owner.node.name != *knot || knots.next().is_some() {
                return Err(SyncConflict::NotRepresentable);
            }
            let body = owner.node.body.clone();
            let target = find_knot_mut(document, knot)?;
            target.node.body = body;
            Ok((
                Some(format!("knot:{knot}")),
                format!("Edited the source of {knot}"),
            ))
        }
        GraphEdit::AppendProse { knot, text } => {
            let text = single_line(text);
            if text.is_empty() {
                return Err(SyncConflict::NotRepresentable);
            }
            let owner = find_knot_mut(document, knot)?;
            let insert_at = owner.node.body.len().saturating_sub(usize::from(
                owner
                    .node
                    .body
                    .last()
                    .is_some_and(|statement| matches!(statement.node, Statement::End)),
            ));
            owner.node.body.insert(
                insert_at,
                Spanned::new(Statement::Text(text), Span::default()),
            );
            Ok((
                Some(format!("knot:{knot}")),
                format!("Added prose to {knot}"),
            ))
        }
    }
}

fn top_level_index(knot: &str, path: &[usize]) -> Result<usize, SyncConflict> {
    match path {
        [index] => Ok(*index),
        _ => Err(missing_statement(knot, path)),
    }
}

fn missing_statement(knot: &str, path: &[usize]) -> SyncConflict {
    SyncConflict::MissingStatement {
        knot: knot.to_owned(),
        path: path.to_vec(),
    }
}

fn single_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ").trim().to_owned()
}

/// Compare two parsed documents without their source locations.
///
/// A graph edit mutates the AST directly and then writes canonical source. If the parser reads
/// that source differently, the edit would silently change story structure, so it is rejected.
fn same_structure(left: &Document, right: &Document) -> bool {
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(mut left), Ok(mut right)) => {
            strip_spans(&mut left);
            strip_spans(&mut right);
            left == right
        }
        _ => false,
    }
}

fn strip_spans(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("span");
            for child in map.values_mut() {
                strip_spans(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strip_spans(item);
            }
        }
        _ => {}
    }
}

fn choice_focus(knot: &str, path: &[usize]) -> String {
    format!(
        "choice:{knot}:{}",
        path.iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(".")
    )
}

fn is_valid_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn find_choice_mut<'a>(
    document: &'a mut Document,
    knot: &str,
    path: &[usize],
) -> Result<&'a mut Choice, SyncConflict> {
    let owner = find_knot_mut(document, knot)?;
    find_choice_in(&mut owner.node.body, path, knot, path)
}

fn find_choice_in<'a>(
    body: &'a mut [Spanned<Statement>],
    remaining: &[usize],
    knot: &str,
    full_path: &[usize],
) -> Result<&'a mut Choice, SyncConflict> {
    let missing = || SyncConflict::MissingChoice {
        knot: knot.to_owned(),
        path: full_path.to_vec(),
    };
    let (first, tail) = remaining.split_first().ok_or_else(&missing)?;
    let statement = body.get_mut(*first).ok_or_else(&missing)?;
    match &mut statement.node {
        Statement::Choice(choice) => {
            if tail.is_empty() {
                Ok(choice)
            } else {
                find_choice_in(&mut choice.body, tail, knot, full_path)
            }
        }
        _ => Err(missing()),
    }
}

fn find_choice_slot<'a>(
    body: &'a mut Vec<Spanned<Statement>>,
    path: &[usize],
) -> Option<(&'a mut Vec<Spanned<Statement>>, usize)> {
    let (last, parents) = path.split_last()?;
    match parents.split_first() {
        None => body
            .get(*last)
            .is_some_and(|statement| matches!(statement.node, Statement::Choice(_)))
            .then_some((body, *last)),
        Some((head, _)) => {
            let statement = body.get_mut(*head)?;
            match &mut statement.node {
                Statement::Choice(choice) => find_choice_slot(&mut choice.body, &path[1..]),
                _ => None,
            }
        }
    }
}

fn count_references(statements: &[Spanned<Statement>], name: &str) -> usize {
    statements
        .iter()
        .map(|statement| match &statement.node {
            Statement::Divert(target) | Statement::Thread(target) => usize::from(target == name),
            Statement::Choice(choice) => {
                usize::from(choice.divert.as_deref() == Some(name))
                    + count_references(&choice.body, name)
            }
            Statement::Conditional(conditional) => {
                conditional
                    .branches
                    .iter()
                    .map(|branch| count_references(&branch.body, name))
                    .sum::<usize>()
                    + conditional
                        .fallback
                        .as_ref()
                        .map_or(0, |fallback| count_references(fallback, name))
            }
            _ => 0,
        })
        .sum()
}

fn rename_references(statements: &mut [Spanned<Statement>], old: &str, new: &str) {
    for statement in statements {
        match &mut statement.node {
            Statement::Divert(target) | Statement::Thread(target) => {
                if target == old {
                    *target = new.to_owned();
                }
            }
            Statement::Choice(choice) => {
                if choice.divert.as_deref() == Some(old) {
                    choice.divert = Some(new.to_owned());
                }
                rename_references(&mut choice.body, old, new);
            }
            Statement::Conditional(conditional) => {
                for branch in &mut conditional.branches {
                    rename_references(&mut branch.body, old, new);
                }
                if let Some(fallback) = &mut conditional.fallback {
                    rename_references(fallback, old, new);
                }
            }
            _ => {}
        }
    }
}

fn find_knot_mut<'a>(
    document: &'a mut Document,
    name: &str,
) -> Result<&'a mut Spanned<Knot>, SyncConflict> {
    document
        .items
        .iter_mut()
        .find_map(|item| match &mut item.node {
            Item::Knot(knot) if knot.node.name == name => Some(knot),
            _ => None,
        })
        .ok_or_else(|| SyncConflict::MissingKnot(name.to_owned()))
}

fn unique_knot_name(document: &Document, preferred: &str) -> String {
    let used = document
        .knots()
        .map(|knot| knot.node.name.as_str())
        .collect::<BTreeSet<_>>();
    let base = sanitize_identifier(preferred);
    if !used.contains(base.as_str()) {
        return base;
    }
    (2..)
        .map(|suffix| format!("{base}_{suffix}"))
        .find(|candidate| !used.contains(candidate.as_str()))
        .expect("an unbounded numeric suffix has an available identifier")
}

fn sanitize_identifier(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            if output.is_empty() && character.is_ascii_digit() {
                output.push_str("knot_");
            }
            output.push(character.to_ascii_lowercase());
            separator = false;
        } else if !output.is_empty() && !separator {
            output.push('_');
            separator = true;
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        "new_knot".to_owned()
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const SOURCE: &str = r#"// opening comment
=== start ===
Welcome.
-> END
"#;

    #[test]
    fn graph_edits_format_valid_source_and_preserve_comments_and_order() {
        let mut model = CanonicalProjectModel::new(SOURCE);
        let added = model
            .apply_graph_edit(
                &GraphEdit::AddKnot {
                    preferred_name: "Second Scene".to_owned(),
                },
                0,
            )
            .expect("graph edit");
        assert!(
            added
                .source
                .starts_with("// opening comment\n=== start ===")
        );
        assert!(added.source.contains("=== second_scene ==="));
        assert_eq!(
            weave_fmt::format_source(&added.source).expect("formatted output parses"),
            added.source
        );

        let linked = model
            .apply_graph_edit(
                &GraphEdit::ConnectKnots {
                    source: "start".to_owned(),
                    target: "second_scene".to_owned(),
                    thread: false,
                },
                added.revision,
            )
            .expect("connect knots");
        assert!(linked.source.contains("-> second_scene"));
        assert!(linked.source.find("=== start ===") < linked.source.find("=== second_scene ==="));
    }

    #[test]
    fn invalid_and_concurrent_text_edits_block_graph_mutation_explicitly() {
        let mut model = CanonicalProjectModel::new(SOURCE);
        let invalid = model.apply_text("=== broken", 0).expect("current edit");
        assert!(matches!(invalid, TextSync::Invalid { .. }));
        assert_eq!(
            model
                .apply_graph_edit(
                    &GraphEdit::AddKnot {
                        preferred_name: "blocked".to_owned(),
                    },
                    invalid.revision(),
                )
                .expect_err("invalid text blocks graph"),
            SyncConflict::InvalidText
        );

        let mut concurrent = CanonicalProjectModel::new(SOURCE);
        concurrent
            .apply_text("// edited\n=== start ===\n-> END\n", 0)
            .expect("text edit");
        assert!(matches!(
            concurrent.apply_graph_edit(
                &GraphEdit::AddKnot {
                    preferred_name: "stale".to_owned(),
                },
                0,
            ),
            Err(SyncConflict::StaleRevision { .. })
        ));
    }

    #[test]
    fn layout_changes_do_not_rewrite_source_and_semantic_undo_is_atomic() {
        let mut model = CanonicalProjectModel::new(SOURCE);
        let original = model.source().to_owned();
        let mut layout = model.layout().clone();
        layout.insert("knot:start".to_owned(), GraphPoint::new(900.0, 420.0));
        assert!(model.capture_layout(layout));
        assert_eq!(model.source(), original);

        model
            .apply_graph_edit(
                &GraphEdit::AddKnot {
                    preferred_name: "temporary".to_owned(),
                },
                model.revision(),
            )
            .expect("add knot");
        assert!(model.source().contains("temporary"));
        assert_eq!(model.undo().as_deref(), Some(original.as_str()));
        assert!(model.redo().unwrap().contains("temporary"));
    }

    const LINKED: &str = r#"=== start ===
Welcome.
* [Leave] -> shore
-> END
=== shore ===
The shore.
<- start
=== loop ===
-> start
"#;

    #[test]
    fn knot_prose_is_replaced_inserted_and_removed() {
        let mut model = CanonicalProjectModel::new(LINKED);
        let edited = model
            .apply_graph_edit(
                &GraphEdit::SetKnotText {
                    knot: "start".to_owned(),
                    text: "A dark welcome.".to_owned(),
                },
                model.revision(),
            )
            .expect("edit prose");
        assert!(edited.source.contains("A dark welcome."));
        assert!(!edited.source.contains("Welcome."));
        assert_eq!(edited.focus_node.as_deref(), Some("knot:start"));

        let multiline = model
            .apply_graph_edit(
                &GraphEdit::SetKnotText {
                    knot: "start".to_owned(),
                    text: "One line.\nAnd another.".to_owned(),
                },
                edited.revision,
            )
            .expect("collapse newlines");
        assert!(multiline.source.contains("One line. And another."));

        let inserted = model
            .apply_graph_edit(
                &GraphEdit::SetKnotText {
                    knot: "loop".to_owned(),
                    text: "Back to the beginning.".to_owned(),
                },
                multiline.revision,
            )
            .expect("insert prose");
        assert!(
            inserted
                .source
                .contains("=== loop ===\nBack to the beginning.\n-> start")
        );

        let removed = model
            .apply_graph_edit(
                &GraphEdit::SetKnotText {
                    knot: "loop".to_owned(),
                    text: "   ".to_owned(),
                },
                inserted.revision,
            )
            .expect("remove prose");
        assert!(!removed.source.contains("Back to the beginning."));
        assert!(removed.source.contains("=== loop ===\n-> start"));
    }

    #[test]
    fn prose_that_parses_as_structure_is_rejected_without_a_change() {
        for hostile in [
            "* [Sneaky] -> shore",
            "+ [Always] -> shore",
            "-> shore",
            "<- shore",
            "=== hijack ===",
            "VAR score = 1",
            "@ scene dusk",
        ] {
            let mut model = CanonicalProjectModel::new(LINKED);
            let before = model.source().to_owned();
            let revision = model.revision();
            let conflict = model
                .apply_graph_edit(
                    &GraphEdit::SetKnotText {
                        knot: "start".to_owned(),
                        text: hostile.to_owned(),
                    },
                    revision,
                )
                .expect_err("structure injection");
            assert!(
                matches!(
                    conflict,
                    SyncConflict::NotRepresentable | SyncConflict::UnparsableEdit
                ),
                "text `{hostile}` must not become structure, got {conflict:?}"
            );
            assert_eq!(model.source(), before);
            assert_eq!(model.revision(), revision);
        }
        assert_eq!(
            CanonicalProjectModel::new(LINKED)
                .apply_graph_edit(
                    &GraphEdit::RenameKnot {
                        knot: "start".to_owned(),
                        name: "END".to_owned(),
                    },
                    0,
                )
                .expect_err("reserved name"),
            SyncConflict::ReservedName("END".to_owned())
        );
    }

    #[test]
    fn renaming_rewrites_references_choices_and_layout_keys() {
        let mut model = CanonicalProjectModel::new(LINKED);
        let mut layout = model.layout().clone();
        layout.insert("knot:start".to_owned(), GraphPoint::new(640.0, 240.0));
        layout.insert("choice:start:1".to_owned(), GraphPoint::new(900.0, 240.0));
        assert!(model.capture_layout(layout));

        let renamed = model
            .apply_graph_edit(
                &GraphEdit::RenameKnot {
                    knot: "start".to_owned(),
                    name: "opening".to_owned(),
                },
                model.revision(),
            )
            .expect("rename");
        assert!(renamed.source.contains("=== opening ==="));
        assert!(!renamed.source.contains("start"));
        assert!(renamed.source.contains("<- opening"));
        assert_eq!(renamed.focus_node.as_deref(), Some("knot:opening"));
        assert_eq!(
            model.layout().get("knot:opening"),
            Some(&GraphPoint::new(640.0, 240.0))
        );
        assert_eq!(
            model.layout().get("choice:opening:1"),
            Some(&GraphPoint::new(900.0, 240.0))
        );

        assert_eq!(
            model
                .apply_graph_edit(
                    &GraphEdit::RenameKnot {
                        knot: "opening".to_owned(),
                        name: "shore".to_owned(),
                    },
                    model.revision(),
                )
                .expect_err("duplicate name"),
            SyncConflict::DuplicateKnot("shore".to_owned())
        );
        assert_eq!(
            model
                .apply_graph_edit(
                    &GraphEdit::RenameKnot {
                        knot: "opening".to_owned(),
                        name: "9 lives".to_owned(),
                    },
                    model.revision(),
                )
                .expect_err("invalid identifier"),
            SyncConflict::InvalidIdentifier("9 lives".to_owned())
        );
    }

    #[test]
    fn removal_refuses_live_references_and_the_last_knot() {
        let mut model = CanonicalProjectModel::new(LINKED);
        assert_eq!(
            model
                .apply_graph_edit(
                    &GraphEdit::RemoveKnot {
                        knot: "start".to_owned(),
                    },
                    model.revision(),
                )
                .expect_err("referenced knot"),
            SyncConflict::KnotInUse {
                knot: "start".to_owned(),
                references: 2,
            }
        );

        let removed = model
            .apply_graph_edit(
                &GraphEdit::RemoveKnot {
                    knot: "loop".to_owned(),
                },
                model.revision(),
            )
            .expect("remove unreferenced knot");
        assert!(!removed.source.contains("=== loop ==="));
        assert_eq!(
            model
                .apply_graph_edit(
                    &GraphEdit::RemoveKnot {
                        knot: "shore".to_owned(),
                    },
                    model.revision(),
                )
                .expect_err("referenced by choice"),
            SyncConflict::KnotInUse {
                knot: "shore".to_owned(),
                references: 1,
            }
        );

        let mut single = CanonicalProjectModel::new(SOURCE);
        assert_eq!(
            single
                .apply_graph_edit(
                    &GraphEdit::RemoveKnot {
                        knot: "start".to_owned(),
                    },
                    single.revision(),
                )
                .expect_err("last knot"),
            SyncConflict::LastKnot
        );
    }

    #[test]
    fn choice_edits_use_statement_paths() {
        let mut model = CanonicalProjectModel::new(LINKED);
        let path = vec![1];
        let edited = model
            .apply_graph_edit(
                &GraphEdit::SetChoiceText {
                    knot: "start".to_owned(),
                    path: path.clone(),
                    text: "Flee".to_owned(),
                },
                model.revision(),
            )
            .expect("edit choice");
        assert!(edited.source.contains("* [Flee] -> shore"));
        assert_eq!(edited.focus_node.as_deref(), Some("choice:start:1"));

        let connected = model
            .apply_graph_edit(
                &GraphEdit::ConnectChoice {
                    knot: "start".to_owned(),
                    path: path.clone(),
                    target: "loop".to_owned(),
                },
                edited.revision,
            )
            .expect("reconnect choice");
        assert!(connected.source.contains("* [Flee] -> loop"));

        let missing = model
            .apply_graph_edit(
                &GraphEdit::SetChoiceText {
                    knot: "start".to_owned(),
                    path: vec![9],
                    text: "Nowhere".to_owned(),
                },
                connected.revision,
            )
            .expect_err("missing choice");
        assert_eq!(
            missing,
            SyncConflict::MissingChoice {
                knot: "start".to_owned(),
                path: vec![9],
            }
        );

        let before_removal = model.source().to_owned();
        let removed = model
            .apply_graph_edit(
                &GraphEdit::RemoveChoice {
                    knot: "start".to_owned(),
                    path,
                },
                connected.revision,
            )
            .expect("remove choice");
        assert!(!removed.source.contains("[Flee]"));
        assert_eq!(model.undo().as_deref(), Some(before_removal.as_str()));
    }

    #[test]
    fn raw_knot_source_replaces_only_that_knot() {
        let mut model = CanonicalProjectModel::new(LINKED);
        let edited = model
            .apply_graph_edit(
                &GraphEdit::ReplaceKnotSource {
                    knot: "shore".to_owned(),
                    source: "A new shore.\n@speaker Mara\n* [Return] -> loop".to_owned(),
                },
                model.revision(),
            )
            .expect("replace knot source");
        assert!(edited.source.contains("A new shore."));
        assert!(edited.source.contains("@speaker Mara"));
        assert!(edited.source.contains("* [Return] -> loop"));
        assert!(edited.source.contains("=== start ==="));
        assert_eq!(edited.focus_node.as_deref(), Some("knot:shore"));
        assert_eq!(model.undo().as_deref(), Some(LINKED));
    }

    #[test]
    fn raw_knot_source_rejections_leave_the_project_untouched() {
        for (source, expected) in [
            ("=== start ===".to_owned(), SyncConflict::NotRepresentable),
            (
                "A line.\n=== hijack ===\nOther.".to_owned(),
                SyncConflict::NotRepresentable,
            ),
            ("* [unterminated".to_owned(), SyncConflict::UnparsableEdit),
        ] {
            let mut model = CanonicalProjectModel::new(LINKED);
            let before = model.source().to_owned();
            let revision = model.revision();
            let conflict = model
                .apply_graph_edit(
                    &GraphEdit::ReplaceKnotSource {
                        knot: "shore".to_owned(),
                        source,
                    },
                    revision,
                )
                .expect_err("rejected source");
            assert_eq!(conflict, expected);
            assert_eq!(model.source(), before);
            assert_eq!(model.revision(), revision);
        }
    }

    proptest! {
        #[test]
        fn repeated_graph_text_graph_cycles_are_stable(count in 1_usize..32) {
            let mut model = CanonicalProjectModel::new(SOURCE);
            for index in 0..count {
                let applied = model.apply_graph_edit(
                    &GraphEdit::AddKnot {
                        preferred_name: format!("scene_{index}"),
                    },
                    model.revision(),
                ).expect("graph edit");
                let source = applied.source;
                let graph_before = GraphDocument::from_ast(model.graph_document())
                    .nodes()
                    .iter()
                    .map(|node| node.id.clone())
                    .collect::<Vec<_>>();
                let text = model.apply_text(source.clone(), model.revision()).expect("round trip");
                prop_assert_eq!(
                    text,
                    TextSync::Unchanged {
                        revision: model.revision(),
                    }
                );
                prop_assert_eq!(
                    weave_fmt::format_source(&source).expect("valid formatted source"),
                    source
                );
                let graph_after = GraphDocument::from_ast(model.graph_document())
                    .nodes()
                    .iter()
                    .map(|node| node.id.clone())
                    .collect::<Vec<_>>();
                prop_assert_eq!(graph_before, graph_after);
            }
        }
    }
}
