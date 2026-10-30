//! Per-chapter caret and selection, remembered across chapter switches.
//!
//! This is view state, not manuscript data, so it lives in its own file beside
//! the bodies. Losing it costs a caret position, not a word.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::editor::{Position, Selection};
use super::tree::ChapterId;

/// Version of the persisted session format.
pub const SESSION_FORMAT_VERSION: u32 = 1;

/// One chapter's caret and selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaretRecord {
    /// Paragraph of the selection anchor.
    pub anchor_paragraph: u32,
    /// Character offset of the selection anchor.
    pub anchor_offset: u32,
    /// Paragraph of the caret.
    pub caret_paragraph: u32,
    /// Character offset of the caret.
    pub caret_offset: u32,
}

impl From<Selection> for CaretRecord {
    fn from(selection: Selection) -> Self {
        Self {
            anchor_paragraph: selection.anchor.paragraph as u32,
            anchor_offset: selection.anchor.offset as u32,
            caret_paragraph: selection.caret.paragraph as u32,
            caret_offset: selection.caret.offset as u32,
        }
    }
}

impl From<CaretRecord> for Selection {
    fn from(record: CaretRecord) -> Self {
        Self {
            anchor: Position {
                paragraph: record.anchor_paragraph as usize,
                offset: record.anchor_offset as usize,
            },
            caret: Position {
                paragraph: record.caret_paragraph as usize,
                offset: record.caret_offset as usize,
            },
        }
    }
}

/// The editor's remembered positions for one manuscript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManuscriptSession {
    /// Version of the persisted format.
    pub version: u32,
    /// Caret per chapter id.
    pub carets: BTreeMap<u64, CaretRecord>,
}

impl Default for ManuscriptSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ManuscriptSession {
    /// Create an empty session.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: SESSION_FORMAT_VERSION,
            carets: BTreeMap::new(),
        }
    }

    /// The remembered selection for a chapter.
    #[must_use]
    pub fn selection(&self, id: ChapterId) -> Option<Selection> {
        self.carets.get(&id.0).copied().map(Selection::from)
    }

    /// Remember a selection for a chapter.
    pub fn set_selection(&mut self, id: ChapterId, selection: Selection) {
        self.carets.insert(id.0, CaretRecord::from(selection));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_round_trips_through_its_record() {
        let selection = Selection {
            anchor: Position {
                paragraph: 2,
                offset: 5,
            },
            caret: Position {
                paragraph: 4,
                offset: 1,
            },
        };
        let session = ManuscriptSession {
            version: SESSION_FORMAT_VERSION,
            carets: BTreeMap::from([(7, CaretRecord::from(selection))]),
        };
        assert_eq!(session.selection(ChapterId(7)), Some(selection));
        assert_eq!(session.selection(ChapterId(8)), None);
    }

    #[test]
    fn the_session_round_trips_through_ron() {
        let mut session = ManuscriptSession::new();
        session.set_selection(
            ChapterId(3),
            Selection::collapsed(Position {
                paragraph: 1,
                offset: 9,
            }),
        );
        let encoded = ron::ser::to_string(&session).expect("encode");
        let decoded: ManuscriptSession = ron::from_str(&encoded).expect("decode");
        assert_eq!(decoded, session);
    }
}
