//! Editor-facing relationship graph inspection and complete review workflow.

use std::collections::BTreeMap;

use weave_character::{
    CharacterCollection, RelationshipDiagnostic, RelationshipEdgeView, RelationshipError,
    RelationshipFilter, RelationshipGraphPolicy, RelationshipGraphRevision, RelationshipKindPack,
    RelationshipProposal, RelationshipProposalConfig, RelationshipReceipt,
    RelationshipReconciliationReport, RelationshipReview, RelationshipReviewDecision,
    apply_relationship_graph_revision, apply_reviewed_relationships, create_relationship_review,
    list_relationships, propose_relationships, reconcile_relationship_graph,
    relationship_dense_matrix_review_csv, relationship_edge_review_csv,
    relationship_graph_diagnostics, validate_relationship_graph, validate_relationship_proposal,
};

/// In-memory editor surface for relationship graph authoring, inspection, and atomic review.
#[derive(Debug, Clone, PartialEq)]
pub struct RelationshipSession {
    collection: CharacterCollection,
    pack: RelationshipKindPack,
    policy: RelationshipGraphPolicy,
    last_proposal: Option<RelationshipProposal>,
    last_receipt: Option<RelationshipReceipt>,
}

impl RelationshipSession {
    /// Open exact graph inputs. Semantic graph problems remain available to reconciliation.
    pub fn open(
        collection: CharacterCollection,
        pack: RelationshipKindPack,
        policy: RelationshipGraphPolicy,
    ) -> Result<Self, RelationshipError> {
        relationship_graph_diagnostics(
            &collection,
            &pack,
            policy.reference_date,
            &policy.safeguards,
        )?;
        Ok(Self {
            collection,
            pack,
            policy,
            last_proposal: None,
            last_receipt: None,
        })
    }

    /// Current accepted collection state.
    #[must_use]
    pub const fn collection(&self) -> &CharacterCollection {
        &self.collection
    }

    /// Exact immutable relationship-kind pack.
    #[must_use]
    pub const fn pack(&self) -> &RelationshipKindPack {
        &self.pack
    }

    /// Exact project safety and reference-date policy used by graph views.
    #[must_use]
    pub const fn policy(&self) -> &RelationshipGraphPolicy {
        &self.policy
    }

    /// Most recently generated immutable proposal, if any.
    #[must_use]
    pub const fn last_proposal(&self) -> Option<&RelationshipProposal> {
        self.last_proposal.as_ref()
    }

    /// Most recently replayed receipt, if any.
    #[must_use]
    pub const fn last_receipt(&self) -> Option<&RelationshipReceipt> {
        self.last_receipt.as_ref()
    }

    /// Validate the whole current graph against pack semantics and project safeguards.
    pub fn validate(&self) -> Result<(), RelationshipError> {
        validate_relationship_graph(
            &self.collection,
            &self.pack,
            self.policy.reference_date,
            &self.policy.safeguards,
        )
    }

    /// Return all redaction-safe graph and safeguard diagnostics without changing state.
    pub fn diagnostics(&self) -> Result<Vec<RelationshipDiagnostic>, RelationshipError> {
        relationship_graph_diagnostics(
            &self.collection,
            &self.pack,
            self.policy.reference_date,
            &self.policy.safeguards,
        )
    }

    /// List stable edge views using the same filter contract as the CLI.
    pub fn list(
        &self,
        filter: &RelationshipFilter,
    ) -> Result<Vec<RelationshipEdgeView>, RelationshipError> {
        list_relationships(
            &self.collection,
            &self.pack,
            self.policy.reference_date,
            &self.policy.safeguards,
            filter,
        )
    }

    /// Inspect one exact owner/edge coordinate after complete graph validation.
    pub fn inspect(
        &self,
        owner_character_id: &str,
        edge_id: &str,
    ) -> Result<Option<RelationshipEdgeView>, RelationshipError> {
        Ok(self
            .list(&RelationshipFilter::default())?
            .into_iter()
            .find(|view| view.owner_character_id == owner_character_id && view.edge.id == edge_id))
    }

    /// Generate a deterministic proposal and retain it for review without mutating the graph.
    pub fn propose(
        &mut self,
        config: &RelationshipProposalConfig,
    ) -> Result<&RelationshipProposal, RelationshipError> {
        let proposal = propose_relationships(&self.collection, &self.pack, config)?;
        self.last_proposal = Some(proposal);
        Ok(self.last_proposal.as_ref().expect("proposal was retained"))
    }

