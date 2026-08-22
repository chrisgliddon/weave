//! Source-language model, parser, diagnostics, and static analysis for Weave.

/// Source abstract syntax tree.
pub mod ast;
/// Human-readable compiler diagnostics.
pub mod diagnostic;
/// Versioned runtime intermediate representation.
pub mod ir;
/// Parser for `.weave` source files.
pub mod parser;
/// Text template parsing shared by checking and lowering.
pub mod template;
/// Static name and type analysis.
pub mod typecheck;

pub use ast::{Document, Span, Spanned};
pub use diagnostic::{Diagnostic, DiagnosticCode, Severity, has_errors};
pub use parser::{parse_document, parse_expression};
pub use template::{TemplatePart, parse_template};
pub use typecheck::{
    DomainModuleSignature, PatternSignature, Type, TypeCheckResult, type_check,
    type_check_with_extensions, type_check_with_patterns,
};
