//! Editor-facing Character health summary and field navigation over the shared read-only audit.

use weave_character::{
    CharacterHealthDiagnostic, CharacterHealthDiagnosticCode, CharacterHealthDiagnosticScope,
    CharacterHealthError, CharacterHealthFilter, CharacterHealthProject, CharacterHealthReport,
    CharacterHealthSeverity, CharacterHealthSummary, audit_character_health,
    filter_character_health_report,
};

/// Stable editor navigation target for one exact aggregate health diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterHealthNavigationLink {
    pub diagnostic_id: String,
    pub code: CharacterHealthDiagnosticCode,
    pub severity: CharacterHealthSeverity,
    pub document_id: Option<String>,
    pub character_id: Option<String>,
    pub field_path: String,
    pub source_file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Read-only editor session. Refreshing the report cannot mutate any loaded project source.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterHealthSession {
    project: CharacterHealthProject,
    report: CharacterHealthReport,
}

impl CharacterHealthSession {
    /// Open and audit one exact loaded project through the same contract as the CLI.
    pub fn open(project: CharacterHealthProject) -> Result<Self, CharacterHealthError> {
        let report = audit_character_health(&project)?;
        Ok(Self { project, report })
    }

    /// Complete immutable report currently shown by the editor health surface.
    #[must_use]
    pub const fn report(&self) -> &CharacterHealthReport {
        &self.report
    }

    /// Compact counts for the editor status and health-summary panel.
    #[must_use]
    pub const fn summary(&self) -> &CharacterHealthSummary {
        &self.report.summary
    }

    /// Exact active and suppressed diagnostics in canonical report order.
    #[must_use]
    pub fn diagnostics(&self) -> &[CharacterHealthDiagnostic] {
        &self.report.diagnostics
    }

    /// Re-audit the same loaded bytes. The project is retained byte-for-byte.
    pub fn refresh(&mut self) -> Result<&CharacterHealthReport, CharacterHealthError> {
        self.report = audit_character_health(&self.project)?;
        Ok(&self.report)
    }

    /// Apply the shared non-mutating report filter used by CLI output.
    pub fn filtered(
        &self,
        filter: &CharacterHealthFilter,
    ) -> Result<CharacterHealthReport, CharacterHealthError> {
        filter_character_health_report(&self.report, filter)
    }

    /// Link every diagnostic to its relevant Character field and source coordinate when available.
    #[must_use]
    pub fn navigation_links(&self) -> Vec<CharacterHealthNavigationLink> {
        self.report
            .diagnostics
            .iter()
            .map(navigation_link)
            .collect()
    }

    /// Resolve one stable diagnostic id into an editor navigation target.
    #[must_use]
    pub fn navigation_link(&self, diagnostic_id: &str) -> Option<CharacterHealthNavigationLink> {
        self.report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.id == diagnostic_id)
            .map(navigation_link)
    }
}

fn navigation_link(diagnostic: &CharacterHealthDiagnostic) -> CharacterHealthNavigationLink {
    let (document_id, character_id) = match &diagnostic.scope {
        CharacterHealthDiagnosticScope::Collection => (None, None),
        CharacterHealthDiagnosticScope::Character { character_id } => {
            (None, Some(character_id.clone()))
        }
        CharacterHealthDiagnosticScope::Document { document_id } => {
            (Some(document_id.clone()), None)
        }
        CharacterHealthDiagnosticScope::CharacterDocument {
            document_id,
            character_id,
        } => (Some(document_id.clone()), Some(character_id.clone())),
    };
    CharacterHealthNavigationLink {
        diagnostic_id: diagnostic.id.clone(),
        code: diagnostic.code,
        severity: diagnostic.severity,
        document_id,
        character_id,
        field_path: diagnostic.path.clone(),
        source_file: diagnostic.source.as_ref().map(|source| source.file.clone()),
        line: diagnostic.source.as_ref().and_then(|source| source.line),
        column: diagnostic.source.as_ref().and_then(|source| source.column),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use weave_character::{CharacterHealthManifest, CharacterHealthProject};

    const MANIFEST: &str = include_str!(
        "../../examples/domain-modules/weave-character/health/unsafe/project.health-manifest.json"
    );
    const COLLECTION: &str = include_str!(
        "../../examples/domain-modules/weave-character/health/unsafe/collection.character-collection.json"
    );
    const EXPRESSION_PACK: &str = include_str!(
        "../../examples/domain-modules/weave-character/health/unsafe/unsafe.expression-pack.json"
    );
    const REPORT: &str = include_str!(
        "../../examples/domain-modules/weave-character/health/unsafe/report.health-report.json"
    );

    #[test]
    fn editor_health_summary_and_navigation_share_the_read_only_audit_contract() {
        let manifest = CharacterHealthManifest::from_json(MANIFEST).unwrap();
        let documents = manifest
            .documents
            .iter()
            .map(|document| {
                let source = match document.path.as_str() {
                    "collection.character-collection.json" => COLLECTION,
                    "unsafe.expression-pack.json" => EXPRESSION_PACK,
                    path => panic!("unexpected health fixture source: {path}"),
                };
                (document.id.clone(), source.to_owned())
            })
            .collect::<BTreeMap<_, _>>();
        let project = CharacterHealthProject::new(manifest, documents).unwrap();
        let original = project.clone();
        let expected = CharacterHealthReport::from_json(REPORT).unwrap();
        let mut session = CharacterHealthSession::open(project).unwrap();

        assert_eq!(session.report(), &expected);
        assert_eq!(session.summary().active_diagnostics, 1);
        let links = session.navigation_links();
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].code,
            CharacterHealthDiagnosticCode::UnsafePersonalization
        );
        assert_eq!(
            links[0].field_path,
            "templates.arrival_greeting.placeholders.secret_token"
        );
        assert_eq!(
            links[0].source_file.as_deref(),
            Some("unsafe.expression-pack.json")
        );
        assert_eq!(
            session.navigation_link(&links[0].diagnostic_id),
            Some(links[0].clone())
        );

        let filtered = session
            .filtered(&CharacterHealthFilter {
                codes: BTreeSet::from([CharacterHealthDiagnosticCode::UnsafePersonalization]),
                ..CharacterHealthFilter::default()
            })
            .unwrap();
        assert_eq!(filtered.diagnostics, expected.diagnostics);
        assert_eq!(session.refresh().unwrap(), &expected);
        assert_eq!(session.project, original);
    }
}
