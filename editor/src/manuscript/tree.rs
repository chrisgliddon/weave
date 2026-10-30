//! The ordered chapter tree: structure, bodies, and the operations on both.
//!
//! A manuscript is one ordered child list per container. The tree holds the
//! structure; the body of a chapter is stored beside it, in its own file, and
//! is loaded only when that chapter is opened. A reorder therefore writes one
//! field, opening a chapter does not read the whole manuscript, and the tree
//! can be diffed on its own.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

/// Version of the persisted tree format.
pub const TREE_FORMAT_VERSION: u32 = 1;

/// Version of the persisted body format.
pub const BODY_FORMAT_VERSION: u32 = 1;

/// Name of the tree file beside a Weave project.
pub const TREE_FILE_NAME: &str = "weave.manuscript.ron";

/// Name of the directory holding chapter bodies.
pub const BODY_DIR_NAME: &str = "weave.manuscript";

/// Distance between neighbouring sibling order values.
///
/// Orders are spaced so that a reorder usually rewrites a single field. The
/// gap is exhausted only after many moves into the same gap, at which point
/// the siblings are renumbered.
const ORDER_GAP: u32 = 1024;

/// Stable identifier for a chapter in a manuscript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChapterId(pub u64);

impl ChapterId {
    /// The identifier used for the manuscript root.
    pub const ROOT: Self = Self(0);

    /// The identifier used for the manuscript trash.
    pub const TRASH: Self = Self(1);

    /// Whether this identifier is reserved for a built-in container.
    #[must_use]
    pub const fn is_reserved(self) -> bool {
        matches!(self.0, 0 | 1)
    }
}

impl std::fmt::Display for ChapterId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "chapter-{}", self.0)
    }
}

/// The structural role of a chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChapterKind {
    /// A chapter that may hold scenes.
    Chapter,
    /// A leaf scene inside a chapter.
    Scene,
    /// A free-form note.
    Note,
}

impl ChapterKind {
    /// Human-readable label for the kind.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Chapter => "Chapter",
            Self::Scene => "Scene",
            Self::Note => "Note",
        }
    }

    /// Whether a chapter of this kind may hold children.
    ///
    /// A scene is a leaf. A chapter holds scenes, and a note holds notes of
    /// its own. A node can be converted between the two roles with
    /// [`ChapterTree::set_kind`].
    #[must_use]
    pub const fn is_container(self) -> bool {
        matches!(self, Self::Chapter | Self::Note)
    }
}

/// A project-scoped status, referenced by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusDefinition {
    /// Stable identifier used by chapter records.
    pub id: i64,
    /// Display name.
    pub name: String,
}

/// A project-scoped label, referenced by id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelDefinition {
    /// Stable identifier used by chapter records.
    pub id: i64,
    /// Display name.
    pub name: String,
    /// Display colour, as three components in the range 0.0 to 1.0.
    pub color: [f32; 3],
}

/// One chapter in the manuscript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    /// Stable identifier, unique within the manuscript.
    pub id: ChapterId,
    /// The container this chapter belongs to.
    pub parent_id: ChapterId,
    /// Position among the parent's children.
    pub order: u32,
    /// Structural role.
    pub kind: ChapterKind,
    /// Display title.
    pub title: String,
    /// Identifier of the status, if one is set.
    pub status_id: Option<i64>,
    /// Identifier of the label, if one is set.
    pub label_id: Option<i64>,
    /// One-line summary of the chapter.
    pub synopsis: String,
}

impl Chapter {
    /// Whether this chapter is the manuscript root.
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.id.0 == 0
    }

    /// Whether this chapter is the manuscript trash.
    #[must_use]
    pub const fn is_trash(&self) -> bool {
        self.id.0 == 1
    }

    /// Whether this chapter is a descendant of `ancestor`.
    ///
    /// The root is an ancestor of every chapter. The trash is a child of the
    /// root, so a chapter that has been trashed is still a descendant of the
    /// root and can never be dropped back into it as a sibling.
    #[must_use]
    pub fn is_descendant_of(&self, ancestor: ChapterId, tree: &ChapterTree) -> bool {
        if self.id == ancestor {
            return false;
        }
        let mut current = self.parent_id;
        let mut steps = 0;
        while current != ChapterId::ROOT {
            if current == ancestor {
                return true;
            }
            let Some(chapter) = tree.chapter(current) else {
                return false;
            };
            current = chapter.parent_id;
            steps += 1;
            if steps > tree.chapters.len() {
                return false;
            }
        }
        ancestor == ChapterId::ROOT
    }
}

