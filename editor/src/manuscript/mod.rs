//! The ordered chapter tree, the chapter bodies, and the manuscript store.
//!
//! A manuscript is one ordered child list per container. The tree carries the
//! structure; each chapter's prose lives in its own file beside the tree and
//! is read only when that chapter is opened. A reorder therefore writes one
//! field, opening a chapter does not read the whole manuscript, and the tree
//! can be diffed on its own.

pub mod body;
pub mod editor;
pub mod format;
pub mod history;
pub mod session;
pub mod store;
pub mod tree;

pub use body::{BodyStore, ChapterBody, Manuscript, NamedStyle, Paragraph, StyleSheet, TextRun};
pub use editor::{
    DirectFormat, Movement, Position, Selection, all_direct, apply_character_style,
    apply_paragraph_style, backspace, insert, move_selection, split_paragraph, toggle_direct,
};
pub use format::{
    LineLayout, LinePiece, LineSegment, ManuscriptFormat, ResolvedStyle, VisualLine, layout,
    line_layout,
};
pub use history::{DEFAULT_HISTORY_LIMIT, ManuscriptHistory};
pub use session::{CaretRecord, ManuscriptSession, SESSION_FORMAT_VERSION};
pub use store::ManuscriptStore;
pub use tree::{
    BODY_DIR_NAME, BODY_FORMAT_VERSION, Chapter, ChapterError, ChapterId, ChapterKind, ChapterTree,
    LabelDefinition, MoveBlocked, StatusDefinition, TREE_FILE_NAME, TREE_FORMAT_VERSION,
};
