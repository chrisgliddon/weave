//! Editor-facing Character corpus commands backed by the shared operation contract.

use weave_character::{
    CharacterCollection, CharacterCorpusError, CharacterJobProgress, CharacterJobStep,
    CharacterOperationRequest, CharacterProfile, CharacterProposal, CharacterProposalReview,
    CharacterReviewDecision, CharacterScope, CharacterSummary, apply_reviewed_character_proposal,
    list_characters, propose_character_operation, resume_character_operation,
    review_character_proposal, show_character, validate_character_collection,
};

/// In-memory editor session with atomic Character proposal application.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterCorpusSession {
    collection: CharacterCollection,
}

impl CharacterCorpusSession {
    /// Open one independently validated collection.
    pub fn open(collection: CharacterCollection) -> Result<Self, CharacterCorpusError> {
        validate_character_collection(&collection)?;
        Ok(Self { collection })
    }

    /// Current accepted collection state.
    #[must_use]
    pub const fn collection(&self) -> &CharacterCollection {
        &self.collection
    }

    /// List deterministic summaries using the shared scope contract.
    pub fn list(
        &self,
        scope: &CharacterScope,
    ) -> Result<Vec<CharacterSummary>, CharacterCorpusError> {
        list_characters(&self.collection, scope)
    }

    /// Show one exact profile by stable id.
    pub fn show(&self, id: &str) -> Result<&CharacterProfile, CharacterCorpusError> {
        show_character(&self.collection, id)
    }

    /// Preview one operation without changing accepted editor state.
    pub fn propose(
        &self,
        request: &CharacterOperationRequest,
    ) -> Result<CharacterProposal, CharacterCorpusError> {
        propose_character_operation(&self.collection, request)
    }

    /// Advance one payload-free resumable proposal job.
    pub fn resume(
        &self,
        request: &CharacterOperationRequest,
        progress: Option<&CharacterJobProgress>,
        max_items: usize,
    ) -> Result<CharacterJobStep, CharacterCorpusError> {
        resume_character_operation(&self.collection, request, progress, max_items)
    }

    /// Create a whole-proposal review through the shared validator.
    pub fn review(
        &self,
        proposal: &CharacterProposal,
        decision: CharacterReviewDecision,
        reviewer: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Result<CharacterProposalReview, CharacterCorpusError> {
        review_character_proposal(proposal, decision, reviewer, rationale)
    }

    /// Reproduce a reviewed proposal and optionally commit it as one in-memory transition.
    ///
    /// Dry-run returns the exact candidate while preserving the current collection. Every error
    /// also leaves the session unchanged.
    pub fn apply(
        &mut self,
        proposal: &CharacterProposal,
        review: &CharacterProposalReview,
        dry_run: bool,
    ) -> Result<CharacterCollection, CharacterCorpusError> {
        let candidate = apply_reviewed_character_proposal(&self.collection, proposal, review)?;
        if !dry_run {
            self.collection = candidate.clone();
        }
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLLECTION: &str = include_str!(
        "../../examples/domain-modules/weave-character/operations/collection.character-collection.json"
    );
    const REQUEST: &str = include_str!(
        "../../examples/domain-modules/weave-character/operations/rename.character-request.json"
    );
    const PROPOSAL: &str = include_str!(
        "../../examples/domain-modules/weave-character/operations/rename.character-proposal.json"
    );
    const REVIEW: &str = include_str!(
        "../../examples/domain-modules/weave-character/operations/rename.character-review.json"
    );
    const RENAMED: &str = include_str!(
        "../../examples/domain-modules/weave-character/operations/renamed.character-collection.json"
    );

    #[test]
    fn editor_preview_resume_and_apply_match_source_and_cli_bytes() {
        let collection = CharacterCollection::from_json(COLLECTION).unwrap();
        let request = CharacterOperationRequest::from_json(REQUEST).unwrap();
        let proposal = CharacterProposal::from_json(PROPOSAL).unwrap();
        let review = CharacterProposalReview::from_json(REVIEW).unwrap();
        let renamed = CharacterCollection::from_json(RENAMED).unwrap();
        let mut session = CharacterCorpusSession::open(collection.clone()).unwrap();

        assert_eq!(session.propose(&request).unwrap(), proposal);
        let first = session.resume(&request, None, 1).unwrap();
        assert!(first.proposal.is_none());
        assert_eq!(
            session
                .resume(&request, Some(&first.progress), 1)
                .unwrap()
                .proposal
                .unwrap(),
            proposal
        );
        assert_eq!(session.apply(&proposal, &review, true).unwrap(), renamed);
        assert_eq!(session.collection(), &collection);
        assert_eq!(session.apply(&proposal, &review, false).unwrap(), renamed);
        assert_eq!(session.collection(), &renamed);
    }

    #[test]
    fn rejected_or_stale_editor_apply_preserves_the_complete_collection() {
        let collection = CharacterCollection::from_json(COLLECTION).unwrap();
        let proposal = CharacterProposal::from_json(PROPOSAL).unwrap();
        let mut session = CharacterCorpusSession::open(collection.clone()).unwrap();
        let rejected = session
            .review(
                &proposal,
                CharacterReviewDecision::Rejected,
                "org.weave.reviewer.editor",
                "Reject the whole proposal from the editor review surface.",
            )
            .unwrap();
        assert!(session.apply(&proposal, &rejected, false).is_err());
        assert_eq!(session.collection(), &collection);

        let mut stale = CharacterProposalReview::from_json(REVIEW).unwrap();
        stale.proposal_sha256 = "0".repeat(64);
        assert!(session.apply(&proposal, &stale, false).is_err());
        assert_eq!(session.collection(), &collection);
    }
}