/// The persisted manuscript tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChapterTree {
    /// Version of the persisted format.
    pub version: u32,
    /// Every chapter, including the root and the trash.
    pub chapters: Vec<Chapter>,
    /// Project-scoped status list.
    pub statuses: Vec<StatusDefinition>,
    /// Project-scoped label list.
    pub labels: Vec<LabelDefinition>,
    /// The next unused chapter identifier.
    pub next_id: u64,
}

impl Default for ChapterTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ChapterTree {
    /// Create a tree holding only the root and the trash.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: TREE_FORMAT_VERSION,
            chapters: vec![
                Chapter {
                    id: ChapterId::ROOT,
                    parent_id: ChapterId::ROOT,
                    order: 0,
                    kind: ChapterKind::Chapter,
                    title: "Manuscript".to_string(),
                    status_id: None,
                    label_id: None,
                    synopsis: String::new(),
                },
                Chapter {
                    id: ChapterId::TRASH,
                    parent_id: ChapterId::ROOT,
                    order: u32::MAX,
                    kind: ChapterKind::Chapter,
                    title: "Trash".to_string(),
                    status_id: None,
                    label_id: None,
                    synopsis: String::new(),
                },
            ],
            statuses: Vec::new(),
            labels: Vec::new(),
            next_id: 2,
        }
    }

    /// Look up a chapter by identifier.
    #[must_use]
    pub fn chapter(&self, id: ChapterId) -> Option<&Chapter> {
        self.chapters.iter().find(|chapter| chapter.id == id)
    }

    /// Look up a chapter by identifier for mutation.
    fn chapter_mut(&mut self, id: ChapterId) -> Option<&mut Chapter> {
        self.chapters.iter_mut().find(|chapter| chapter.id == id)
    }

    /// The children of a container, in tree order.
    ///
    /// The trash always sorts last among the root's children, so it reads
    /// as a bin at the end of the list rather than as a sibling.
    #[must_use]
    pub fn children(&self, parent: ChapterId) -> Vec<&Chapter> {
        let mut children: Vec<&Chapter> = self
            .chapters
            .iter()
            .filter(|chapter| chapter.parent_id == parent && chapter.id != parent)
            .collect();
        children.sort_by_key(|chapter| (chapter.order, chapter.id));
        children
    }

    /// The identifiers of a container's children, in tree order.
    #[must_use]
    pub fn child_ids(&self, parent: ChapterId) -> Vec<ChapterId> {
        self.children(parent)
            .into_iter()
            .map(|chapter| chapter.id)
            .collect()
    }

    /// The number of chapters in the tree, including the built-in containers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.chapters.len()
    }

    /// Whether the tree holds nothing but the built-in containers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chapters.len() <= 2
    }

    /// The identifier that will be given to the next created chapter.
    #[must_use]
    pub fn next_id(&self) -> ChapterId {
        ChapterId(self.next_id)
    }

    /// Add a chapter to a container.
    ///
    /// The new chapter is placed after the last existing child.
    ///
    /// # Errors
    ///
    /// Returns an error when the parent is unknown, when the parent cannot
    /// hold children, or when the title is empty.
    pub fn add_chapter(
        &mut self,
        parent: ChapterId,
        kind: ChapterKind,
        title: &str,
    ) -> Result<ChapterId, ChapterError> {
        self.ensure_container(parent)?;
        if title.trim().is_empty() {
            return Err(ChapterError::EmptyTitle);
        }
        let id = self.next_id();
        self.next_id += 1;
        let order = self.next_child_order(parent);
        self.chapters.push(Chapter {
            id,
            parent_id: parent,
            order,
            kind,
            title: title.trim().to_string(),
            status_id: None,
            label_id: None,
            synopsis: String::new(),
        });
        Ok(id)
    }

    /// Move a chapter to a new position among its siblings.
    ///
    /// `index` is the position the chapter should end up at, counted among
    /// the parent's children with the chapter itself removed.
    ///
    /// # Errors
    ///
    /// Returns an error when the chapter is unknown, when it is a built-in
    /// container, when the index is out of range, or when the move would
    /// place a container inside its own descendant.
    pub fn move_chapter(&mut self, id: ChapterId, index: usize) -> Result<(), ChapterError> {
        let parent = self
            .chapter(id)
            .ok_or(ChapterError::UnknownChapter(id))?
            .parent_id;
        self.move_within(id, parent, Some(index))
    }

    /// Move a chapter to a new parent, at the end of that parent's children.
    ///
    /// # Errors
    ///
    /// Returns an error when either chapter is unknown, when the chapter is a
    /// built-in container, when the new parent cannot hold children, or when
    /// the move would place a container inside its own descendant.
    pub fn reparent_chapter(
        &mut self,
        id: ChapterId,
        new_parent: ChapterId,
    ) -> Result<(), ChapterError> {
        self.reparent_to(id, new_parent, None)
    }

    /// Move a chapter to a new parent at a given position.
    ///
    /// A position of `None` places the chapter at the end of the new
    /// parent's children.
    ///
    /// # Errors
    ///
    /// Returns an error when either chapter is unknown, when the chapter is a
    /// built-in container, when the new parent cannot hold children, when
    /// the index is out of range, or when the move would place a container
    /// inside its own descendant.
    pub fn reparent_to(
        &mut self,
        id: ChapterId,
        new_parent: ChapterId,
        index: Option<usize>,
    ) -> Result<(), ChapterError> {
        let source = self
            .chapter(id)
            .ok_or(ChapterError::UnknownChapter(id))?
            .parent_id;
        self.move_within(id, new_parent, index)?;
        if source != new_parent {
            self.set_parent(id, new_parent);
        }
        Ok(())
    }

    /// The index of a chapter among its siblings.
    fn sibling_index(&self, id: ChapterId) -> Result<usize, ChapterError> {
        let chapter = self.chapter(id).ok_or(ChapterError::UnknownChapter(id))?;
        self.child_ids(chapter.parent_id)
            .iter()
            .position(|sibling| *sibling == id)
            .ok_or(ChapterError::UnknownChapter(id))
    }

    /// Move a chapter one place earlier among its siblings.
    ///
    /// # Errors
    ///
    /// Returns [`MoveBlocked::FirstSibling`] when there is no earlier place.
    pub fn move_up(&mut self, id: ChapterId) -> Result<(), ChapterError> {
        let index = self.sibling_index(id)?;
        if index == 0 {
            return Err(ChapterError::Blocked(id, MoveBlocked::FirstSibling));
        }
        self.move_chapter(id, index - 1)
    }

    /// Move a chapter one place later among its siblings.
    ///
    /// # Errors
    ///
    /// Returns [`MoveBlocked::LastSibling`] when there is no later place.
    pub fn move_down(&mut self, id: ChapterId) -> Result<(), ChapterError> {
        let index = self.sibling_index(id)?;
        let len = self
            .chapter(id)
            .map(|chapter| self.child_ids(chapter.parent_id).len())
            .unwrap_or(0);
        if index + 1 >= len {
            return Err(ChapterError::Blocked(id, MoveBlocked::LastSibling));
        }
        self.move_chapter(id, index + 1)
    }

    /// Move a chapter to the start of its sibling list.
    ///
    /// # Errors
    ///
    /// Returns an error when the chapter is unknown or reserved.
    pub fn move_to_front(&mut self, id: ChapterId) -> Result<(), ChapterError> {
        self.move_chapter(id, 0)
    }

    /// Move a chapter to the end of its sibling list.
    ///
    /// # Errors
    ///
    /// Returns an error when the chapter is unknown or reserved.
    pub fn move_to_back(&mut self, id: ChapterId) -> Result<(), ChapterError> {
        let chapter = self.chapter(id).ok_or(ChapterError::UnknownChapter(id))?;
        let len = self.child_ids(chapter.parent_id).len();
        self.move_chapter(id, len.saturating_sub(1))
    }

    /// Promote a chapter one level, placing it just after its former parent.
    ///
    /// This is not expressible as a drag among siblings, so it is a command of
    /// its own.
    ///
    /// # Errors
    ///
    /// Returns [`MoveBlocked::AtRoot`] when the chapter is already a child of
    /// the manuscript root.
    pub fn promote(&mut self, id: ChapterId) -> Result<(), ChapterError> {
        let chapter = self.chapter(id).ok_or(ChapterError::UnknownChapter(id))?;
        let parent = chapter.parent_id;
        if parent == ChapterId::ROOT {
            return Err(ChapterError::Blocked(id, MoveBlocked::AtRoot));
        }
        let grandparent = self
            .chapter(parent)
            .ok_or(ChapterError::UnknownParent(id, parent))?
            .parent_id;
        let parent_index = self.sibling_index(parent)?;
        self.reparent_to(id, grandparent, Some(parent_index + 1))
    }

    /// Demote a chapter one level, making it the first child of the sibling
    /// before it.
    ///
    /// # Errors
    ///
    /// Returns [`MoveBlocked::NoEarlierSibling`] when there is nothing before
    /// it, and [`MoveBlocked::SiblingIsLeaf`] when that sibling cannot hold
    /// children.
    pub fn demote(&mut self, id: ChapterId) -> Result<(), ChapterError> {
        let index = self.sibling_index(id)?;
        if index == 0 {
            return Err(ChapterError::Blocked(id, MoveBlocked::NoEarlierSibling));
        }
        let chapter = self.chapter(id).ok_or(ChapterError::UnknownChapter(id))?;
        let parent = chapter.parent_id;
        let earlier = self.child_ids(parent)[index - 1];
        let target = self
            .chapter(earlier)
            .ok_or(ChapterError::UnknownChapter(earlier))?;
        if !target.kind.is_container() {
            return Err(ChapterError::Blocked(id, MoveBlocked::SiblingIsLeaf));
        }
        self.reparent_to(id, earlier, Some(0))
    }

    /// Move a chapter within the siblings of one parent.
    fn move_within(
        &mut self,
        id: ChapterId,
        new_parent: ChapterId,
        index: Option<usize>,
    ) -> Result<(), ChapterError> {
        let chapter = self.chapter(id).ok_or(ChapterError::UnknownChapter(id))?;
        if chapter.id.is_reserved() {
            return Err(ChapterError::ReservedChapter(id));
        }
        self.ensure_container(new_parent)?;
        if id == new_parent {
            return Err(ChapterError::Cycle(id, new_parent));
        }
        // Moving a container into its own subtree would detach that subtree
        // from the tree, so the move is refused.
        if self
            .chapter(new_parent)
            .is_some_and(|parent| parent.is_descendant_of(id, self))
        {
            return Err(ChapterError::Cycle(id, new_parent));
        }

        let siblings: Vec<ChapterId> = self
            .movable_children(new_parent)
            .into_iter()
            .map(|chapter| chapter.id)
            .filter(|sibling| *sibling != id)
            .collect();
        let index = index.unwrap_or(siblings.len());
        if index > siblings.len() {
            return Err(ChapterError::IndexOutOfRange {
                index,
                len: siblings.len(),
            });
        }

        let order = self.order_for_index(&siblings, index);
        let parent = self
            .chapter_mut(id)
            .ok_or(ChapterError::UnknownChapter(id))?;
        parent.parent_id = new_parent;
        parent.order = order;
        self.renumber_if_needed(new_parent);
        Ok(())
    }

    /// Set a chapter's parent without changing its order.
    fn set_parent(&mut self, id: ChapterId, parent: ChapterId) {
        if let Some(chapter) = self.chapter_mut(id) {
            chapter.parent_id = parent;
        }
    }

    /// The order value for a chapter added to the end of a container.
    ///
    /// The trash is skipped, because it always sorts last among the root's
    /// children and a new root child must land before it.
    fn next_child_order(&self, parent: ChapterId) -> u32 {
        self.children(parent)
            .iter()
            .rev()
            .find(|chapter| !chapter.is_trash())
            .map_or(ORDER_GAP, |chapter| chapter.order.saturating_add(ORDER_GAP))
    }

    /// The children of a container that can be reordered among themselves.
    ///
    /// The trash is excluded, because it is a bin rather than a member of
    /// the manuscript's order.
    #[must_use]
    fn movable_children(&self, parent: ChapterId) -> Vec<&Chapter> {
        self.children(parent)
            .into_iter()
            .filter(|chapter| !chapter.is_trash())
            .collect()
    }

    /// The order value that places a chapter at `index` among `siblings`.
    fn order_for_index(&self, siblings: &[ChapterId], index: usize) -> u32 {
        match (siblings.get(index.wrapping_sub(1)), siblings.get(index)) {
            (Some(before), Some(after)) => {
                let before_order = self.order_of(*before);
                let after_order = self.order_of(*after);
                match after_order.checked_sub(before_order) {
                    Some(gap) if gap > 1 => before_order + gap / 2,
                    _ => before_order.saturating_add(1),
                }
            }
            (Some(before), None) => self.order_of(*before).saturating_add(ORDER_GAP),
            (None, Some(after)) => self.order_of(*after) / 2,
            (None, None) => ORDER_GAP,
        }
    }

    fn order_of(&self, id: ChapterId) -> u32 {
        self.chapter(id).map_or(0, |chapter| chapter.order)
    }

    /// Renumber a container's children when their orders are no longer
    /// strictly increasing.
    ///
    /// The trash keeps its position at the end of the root's children.
    fn renumber_if_needed(&mut self, parent: ChapterId) {
        let children: Vec<ChapterId> = self
            .movable_children(parent)
            .into_iter()
            .map(|chapter| chapter.id)
            .collect();
        let mut previous = 0;
        let mut needs_renumber = false;
        for id in &children {
            let order = self.order_of(*id);
            if order <= previous {
                needs_renumber = true;
                break;
            }
            previous = order;
        }
        if !needs_renumber {
            return;
        }
        for (index, id) in children.into_iter().enumerate() {
            if let Some(chapter) = self.chapter_mut(id) {
                chapter.order = (index as u32 + 1) * ORDER_GAP;
            }
        }
    }

    /// Move a chapter into the trash.
    ///
    /// # Errors
    ///
    /// Returns an error when the chapter is unknown or is a built-in
    /// container.
    pub fn trash_chapter(&mut self, id: ChapterId) -> Result<(), ChapterError> {
        let trash = self
            .children(ChapterId::TRASH)
            .last()
            .map_or(ORDER_GAP, |chapter| chapter.order.saturating_add(ORDER_GAP));
        let chapter = self
            .chapter_mut(id)
            .ok_or(ChapterError::UnknownChapter(id))?;
        if chapter.id.is_reserved() {
            return Err(ChapterError::ReservedChapter(id));
        }
        chapter.parent_id = ChapterId::TRASH;
        chapter.order = trash;
        Ok(())
    }

    /// Restore a chapter from the trash to the root.
    ///
    /// # Errors
    ///
    /// Returns an error when the chapter is unknown or is a built-in
    /// container.
    pub fn restore_chapter(&mut self, id: ChapterId) -> Result<(), ChapterError> {
        let order = self.next_child_order(ChapterId::ROOT);
        let chapter = self
            .chapter_mut(id)
            .ok_or(ChapterError::UnknownChapter(id))?;
        if chapter.id.is_reserved() {
            return Err(ChapterError::ReservedChapter(id));
        }
        chapter.parent_id = ChapterId::ROOT;
        chapter.order = order;
        Ok(())
    }

    /// Change a chapter's kind.
    ///
    /// Converting a container into a leaf is refused while the container
    /// still holds children, so the move is never silently lossy.
    ///
    /// # Errors
    ///
    /// Returns an error when the chapter is unknown, when the kind is
    /// unchanged, or when the chapter holds children and cannot become a
    /// leaf.
    pub fn set_kind(&mut self, id: ChapterId, kind: ChapterKind) -> Result<(), ChapterError> {
        let chapter = self.chapter(id).ok_or(ChapterError::UnknownChapter(id))?;
        if chapter.kind == kind {
            return Err(ChapterError::UnchangedKind(id));
        }
        if !kind.is_container() && !self.children(id).is_empty() {
            return Err(ChapterError::NotEmpty(id));
        }
        let chapter = self
            .chapter_mut(id)
            .ok_or(ChapterError::UnknownChapter(id))?;
        chapter.kind = kind;
        Ok(())
    }

    /// Set a chapter's synopsis.
    ///
    /// # Errors
    ///
    /// Returns an error when the chapter is unknown.
    pub fn set_synopsis(&mut self, id: ChapterId, synopsis: &str) -> Result<(), ChapterError> {
        let chapter = self
            .chapter_mut(id)
            .ok_or(ChapterError::UnknownChapter(id))?;
        chapter.synopsis = synopsis.to_string();
        Ok(())
    }

    /// Set a chapter's title.
    ///
    /// # Errors
    ///
    /// Returns an error when the chapter is unknown or the title is empty.
    pub fn set_title(&mut self, id: ChapterId, title: &str) -> Result<(), ChapterError> {
        if title.trim().is_empty() {
            return Err(ChapterError::EmptyTitle);
        }
        let chapter = self
            .chapter_mut(id)
            .ok_or(ChapterError::UnknownChapter(id))?;
        chapter.title = title.trim().to_string();
        Ok(())
    }

    /// Set a chapter's status.
    ///
    /// # Errors
    ///
    /// Returns an error when the chapter is unknown or the status is not
    /// defined by the project.
    pub fn set_status(
        &mut self,
        id: ChapterId,
        status_id: Option<i64>,
    ) -> Result<(), ChapterError> {
        if let Some(status_id) = status_id
            && !self.statuses.iter().any(|status| status.id == status_id)
        {
            return Err(ChapterError::UnknownStatus(status_id));
        }
        let chapter = self
            .chapter_mut(id)
            .ok_or(ChapterError::UnknownChapter(id))?;
        chapter.status_id = status_id;
        Ok(())
    }
    /// Set a chapter's label.
    ///
    /// # Errors
    ///
    /// Returns an error when the chapter is unknown or the label is not
    /// defined by the project.
    pub fn set_label(&mut self, id: ChapterId, label_id: Option<i64>) -> Result<(), ChapterError> {
        if let Some(label_id) = label_id
            && !self.labels.iter().any(|label| label.id == label_id)
        {
            return Err(ChapterError::UnknownLabel(label_id));
        }
        let chapter = self
            .chapter_mut(id)
            .ok_or(ChapterError::UnknownChapter(id))?;
        chapter.label_id = label_id;
        Ok(())
    }

    /// Add a status to the project-scoped list.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is already in use.
    pub fn add_status(&mut self, name: &str) -> Result<i64, ChapterError> {
        let id = self
            .statuses
            .iter()
            .map(|status| status.id)
            .max()
            .unwrap_or(0)
            + 1;
        if self.statuses.iter().any(|status| status.id == id) {
            return Err(ChapterError::DuplicateStatus(id));
        }
        self.statuses.push(StatusDefinition {
            id,
            name: name.to_string(),
        });
        Ok(id)
    }

    /// Add a label to the project-scoped list.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is already in use.
    pub fn add_label(&mut self, name: &str, color: [f32; 3]) -> Result<i64, ChapterError> {
        let id = self.labels.iter().map(|label| label.id).max().unwrap_or(0) + 1;
        if self.labels.iter().any(|label| label.id == id) {
            return Err(ChapterError::DuplicateLabel(id));
        }
        self.labels.push(LabelDefinition {
            id,
            name: name.to_string(),
            color,
        });
        Ok(id)
    }

    /// Remove every chapter in the trash.
    pub fn empty_trash(&mut self) -> Vec<ChapterId> {
        let removed: Vec<ChapterId> = self.child_ids(ChapterId::TRASH).into_iter().collect();
        self.chapters
            .retain(|chapter| chapter.parent_id != ChapterId::TRASH);
        removed
    }

    /// Check that the tree is internally consistent.
    ///
    /// # Errors
    ///
    /// Returns an error naming the first inconsistency found.
    pub fn validate(&self) -> Result<(), ChapterError> {
        if self.version != TREE_FORMAT_VERSION {
            return Err(ChapterError::UnsupportedVersion(self.version));
        }
        let mut seen: BTreeSet<ChapterId> = BTreeSet::new();
        for chapter in &self.chapters {
            if !seen.insert(chapter.id) {
                return Err(ChapterError::DuplicateId(chapter.id));
            }
        }
        for chapter in &self.chapters {
            if chapter.is_root() {
                continue;
            }
            if !seen.contains(&chapter.parent_id) {
                return Err(ChapterError::UnknownParent(chapter.id, chapter.parent_id));
            }
            if chapter.is_descendant_of(chapter.id, self) {
                return Err(ChapterError::Cycle(chapter.id, chapter.parent_id));
            }
        }
        for parent in seen.iter().copied() {
            let mut orders: BTreeSet<u32> = BTreeSet::new();
            for chapter in self.movable_children(parent) {
                if !orders.insert(chapter.order) {
                    return Err(ChapterError::DuplicateOrder(parent, chapter.order));
                }
            }
        }
        Ok(())
    }

    /// Check that a container can hold children.
    fn ensure_container(&self, parent: ChapterId) -> Result<(), ChapterError> {
        let chapter = self
            .chapter(parent)
            .ok_or(ChapterError::UnknownChapter(parent))?;
        if !chapter.kind.is_container() {
            return Err(ChapterError::NotAContainer(parent));
        }
        Ok(())
    }

    /// The chapters of a container, keyed by identifier, for rendering.
    #[must_use]
    pub fn indexed(&self) -> HashMap<ChapterId, &Chapter> {
        self.chapters
            .iter()
            .map(|chapter| (chapter.id, chapter))
            .collect()
    }

    /// The status list, keyed by identifier.
    #[must_use]
    pub fn statuses_by_id(&self) -> BTreeMap<i64, &StatusDefinition> {
        self.statuses
            .iter()
            .map(|status| (status.id, status))
            .collect()
    }

    /// The label list, keyed by identifier.
    #[must_use]
    pub fn labels_by_id(&self) -> BTreeMap<i64, &LabelDefinition> {
        self.labels.iter().map(|label| (label.id, label)).collect()
    }
}

