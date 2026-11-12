//! Per-knot rich blocks, view modes, and remembered view preferences.
//!
//! A knot is presented either as rich blocks with labelled controls or as raw
//! source. The default mode is a preference and every knot remembers the mode
//! its author last chose. Rich blocks are a projection of the parsed document;
//! every edit still goes through [`crate::sync::CanonicalProjectModel`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use weave_core::ast::{Cue, Document, Statement};

/// How one knot is presented to its author.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnotViewMode {
    /// Labelled blocks and buttons.
    #[default]
    Rich,
    /// Raw Weave source for the knot.
    Source,
}

impl KnotViewMode {
    /// Author-facing name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rich => "Rich",
            Self::Source => "Source",
        }
    }

    /// The other mode.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Rich => Self::Source,
            Self::Source => Self::Rich,
        }
    }
}

/// The preference default plus the remembered mode of each knot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotViewPreferences {
    default_mode: KnotViewMode,
    #[serde(default)]
    per_knot: BTreeMap<String, KnotViewMode>,
}

impl Default for KnotViewPreferences {
    fn default() -> Self {
        Self::new(KnotViewMode::Rich)
    }
}

impl KnotViewPreferences {
    /// Create preferences with an explicit default and no per-knot choices.
    #[must_use]
    pub fn new(default_mode: KnotViewMode) -> Self {
        Self {
            default_mode,
            per_knot: BTreeMap::new(),
        }
    }

    /// Mode used by a knot with no remembered choice.
    #[must_use]
    pub const fn default_mode(&self) -> KnotViewMode {
        self.default_mode
    }

    /// Change the preference default. Remembered knot choices are unaffected.
    pub fn set_default_mode(&mut self, mode: KnotViewMode) {
        self.default_mode = mode;
    }

    /// Mode to show for one knot: its remembered choice, else the default.
    #[must_use]
    pub fn mode_for(&self, knot: &str) -> KnotViewMode {
        self.per_knot
            .get(knot)
            .copied()
            .unwrap_or(self.default_mode)
    }

    /// Remember the mode chosen for one knot.
    pub fn set_mode_for(&mut self, knot: &str, mode: KnotViewMode) {
        self.per_knot.insert(knot.to_owned(), mode);
    }

    /// Forget one knot, for example after it is renamed or removed.
    pub fn forget(&mut self, knot: &str) {
        self.per_knot.remove(knot);
    }

    /// Rename a remembered knot so its choice survives a rename.
    pub fn rename(&mut self, old: &str, new: &str) {
        if let Some(mode) = self.per_knot.remove(old) {
            self.per_knot.insert(new.to_owned(), mode);
        }
    }

    /// Parse persisted preferences. Unknown knots are kept until explicitly
    /// forgotten, and unknown modes fail rather than guess.
    pub fn from_json(source: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(source)
    }

    /// Serialize preferences for project-local storage.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// One renderable block of a knot body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnotBlock {
    /// Statement path inside the knot body, matching graph choice coordinates.
    pub path: Vec<usize>,
    /// What the block shows and how it can be edited.
    pub kind: KnotBlockKind,
}

/// Editable or read-only content of one block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnotBlockKind {
    /// Narrative prose.
    Prose { text: String },
    /// Selectable branch.
    Choice {
        label: String,
        target: Option<String>,
        conditional: bool,
    },
    /// Unconditional divert.
    Divert { target: String },
    /// Call that returns.
    Thread { target: String },
    /// Story end.
    End,
    /// Presentation cue.
    Cue { description: String },
    /// Variable, list, or state declaration.
    Declaration { description: String },
    /// Variable assignment or list mutation.
    Assignment { description: String },
    /// Ordered conditional.
    Conditional { branches: usize },
    /// Source comment.
    Comment { text: String },
    /// Blank line.
    Blank,
}

impl KnotBlockKind {
    /// Short, author-facing block name.
    #[must_use]
    pub const fn title(&self) -> &'static str {
        match self {
            Self::Prose { .. } => "Prose",
            Self::Choice { .. } => "Choice",
            Self::Divert { .. } => "Divert",
            Self::Thread { .. } => "Thread",
            Self::End => "End",
            Self::Cue { .. } => "Cue",
            Self::Declaration { .. } => "Declaration",
            Self::Assignment { .. } => "Assignment",
            Self::Conditional { .. } => "Conditional",
            Self::Comment { .. } => "Comment",
            Self::Blank => "Blank",
        }
    }

    /// Whether rich mode can edit this block in place.
    #[must_use]
    pub const fn editable(&self) -> bool {
        matches!(self, Self::Prose { .. } | Self::Choice { .. })
    }

    /// Whether rich mode can remove this block.
    #[must_use]
    pub const fn removable(&self) -> bool {
        matches!(
            self,
            Self::Prose { .. }
                | Self::Choice { .. }
                | Self::Cue { .. }
                | Self::Comment { .. }
                | Self::Blank
                | Self::End
        )
    }
}

/// Project one knot into rich blocks.
///
/// Returns `None` when the knot does not exist.
#[must_use]
pub fn blocks(document: &Document, knot: &str) -> Option<Vec<KnotBlock>> {
    let body = document
        .knots()
        .find(|candidate| candidate.node.name == knot)?
        .node
        .body
        .clone();
    Some(
        body.iter()
            .enumerate()
            .map(|(index, statement)| KnotBlock {
                path: vec![index],
                kind: block_kind(&statement.node),
            })
            .collect(),
    )
}

