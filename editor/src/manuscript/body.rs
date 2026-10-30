//! Chapter bodies, stored beside the tree and loaded on demand.
//!
//! A body holds the prose of one chapter. Bodies live in their own files so
//! that the tree can be read, diffed, and reordered without touching the
//! prose, and so that opening one chapter does not read the others.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::tree::{BODY_FORMAT_VERSION, ChapterError, ChapterId, ChapterTree};

/// One run of text with a single character style.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TextRun {
    /// The text itself.
    pub text: String,
    /// The character style applied to the run, if any.
    pub character_style: Option<String>,
    /// Whether the run is bold.
    pub bold: bool,
    /// Whether the run is italic.
    pub italic: bool,
    /// Whether the run is underlined.
    pub underline: bool,
}

impl TextRun {
    /// Create a plain run of text.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            character_style: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

/// One paragraph of a body.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Paragraph {
    /// The runs of the paragraph, in order.
    pub runs: Vec<TextRun>,
    /// The paragraph style applied, if any.
    pub style: Option<String>,
}

impl Paragraph {
    /// Create a paragraph holding one run of plain text.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            runs: vec![TextRun::plain(text)],
            style: None,
        }
    }

    /// The plain text of the paragraph, with no styling.
    #[must_use]
    pub fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

/// The prose of one chapter.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChapterBody {
    /// Version of the persisted format.
    pub version: u32,
    /// The paragraphs of the body, in order.
    pub paragraphs: Vec<Paragraph>,
}

impl ChapterBody {
    /// Create an empty body.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: BODY_FORMAT_VERSION,
            paragraphs: Vec::new(),
        }
    }

    /// The plain text of the body, with paragraphs separated by newlines.
    #[must_use]
    pub fn text(&self) -> String {
        self.paragraphs
            .iter()
            .map(Paragraph::text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The number of words in the body.
    #[must_use]
    pub fn word_count(&self) -> usize {
        self.text()
            .split_whitespace()
            .filter(|word| !word.is_empty())
            .count()
    }

    /// The number of characters in the body.
    #[must_use]
    pub fn character_count(&self) -> usize {
        self.text().chars().count()
    }
}

/// The project-scoped style sheet.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StyleSheet {
    /// Version of the persisted format.
    pub version: u32,
    /// The named paragraph styles.
    pub paragraph_styles: Vec<NamedStyle>,
    /// The named character styles.
    pub character_styles: Vec<NamedStyle>,
}

/// One named style in the style sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedStyle {
    /// The name the style is referenced by.
    pub name: String,
    /// The point size of the style.
    pub size: f32,
    /// Whether the style is bold.
    pub bold: bool,
    /// Whether the style is italic.
    pub italic: bool,
}

impl StyleSheet {
    /// Create an empty style sheet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: BODY_FORMAT_VERSION,
            paragraph_styles: Vec::new(),
            character_styles: Vec::new(),
        }
    }
}

/// Reads and writes chapter bodies, one file per chapter.
#[derive(Debug, Clone)]
pub struct BodyStore {
    directory: PathBuf,
}

impl BodyStore {
    /// Open a store rooted at a directory.
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// The directory the store reads and writes.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The path a chapter's body is stored at.
    #[must_use]
    pub fn path_for(&self, id: ChapterId) -> PathBuf {
        self.directory.join(format!("{}.ron", id.0))
    }

    /// Read a chapter's body.
    ///
    /// A chapter with no body file reads as an empty body, so a body is
    /// optional rather than an error.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read or parsed.
    pub fn read(&self, id: ChapterId) -> Result<ChapterBody, ChapterError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Ok(ChapterBody::new());
        }
        let text = std::fs::read_to_string(&path)?;
        ron::from_str(&text).map_err(|error| ChapterError::InvalidData(error.to_string()))
    }

    /// Write a chapter's body.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory cannot be created or the file
    /// cannot be written.
    pub fn write(&self, id: ChapterId, body: &ChapterBody) -> Result<(), ChapterError> {
        let encoded = ron::ser::to_string_pretty(body, ron::ser::PrettyConfig::default())
            .map_err(|error| ChapterError::InvalidData(error.to_string()))?;
        crate::project::atomic_write(&self.path_for(id), encoded.as_bytes())?;
        Ok(())
    }

    /// Remove a chapter's body.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists and cannot be removed.
    pub fn remove(&self, id: ChapterId) -> Result<(), ChapterError> {
        let path = self.path_for(id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Whether a chapter has a body file.
    #[must_use]
    pub fn exists(&self, id: ChapterId) -> bool {
        self.path_for(id).exists()
    }
}

/// A manuscript: the tree, the bodies, and the style sheet.
#[derive(Debug, Clone)]
pub struct Manuscript {
    tree: ChapterTree,
    bodies: BodyStore,
}

impl Manuscript {
    /// Create a manuscript with a tree and a body store.
    #[must_use]
    pub fn new(tree: ChapterTree, bodies: BodyStore) -> Self {
        Self { tree, bodies }
    }

    /// The ordered chapter tree.
    #[must_use]
    pub fn tree(&self) -> &ChapterTree {
        &self.tree
    }

    /// The ordered chapter tree, for mutation.
    pub fn tree_mut(&mut self) -> &mut ChapterTree {
        &mut self.tree
    }

    /// The body store.
    #[must_use]
    pub fn bodies(&self) -> &BodyStore {
        &self.bodies
    }

    /// Read a chapter's body.
    ///
    /// # Errors
    ///
    /// Returns an error when the body file cannot be read or parsed.
    pub fn read_body(&self, id: ChapterId) -> Result<ChapterBody, ChapterError> {
        self.bodies.read(id)
    }

    /// Write a chapter's body.
    ///
    /// # Errors
    ///
    /// Returns an error when the body file cannot be written.
    pub fn write_body(&self, id: ChapterId, body: &ChapterBody) -> Result<(), ChapterError> {
        self.bodies.write(id, body)
    }
}
