//! Editor-facing normalized expression authoring and deterministic dialogue preview.

use weave_character::{
    CharacterProfile, ExpressionAssignmentReceipt, ExpressionAssignmentRequest,
    ExpressionCoverageReport, ExpressionError, ExpressionFilter, ExpressionLintReport,
    ExpressionPack, ExpressionRecord, ExpressionRecordKind, ExpressionResolution,
    ExpressionResolutionRequest, ExpressionRevision, apply_expression_revision,
    assign_expression_pack, inspect_expression_coverage, lint_expression, list_expression_records,
    normalize_expression_revision, resolve_expression_dialogue, show_expression_record,
    validate_expression_pack, validate_expression_profile,
};

/// In-memory editor surface shared by expression forms, inspectors, and dialogue previews.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpressionSession {
    profile: CharacterProfile,
    packs: Vec<ExpressionPack>,
    last_assignment: Option<ExpressionAssignmentReceipt>,
}

impl ExpressionSession {
    /// Open exact profile and public-pack inputs without invoking any external service.
    pub fn open(
        profile: CharacterProfile,
        packs: Vec<ExpressionPack>,
    ) -> Result<Self, ExpressionError> {
        for pack in &packs {
            validate_expression_pack(pack)?;
        }
        validate_expression_profile(&profile, &packs)?;
        Ok(Self {
            profile,
            packs,
            last_assignment: None,
        })
    }

    /// Current accepted Character state.
    #[must_use]
    pub const fn profile(&self) -> &CharacterProfile {
        &self.profile
    }

    /// Exact immutable public packs available to the session.
    #[must_use]
    pub fn packs(&self) -> &[ExpressionPack] {
        &self.packs
    }

    /// Most recently reproduced assignment receipt, including a dry-run receipt.
    #[must_use]
    pub const fn last_assignment(&self) -> Option<&ExpressionAssignmentReceipt> {
        self.last_assignment.as_ref()
    }

    /// Canonically normalize an authored revision without changing editor state.
    pub fn normalize(revision: &ExpressionRevision) -> Result<ExpressionRevision, ExpressionError> {
        normalize_expression_revision(revision)
    }

    /// Add, edit, or remove expression records atomically through one normalized revision.
    pub fn revise(
        &mut self,
        revision: &ExpressionRevision,
        dry_run: bool,
    ) -> Result<CharacterProfile, ExpressionError> {
        let output = apply_expression_revision(&self.profile, revision)?;
        if !dry_run {
            self.profile = output.clone();
            self.last_assignment = None;
        }
        Ok(output)
    }

    /// Assign reviewed records from one exact available pack and optionally commit the result.
    pub fn assign(
        &mut self,
        pack_id: &str,
        request: &ExpressionAssignmentRequest,
        dry_run: bool,
    ) -> Result<&ExpressionAssignmentReceipt, ExpressionError> {
        let pack = self
            .packs
            .iter()
            .find(|pack| pack.id == pack_id)
            .ok_or_else(missing_editor_pack)?;
        let receipt = assign_expression_pack(&self.profile, pack, request)?;
        if !dry_run {
            self.profile = receipt.output_profile.clone();
        }
        self.last_assignment = Some(receipt);
        Ok(self
            .last_assignment
            .as_ref()
            .expect("assignment receipt was retained"))
    }

    /// List exact normalized records using the same deterministic filter as the CLI.
    pub fn list(
        &self,
        filter: &ExpressionFilter,
    ) -> Result<Vec<ExpressionRecord>, ExpressionError> {
        list_expression_records(&self.profile, filter)
    }

    /// Show one exact normalized expression record.
    pub fn show(
        &self,
        kind: ExpressionRecordKind,
        id: &str,
    ) -> Result<Option<ExpressionRecord>, ExpressionError> {
        show_expression_record(&self.profile, kind, id)
    }

    /// Return non-mutating editorial and contract diagnostics without rewriting text.
    pub fn lint(&self) -> Result<ExpressionLintReport, ExpressionError> {
        lint_expression(&self.profile, &self.packs)
    }

    /// Inspect complete record, category, and dialogue-scenario coverage.
    pub fn coverage(&self) -> Result<ExpressionCoverageReport, ExpressionError> {
        inspect_expression_coverage(&self.profile, &self.packs)
    }

    /// Validate current expression data against every exact available pack.
    pub fn validate(&self) -> Result<(), ExpressionError> {
        validate_expression_profile(&self.profile, &self.packs)
    }

