//! Editor-facing explainable classification, vocation, social-role, and narrative-role workflow.

use std::collections::BTreeMap;

use weave_character::{
    CharacterCollection, ProjectionConfig, ProjectionLockRevision, ProjectionPack,
    ProjectionProposal, ProjectionProposedAssignment, ProjectionReceipt, ProjectionReview,
    ProjectionReviewDecision, apply_projection_lock_revision, apply_projection_review,
    create_projection_review, propose_projections, validate_projection_config,
};

/// Atomic editor session backed only by the shared host-independent projection contract.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionSession {
    collection: CharacterCollection,
    pack: ProjectionPack,
    config: ProjectionConfig,
    seed: u64,
    last_proposal: Option<ProjectionProposal>,
    last_receipt: Option<ProjectionReceipt>,
}

impl ProjectionSession {
    /// Open exact collection, pack, configuration, and seed inputs without changing any value.
    pub fn open(
        collection: CharacterCollection,
        pack: ProjectionPack,
        config: ProjectionConfig,
        seed: u64,
    ) -> Result<Self, weave_character::CharacterError> {
        let _ = propose_projections(&collection, &pack, &config, seed)?;
        Ok(Self {
            collection,
            pack,
            config,
            seed,
            last_proposal: None,
            last_receipt: None,
        })
    }

    #[must_use]
    pub const fn collection(&self) -> &CharacterCollection {
        &self.collection
    }

    #[must_use]
    pub const fn pack(&self) -> &ProjectionPack {
        &self.pack
    }

