//! Reading and writing a manuscript beside a Weave project file.

use std::path::{Path, PathBuf};

use super::body::{BodyStore, StyleSheet};
use super::tree::{BODY_DIR_NAME, ChapterError, ChapterTree, TREE_FILE_NAME, TREE_FORMAT_VERSION};
use crate::project::atomic_write;

/// A manuscript loaded from, or bound to, a project directory.
#[derive(Debug, Clone)]
pub struct ManuscriptStore {
    tree_path: PathBuf,
    body_store: BodyStore,
    style_path: PathBuf,
}

impl ManuscriptStore {
    /// Bind a store to the directory holding a project file.
    ///
    /// The tree and the bodies are written beside the project file, so a
    /// manuscript travels with the project it belongs to.
    #[must_use]
    pub fn beside(project_path: impl AsRef<Path>) -> Self {
        let directory = project_path
            .as_ref()
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        Self::in_directory(directory)
    }

    /// Bind a store to a directory.
    #[must_use]
    pub fn in_directory(directory: impl Into<PathBuf>) -> Self {
        let directory = directory.into();
        Self {
            tree_path: directory.join(TREE_FILE_NAME),
            body_store: BodyStore::new(directory.join(BODY_DIR_NAME)),
            style_path: directory.join(BODY_DIR_NAME).join("styles.ron"),
        }
    }

    /// The path the tree is written to.
    #[must_use]
    pub fn tree_path(&self) -> &Path {
        &self.tree_path
    }

    /// The path the style sheet is written to.
    #[must_use]
    pub fn style_path(&self) -> &Path {
        &self.style_path
    }

    /// The body store.
    #[must_use]
    pub fn bodies(&self) -> &BodyStore {
        &self.body_store
    }

    /// Whether a manuscript has been written to this location.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.tree_path.exists()
    }

    /// Read the tree.
    ///
    /// A location with no manuscript reads as a fresh tree, so a project
    /// without a manuscript opens cleanly.
    ///
    /// # Errors
    ///
    /// Returns an error when the tree file cannot be read or parsed.
    pub fn read_tree(&self) -> Result<ChapterTree, ChapterError> {
        if !self.tree_path.exists() {
            return Ok(ChapterTree::new());
        }
        let text = std::fs::read_to_string(&self.tree_path)?;
        let tree: ChapterTree =
            ron::from_str(&text).map_err(|error| ChapterError::InvalidData(error.to_string()))?;
        tree.validate()?;
        Ok(tree)
    }

    /// Write the tree.
    ///
    /// # Errors
    ///
    /// Returns an error when the tree is inconsistent, or when the file
    /// cannot be written.
    pub fn write_tree(&self, tree: &ChapterTree) -> Result<(), ChapterError> {
        tree.validate()?;
        let encoded = ron::ser::to_string_pretty(tree, ron::ser::PrettyConfig::default())
            .map_err(|error| ChapterError::InvalidData(error.to_string()))?;
        atomic_write(&self.tree_path, encoded.as_bytes())?;
        Ok(())
    }

    /// Read the style sheet.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read or parsed.
    pub fn read_styles(&self) -> Result<StyleSheet, ChapterError> {
        if !self.style_path.exists() {
            return Ok(StyleSheet::new());
        }
        let text = std::fs::read_to_string(&self.style_path)?;
        ron::from_str(&text).map_err(|error| ChapterError::InvalidData(error.to_string()))
    }

    /// Write the style sheet.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be written.
    pub fn write_styles(&self, styles: &StyleSheet) -> Result<(), ChapterError> {
        let encoded = ron::ser::to_string_pretty(styles, ron::ser::PrettyConfig::default())
            .map_err(|error| ChapterError::InvalidData(error.to_string()))?;
        atomic_write(&self.style_path, encoded.as_bytes())?;
        Ok(())
    }

    /// The version of the tree format this store writes.
    #[must_use]
    pub const fn format_version(&self) -> u32 {
        TREE_FORMAT_VERSION
    }
}
