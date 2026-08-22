//! Editor-facing deterministic Character presentation catalog workflow.

use std::collections::BTreeMap;

use weave_character::{
    CharacterCollection, PresentationAllocationRequest, PresentationCatalog,
    PresentationLockRevision, PresentationProposal, PresentationReceipt, PresentationReview,
    PresentationReviewDecision, apply_presentation_lock_revision, apply_presentation_review,
    propose_presentation_allocations, review_presentation_proposal,
    validate_presentation_allocation_request,
};

/// Atomic editor session backed only by the shared host-independent presentation contract.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterPresentationSession {
    collection: CharacterCollection,
    catalog: PresentationCatalog,
    request: PresentationAllocationRequest,
    last_proposal: Option<PresentationProposal>,
    last_receipt: Option<PresentationReceipt>,
}

impl CharacterPresentationSession {
    /// Open exact collection, catalog, and request inputs without changing any value.
    pub fn open(
        collection: CharacterCollection,
        catalog: PresentationCatalog,
        request: PresentationAllocationRequest,
    ) -> Result<Self, weave_character::CharacterError> {
        let _ = propose_presentation_allocations(&collection, &catalog, &request)?;
        Ok(Self {
            collection,
            catalog,
            request,
            last_proposal: None,
            last_receipt: None,
        })
    }

    #[must_use]
    pub const fn collection(&self) -> &CharacterCollection {
        &self.collection
    }

    #[must_use]
    pub const fn catalog(&self) -> &PresentationCatalog {
        &self.catalog
    }

    #[must_use]
    pub const fn request(&self) -> &PresentationAllocationRequest {
        &self.request
    }

    #[must_use]
    pub const fn last_proposal(&self) -> Option<&PresentationProposal> {
        self.last_proposal.as_ref()
    }

    #[must_use]
    pub const fn last_receipt(&self) -> Option<&PresentationReceipt> {
        self.last_receipt.as_ref()
    }

    /// Produce the exact source/CLI-equivalent dry-run and retain it for inspection.
    pub fn propose(&mut self) -> Result<&PresentationProposal, weave_character::CharacterError> {
        let proposal =
            propose_presentation_allocations(&self.collection, &self.catalog, &self.request)?;
        self.last_proposal = Some(proposal);
        self.last_receipt = None;
        Ok(self.last_proposal.as_ref().expect("proposal just assigned"))
    }

    /// Build one complete review without mutating editor state.
    pub fn review(
        &self,
        proposal: &PresentationProposal,
        reviewer: impl Into<String>,
        rationale: impl Into<String>,
        decisions: BTreeMap<String, BTreeMap<String, PresentationReviewDecision>>,
    ) -> Result<PresentationReview, weave_character::CharacterError> {
        review_presentation_proposal(proposal, reviewer, rationale, decisions)
    }

    /// Replay one review. Dry-run retains the prior collection; commit swaps it atomically.
    pub fn apply(
        &mut self,
        proposal: &PresentationProposal,
        review: &PresentationReview,
        dry_run: bool,
    ) -> Result<&PresentationReceipt, weave_character::CharacterError> {
        let receipt = apply_presentation_review(&self.collection, proposal, review)?;
        if !dry_run {
            self.collection = receipt.output_collection.clone();
        }
        self.last_receipt = Some(receipt);
        Ok(self.last_receipt.as_ref().expect("receipt just assigned"))
    }

    /// Apply a lock/unlock request atomically, or validate it without changing editor state.
    pub fn revise_locks(
        &mut self,
        revision: &PresentationLockRevision,
        dry_run: bool,
    ) -> Result<CharacterCollection, weave_character::CharacterError> {
        let candidate = apply_presentation_lock_revision(&self.collection, revision)?;
        if !dry_run {
            self.collection = candidate.clone();
            self.last_proposal = None;
            self.last_receipt = None;
        }
        Ok(candidate)
    }

    /// Replace the exact request after a commit or author choice, clearing stale previews.
    pub fn set_request(
        &mut self,
        request: PresentationAllocationRequest,
    ) -> Result<(), weave_character::CharacterError> {
        validate_presentation_allocation_request(&request)?;
        self.request = request;
        self.last_proposal = None;
        self.last_receipt = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_character::{
        LockState, PRESENTATION_LOCK_REVISION_FORMAT_VERSION, PresentationLockTarget,
        collection_fingerprint,
    };

    const COLLECTION: &str = include_str!(
        "../../examples/domain-modules/weave-character/presentation/input.character-collection.json"
    );
    const CATALOG: &str = include_str!(
        "../../examples/domain-modules/weave-character/presentation/glasswind.presentation-catalog.json"
    );
    const REQUEST: &str = include_str!(
        "../../examples/domain-modules/weave-character/presentation/allocation.presentation-request.json"
    );
    const PROPOSAL: &str = include_str!(
        "../../examples/domain-modules/weave-character/presentation/proposal.presentation-proposal.json"
    );
    const REVIEW: &str = include_str!(
        "../../examples/domain-modules/weave-character/presentation/review.presentation-review.json"
    );
    const RECEIPT: &str = include_str!(
        "../../examples/domain-modules/weave-character/presentation/receipt.presentation-receipt.json"
    );

    #[test]
    fn editor_dry_run_commit_and_lock_match_shared_artifacts() {
        let collection = CharacterCollection::from_json(COLLECTION).unwrap();
        let catalog = PresentationCatalog::from_json(CATALOG).unwrap();
        let request = PresentationAllocationRequest::from_json(REQUEST).unwrap();
        let proposal = PresentationProposal::from_json(PROPOSAL).unwrap();
        let review = PresentationReview::from_json(REVIEW).unwrap();
        let expected_receipt = PresentationReceipt::from_json(RECEIPT).unwrap();
        let mut session =
            CharacterPresentationSession::open(collection.clone(), catalog.clone(), request)
                .unwrap();

        assert_eq!(session.propose().unwrap(), &proposal);
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

        let lock_revision = PresentationLockRevision {
            revision_format_version: PRESENTATION_LOCK_REVISION_FORMAT_VERSION,
            id: "org.weave.character.presentation.unlock_avatar".to_owned(),
            expected_input_sha256: collection_fingerprint(session.collection()).unwrap(),
            targets: vec![PresentationLockTarget {
                character_id: "org.weave.character.ari_vale".to_owned(),
                slot_id: "avatar".to_owned(),
            }],
            lock: LockState::Unlocked,
            rationale: "Unlock the reviewed avatar for a later deterministic rebalance.".to_owned(),
            provenance: catalog.provenance,
        };
        let dry_run = session.revise_locks(&lock_revision, true).unwrap();
        assert_ne!(dry_run, *session.collection());
        let committed = session.revise_locks(&lock_revision, false).unwrap();
        assert_eq!(session.collection(), &committed);
    }
}
