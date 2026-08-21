//! Bridge to canonical compiler, runtime, and pattern data.

use weave_compiler::{CompileOptions, compile};
use weave_core::ir::StoryIr;
use weave_core::{Diagnostic, Document, parse_document};
use weave_patterns::{
    PatternDefinition, elder_futhark_definition, i_ching_definition, tarot_definition,
};
use weave_runtime::Story;

/// Failure while preparing editor-owned canonical domain state.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    /// Current source does not compile.
    #[error("source has {count} compiler diagnostic(s)")]
    Compile {
        /// Number of diagnostics retained for inline display.
        count: usize,
    },
    /// Compiled data could not initialize a preview runtime.
    #[error("runtime initialization failed: {0}")]
    Runtime(#[from] weave_runtime::RuntimeError),
}

/// Canonical source, compiled IR, diagnostics, and preview runtime for one editor session.
#[derive(Debug)]
pub struct DomainSession {
    source: String,
    source_name: Option<String>,
    seed: u64,
    diagnostics: Vec<Diagnostic>,
    document: Option<Document>,
    last_valid_story: Option<StoryIr>,
    runtime: Option<Story>,
}

impl DomainSession {
    /// Create an empty session with deterministic preview entropy.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            source: String::new(),
            source_name: None,
            seed,
            diagnostics: Vec::new(),
            document: None,
            last_valid_story: None,
            runtime: None,
        }
    }

    /// Compile source through `weave-compiler` and initialize `weave-runtime` on success.
    ///
    /// A failed compile replaces diagnostics but deliberately preserves the last valid IR and
    /// preview runtime.
    pub fn compile_source(
        &mut self,
        source: impl Into<String>,
        source_name: Option<String>,
    ) -> Result<(), DomainError> {
        self.source = source.into();
        self.source_name = source_name;
        let parsed = parse_document(&self.source);
        let compiled = match compile(
            &self.source,
            &CompileOptions {
                source_name: self.source_name.clone(),
            },
        ) {
            Ok(compiled) => compiled,
            Err(error) => {
                self.diagnostics = error.diagnostics;
                self.document = parsed.ok();
                return Err(DomainError::Compile {
                    count: self.diagnostics.len(),
                });
            }
        };
        let runtime = Story::with_seed(compiled.story.clone(), self.seed)?;
        self.diagnostics = compiled.diagnostics;
        self.document = parsed.ok();
        self.last_valid_story = Some(compiled.story);
        self.runtime = Some(runtime);
        Ok(())
    }

    /// Current source, including invalid in-progress edits.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Compiler diagnostics for the current source.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Current syntax tree when the in-progress source is parseable.
    #[must_use]
    pub const fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    /// Most recent valid compiled story.
    #[must_use]
    pub const fn last_valid_story(&self) -> Option<&StoryIr> {
        self.last_valid_story.as_ref()
    }

    /// Preview runtime created from the most recent valid source.
    #[must_use]
    pub const fn runtime(&self) -> Option<&Story> {
        self.runtime.as_ref()
    }

    /// Mutable preview runtime for play controls.
    #[must_use]
    pub const fn runtime_mut(&mut self) -> Option<&mut Story> {
        self.runtime.as_mut()
    }
}

/// Built-in definitions exposed to the editor without duplicating pattern records.
#[must_use]
pub fn builtin_pattern_definitions() -> Vec<PatternDefinition> {
    vec![
        tarot_definition(),
        i_ching_definition(),
        elder_futhark_definition(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SOURCE: &str = "=== start ===\nHello.\n-> END\n";

    #[test]
    fn compiler_runtime_and_pattern_models_are_shared_directly() {
        let mut session = DomainSession::new(17);
        session
            .compile_source(VALID_SOURCE, Some("story.weave".to_owned()))
            .expect("valid source initializes a preview");
        assert!(session.last_valid_story().is_some());
        assert!(session.runtime().is_some());
        assert!(session.diagnostics().is_empty());
        assert_eq!(builtin_pattern_definitions().len(), 3);
    }

    #[test]
    fn invalid_edits_keep_the_last_valid_preview() {
        let mut session = DomainSession::new(5);
        session
            .compile_source(VALID_SOURCE, None)
            .expect("valid source compiles");
        let valid_entry = session
            .last_valid_story()
            .expect("valid story")
            .entry
            .clone();
        let error = session
            .compile_source("=== broken", None)
            .expect_err("invalid source fails");
        assert!(matches!(error, DomainError::Compile { .. }));
        assert!(!session.diagnostics().is_empty());
        assert_eq!(
            session
                .last_valid_story()
                .expect("last valid remains")
                .entry,
            valid_entry
        );
        assert!(session.runtime().is_some());
    }
}