/// The knot body exactly as the formatter writes it, without the knot header.
///
/// Source mode edits this text. Formatting a single-knot document is syntactic, so declarations
/// the knot references are not required here.
#[must_use]
pub fn knot_source(document: &Document, knot: &str) -> Option<String> {
    let item = document.items.iter().find_map(|item| match &item.node {
        weave_core::ast::Item::Knot(owner) if owner.node.name == knot => Some(item.clone()),
        _ => None,
    })?;
    let single = Document { items: vec![item] };
    let formatted = weave_fmt::format_document(&single);
    let body = formatted.split_once('\n').map_or("", |(_, rest)| rest);
    Some(body.trim_end_matches('\n').to_owned())
}

fn block_kind(statement: &Statement) -> KnotBlockKind {
    match statement {
        Statement::Text(text) => KnotBlockKind::Prose { text: text.clone() },
        Statement::Choice(choice) => KnotBlockKind::Choice {
            label: choice.text.clone(),
            target: choice.divert.clone(),
            conditional: choice.condition.is_some(),
        },
        Statement::Divert(target) => KnotBlockKind::Divert {
            target: target.clone(),
        },
        Statement::Thread(target) => KnotBlockKind::Thread {
            target: target.clone(),
        },
        Statement::End => KnotBlockKind::End,
        Statement::Cue(cue) => KnotBlockKind::Cue {
            description: cue_description(cue),
        },
        Statement::Declare(declaration) => KnotBlockKind::Declaration {
            description: format!("{:?} {}", declaration.kind, declaration.name),
        },
        Statement::Assign { name, .. } => KnotBlockKind::Assignment {
            description: format!("Set {name}"),
        },
        Statement::MutateList {
            operation, name, ..
        } => KnotBlockKind::Assignment {
            description: format!("{operation:?} {name}"),
        },
        Statement::Conditional(conditional) => KnotBlockKind::Conditional {
            branches: conditional.branches.len(),
        },
        Statement::Comment(text) => KnotBlockKind::Comment { text: text.clone() },
        Statement::Blank => KnotBlockKind::Blank,
    }
}

fn cue_description(cue: &Cue) -> String {
    match cue {
        Cue::Scene(name) => format!("Scene {name}"),
        Cue::Show(stage) => format!("Show {stage}"),
        Cue::Hide(stage) => format!("Hide {stage}"),
        Cue::Speaker(Some(speaker)) => format!("Speaker {speaker}"),
        Cue::Speaker(None) => "Narration".to_owned(),
        Cue::To(Some(channel)) => format!("To {channel}"),
        Cue::To(None) => "To narration".to_owned(),
        Cue::Move { stage, to, .. } => format!("Move {stage} to {to}"),
        Cue::Swap { stage, variant, .. } => format!("Swap {stage} to {variant}"),
        Cue::Transition { kind, ms, .. } => format!("Transition {kind} {ms}ms"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"=== start ===
The ferry leaves at dusk.
* [Board] -> shore
@speaker Mara
-> END
=== shore ===
The far bank.
"#;

    fn document() -> Document {
        weave_core::parse_document(SOURCE).expect("fixture parses")
    }

    #[test]
    fn blocks_project_every_statement_in_order() {
        let projected = blocks(&document(), "start").expect("knot exists");
        assert_eq!(projected.len(), 4);
        assert_eq!(projected[0].path, vec![0]);
        assert_eq!(
            projected[0].kind,
            KnotBlockKind::Prose {
                text: "The ferry leaves at dusk.".to_owned()
            }
        );
        assert_eq!(
            projected[1].kind,
            KnotBlockKind::Choice {
                label: "Board".to_owned(),
                target: Some("shore".to_owned()),
                conditional: false,
            }
        );
        assert_eq!(projected[2].kind.title(), "Cue");
        assert!(projected[3].kind.removable());
        assert!(!projected[3].kind.editable());
        assert!(blocks(&document(), "missing").is_none());
    }

    #[test]
    fn knot_source_is_the_formatted_body_without_the_header() {
        let document = document();
        let source = knot_source(&document, "start").expect("knot exists");
        assert!(!source.contains("=== start ==="));
        assert!(source.contains("The ferry leaves at dusk."));
        assert!(source.contains("* [Board] -> shore"));
        assert!(source.contains("@speaker Mara"));
        assert!(knot_source(&document, "missing").is_none());
    }

    #[test]
    fn preferences_remember_each_knot_and_keep_the_default() {
        let mut preferences = KnotViewPreferences::default();
        assert_eq!(preferences.mode_for("start"), KnotViewMode::Rich);
        preferences.set_mode_for("start", KnotViewMode::Source);
        assert_eq!(preferences.mode_for("start"), KnotViewMode::Source);
        assert_eq!(preferences.mode_for("shore"), KnotViewMode::Rich);

        preferences.set_default_mode(KnotViewMode::Source);
        assert_eq!(preferences.mode_for("shore"), KnotViewMode::Source);
        assert_eq!(preferences.mode_for("start"), KnotViewMode::Source);

        preferences.set_mode_for("start", KnotViewMode::Rich);
        preferences.set_default_mode(KnotViewMode::Rich);
        preferences.rename("start", "opening");
        assert_eq!(preferences.mode_for("opening"), KnotViewMode::Rich);
        assert_eq!(preferences.mode_for("start"), KnotViewMode::Rich);
        preferences.forget("opening");
        assert_eq!(preferences.mode_for("opening"), KnotViewMode::Rich);

        let json = preferences.to_json().expect("serializes");
        assert_eq!(
            KnotViewPreferences::from_json(&json).expect("parses"),
            preferences
        );
        assert!(KnotViewPreferences::from_json(r#"{"default_mode":"loud"}"#).is_err());
    }
}