    #[must_use]
    pub const fn config(&self) -> &ProjectionConfig {
        &self.config
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub const fn last_proposal(&self) -> Option<&ProjectionProposal> {
        self.last_proposal.as_ref()
    }

    #[must_use]
    pub const fn last_receipt(&self) -> Option<&ProjectionReceipt> {
        self.last_receipt.as_ref()
    }

    /// Produce the exact source/CLI-equivalent dry-run and retain every evidence trace.
    pub fn propose(&mut self) -> Result<&ProjectionProposal, weave_character::CharacterError> {
        let proposal = propose_projections(&self.collection, &self.pack, &self.config, self.seed)?;
        self.last_proposal = Some(proposal);
        self.last_receipt = None;
        Ok(self.last_proposal.as_ref().expect("proposal just assigned"))
    }

    /// Return one complete evidence, threshold, capacity, reservation, and tie-break trace.
    pub fn inspect<'a>(
        &self,
        proposal: &'a ProjectionProposal,
        character_id: &str,
        taxonomy_id: &str,
    ) -> Option<&'a ProjectionProposedAssignment> {
        proposal
            .assignments
            .get(character_id)
            .and_then(|values| values.get(taxonomy_id))
    }

    /// Build one complete review without mutating editor state.
    pub fn review(
        &self,
        proposal: &ProjectionProposal,
        reviewer: impl Into<String>,
        rationale: impl Into<String>,
        decisions: BTreeMap<String, BTreeMap<String, ProjectionReviewDecision>>,
    ) -> Result<ProjectionReview, weave_character::CharacterError> {
        create_projection_review(proposal, reviewer, rationale, decisions)
    }

    /// Replay one review. Dry-run retains the prior collection; commit swaps it atomically.
    pub fn apply(
        &mut self,
        proposal: &ProjectionProposal,
        review: &ProjectionReview,
        dry_run: bool,
    ) -> Result<&ProjectionReceipt, weave_character::CharacterError> {
        let receipt = apply_projection_review(&self.collection, proposal, review)?;
        if !dry_run {
            self.collection = receipt.output_collection.clone();
            self.last_proposal = None;
        }
        self.last_receipt = Some(receipt);
        Ok(self.last_receipt.as_ref().expect("receipt just assigned"))
    }

    /// Apply a lock/unlock request atomically, or validate it without changing editor state.
    pub fn revise_locks(
        &mut self,
        revision: &ProjectionLockRevision,
        dry_run: bool,
    ) -> Result<CharacterCollection, weave_character::CharacterError> {
        let candidate = apply_projection_lock_revision(&self.collection, revision)?;
        if !dry_run {
            self.collection = candidate.clone();
            self.last_proposal = None;
            self.last_receipt = None;
        }
        Ok(candidate)
    }

    /// Replace project selection/capacity policy and clear stale previews.
    pub fn set_config(
        &mut self,
        config: ProjectionConfig,
    ) -> Result<(), weave_character::CharacterError> {
        validate_projection_config(&config, &self.pack, &self.collection)?;
        self.config = config;
        self.last_proposal = None;
        self.last_receipt = None;
        Ok(())
    }

    /// Replace the deterministic tie-break seed and clear stale previews.
    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.last_proposal = None;
        self.last_receipt = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLLECTION: &str = include_str!(
        "../../examples/domain-modules/weave-character/projections/input.character-collection.json"
    );
    const PACK: &str = include_str!(
        "../../examples/domain-modules/weave-character/projections/glasswind.projection-pack.json"
    );
    const CONFIG: &str = include_str!(
        "../../examples/domain-modules/weave-character/projections/selection.projection-config.json"
    );
    const PROPOSAL: &str = include_str!(
        "../../examples/domain-modules/weave-character/projections/proposal.projection-proposal.json"
    );
    const REVIEW: &str = include_str!(
        "../../examples/domain-modules/weave-character/projections/review.projection-review.json"
    );
    const RECEIPT: &str = include_str!(
        "../../examples/domain-modules/weave-character/projections/receipt.projection-receipt.json"
    );
    const LOCK_REVISION: &str = include_str!(
        "../../examples/domain-modules/weave-character/projections/unlock.projection-lock-revision.json"
    );
    const REBALANCE_CONFIG: &str = include_str!(
        "../../examples/domain-modules/weave-character/projections/rebalance.projection-config.json"
    );
    const REBALANCE_PROPOSAL: &str = include_str!(
        "../../examples/domain-modules/weave-character/projections/rebalance.projection-proposal.json"
    );

    #[test]
    fn editor_inspection_review_commit_lock_and_rebalance_match_shared_artifacts() {
        let collection = CharacterCollection::from_json(COLLECTION).unwrap();
        let pack = ProjectionPack::from_json(PACK).unwrap();
        let config = ProjectionConfig::from_json(CONFIG).unwrap();
        let proposal = ProjectionProposal::from_json(PROPOSAL).unwrap();
        let review = ProjectionReview::from_json(REVIEW).unwrap();
        let expected_receipt = ProjectionReceipt::from_json(RECEIPT).unwrap();
        let mut session =
            ProjectionSession::open(collection.clone(), pack, config, 20_260_824).unwrap();

        assert_eq!(session.propose().unwrap(), &proposal);
        let inspected = session
            .inspect(
                &proposal,
                "org.weave.character.ari_vale",
                "org.weave.projection.glasswind_lenses.narrative_role",
            )
            .unwrap();
        assert!(inspected.candidates.iter().all(|candidate| {
            !candidate.ordered_evidence.is_empty()
                && !candidate.seeded_sha256.is_empty()
                && !candidate.explanation.is_empty()
        }));

        assert_eq!(
            session.apply(&proposal, &review, true).unwrap(),
            &expected_receipt
        );
        assert_eq!(session.collection(), &collection);
        assert_eq!(
            session.apply(&proposal, &review, false).unwrap(),
            &expected_receipt
        );
        assert_eq!(session.collection(), &expected_receipt.output_collection);

        let revision = ProjectionLockRevision::from_json(LOCK_REVISION).unwrap();
        let dry_run = session.revise_locks(&revision, true).unwrap();
        assert_ne!(dry_run, *session.collection());
        let committed = session.revise_locks(&revision, false).unwrap();
        assert_eq!(session.collection(), &committed);

        session
            .set_config(ProjectionConfig::from_json(REBALANCE_CONFIG).unwrap())
            .unwrap();
        session.set_seed(20_260_825);
        assert_eq!(
            session.propose().unwrap(),
            &ProjectionProposal::from_json(REBALANCE_PROPOSAL).unwrap()
        );
    }
}