    /// Create one complete decision map against the retained exact proposal.
    pub fn review(
        &self,
        reviewer: impl Into<String>,
        rationale: impl Into<String>,
        decisions: BTreeMap<String, RelationshipReviewDecision>,
    ) -> Result<RelationshipReview, RelationshipError> {
        let proposal = self
            .last_proposal
            .as_ref()
            .ok_or_else(missing_editor_proposal)?;
        create_relationship_review(proposal, decisions, reviewer, rationale)
    }

    /// Replay a complete review and optionally commit it as one editor transition.
    ///
    /// Dry-run and all errors preserve collection, policy, and retained proposal state.
    pub fn apply(
        &mut self,
        review: &RelationshipReview,
        dry_run: bool,
    ) -> Result<&RelationshipReceipt, RelationshipError> {
        let proposal = self
            .last_proposal
            .as_ref()
            .ok_or_else(missing_editor_proposal)?;
        let receipt = apply_reviewed_relationships(&self.collection, proposal, review)?;
        if !dry_run {
            self.collection = receipt.output_collection.clone();
            self.policy.reference_date = proposal.config.reference_date;
            self.policy.safeguards = proposal.config.safeguards.clone();
        }
        self.last_receipt = Some(receipt);
        Ok(self.last_receipt.as_ref().expect("receipt was retained"))
    }

    /// Apply direct authored/imported add and remove operations through one atomic revision.
    pub fn revise(
        &mut self,
        revision: &RelationshipGraphRevision,
        dry_run: bool,
    ) -> Result<CharacterCollection, RelationshipError> {
        let output = apply_relationship_graph_revision(&self.collection, &self.pack, revision)?;
        if !dry_run {
            self.collection = output.clone();
            self.policy.reference_date = revision.reference_date;
            self.policy.safeguards = revision.safeguards.clone();
            self.last_proposal = None;
            self.last_receipt = None;
        }
        Ok(output)
    }

    /// Produce deterministic repair suggestions without mutating the graph.
    pub fn reconcile(&self) -> Result<RelationshipReconciliationReport, RelationshipError> {
        reconcile_relationship_graph(
            &self.collection,
            &self.pack,
            self.policy.reference_date,
            &self.policy.safeguards,
        )
    }

    /// Export a clearly labeled row-oriented authoring-review CSV.
    pub fn edge_review_csv(
        &self,
        filter: &RelationshipFilter,
    ) -> Result<String, RelationshipError> {
        relationship_edge_review_csv(
            &self.collection,
            &self.pack,
            self.policy.reference_date,
            &self.policy.safeguards,
            filter,
        )
    }

    /// Export a clearly labeled dense authoring-review matrix CSV.
    pub fn matrix_review_csv(
        &self,
        filter: &RelationshipFilter,
    ) -> Result<String, RelationshipError> {
        relationship_dense_matrix_review_csv(
            &self.collection,
            &self.pack,
            self.policy.reference_date,
            &self.policy.safeguards,
            filter,
        )
    }

    /// Report whether a retained proposal is stale or malformed against current editor state.
    #[must_use]
    pub fn is_stale(&self, proposal: &RelationshipProposal) -> bool {
        if validate_relationship_proposal(proposal).is_err() {
            return true;
        }
        match propose_relationships(&self.collection, &self.pack, &proposal.config) {
            Ok(reproduced) => reproduced != *proposal,
            Err(_) => true,
        }
    }
}