/// Why a positional move cannot happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveBlocked {
    /// The chapter is already the first of its siblings.
    FirstSibling,
    /// The chapter is already the last of its siblings.
    LastSibling,
    /// The chapter is already a child of the manuscript root.
    AtRoot,
    /// No earlier sibling exists to nest the chapter inside.
    NoEarlierSibling,
    /// The earlier sibling is a leaf and cannot hold children.
    SiblingIsLeaf,
}

impl std::fmt::Display for MoveBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::FirstSibling => "it is already the first sibling",
            Self::LastSibling => "it is already the last sibling",
            Self::AtRoot => "it is already at the root level",
            Self::NoEarlierSibling => "there is no earlier sibling to nest inside",
            Self::SiblingIsLeaf => "the earlier sibling cannot hold children",
        })
    }
}

/// Something that can go wrong while editing a chapter tree.
#[derive(Debug, thiserror::Error)]
pub enum ChapterError {
    /// The chapter identifier is not present in the tree.
    #[error("unknown chapter: {0}")]
    UnknownChapter(ChapterId),
    /// The chapter identifier is used more than once.
    #[error("duplicate chapter id: {0}")]
    DuplicateId(ChapterId),
    /// The parent identifier is not present in the tree.
    #[error("chapter {0} has unknown parent {1}")]
    UnknownParent(ChapterId, ChapterId),
    /// The chapter cannot hold children.
    #[error("chapter {0} is not a container")]
    NotAContainer(ChapterId),
    /// The chapter is one of the built-in containers.
    #[error("chapter {0} is a built-in container")]
    ReservedChapter(ChapterId),
    /// The chapter already has the requested kind.
    #[error("chapter {0} already has that kind")]
    UnchangedKind(ChapterId),
    /// The chapter holds children and cannot become a leaf.
    #[error("chapter {0} still holds children")]
    NotEmpty(ChapterId),
    /// The move would place a container inside its own descendant.
    #[error("chapter {0} cannot be moved into {1}")]
    Cycle(ChapterId, ChapterId),
    /// The relative move is not possible from the current position.
    #[error("chapter {0} cannot move: {1}")]
    Blocked(ChapterId, MoveBlocked),
    /// The requested position is past the end of the sibling list.
    #[error("position {index} is past the end of a {len} item list")]
    IndexOutOfRange {
        /// The requested position.
        index: usize,
        /// The number of positions available.
        len: usize,
    },
    /// Two siblings share an order value.
    #[error("siblings of {0} share order {1}")]
    DuplicateOrder(ChapterId, u32),
    /// The chapter title is empty.
    #[error("a chapter needs a title")]
    EmptyTitle,
    /// The status identifier is not defined by the project.
    #[error("unknown status: {0}")]
    UnknownStatus(i64),
    /// The label identifier is not defined by the project.
    #[error("unknown label: {0}")]
    UnknownLabel(i64),
    /// The status identifier is already in use.
    #[error("duplicate status: {0}")]
    DuplicateStatus(i64),
    /// The label identifier is already in use.
    #[error("duplicate label: {0}")]
    DuplicateLabel(i64),
    /// The persisted format version is not supported.
    #[error("unsupported manuscript format version: {0}")]
    UnsupportedVersion(u32),
    /// The manuscript file could not be read or written.
    #[error("manuscript I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// The manuscript file could not be parsed.
    #[error("manuscript data is invalid: {0}")]
    InvalidData(String),
}

