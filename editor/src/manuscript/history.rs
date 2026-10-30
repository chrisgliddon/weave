//! Bounded undo history for structural manuscript edits.
//!
//! Every structural change records the tree as it was before the change. A
//! whole-tree snapshot is affordable here because the tree is the structure
//! only: the prose lives in separate body files and is never copied. Undo
//! therefore restores names, order, and placement without touching a word of
//! the manuscript.

use super::tree::ChapterTree;

/// How many structural edits stay undoable.
pub const DEFAULT_HISTORY_LIMIT: usize = 100;

/// Snapshots of the tree taken before each structural edit.
#[derive(Debug, Clone)]
pub struct ManuscriptHistory {
    past: Vec<ChapterTree>,
    limit: usize,
}

impl Default for ManuscriptHistory {
    fn default() -> Self {
        Self::new(DEFAULT_HISTORY_LIMIT)
    }
}

impl ManuscriptHistory {
    /// Create a history that keeps at most `limit` snapshots.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            past: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Record the tree as it was before a change.
    pub fn record(&mut self, tree: &ChapterTree) {
        if self.past.len() == self.limit {
            self.past.remove(0);
        }
        self.past.push(tree.clone());
    }

    /// Take the most recent snapshot, if any.
    pub fn undo(&mut self) -> Option<ChapterTree> {
        self.past.pop()
    }

    /// Whether an undo is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// Number of undo steps available.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.past.len()
    }

    /// Drop every snapshot, for example when a different project is opened.
    pub fn clear(&mut self) {
        self.past.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manuscript::tree::{ChapterId, ChapterKind};

    fn edited(limit: usize) -> (ChapterTree, ManuscriptHistory, ChapterId) {
        let mut tree = ChapterTree::new();
        let id = tree
            .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "One")
            .expect("chapter");
        let history = ManuscriptHistory::new(limit);
        (tree, history, id)
    }

    #[test]
    fn undo_returns_the_tree_from_before_the_change() {
        let (mut tree, mut history, id) = edited(DEFAULT_HISTORY_LIMIT);
        history.record(&tree);
        tree.set_title(id, "Renamed").expect("rename");

        let restored = history.undo().expect("snapshot");
        assert_eq!(restored.chapter(id).expect("chapter").title, "One");
        assert_eq!(tree.chapter(id).expect("chapter").title, "Renamed");
        assert!(!history.can_undo());
    }

    #[test]
    fn undo_steps_back_through_several_edits_in_order() {
        let (mut tree, mut history, id) = edited(DEFAULT_HISTORY_LIMIT);
        for title in ["Two", "Three", "Four"] {
            history.record(&tree);
            tree.set_title(id, title).expect("rename");
        }
        assert_eq!(history.depth(), 3);

        let third = history.undo().expect("third");
        assert_eq!(third.chapter(id).expect("chapter").title, "Three");
        let second = history.undo().expect("second");
        assert_eq!(second.chapter(id).expect("chapter").title, "Two");
        let first = history.undo().expect("first");
        assert_eq!(first.chapter(id).expect("chapter").title, "One");
        assert!(history.undo().is_none());
    }

    #[test]
    fn the_oldest_snapshot_is_dropped_when_the_limit_is_reached() {
        let (mut tree, mut history, id) = edited(2);
        for title in ["Two", "Three", "Four"] {
            history.record(&tree);
            tree.set_title(id, title).expect("rename");
        }
        assert_eq!(history.depth(), 2);
        assert_eq!(
            history
                .undo()
                .expect("newest")
                .chapter(id)
                .expect("chapter")
                .title,
            "Three"
        );
        assert_eq!(
            history
                .undo()
                .expect("older")
                .chapter(id)
                .expect("chapter")
                .title,
            "Two"
        );
        assert!(history.undo().is_none());
    }

    #[test]
    fn clearing_history_removes_every_snapshot() {
        let (tree, mut history, _id) = edited(DEFAULT_HISTORY_LIMIT);
        history.record(&tree);
        history.clear();
        assert!(!history.can_undo());
        assert_eq!(history.depth(), 0);
    }
}