    /// Preview one assigned line deterministically using only typed public context.
    pub fn resolve(
        &self,
        pack_id: &str,
        request: &ExpressionResolutionRequest,
    ) -> Result<ExpressionResolution, ExpressionError> {
        let pack = self
            .packs
            .iter()
            .find(|pack| pack.id == pack_id)
            .ok_or_else(missing_editor_pack)?;
        resolve_expression_dialogue(&self.profile, pack, request)
    }
}

fn missing_editor_pack() -> ExpressionError {
    // Reuse the public redaction-safe contract rather than exposing an editor-only error payload.
    ExpressionPack::from_json("{}").expect_err("empty expression pack is invalid")
}

#[cfg(test)]
mod tests {
    use super::*;

    const REVISED: &str = include_str!(
        "../../examples/domain-modules/weave-character/expression/revised.character.json"
    );
    const PACK: &str = include_str!(
        "../../examples/domain-modules/weave-character/expression/glasswind.expression-pack.json"
    );
    const RAW_REVISION: &str = include_str!(
        "../../examples/domain-modules/weave-character/expression/raw.expression-revision.json"
    );
    const NORMALIZED_REVISION: &str = include_str!(
        "../../examples/domain-modules/weave-character/expression/normalized.expression-revision.json"
    );
    const REQUEST: &str = include_str!(
        "../../examples/domain-modules/weave-character/expression/assignment.expression-request.json"
    );
    const RECEIPT: &str = include_str!(
        "../../examples/domain-modules/weave-character/expression/assignment.expression-receipt.json"
    );
    const RESOLUTION_REQUEST: &str = include_str!(
        "../../examples/domain-modules/weave-character/expression/contextual.expression-resolution-request.json"
    );
    const RESOLUTION: &str = include_str!(
        "../../examples/domain-modules/weave-character/expression/contextual.expression-resolution.json"
    );

    fn session() -> ExpressionSession {
        ExpressionSession::open(
            CharacterProfile::from_json(REVISED).unwrap(),
            vec![ExpressionPack::from_json(PACK).unwrap()],
        )
        .unwrap()
    }

    #[test]
    fn editor_normalizes_without_rewriting_the_input() {
        let raw: ExpressionRevision = weave_domain::parse_strict_json(RAW_REVISION).unwrap();
        let original = raw.clone();
        let normalized = ExpressionSession::normalize(&raw).unwrap();
        assert_eq!(raw, original);
        assert_eq!(
            normalized,
            ExpressionRevision::from_json(NORMALIZED_REVISION).unwrap()
        );
    }

    #[test]
    fn editor_assignment_dry_run_and_commit_are_atomic() {
        let mut session = session();
        let request = ExpressionAssignmentRequest::from_json(REQUEST).unwrap();
        let expected = ExpressionAssignmentReceipt::from_json(RECEIPT).unwrap();
        let input = session.profile().clone();

        let dry_run = session
            .assign("org.weave.expression.glasswind", &request, true)
            .unwrap()
            .clone();
        assert_eq!(dry_run, expected);
        assert_eq!(session.profile(), &input);

        let committed = session
            .assign("org.weave.expression.glasswind", &request, false)
            .unwrap()
            .clone();
        assert_eq!(committed, expected);
        assert_eq!(session.profile(), &expected.output_profile);
        assert!(session.validate().is_ok());
        assert!(session.coverage().unwrap().diagnostics.is_empty());
    }

    #[test]
    fn editor_list_show_lint_and_resolution_share_the_checked_contract() {
        let mut session = session();
        let request = ExpressionAssignmentRequest::from_json(REQUEST).unwrap();
        session
            .assign("org.weave.expression.glasswind", &request, false)
            .unwrap();

        let terms = session
            .list(&ExpressionFilter {
                kinds: vec![ExpressionRecordKind::Term],
                ..ExpressionFilter::default()
            })
            .unwrap();
        assert!(terms.len() >= 3);
        assert!(
            session
                .show(ExpressionRecordKind::Term, "trailmark")
                .unwrap()
                .is_some()
        );
        assert!(session.lint().unwrap().diagnostics.is_empty());

        let resolution_request =
            ExpressionResolutionRequest::from_json(RESOLUTION_REQUEST).unwrap();
        let resolution = session
            .resolve("org.weave.expression.glasswind", &resolution_request)
            .unwrap();
        assert_eq!(
            resolution,
            ExpressionResolution::from_json(RESOLUTION).unwrap()
        );
    }
}