impl PartialEq for ChapterError {
    /// Compare two errors by their meaning.
    ///
    /// The I/O variant compares by error kind and message, because
    /// `std::io::Error` has no equality of its own.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::UnknownChapter(left), Self::UnknownChapter(right)) => left == right,
            (Self::DuplicateId(left), Self::DuplicateId(right)) => left == right,
            (Self::UnknownParent(left, left_parent), Self::UnknownParent(right, right_parent)) => {
                left == right && left_parent == right_parent
            }
            (Self::NotAContainer(left), Self::NotAContainer(right)) => left == right,
            (Self::ReservedChapter(left), Self::ReservedChapter(right)) => left == right,
            (Self::UnchangedKind(left), Self::UnchangedKind(right)) => left == right,
            (Self::NotEmpty(left), Self::NotEmpty(right)) => left == right,
            (Self::Cycle(left, left_target), Self::Cycle(right, right_target)) => {
                left == right && left_target == right_target
            }
            (
                Self::IndexOutOfRange {
                    index: left_index,
                    len: left_len,
                },
                Self::IndexOutOfRange {
                    index: right_index,
                    len: right_len,
                },
            ) => left_index == right_index && left_len == right_len,
            (Self::DuplicateOrder(left, left_order), Self::DuplicateOrder(right, right_order)) => {
                left == right && left_order == right_order
            }
            (Self::EmptyTitle, Self::EmptyTitle) => true,
            (Self::UnknownStatus(left), Self::UnknownStatus(right)) => left == right,
            (Self::UnknownLabel(left), Self::UnknownLabel(right)) => left == right,
            (Self::DuplicateStatus(left), Self::DuplicateStatus(right)) => left == right,
            (Self::DuplicateLabel(left), Self::DuplicateLabel(right)) => left == right,
            (Self::UnsupportedVersion(left), Self::UnsupportedVersion(right)) => left == right,
            (Self::Io(left), Self::Io(right)) => {
                left.kind() == right.kind() && left.to_string() == right.to_string()
            }
            (Self::InvalidData(left), Self::InvalidData(right)) => left == right,
            (Self::Blocked(left, left_reason), Self::Blocked(right, right_reason)) => {
                left == right && left_reason == right_reason
            }
            _ => false,
        }
    }
}