fn missing_editor_proposal() -> RelationshipError {
    // Parsing a minimal invalid document produces the public, redaction-safe missing-state error
    // vocabulary without exposing author content or adding a second error contract.
    RelationshipProposal::from_json("{}").expect_err("empty proposal is invalid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_character::{
        CharacterCollection, RelationshipDiagnosticCode, RelationshipEdgeOrigin,
        RelationshipGraphRevision,
    };

    const BLANK: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/blank.character-collection.json"
    );
    const PACK: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/reference.relationship-kind-pack.json"
    );
    const POLICY: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/project.relationship-policy.json"
    );
    const REVISION: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/authored.relationship-revision.json"
    );
    const INPUT: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/input.character-collection.json"
    );
    const CONFIG: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/scoring.relationship-config.json"
    );
    const PROPOSAL: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/proposal.relationship-proposal.json"
    );
    const DECISIONS: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/decisions.relationship-review.json"
    );
    const REVIEW: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/review.relationship-review.json"
    );
    const RECEIPT: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/receipt.relationship-receipt.json"
    );
    const APPLIED: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/applied.character-collection.json"
    );
    const CONFLICTED: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/conflicted.character-collection.json"
    );
    const RECONCILIATION: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/reconciliation.relationship-reconciliation.json"
    );
    const EDGE_CSV: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/edges.review.csv"
    );
    const MATRIX_CSV: &str = include_str!(
        "../../examples/domain-modules/weave-character/relationships/matrix.review.csv"
    );

    fn pack() -> RelationshipKindPack {
        RelationshipKindPack::from_json(PACK).unwrap()
    }

    fn policy() -> RelationshipGraphPolicy {
        RelationshipGraphPolicy::from_json(POLICY).unwrap()
    }

    #[test]
    fn direct_revision_dry_run_commit_filter_and_inspect_are_one_editor_transition() {
        let blank = CharacterCollection::from_json(BLANK).unwrap();
        let expected = CharacterCollection::from_json(INPUT).unwrap();
        let revision = RelationshipGraphRevision::from_json(REVISION).unwrap();
        let mut session = RelationshipSession::open(blank.clone(), pack(), policy()).unwrap();

        assert_eq!(session.revise(&revision, true).unwrap(), expected);
        assert_eq!(session.collection(), &blank);
        assert_eq!(session.revise(&revision, false).unwrap(), expected);
        assert_eq!(session.collection(), &expected);

        let imported = session
            .list(&RelationshipFilter {
                origins: vec![RelationshipEdgeOrigin::Imported],
                ..RelationshipFilter::default()
            })
            .unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].edge.id, "mentor_ari_sable");
        assert_eq!(
            session
                .inspect("org.weave.character.ari_vale", "mentor_ari_sable")
                .unwrap()
                .unwrap(),
            imported[0]
        );
    }

    #[test]
    fn proposal_review_dry_run_and_commit_reproduce_checked_editor_bytes() {
        let input = CharacterCollection::from_json(INPUT).unwrap();
        let expected_proposal = RelationshipProposal::from_json(PROPOSAL).unwrap();
        let expected_review = RelationshipReview::from_json(REVIEW).unwrap();
        let expected_receipt = RelationshipReceipt::from_json(RECEIPT).unwrap();
        let expected_applied = CharacterCollection::from_json(APPLIED).unwrap();
        let config = RelationshipProposalConfig::from_json(CONFIG).unwrap();
        let decisions: BTreeMap<String, RelationshipReviewDecision> =
            serde_json::from_str(DECISIONS).unwrap();
        let mut session = RelationshipSession::open(input.clone(), pack(), policy()).unwrap();

        assert_eq!(session.propose(&config).unwrap(), &expected_proposal);
        let retained_proposal = session.last_proposal().unwrap().clone();
        let review = session
            .review(
                expected_review.reviewer.clone(),
                expected_review.rationale.clone(),
                decisions,
            )
            .unwrap();
        assert_eq!(review, expected_review);
        assert_eq!(session.apply(&review, true).unwrap(), &expected_receipt);
        assert_eq!(session.collection(), &input);
        assert!(!session.is_stale(&retained_proposal));

        assert_eq!(session.apply(&review, false).unwrap(), &expected_receipt);
        assert_eq!(session.collection(), &expected_applied);
        assert!(session.is_stale(&retained_proposal));
        assert_eq!(
            session
                .edge_review_csv(&RelationshipFilter::default())
                .unwrap(),
            EDGE_CSV
        );
        assert_eq!(
            session
                .matrix_review_csv(&RelationshipFilter::default())
                .unwrap(),
            MATRIX_CSV
        );
    }

    #[test]
    fn conflicted_graph_opens_for_redaction_safe_reconciliation_without_mutation() {
        let conflicted = CharacterCollection::from_json(CONFLICTED).unwrap();
        let expected = RelationshipReconciliationReport::from_json(RECONCILIATION).unwrap();
        let session = RelationshipSession::open(conflicted.clone(), pack(), policy()).unwrap();

        assert!(session.validate().is_err());
        let diagnostics = session.diagnostics().unwrap();
        for code in [
            RelationshipDiagnosticCode::BrokenInverse,
            RelationshipDiagnosticCode::StaleEvidence,
            RelationshipDiagnosticCode::DuplicateEdge,
        ] {
            assert!(diagnostics.iter().any(|value| value.code == code));
        }
        assert_eq!(session.reconcile().unwrap(), expected);
        assert_eq!(session.collection(), &conflicted);
    }
}
