//! Diagnostic types shared by the parser, checker, and compiler.

use std::fmt;

use crate::ast::Span;

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Compilation cannot continue.
    Error,
    /// Compilation can continue, but the source is suspicious.
    Warning,
}

/// Stable diagnostic identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticCode(pub &'static str);

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

/// A source diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable code for tools and tests.
    pub code: DiagnosticCode,
    /// Severity of the problem.
    pub severity: Severity,
    /// Human-readable explanation.
    pub message: String,
    /// Relevant source location, when known.
    pub span: Option<Span>,
    /// Optional remediation guidance.
    pub help: Option<String>,
}

impl Diagnostic {
    /// Construct an error diagnostic.
    #[must_use]
    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code: DiagnosticCode(code),
            severity: Severity::Error,
            message: message.into(),
            span: None,
            help: None,
        }
    }

    /// Construct a warning diagnostic.
    #[must_use]
    pub fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code: DiagnosticCode(code),
            severity: Severity::Warning,
            message: message.into(),
            span: None,
            help: None,
        }
    }

    /// Attach a source location.
    #[must_use]
    pub const fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach remediation guidance.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        if let Some(span) = self.span {
            write!(
                formatter,
                "{severity}[{}] at {}:{}: {}",
                self.code, span.line, span.column, self.message
            )?;
        } else {
            write!(formatter, "{severity}[{}]: {}", self.code, self.message)?;
        }
        if let Some(help) = &self.help {
            write!(formatter, "\n  help: {help}")?;
        }
        Ok(())
    }
}

/// Whether a diagnostic collection contains at least one error.
#[must_use]
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
}
