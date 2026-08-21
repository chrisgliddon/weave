//! Canonical source/AST/graph synchronization with revision conflicts and coherent history.

use std::collections::{BTreeMap, BTreeSet};

use weave_core::ast::{Choice, Document, Item, Knot, Span, Spanned, Statement};
use weave_core::{Diagnostic, parse_document};

use crate::graph::{GraphDocument, GraphPoint};

const HISTORY_LIMIT: usize = 500;

/// Semantic edit requested by the graph surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphEdit {
    AddKnot {
        preferred_name: String,
    },
    AddChoice {
        knot: String,
        label: String,
        target: String,
    },
    ConnectKnots {
        source: String,
        target: String,
        thread: bool,
    },
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
        let reparsed = parse_document(&source)
            .expect("formatter output from a parsed and locally-mutated document remains valid");
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
